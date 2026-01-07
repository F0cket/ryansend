use anyhow::{anyhow, Result};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use chrono::{DateTime, Duration, Utc};
use rusty_paseto::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config::Config;

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenClaims {
    pub path: String,
    pub exp: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminTokenClaims {
    pub user: String,
    pub exp: DateTime<Utc>,
}

pub async fn generate_token(
    config: &Config,
    file_path: &Path,
    expires_in_seconds: u64,
) -> Result<String> {
    // Verify the file exists
    if !file_path.exists() {
        return Err(anyhow!("File does not exist: {}", file_path.display()));
    }

    let now = Utc::now();
    let exp = now + Duration::seconds(expires_in_seconds as i64);

    let claims = TokenClaims {
        path: file_path.to_string_lossy().to_string(),
        exp,
    };

    // Parse PASERK key from config
    let key = PasetoSymmetricKey::<V4, Local>::try_from_paserk_str(&config.secret_key)
        .map_err(|e| anyhow!("Invalid PASERK key in config: {}", e))?;

    // Build PASETO token with claims
    let token = PasetoBuilder::<V4, Local>::default()
        .set_claim(ExpirationClaim::try_from(claims.exp.to_rfc3339())?)
        .set_claim(CustomClaim::try_from(("path", claims.path.clone()))?)
        .build(&key)?;

    Ok(token)
}

pub async fn generate_url(
    config: &Config,
    file_path: &Path,
    expires_in_seconds: u64,
) -> Result<String> {
    let token = generate_token(config, file_path, expires_in_seconds).await?;
    let download_url = format!(
        "{}/download?token={}",
        config.base_url.trim_end_matches('/'),
        token
    );
    Ok(download_url)
}

pub async fn verify_token_and_get_path(secret_key: &str, token: &str) -> Result<String> {
    // Parse PASERK key from config
    let key = PasetoSymmetricKey::<V4, Local>::try_from_paserk_str(secret_key)
        .map_err(|e| anyhow!("Invalid PASERK key in config: {}", e))?;

    // Parse and validate PASETO token
    let parsed_token = PasetoParser::<V4, Local>::default().parse(token, &key)?;

    // Extract the path from the custom claim
    let path = parsed_token["path"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing or invalid path claim"))?;

    Ok(path.to_string())
}

pub async fn verify_admin_token(config: &Config, token: &str) -> Result<bool> {
    let key = PasetoSymmetricKey::<V4, Local>::try_from_paserk_str(&config.secret_key)
        .map_err(|e| anyhow!("Invalid PASERK key: {}", e))?;

    let parsed_token = PasetoParser::<V4, Local>::default()
        .parse(token, &key)
        .map_err(|_| anyhow!("Invalid token"))?;

    let user = parsed_token["user"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing user claim"))?;

    Ok(user == "admin")
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed_hash) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok(),
        Err(_) => false,
    }
}
