use askama::Template;
use axum::{
    body::Body,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

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
                .expect("Failed to build 404 error response")
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
                .expect("Failed to build 401 unauthorized error response")
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
                .expect("Failed to build 403 forbidden error response")
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
                .expect("Failed to build 500 internal server error response")
        }
        _ => Response::builder()
            .status(status)
            .body(Body::empty())
            .expect("Failed to build fallback error response"),
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
                .expect("Failed to build admin unauthorized error response")
        }
        _ => make_error_response(status), // Use general error for other statuses
    }
}
