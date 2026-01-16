use anyhow::{anyhow, Result};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use timedmap::TimedMap;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;

use crate::auth::verify_token_and_get_claims;
use crate::config::Config;
use crate::error::{handle_404, AppError, AppResult};
use crate::logging_middleware::logging_middleware;
use crate::tls::{self, ChallengeStore};
use crate::ReloadSignal;

#[derive(Debug, Clone)]
struct ByteRange {
    start: u64,
    end: Option<u64>,
}

fn parse_range_header(range_header: &str, file_size: u64) -> Option<ByteRange> {
    // Parse "bytes=start-end" format
    if !range_header.starts_with("bytes=") {
        return None;
    }

    let range_spec = &range_header[6..]; // Remove "bytes=" prefix
    let parts: Vec<&str> = range_spec.split('-').collect();

    if parts.len() != 2 {
        return None;
    }

    let start_str = parts[0].trim();
    let end_str = parts[1].trim();

    // Handle different range formats:
    // "start-end", "start-", "-suffix"
    let range = if start_str.is_empty() {
        // Suffix range: "-500" means last 500 bytes
        if let Ok(suffix) = end_str.parse::<u64>() {
            let start = file_size.saturating_sub(suffix);
            ByteRange {
                start,
                end: Some(file_size - 1),
            }
        } else {
            return None;
        }
    } else if let Ok(start) = start_str.parse::<u64>() {
        if start >= file_size {
            return None; // Start beyond file size
        }

        let end = if end_str.is_empty() {
            // "start-" means from start to end of file
            Some(file_size - 1)
        } else if let Ok(end_val) = end_str.parse::<u64>() {
            // "start-end" format
            Some(std::cmp::min(end_val, file_size - 1))
        } else {
            return None;
        };

        ByteRange { start, end }
    } else {
        return None;
    };

    Some(range)
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub download_tracker: Arc<Mutex<TimedMap<String, u32>>>,
    pub challenge_store: ChallengeStore,
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    token: String,
}

/// Handle ACME HTTP-01 challenge requests
/// Path: /.well-known/acme-challenge/{token}
pub async fn acme_challenge_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    debug!("ACME challenge request for token: {}", token);

    match tls::serve_acme_challenge(&token, state.challenge_store).await {
        Ok(key_auth) => {
            info!("Serving ACME challenge for token: {}", token);
            (StatusCode::OK, key_auth)
        }
        Err(e) => {
            warn!("ACME challenge token not found: {} - {}", token, e);
            (StatusCode::NOT_FOUND, "Challenge not found".to_string())
        }
    }
}

