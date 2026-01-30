use askama::Template;
use axum::{
    body::Body,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use log::error;

// Custom error type that wraps anyhow::Error and implements IntoResponse
pub struct AppError {
    inner: anyhow::Error,
    status: StatusCode,
}

impl AppError {
    pub fn new(status: StatusCode, error: anyhow::Error) -> Self {
        Self {
            inner: error,
            status,
        }
    }

    pub fn not_found(error: anyhow::Error) -> Self {
        Self::new(StatusCode::NOT_FOUND, error)
    }

    pub fn unauthorized(error: anyhow::Error) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, error)
    }

    pub fn internal_server_error(error: anyhow::Error) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!("Application error ({}): {:?}", self.status, self.inner);

        // For security, we don't expose internal error details to clients
        // Instead, we return appropriate error pages based on status
        make_error_response(self.status)
    }
}

// This enables using `?` on functions that return `Result<T, anyhow::Error>`
// Default to 500 Internal Server Error for generic errors
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self::internal_server_error(err.into())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Template)]
#[template(path = "error_404.html")]
pub struct Error404Template;

#[derive(Template)]
#[template(path = "error_401.html")]
pub struct Error401Template;

#[derive(Template)]
#[template(path = "error_401_general.html")]
pub struct Error401GeneralTemplate;

#[derive(Template)]
#[template(path = "error_500.html")]
pub struct Error500Template;

pub async fn handle_404() -> impl IntoResponse {
    let template = Error404Template;
    let html = template
        .render()
        .unwrap_or_else(|_| "404 Not Found".to_string());
    (StatusCode::NOT_FOUND, Html(html))
}

pub fn make_error_response(status: StatusCode) -> Response {
    match status {
        StatusCode::NOT_FOUND => {
            let template = Error404Template;
            let html = template
                .render()
                .unwrap_or_else(|_| "404 Not Found".to_string());
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("content-type", "text/html")
                .body(Body::from(html))
                .unwrap_or_else(|_| {
                    error!("Failed to build 404 error response");
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Internal Server Error"))
                        .unwrap_or_default()
                })
        }
        StatusCode::UNAUTHORIZED => {
            let template = Error401GeneralTemplate;
            let html = template
                .render()
                .unwrap_or_else(|_| "401 Unauthorized".to_string());
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "text/html")
                .body(Body::from(html))
                .unwrap_or_else(|_| {
                    error!("Failed to build 401 unauthorized error response");
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Internal Server Error"))
                        .unwrap_or_default()
                })
        }
        StatusCode::FORBIDDEN => {
            let template = Error401GeneralTemplate;
            let html = template
                .render()
                .unwrap_or_else(|_| "403 Forbidden".to_string());
            Response::builder()
                .status(StatusCode::FORBIDDEN)
                .header("content-type", "text/html")
                .body(Body::from(html))
                .unwrap_or_else(|_| {
                    error!("Failed to build 403 forbidden error response");
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Internal Server Error"))
                        .unwrap_or_default()
                })
        }
        StatusCode::INTERNAL_SERVER_ERROR => {
            let template = Error500Template;
            let html = template
                .render()
                .unwrap_or_else(|_| "500 Internal Server Error".to_string());
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("content-type", "text/html")
                .body(Body::from(html))
                .unwrap_or_else(|_| {
                    error!("Failed to build 500 internal server error response");
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Internal Server Error"))
                        .unwrap_or_default()
                })
        }
        _ => Response::builder()
            .status(status)
            .body(Body::empty())
            .unwrap_or_else(|_| {
                error!("Failed to build fallback error response");
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("Internal Server Error"))
                    .unwrap_or_default()
            }),
    }
}

pub fn make_admin_error_response(status: StatusCode) -> Response {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            let template = Error401Template; // Admin 401 with login prompt
            let html = template
                .render()
                .unwrap_or_else(|_| "401 Unauthorized".to_string());
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "text/html")
                .body(Body::from(html))
                .unwrap_or_else(|_| {
                    error!("Failed to build admin unauthorized error response");
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Internal Server Error"))
                        .unwrap_or_default()
                })
        }
        _ => make_error_response(status), // Use general error for other statuses
    }
}
