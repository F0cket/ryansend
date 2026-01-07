use anyhow::{anyhow, Result};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::path::PathBuf;
use tokio::fs;
use tokio_util::io::ReaderStream;

use crate::auth::verify_token_and_get_path;
use crate::config::Config;
use crate::error::{handle_404, make_error_response};

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
    Query(params): Query<DownloadQuery>,
) -> Response {
    debug!(
        "Download request with token: {}...",
        &params.token[..std::cmp::min(20, params.token.len())]
    );

    let file_path = match verify_token_and_get_path(&state.config.secret_key, &params.token).await {
        Ok(path) => path,
        Err(e) => {
            warn!("Token verification failed: {}", e);
            return make_error_response(StatusCode::NOT_FOUND);
        }
    };

    let path = PathBuf::from(&file_path);

    if !path.exists() {
        warn!("File not found: {}", file_path);
        return make_error_response(StatusCode::NOT_FOUND);
    }

    let file = match fs::File::open(&path).await {
        Ok(file) => file,
        Err(e) => {
            error!("Failed to open file {}: {}", file_path, e);
            return make_error_response(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");

    // Get file size for logging and content-length header
    let file_size = match file.metadata().await {
        Ok(metadata) => metadata.len(),
        Err(e) => {
            error!("Failed to get file metadata for {}: {}", file_path, e);
            return make_error_response(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    info!(
        "File downloaded: '{}' ({} bytes) from path: {}",
        file_name, file_size, file_path
    );

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut response = Response::new(body);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", file_name))
            .unwrap_or(HeaderValue::from_static("attachment")),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&file_size.to_string()).unwrap_or(HeaderValue::from_static("0")),
    );

    response
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

    let bind_address = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .map_err(|e| anyhow!("Failed to bind to port {}: {}", config.port, e))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow!("Server error: {}", e))?;

    Ok(())
}