pub async fn download_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<DownloadQuery>,
) -> AppResult<Response> {
    debug!(
        "Download request with token: {}...",
        &params.token[..std::cmp::min(20, params.token.len())]
    );

    let claims = verify_token_and_get_claims(&state.config.secret_key, &params.token)
        .await
        .map_err(|e| {
            warn!("Token verification failed: {}", e);
            AppError::not_found(anyhow::anyhow!("Token verification failed"))
        })?;

    // Check if token has max_uses limit
    if let Some(max_uses) = claims.max_uses {
        #[allow(unused_mut)]
        let mut tracker = state.download_tracker.lock().await;
        let current_uses: u32 = tracker.get(&claims.id).unwrap_or_default();

        if current_uses >= max_uses {
            warn!(
                "Token {} has exceeded max uses ({}/{})",
                claims.id, current_uses, max_uses
            );
            return Err(AppError::not_found(anyhow::anyhow!(
                "Token has exceeded maximum uses"
            )));
        }

        // Calculate duration until one minute past token expiration
        let now = chrono::Utc::now();
        let track_until = claims.exp + chrono::Duration::minutes(1);
        let duration_secs = (track_until - now).num_seconds().max(0) as u64;

        // Update usage count
        tracker.insert(
            claims.id.clone(),
            current_uses + 1,
            std::time::Duration::from_secs(duration_secs),
        );

        info!(
            "Token {} usage: {}/{}",
            claims.id,
            current_uses + 1,
            max_uses
        );
    }

    let path = PathBuf::from(&claims.path);

    if !path.exists() {
        warn!("File not found: {}", claims.path);
        return Err(AppError::not_found(anyhow::anyhow!(
            "File not found: {}",
            claims.path
        )));
    }

    let file = fs::File::open(&path).await.map_err(|e| {
        error!("Failed to open file {}: {}", claims.path, e);
        anyhow::anyhow!("Failed to open file: {}", e)
    })?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");

    // Get file size for logging and content-length header
    let file_size = file
        .metadata()
        .await
        .map_err(|e| {
            error!("Failed to get file metadata for {}: {}", claims.path, e);
            anyhow::anyhow!("Failed to get file metadata: {}", e)
        })?
        .len();

    // Check for Range header to support resume functionality
    let range_request = headers
        .get(header::RANGE)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| parse_range_header(h, file_size));

    // Determine response parameters based on range request
    let (status_code, start, end, content_length, file_reader) = if let Some(range) = range_request
    {
        let start = range.start;
        let end = range.end.unwrap_or(file_size - 1);
        let content_length = end - start + 1;

        debug!(
            "Range request for '{}': bytes {}-{}/{} ({} bytes)",
            file_name, start, end, file_size, content_length
        );

        // Create a new file handle for seeking
        let mut seekable_file = fs::File::open(&path).await.map_err(|e| {
            error!("Failed to reopen file {}: {}", claims.path, e);
            anyhow::anyhow!("Failed to reopen file: {}", e)
        })?;

        // Seek to the start position
        seekable_file
            .seek(SeekFrom::Start(start))
            .await
            .map_err(|e| {
                error!(
                    "Failed to seek to position {} in file {}: {}",
                    start, claims.path, e
                );
                anyhow::anyhow!("Failed to seek to position {}: {}", start, e)
            })?;

        // Take only the requested range
        let limited_file = seekable_file.take(content_length);

        let note_info = match &claims.note {
            Some(note) => format!(" (note: \"{}\")", note),
            None => String::new(),
        };
        info!(
            "Partial file served: '{}' bytes {}-{}/{} ({} bytes) from path: {} [token_id: {}]{}",
            file_name, start, end, file_size, content_length, claims.path, claims.id, note_info
        );

        (
            StatusCode::PARTIAL_CONTENT,
            start,
            end,
            content_length,
            Box::new(limited_file) as Box<dyn tokio::io::AsyncRead + Unpin + Send>,
        )
    } else {
        // No range request, serve the entire file
        let note_info = match &claims.note {
            Some(note) => format!(" (note: \"{}\")", note),
            None => String::new(),
        };
        info!(
            "File downloaded: '{}' ({} bytes) from path: {} [token_id: {}]{}",
            file_name, file_size, claims.path, claims.id, note_info
        );

        (
            StatusCode::OK,
            0,
            file_size - 1,
            file_size,
            Box::new(file) as Box<dyn tokio::io::AsyncRead + Unpin + Send>,
        )
    };

    // Create the response stream and body
    let stream = ReaderStream::new(file_reader);
    let body = Body::from_stream(stream);

    let mut response = Response::builder()
        .status(status_code)
        .body(body)
        .map_err(|e| anyhow::anyhow!("Failed to build response: {}", e))?;

    // Set common headers
    let response_headers = response.headers_mut();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", file_name))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    response_headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    response_headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));

    // Add Content-Range header for partial content
    if status_code == StatusCode::PARTIAL_CONTENT {
        response_headers.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, file_size))
                .unwrap_or_else(|_| HeaderValue::from_static("bytes 0-0/0")),
        );
    }

    Ok(response)
}

