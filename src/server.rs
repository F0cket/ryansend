use anyhow::{anyhow, Result};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::io::SeekFrom;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::auth::verify_token_and_get_path;
use crate::config::Config;
use crate::error::{handle_404, AppError, AppResult};

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
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    token: String,
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

    let file_path = verify_token_and_get_path(&state.config.secret_key, &params.token)
        .await
        .map_err(|e| {
            warn!("Token verification failed: {}", e);
            AppError::not_found(anyhow::anyhow!("Token verification failed"))
        })?;

    let path = PathBuf::from(&file_path);

    if !path.exists() {
        warn!("File not found: {}", file_path);
        return Err(AppError::not_found(anyhow::anyhow!(
            "File not found: {}",
            file_path
        )));
    }

    let file = fs::File::open(&path).await.map_err(|e| {
        error!("Failed to open file {}: {}", file_path, e);
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
            error!("Failed to get file metadata for {}: {}", file_path, e);
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
            error!("Failed to reopen file {}: {}", file_path, e);
            anyhow::anyhow!("Failed to reopen file: {}", e)
        })?;

        // Seek to the start position
        seekable_file
            .seek(SeekFrom::Start(start))
            .await
            .map_err(|e| {
                error!(
                    "Failed to seek to position {} in file {}: {}",
                    start, file_path, e
                );
                anyhow::anyhow!("Failed to seek to position {}: {}", start, e)
            })?;

        // Take only the requested range
        let limited_file = seekable_file.take(content_length);

        info!(
            "Partial file served: '{}' bytes {}-{}/{} ({} bytes) from path: {}",
            file_name, start, end, file_size, content_length, file_path
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
        info!(
            "File downloaded: '{}' ({} bytes) from path: {}",
            file_name, file_size, file_path
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

pub async fn run_server(config: Config) -> Result<()> {
    let state = AppState {
        config: config.clone(),
    };

    let app = Router::new()
        .route("/download", get(download_handler))
        .fallback(handle_404)
        .with_state(state);

    info!("Starting server on http://0.0.0.0:{}", config.port);

    // Show admin panel info if enabled
    if let Some(admin_config) = &config.admin {
        if admin_config.enabled {
            info!("📋 Admin panel available at: http://localhost:{}/admin/login", admin_config.port);
        }
    }

    let bind_address = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .map_err(|e| anyhow!("Failed to bind to port {}: {}", config.port, e))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow!("Server error: {}", e))?;

    Ok(())
}
