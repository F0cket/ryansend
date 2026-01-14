use anyhow::{anyhow, Result};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use chrono::{DateTime, Duration, Utc};
use log::info;
use rand::{rng, Rng};
use rusty_paseto::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config::Config;

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenClaims {
    pub path: String,
    pub exp: DateTime<Utc>,
    pub id: String,
    pub max_uses: Option<u32>,
    pub note: Option<String>,
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
    generate_token_with_options(config, file_path, expires_in_seconds, None, None).await
}

pub async fn generate_token_with_options(
    config: &Config,
    file_path: &Path,
    expires_in_seconds: u64,
    max_uses: Option<u32>,
    note: Option<String>,
) -> Result<String> {
    // Verify the file exists
    if !file_path.exists() {
        return Err(anyhow!("File does not exist: {}", file_path.display()));
    }

    let now = Utc::now();
    let exp = now + Duration::seconds(expires_in_seconds as i64);

    // Generate unique 8-byte (16 character hex) ID
    let mut id_bytes = [0u8; 8];
    rng().fill(&mut id_bytes);
    let id = hex::encode(id_bytes);

    let claims = TokenClaims {
        path: file_path.to_string_lossy().to_string(),
        exp,
        id,
        max_uses,
        note,
    };

    // Parse PASERK key from config
    let key = PasetoSymmetricKey::<V4, Local>::try_from_paserk_str(&config.secret_key)
        .map_err(|e| anyhow!("Invalid PASERK key in config: {}", e))?;

    // Build PASETO token with claims
    let token = if let (Some(max_uses), Some(ref note)) = (max_uses, &claims.note) {
        PasetoBuilder::<V4, Local>::default()
            .set_claim(ExpirationClaim::try_from(claims.exp.to_rfc3339())?)
            .set_claim(CustomClaim::try_from(("path", claims.path.clone()))?)
            .set_claim(CustomClaim::try_from(("id", claims.id.clone()))?)
            .set_claim(CustomClaim::try_from(("max_uses", max_uses.to_string()))?)
            .set_claim(CustomClaim::try_from(("note", note.clone()))?)
            .build(&key)?
    } else if let Some(max_uses) = max_uses {
        PasetoBuilder::<V4, Local>::default()
            .set_claim(ExpirationClaim::try_from(claims.exp.to_rfc3339())?)
            .set_claim(CustomClaim::try_from(("path", claims.path.clone()))?)
            .set_claim(CustomClaim::try_from(("id", claims.id.clone()))?)
            .set_claim(CustomClaim::try_from(("max_uses", max_uses.to_string()))?)
            .build(&key)?
    } else if let Some(ref note) = claims.note {
        PasetoBuilder::<V4, Local>::default()
            .set_claim(ExpirationClaim::try_from(claims.exp.to_rfc3339())?)
            .set_claim(CustomClaim::try_from(("path", claims.path.clone()))?)
            .set_claim(CustomClaim::try_from(("id", claims.id.clone()))?)
            .set_claim(CustomClaim::try_from(("note", note.clone()))?)
            .build(&key)?
    } else {
        PasetoBuilder::<V4, Local>::default()
            .set_claim(ExpirationClaim::try_from(claims.exp.to_rfc3339())?)
            .set_claim(CustomClaim::try_from(("path", claims.path.clone()))?)
            .set_claim(CustomClaim::try_from(("id", claims.id.clone()))?)
            .build(&key)?
    };

    Ok(token)
}

pub async fn generate_url(
    config: &Config,
    file_path: &Path,
    expires_in_seconds: u64,
) -> Result<String> {
    let token = generate_token(config, file_path, expires_in_seconds).await?;

    // Extract token ID for logging
    let claims = verify_token_and_get_claims(&config.secret_key, &token).await?;
    let note_info = match &claims.note {
        Some(note) => format!(" (note: \"{}\")", note),
        None => String::new(),
    };
    info!(
        "Download URL generated for path: {} [token_id: {}]{}",
        file_path.display(),
        claims.id,
        note_info
    );

    let download_url = format!(
        "{}/download?token={}",
        config.base_url.trim_end_matches('/'),
        token
    );
    Ok(download_url)
}

pub async fn generate_url_with_options(
    config: &Config,
    file_path: &Path,
    expires_in_seconds: u64,
    max_uses: Option<u32>,
    note: Option<String>,
) -> Result<String> {
    let token =
        generate_token_with_options(config, file_path, expires_in_seconds, max_uses, note).await?;

    // Extract token ID for logging
    let claims = verify_token_and_get_claims(&config.secret_key, &token).await?;
    let usage_info = match max_uses {
        Some(uses) => format!(" (max_uses: {})", uses),
        None => String::new(),
    };
    let note_info = match &claims.note {
        Some(note) => format!(" (note: \"{}\")", note),
        None => String::new(),
    };
    info!(
        "Download URL generated for path: {} [token_id: {}]{}{}",
        file_path.display(),
        claims.id,
        usage_info,
        note_info
    );

    let download_url = format!(
        "{}/download?token={}",
        config.base_url.trim_end_matches('/'),
        token
    );
    Ok(download_url)
}

pub async fn verify_token_and_get_claims(secret_key: &str, token: &str) -> Result<TokenClaims> {
    // Parse PASERK key from config
    let key = PasetoSymmetricKey::<V4, Local>::try_from_paserk_str(secret_key)
        .map_err(|e| anyhow!("Invalid PASERK key in config: {}", e))?;

    // Parse and validate PASETO token
    let parsed_token = PasetoParser::<V4, Local>::default().parse(token, &key)?;

    // Extract claims
    let path = parsed_token["path"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing or invalid path claim"))?;

    let id = parsed_token["id"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing or invalid id claim"))?;

    let exp_str = parsed_token["exp"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing or invalid exp claim"))?;

    let exp = DateTime::parse_from_rfc3339(exp_str)
        .map_err(|e| anyhow!("Invalid expiration format: {}", e))?
        .with_timezone(&Utc);

    let max_uses = parsed_token["max_uses"]
        .as_str()
        .and_then(|s| s.parse::<u32>().ok());

    let note = parsed_token["note"].as_str().map(|s| s.to_string());

    Ok(TokenClaims {
        path: path.to_string(),
        exp,
        id: id.to_string(),
        max_uses,
        note,
    })
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
