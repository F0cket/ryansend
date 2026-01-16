use axum::{body::Body, extract::Request, middleware::Next, response::Response};
use log::{error, info, warn};
use std::time::Instant;

/// Middleware that logs each request with path (without query params) and response status code
/// Uses different log levels based on status code: info for success, warn for 4xx, error for 5xx
pub async fn logging_middleware(request: Request<Body>, next: Next) -> Response {
    let start = Instant::now();

    // Extract the path without query parameters
    let path = request.uri().path().to_string();
    let method = request.method().clone();

    // Process the request
    let response = next.run(request).await;

    // Calculate request duration
    let duration = start.elapsed();
    let status = response.status();

    // Log the request with appropriate level based on status code
    let status_code = status.as_u16();
    let duration_ms = duration.as_secs_f64() * 1000.0;

    match status_code {
        500..=599 => error!(
            "{} {} -> {} ({:.2}ms)",
            method, path, status_code, duration_ms
        ),
        400..=499 => warn!(
            "{} {} -> {} ({:.2}ms)",
            method, path, status_code, duration_ms
        ),
        _ => info!(
            "{} {} -> {} ({:.2}ms)",
            method, path, status_code, duration_ms
        ),
    }

    response
}
