use anyhow::{anyhow, Result};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};
use rand::prelude::*;
use rusty_paseto::prelude::*;
use serde::{Deserialize, Serialize};
use url::Url;

use tokio::fs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub base_url: String,
    pub port: u16,
    pub secret_key: String,
    pub admin: Option<AdminConfig>,
    #[serde(default)]
    pub remove_kofi: bool,
    #[serde(default)]
    pub tls_port: Option<u16>,
    #[serde(default)]
    pub cert_path: Option<String>,
    #[serde(default)]
    pub cert_key_path: Option<String>,
    #[serde(default)]
    pub use_letsencrypt_cert: bool,
    #[serde(default)]
    pub lets_encrypt: Option<LetsEncryptConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LetsEncryptConfig {
    pub cert: Option<String>,
    pub key: Option<String>,
    pub expiry: Option<String>,
    pub acme_email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdminConfig {
    pub enabled: bool,
    pub port: u16,
    pub password: String,
    pub sharing_root: String,
    #[serde(default)]
    pub tls_port: Option<u16>,
}

impl Config {
    /// Check if TLS certificate is configured
    pub fn has_tls_cert(&self) -> bool {
        if self.use_letsencrypt_cert {
            // Check if Let's Encrypt cert is available
            self.lets_encrypt
                .as_ref()
                .map(|le| le.cert.is_some() && le.key.is_some())
                .unwrap_or(false)
        } else {
            // Check if file paths are configured
            self.cert_path.is_some() && self.cert_key_path.is_some()
        }
    }

    /// Check if certificate renewal is needed (within 30 days of expiry)
    /// Only applies to Let's Encrypt certificates
    pub fn is_cert_renewal_needed(&self) -> bool {
        if !self.use_letsencrypt_cert {
            return false; // File-based certs don't auto-renew
        }

        if let Some(le_config) = &self.lets_encrypt {
            if let Some(expiry_str) = &le_config.expiry {
                if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(expiry_str) {
                    let now = chrono::Utc::now();
                    let days_until_expiry = (expiry.with_timezone(&chrono::Utc) - now).num_days();
                    return days_until_expiry < 30;
                }
            }
        }
        false
    }

    /// Extract domain from base_url for certificate generation
    pub fn get_domain(&self) -> Result<String> {
        let url = Url::parse(&self.base_url)
            .map_err(|e| anyhow!("Invalid base_url '{}': {}", self.base_url, e))?;

        let host = url
            .host_str()
            .ok_or_else(|| anyhow!("No host found in base_url: {}", self.base_url))?;

        // Remove port if present (e.g., "example.com:3000" -> "example.com")
        let domain = host.split(':').next().unwrap_or(host);

        // Don't allow localhost or IP addresses for Let's Encrypt
        if domain == "localhost" || domain.parse::<std::net::IpAddr>().is_ok() {
            return Err(anyhow!(
                "Domain '{}' is not suitable for Let's Encrypt certificates. Use a proper domain name.",
                domain
            ));
        }

        Ok(domain.to_string())
    }
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

    // Override admin port with environment variable if present
    if let Ok(env_admin_port) = std::env::var("RYANSEND_ADMIN_PORT") {
        if let Some(ref mut admin) = config.admin {
            admin.port = env_admin_port.parse().unwrap_or(admin.port);
        }
    }

    // Override TLS port with environment variable if present
    if let Ok(env_tls_port) = std::env::var("RYANSEND_TLS_PORT") {
        config.tls_port = env_tls_port.parse().ok();
    }

    // Override cert path with environment variable if present
    if let Ok(env_cert_path) = std::env::var("RYANSEND_CERT_PATH") {
        config.cert_path = Some(env_cert_path);
    }

    // Override cert key path with environment variable if present
    if let Ok(env_cert_key_path) = std::env::var("RYANSEND_CERT_KEY_PATH") {
        config.cert_key_path = Some(env_cert_key_path);
    }

    // Override use_letsencrypt_cert with environment variable if present
    if let Ok(env_use_le) = std::env::var("RYANSEND_USE_LETSENCRYPT_CERT") {
        config.use_letsencrypt_cert = env_use_le.parse().unwrap_or(config.use_letsencrypt_cert);
    }

    // Override admin TLS port with environment variable if present
    if let Ok(env_admin_tls_port) = std::env::var("RYANSEND_ADMIN_TLS_PORT") {
        if let Some(ref mut admin) = config.admin {
            admin.tls_port = Some(
                env_admin_tls_port
                    .parse()
                    .unwrap_or(admin.tls_port.unwrap_or(3444)),
            );
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

    // Use environment variable for admin port, otherwise default to 3001
    let admin_port = std::env::var("RYANSEND_ADMIN_PORT")
        .ok()
        .and_then(|port_str| port_str.parse().ok())
        .unwrap_or(3001);

    // Use environment variable for admin TLS port if provided
    let admin_tls_port = std::env::var("RYANSEND_ADMIN_TLS_PORT")
        .ok()
        .and_then(|port_str| port_str.parse().ok());

    let admin_config = AdminConfig {
        enabled: default_admin_enabled,
        port: admin_port,
        password: password_hash,
        sharing_root: ".".to_string(),
        tls_port: admin_tls_port,
    };

    // Use environment variable for TLS port if provided
    let tls_port = std::env::var("RYANSEND_TLS_PORT")
        .ok()
        .and_then(|port_str| port_str.parse().ok());

    let config = Config {
        base_url: base_url.clone(),
        port,
        secret_key: paserk_string.clone(),
        admin: Some(admin_config),
        remove_kofi: false,
        tls_port,
        cert_path: Some("cert.pem".to_string()),
        cert_key_path: Some("key.pem".to_string()),
        use_letsencrypt_cert: false,
        lets_encrypt: None,
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

/// Save the current config to the config file
pub async fn save_config(config: &Config) -> Result<()> {
    let config_content =
        serde_yaml::to_string(config).map_err(|e| anyhow!("Failed to serialize config: {}", e))?;

    let config_path = get_config_file_path();
    fs::write(&config_path, config_content)
        .await
        .map_err(|e| anyhow!("Failed to write {}: {}", config_path, e))?;

    log::info!("✅ Configuration saved to {}", config_path);
    Ok(())
}

/// Update config with environment variables preserved
/// This ensures that if env vars are set during config generation, they get written to the file
#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[tokio::test]
    async fn test_admin_port_env_var() {
        // Set up a test config content
        let test_config = r#"
base_url: "http://localhost:3000"
port: 3000
secret_key: "test-key"
admin:
  enabled: true
  port: 3001
  password: "test-password"
  sharing_root: "."
remove_kofi: false
"#;

        // Parse the config
        let mut config: Config =
            serde_yaml::from_str(test_config).expect("Failed to parse test config YAML");

        // Set the environment variable
        env::set_var("RYANSEND_ADMIN_PORT", "4001");

        // Simulate the environment variable override logic from load_config
        if let Ok(env_admin_port) = env::var("RYANSEND_ADMIN_PORT") {
            if let Some(ref mut admin) = config.admin {
                admin.port = env_admin_port.parse().unwrap_or(admin.port);
            }
        }

        // Verify the port was overridden
        assert_eq!(
            config
                .admin
                .as_ref()
                .expect("Admin config should be present")
                .port,
            4001
        );

        // Clean up
        env::remove_var("RYANSEND_ADMIN_PORT");
    }

    #[test]
    fn test_init_config_with_admin_port_env() {
        // Set the environment variable
        env::set_var("RYANSEND_ADMIN_PORT", "5001");

        // Test the admin port logic from init_config
        let admin_port = env::var("RYANSEND_ADMIN_PORT")
            .ok()
            .and_then(|port_str| port_str.parse().ok())
            .unwrap_or(3001);

        assert_eq!(admin_port, 5001);

        // Clean up
        env::remove_var("RYANSEND_ADMIN_PORT");

        // Test default fallback
        let default_port = env::var("RYANSEND_ADMIN_PORT")
            .ok()
            .and_then(|port_str| port_str.parse().ok())
            .unwrap_or(3001);

        assert_eq!(default_port, 3001);
    }

    #[tokio::test]
    async fn test_tls_port_env_vars() {
        // Set up a test config content
        let test_config = r#"
base_url: "http://localhost:3000"
port: 3000
secret_key: "test-key"
admin:
  enabled: true
  port: 3001
  password: "test-password"
  sharing_root: "."
remove_kofi: false
"#;

        // Parse the config
        let mut config: Config =
            serde_yaml::from_str(test_config).expect("Failed to parse test config YAML");

        // Set the TLS environment variables
        env::set_var("RYANSEND_TLS_PORT", "8443");
        env::set_var("RYANSEND_ADMIN_TLS_PORT", "8444");

        // Simulate the environment variable override logic from load_config
        if let Ok(env_tls_port) = env::var("RYANSEND_TLS_PORT") {
            config.tls_port = env_tls_port.parse().ok();
        }

        if let Ok(env_admin_tls_port) = env::var("RYANSEND_ADMIN_TLS_PORT") {
            if let Some(ref mut admin) = config.admin {
                admin.tls_port = Some(
                    env_admin_tls_port
                        .parse()
                        .unwrap_or(admin.tls_port.unwrap_or(3444)),
                );
            }
        }

        // Verify the ports were set correctly
        assert_eq!(config.tls_port, Some(8443));
        assert_eq!(
            config
                .admin
                .as_ref()
                .expect("Admin config should be present")
                .tls_port,
            Some(8444)
        );

        // Clean up
        env::remove_var("RYANSEND_TLS_PORT");
        env::remove_var("RYANSEND_ADMIN_TLS_PORT");
    }

    #[test]
    fn test_get_domain() {
        let config = Config {
            base_url: "https://example.com".to_string(),
            port: 3000,
            secret_key: "test-key".to_string(),
            admin: None,
            remove_kofi: false,
            tls_port: None,
            cert_path: None,
            cert_key_path: None,
            use_letsencrypt_cert: false,
            lets_encrypt: None,
        };

        assert_eq!(config.get_domain().unwrap(), "example.com");

        let config_with_port = Config {
            base_url: "https://example.com:8080".to_string(),
            port: 3000,
            secret_key: "test-key".to_string(),
            admin: None,
            remove_kofi: false,
            tls_port: None,
            cert_path: None,
            cert_key_path: None,
            use_letsencrypt_cert: false,
            lets_encrypt: None,
        };

        assert_eq!(config_with_port.get_domain().unwrap(), "example.com");

        let config_localhost = Config {
            base_url: "http://localhost:3000".to_string(),
            port: 3000,
            secret_key: "test-key".to_string(),
            admin: None,
            remove_kofi: false,
            tls_port: None,
            cert_path: None,
            cert_key_path: None,
            use_letsencrypt_cert: false,
            lets_encrypt: None,
        };

        assert!(config_localhost.get_domain().is_err());
    }

    #[test]
    fn test_has_tls_cert() {
        let config_no_cert = Config {
            base_url: "https://example.com".to_string(),
            port: 3000,
            secret_key: "test-key".to_string(),
            admin: None,
            remove_kofi: false,
            tls_port: None,
            cert_path: None,
            cert_key_path: None,
            use_letsencrypt_cert: false,
            lets_encrypt: None,
        };

        assert!(!config_no_cert.has_tls_cert());

        let config_with_file_cert = Config {
            base_url: "https://example.com".to_string(),
            port: 3000,
            secret_key: "test-key".to_string(),
            admin: None,
            remove_kofi: false,
            tls_port: Some(443),
            cert_path: None,
            cert_key_path: None,
            use_letsencrypt_cert: true,
            lets_encrypt: Some(LetsEncryptConfig {
                cert: Some("cert-data".to_string()),
                key: Some("key-data".to_string()),
                expiry: None,
                acme_email: None,
            }),
        };

        assert!(config_with_file_cert.has_tls_cert());

        let config_with_file_cert = Config {
            base_url: "https://example.com".to_string(),
            port: 3000,
            secret_key: "test-key".to_string(),
            admin: None,
            remove_kofi: false,
            tls_port: Some(443),
            cert_path: Some("cert.pem".to_string()),
            cert_key_path: Some("key.pem".to_string()),
            use_letsencrypt_cert: false,
            lets_encrypt: None,
        };

        assert!(config_with_file_cert.has_tls_cert());
    }

    #[test]
    fn test_cert_renewal_needed() {
        let now = chrono::Utc::now();
        let future_expiry = now + chrono::Duration::days(31); // More than 30 days, no renewal needed
        let near_expiry = now + chrono::Duration::days(10); // Less than 30 days, renewal needed

        let config_future = Config {
            base_url: "https://example.com".to_string(),
            port: 3000,
            secret_key: "test-key".to_string(),
            admin: None,
            remove_kofi: false,
            tls_port: Some(443),
            cert_path: None,
            cert_key_path: None,
            use_letsencrypt_cert: true,
            lets_encrypt: Some(LetsEncryptConfig {
                cert: Some("cert-data".to_string()),
                key: Some("key-data".to_string()),
                expiry: Some(future_expiry.to_rfc3339()),
                acme_email: None,
            }),
        };

        assert!(!config_future.is_cert_renewal_needed());

        let config_near = Config {
            base_url: "https://example.com".to_string(),
            port: 3000,
            secret_key: "test-key".to_string(),
            admin: None,
            remove_kofi: false,
            tls_port: Some(443),
            cert_path: None,
            cert_key_path: None,
            use_letsencrypt_cert: true,
            lets_encrypt: Some(LetsEncryptConfig {
                cert: Some("cert-data".to_string()),
                key: Some("key-data".to_string()),
                expiry: Some(near_expiry.to_rfc3339()),
                acme_email: None,
            }),
        };

        assert!(config_near.is_cert_renewal_needed());
    }
}