pub async fn run_server(
    config: Config,
    mut reload_rx: tokio::sync::broadcast::Receiver<ReloadSignal>,
    challenge_store: tls::ChallengeStore,
) -> Result<()> {
    // Start certificate renewal background task
    let _renewal_task = tls::start_renewal_task(config.clone(), challenge_store.clone());

    let state = AppState {
        config: config.clone(),
        download_tracker: Arc::new(Mutex::new(TimedMap::new())),
        challenge_store: challenge_store.clone(),
    };

    let app = Router::new()
        .route("/download", get(download_handler))
        .route(
            "/.well-known/acme-challenge/:token",
            get(acme_challenge_handler),
        )
        .fallback(handle_404)
        .with_state(state)
        .layer(middleware::from_fn(logging_middleware));

    // Check if TLS is configured
    let has_tls = config.has_tls_cert() && config.tls_port.is_some();

    if has_tls {
        // Load TLS certificate
        match tls::load_cert_from_config(&config).await? {
            Some(tls_cert) => {
                let server_config = tls_cert.into_server_config()?;
                let tls_port = match config.tls_port {
                    Some(port) => port,
                    None => {
                        error!("TLS port is not configured");
                        return Err(anyhow!("TLS port is required when TLS is enabled"));
                    }
                };

                info!("🔒 Starting HTTPS server on https://0.0.0.0:{}", tls_port);
                info!("🔓 Starting HTTP server on http://0.0.0.0:{}", config.port);

                // Show admin panel info if enabled
                if let Some(admin_config) = &config.admin {
                    if admin_config.enabled {
                        info!(
                            "📋 Admin panel available at: http://localhost:{}/admin/login",
                            admin_config.port
                        );
                    }
                }

                // Start HTTP server for ACME challenges
                let http_bind_address = format!("0.0.0.0:{}", config.port);
                let http_listener = tokio::net::TcpListener::bind(&http_bind_address)
                    .await
                    .map_err(|e| anyhow!("Failed to bind to HTTP port {}: {}", config.port, e))?;

                // Start HTTPS server
                let https_bind_address = format!("0.0.0.0:{}", tls_port);

                // Clone app for both servers
                let http_app = app.clone();
                let https_app = app;

                // Run both servers concurrently with graceful shutdown
                let http_server = async move {
                    axum::serve(http_listener, http_app)
                        .with_graceful_shutdown(async move {
                            reload_rx.recv().await.ok();
                            info!("HTTP server shutting down for reload...");
                        })
                        .await
                        .map_err(|e| anyhow!("HTTP server error: {}", e))
                };

                let https_server = async move {
                    let rustls_config = axum_server::tls_rustls::RustlsConfig::from_config(
                        std::sync::Arc::new(server_config),
                    );
                    let addr = match https_bind_address.parse() {
                        Ok(addr) => addr,
                        Err(e) => {
                            error!(
                                "Failed to parse HTTPS bind address '{}': {}",
                                https_bind_address, e
                            );
                            return Err(anyhow!("Failed to parse HTTPS bind address: {}", e));
                        }
                    };
                    axum_server::bind_rustls(addr, rustls_config)
                        .serve(https_app.into_make_service())
                        .await
                        .map_err(|e| anyhow!("HTTPS server error: {}", e))
                };

                tokio::try_join!(http_server, https_server)?;
                Ok(())
            }
            None => {
                warn!(
                    "TLS configured but certificate could not be loaded, falling back to HTTP only"
                );
                // Fall through to HTTP-only mode
                run_http_only_server(config, app, reload_rx).await
            }
        }
    } else {
        info!("Starting server on http://0.0.0.0:{}", config.port);

        // Show admin panel info if enabled
        if let Some(admin_config) = &config.admin {
            if admin_config.enabled {
                info!(
                    "📋 Admin panel available at: http://localhost:{}/admin/login",
                    admin_config.port
                );
            }
        }

        run_http_only_server(config, app, reload_rx).await
    }
}

/// Run HTTP-only server (fallback when TLS is not configured)
async fn run_http_only_server(
    config: Config,
    app: Router,
    mut reload_rx: tokio::sync::broadcast::Receiver<crate::ReloadSignal>,
) -> Result<()> {
    let bind_address = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .map_err(|e| anyhow!("Failed to bind to port {}: {}", config.port, e))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            reload_rx.recv().await.ok();
            info!("HTTP server shutting down for reload...");
        })
        .await
        .map_err(|e| anyhow!("Server error: {}", e))?;

    Ok(())
}
