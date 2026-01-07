use anyhow::Result;
use askama::Template;
use axum::{
    body::Body,
    extract::{Form, Query, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use axum_extra::extract::{cookie::Cookie, CookieJar};
use chrono::{Duration, Utc};
use log::info;
use rusty_paseto::prelude::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::auth::{generate_token, generate_url, verify_admin_token, AdminTokenClaims};
use crate::config::Config;
use crate::error::{handle_404, make_admin_error_response};
use crate::server::AppState;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "files.html")]
struct FileBrowserTemplate {
    current_path: String,
    entries: Vec<FileEntry>,
    show_parent: bool,
    parent_path: String,
}

#[derive(Clone)]
struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
    size_display: String,
}

#[derive(Deserialize)]
struct LoginForm {
    password: String,
}

#[derive(Deserialize)]
struct FilesQuery {
    path: Option<String>,
}

#[derive(Deserialize)]
struct AdminDownloadQuery {
    path: String,
}

async fn admin_auth_middleware(
    State(state): State<AppState>,
    cookies: CookieJar,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Check if admin panel is enabled
    match &state.config.admin {
        Some(config) if config.enabled => config,
        _ => return make_admin_error_response(StatusCode::NOT_FOUND),
    };

    // Check for admin authentication cookie
    if let Some(admin_cookie) = cookies.get("admin_token") {
        if verify_admin_token(&state.config, admin_cookie.value())
            .await
            .unwrap_or(false)
        {
            // Authentication successful, continue to the handler
            return next.run(request).await;
        }
    }

    // Authentication failed, redirect to login
    make_admin_error_response(StatusCode::UNAUTHORIZED)
}

async fn admin_root_handler(State(state): State<AppState>, cookies: CookieJar) -> Response {
    // Check if admin panel is enabled
    match &state.config.admin {
        Some(config) if config.enabled => config,
        _ => return make_admin_error_response(StatusCode::NOT_FOUND),
    };

    // Check for admin authentication cookie
    if let Some(admin_cookie) = cookies.get("admin_token") {
        if verify_admin_token(&state.config, admin_cookie.value())
            .await
            .unwrap_or(false)
        {
            // Valid token - redirect to files page
            return Response::builder()
                .status(StatusCode::SEE_OTHER)
                .header("location", "/admin/files")
                .body(Body::empty())
                .expect("Failed to build redirect response to admin files");
        }
    }

    // No valid token - redirect to login page
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header("location", "/admin/login")
        .body(Body::empty())
        .expect("Failed to build redirect response to admin login")
}

async fn admin_login_page() -> Html<String> {
    let template = LoginTemplate { error: None };
    Html(
        template
            .render()
            .unwrap_or_else(|_| "Template error".to_string()),
    )
}

