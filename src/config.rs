use anyhow::{anyhow, Result};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};
use rand::prelude::*;
use rusty_paseto::prelude::*;
use serde::{Deserialize, Serialize};

use tokio::fs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub base_url: String,
    pub port: u16,
    pub secret_key: String,
    pub admin: Option<AdminConfig>,
    #[serde(default)]
    pub remove_kofi: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdminConfig {
    pub enabled: bool,
    pub port: u16,
    pub password: String,
    pub sharing_root: String,
}

pub fn get_config_file_path() -> String {
    std::env::var("RYANSEND_CONFIG_FILE").unwrap_or_else(|_| "config.yaml".to_string())
}

pub async fn load_config() -> Result<Config> {
    let config_path = get_config_file_path();
    let config_content = fs::read_to_string(&config_path)
        .await
        .map_err(|_| anyhow!("Failed to read {}. Make sure it exists", config_path))?;

    let mut config: Config = serde_yaml::from_str(&config_content)
        .map_err(|e| anyhow!("Failed to parse {}: {}", config_path, e))?;

    // Set default value for remove_kofi if not present
    // The #[serde(default)] attribute handles this automatically, but this comment clarifies intent

    // Override base_url with environment variable if present
    if let Ok(env_base_url) = std::env::var("RYANSEND_BASE_URL") {
        config.base_url = env_base_url;
    }

    // Override port with environment variable if present
    if let Ok(env_port) = std::env::var("RYANSEND_PORT") {
        config.port = env_port.parse().unwrap_or(config.port);
    }

    // Override admin enabled with environment variable if present
    if let Ok(env_admin) = std::env::var("RYANSEND_ADMIN_ENABLED") {
        if let Some(ref mut admin) = config.admin {
            admin.enabled = env_admin.parse().unwrap_or(admin.enabled);
        }
    }

    // Override admin sharing root with environment variable if present
    if let Ok(env_sharing_root) = std::env::var("RYANSEND_ADMIN_SHARING_ROOT") {
        if let Some(ref mut admin) = config.admin {
            admin.sharing_root = env_sharing_root;
        }
    }

    Ok(config)
}

fn generate_argon2_hash(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow!("Failed to hash password: {}", e))?;
    Ok(password_hash.to_string())
}

fn generate_random_password() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    const PASSWORD_LEN: usize = 20;
    let mut rng = rand::rng();

    (0..PASSWORD_LEN)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

pub async fn init_config(base_url: String, port: u16) -> Result<Option<String>> {
    let mut key_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut key_bytes);

    // Create PASETO key and convert to PASERK
    let key = PasetoSymmetricKey::<V4, Local>::from(Key::from(key_bytes));
    let paserk_string = key.to_paserk_string();

    // Check if admin panel should be enabled by default
    let default_admin_enabled = std::env::var("RYANSEND_DEFAULT_ADMIN_PANEL")
        .map(|v| v.parse().unwrap_or(false))
        .unwrap_or(false);

    // Always generate a random admin password
    let random_password = generate_random_password();
    let password_hash =
        generate_argon2_hash(&random_password).unwrap_or_else(|_| "admin".to_string());

    let admin_config = AdminConfig {
        enabled: default_admin_enabled,
        port: 3001,
        password: password_hash,
        sharing_root: ".".to_string(),
    };

    let config = Config {
        base_url: base_url.clone(),
        port,
        secret_key: paserk_string.clone(),
        admin: Some(admin_config),
        remove_kofi: false,
    };

    let config_path = get_config_file_path();
    if tokio::fs::try_exists(&config_path).await.unwrap_or(false) {
        return Err(anyhow!(
            "{} already exists. Remove it first or use a different directory.",
            config_path
        ));
    }

    let config_content =
        serde_yaml::to_string(&config).map_err(|e| anyhow!("Failed to serialize config: {}", e))?;

    fs::write(&config_path, config_content)
        .await
        .map_err(|e| anyhow!("Failed to write {}: {}", config_path, e))?;

    log::info!("✅ Created {} with new PASETO key", config_path);
    log::info!("Base URL: {}", base_url);
    log::debug!("PASERK: {}", paserk_string);

    Ok(Some(random_password))
}

pub async fn update_admin_password(new_password: &str) -> Result<()> {
    // Validate password length
    if new_password.len() < 15 {
        return Err(anyhow!("Password must be at least 15 characters long"));
    }

    // Load the existing config
    let mut config = load_config().await?;

    // Generate new password hash
    let password_hash = generate_argon2_hash(new_password)?;

    // Update the admin password
    match config.admin {
        Some(ref mut admin) => {
            admin.password = password_hash;
        }
        None => {
            return Err(anyhow!(
                "Admin configuration not found in {}",
                get_config_file_path()
            ));
        }
    }

    // Write the updated config back to file
    let config_content =
        serde_yaml::to_string(&config).map_err(|e| anyhow!("Failed to serialize config: {}", e))?;

    let config_path = get_config_file_path();
    fs::write(&config_path, config_content)
        .await
        .map_err(|e| anyhow!("Failed to write updated {}: {}", config_path, e))?;

    log::info!("✅ Admin password updated successfully");

    Ok(())
}