async fn admin_login_handler(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> Response {
    let admin_config = match &state.config.admin {
        Some(config) if config.enabled => config,
        _ => return make_admin_error_response(StatusCode::NOT_FOUND),
    };

    // Verify password using Argon2
    let is_valid = crate::auth::verify_password(&form.password, &admin_config.password);

    if !is_valid {
        let template = LoginTemplate {
            error: Some("Invalid password".to_string()),
        };
        let html = template
            .render()
            .unwrap_or_else(|_| "Template error".to_string());
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("content-type", "text/html")
            .body(Body::from(html))
            .expect("Failed to build unauthorized response");
    }

    // Generate admin token
    let now = Utc::now();
    let exp = now + Duration::hours(24);

    let claims = AdminTokenClaims {
        user: "admin".to_string(),
        exp,
    };

    let key = match PasetoSymmetricKey::<V4, Local>::try_from_paserk_str(&state.config.secret_key) {
        Ok(key) => key,
        Err(_) => return make_admin_error_response(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let token = match PasetoBuilder::<V4, Local>::default()
        .set_claim(
            ExpirationClaim::try_from(claims.exp.to_rfc3339())
                .expect("Failed to convert expiration claim to RFC3339"),
        )
        .set_claim(
            CustomClaim::try_from(("user", claims.user.clone()))
                .expect("Failed to create user custom claim"),
        )
        .build(&key)
    {
        Ok(token) => token,
        Err(_) => return make_admin_error_response(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Set cookie and redirect to file browser
    let cookie = Cookie::build(("admin_token", token))
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Strict)
        .path("/")
        .max_age(time::Duration::seconds(86400));

    let cookies = CookieJar::new().add(cookie);

    (cookies, axum::response::Redirect::to("/admin/files")).into_response()
}

fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

async fn admin_files_handler(
    State(state): State<AppState>,
    Query(query): Query<FilesQuery>,
) -> Response {
    // Get the sharing root from admin config (middleware ensures admin is enabled)
    let sharing_root = &state
        .config
        .admin
        .as_ref()
        .expect("Admin config should exist when middleware passes")
        .sharing_root;

    let requested_path = query.path.unwrap_or_else(|| ".".to_string());
    let base_path = PathBuf::from(sharing_root);

    // Construct the full path, ensuring it's within the sharing root
    let mut full_path = base_path.clone();
    if requested_path != "." {
        full_path.push(&requested_path);
    }

    // Canonicalize paths to prevent directory traversal attacks
    let canonical_base = match base_path.canonicalize() {
        Ok(path) => path,
        Err(_) => return make_admin_error_response(StatusCode::NOT_FOUND),
    };
    let canonical_full = match full_path.canonicalize() {
        Ok(path) => path,
        Err(_) => return make_admin_error_response(StatusCode::NOT_FOUND),
    };

    // Ensure the requested path is within the sharing root
    if !canonical_full.starts_with(&canonical_base) {
        return make_admin_error_response(StatusCode::FORBIDDEN);
    }

    let path = canonical_full;
    let current_path = path
        .strip_prefix(&canonical_base)
        .unwrap_or(Path::new("."))
        .to_string_lossy()
        .to_string();

    let mut entries = Vec::new();

    if path.is_dir() {
        let mut dir_entries = match fs::read_dir(&path).await {
            Ok(entries) => entries,
            Err(_) => return make_admin_error_response(StatusCode::INTERNAL_SERVER_ERROR),
        };

        while let Ok(Some(entry)) = dir_entries.next_entry().await {
            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            let is_dir = entry_path.is_dir();
            let size_display = if is_dir {
                "-".to_string()
            } else {
                match entry.metadata().await {
                    Ok(metadata) => format_file_size(metadata.len()),
                    Err(_) => "?".to_string(),
                }
            };

            // Store relative path from sharing root for the entry
            let relative_entry_path = entry_path
                .strip_prefix(&canonical_base)
                .unwrap_or(&entry_path)
                .to_string_lossy()
                .to_string();

            entries.push(FileEntry {
                name: name.clone(),
                path: relative_entry_path,
                is_dir,
                size_display,
            });
        }
    }

    // Sort entries: directories first, then files, both by name
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    let show_parent = current_path != "." && current_path != "/" && current_path != "";
    let parent_path = if show_parent {
        let parent = path.parent().unwrap_or(&canonical_base);
        parent
            .strip_prefix(&canonical_base)
            .unwrap_or(Path::new("."))
            .to_string_lossy()
            .to_string()
    } else {
        ".".to_string()
    };

    let template = FileBrowserTemplate {
        current_path,
        entries,
        show_parent,
        parent_path,
    };

    let html = template
        .render()
        .unwrap_or_else(|_| "Template error".to_string());
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html")
        .body(Body::from(html))
        .expect("Failed to build file browser response")
}

async fn admin_download_handler(
    State(state): State<AppState>,
    Query(query): Query<AdminDownloadQuery>,
) -> Response {
    // Middleware ensures admin is enabled and authenticated
    let admin_config = state
        .config
        .admin
        .as_ref()
        .expect("Admin config should exist when middleware passes");

    // Construct and validate file path within sharing root
    let base_path = PathBuf::from(&admin_config.sharing_root);
    let mut full_path = base_path.clone();
    if query.path != "." {
        full_path.push(&query.path);
    }

    // Canonicalize paths to prevent directory traversal attacks
    let canonical_base = match base_path.canonicalize() {
        Ok(path) => path,
        Err(_) => return make_admin_error_response(StatusCode::NOT_FOUND),
    };
    let canonical_full = match full_path.canonicalize() {
        Ok(path) => path,
        Err(_) => return make_admin_error_response(StatusCode::NOT_FOUND),
    };

    // Ensure the requested path is within the sharing root
    if !canonical_full.starts_with(&canonical_base) {
        return make_admin_error_response(StatusCode::FORBIDDEN);
    }

    let file_path = canonical_full;

    if !file_path.exists() || file_path.is_dir() {
        return make_admin_error_response(StatusCode::NOT_FOUND);
    }

    // Generate a temporary download token (1 hour)
    let download_token = match generate_token(&state.config, &file_path, 3600).await {
        Ok(token) => token,
        Err(_) => return make_admin_error_response(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let download_url = format!(
        "{}/download?token={}",
        state.config.base_url.trim_end_matches('/'),
        download_token
    );

    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header("location", download_url)
        .body(Body::empty())
        .expect("Failed to build download redirect response")
}

async fn admin_share_handler(
    State(state): State<AppState>,
    Query(query): Query<AdminDownloadQuery>,
) -> Response {
    // Middleware ensures admin is enabled and authenticated
    let admin_config = state
        .config
        .admin
        .as_ref()
        .expect("Admin config should exist when middleware passes");

    // Construct and validate file path within sharing root
    let base_path = PathBuf::from(&admin_config.sharing_root);
    let mut full_path = base_path.clone();
    if query.path != "." {
        full_path.push(&query.path);
    }

    // Canonicalize paths to prevent directory traversal attacks
    let canonical_base = match base_path.canonicalize() {
        Ok(path) => path,
        Err(_) => return make_admin_error_response(StatusCode::NOT_FOUND),
    };
    let canonical_full = match full_path.canonicalize() {
        Ok(path) => path,
        Err(_) => return make_admin_error_response(StatusCode::NOT_FOUND),
    };

    // Ensure the requested path is within the sharing root
    if !canonical_full.starts_with(&canonical_base) {
        return make_admin_error_response(StatusCode::FORBIDDEN);
    }

    let file_path = canonical_full;

    if !file_path.exists() || file_path.is_dir() {
        return make_admin_error_response(StatusCode::NOT_FOUND);
    }

    // Generate share URL (24 hours)
    let share_url = match generate_url(&state.config, &file_path, 86400).await {
        Ok(url) => url,
        Err(_) => return make_admin_error_response(StatusCode::INTERNAL_SERVER_ERROR),
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain")
        .body(Body::from(share_url))
        .expect("Failed to build share URL response")
}

async fn admin_logout_handler() -> Response {
    let cookie = Cookie::build(("admin_token", ""))
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Strict)
        .path("/")
        .max_age(time::Duration::seconds(0));

    let cookies = CookieJar::new().add(cookie);

    (cookies, axum::response::Redirect::to("/admin/login")).into_response()
}

pub async fn run_admin_server(config: Config) -> Result<()> {
    let admin_config = match &config.admin {
        Some(admin) if admin.enabled => admin,
        _ => return Ok(()), // Admin disabled, do nothing
    };

    let state = AppState {
        config: config.clone(),
    };

    let protected_routes = Router::new()
        .route("/admin/files", get(admin_files_handler))
        .route("/admin/download", get(admin_download_handler))
        .route("/admin/share", get(admin_share_handler))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ));

    let app = Router::new()
        .route("/", get(admin_root_handler))
        .route("/admin/login", get(admin_login_page))
        .route("/admin/login", post(admin_login_handler))
        .route("/admin/logout", get(admin_logout_handler))
        .merge(protected_routes)
        .fallback(handle_404)
        .with_state(state);

    info!(
        "Starting admin server on http://0.0.0.0:{}",
        admin_config.port
    );

    let bind_address = format!("0.0.0.0:{}", admin_config.port);
    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to bind admin server to port {}: {}",
                admin_config.port,
                e
            )
        })?;

    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("Admin server error: {}", e))?;

    Ok(())
}
