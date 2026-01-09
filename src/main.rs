// Enforce memory safety: no unsafe code allowed in this crate
// (dependencies may still use unsafe code, but our code cannot)
#![forbid(unsafe_code)]

mod admin;
mod auth;
mod config;
mod error;
mod server;

use anyhow::Result;
use clap::{Parser, Subcommand};
use log::{error, info};
use std::path::PathBuf;

use crate::admin::run_admin_server;
use crate::auth::generate_url;
use crate::config::{init_config, load_config, update_admin_password};
use crate::server::run_server;

#[derive(Parser)]
#[command(name = "ryansend")]
#[command(about = "A file sharing tool to generate and host authenticated links to download files")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(long, default_value = "http://localhost:3000")]
        base_url: String,
        #[arg(long, default_value = "3000")]
        port: u16,
    },
    Start,
    Share {
        path: PathBuf,
        #[arg(long, default_value = "3600")]
        expires_in: u64, // seconds, default 1 hour
    },
    SetPassword,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set default log level to info if RUST_LOG is not set
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { base_url, port } => {
            if let Some(password) = init_config(base_url, port).await? {
                let admin_enabled = std::env::var("RYANSEND_DEFAULT_ADMIN_PANEL")
                    .map(|v| v.parse().unwrap_or(false))
                    .unwrap_or(false);

                if admin_enabled {
                    info!("🔧 Admin panel enabled by default!");
                    let admin_port = std::env::var("RYANSEND_ADMIN_PORT")
                        .ok()
                        .and_then(|port_str| port_str.parse().ok())
                        .unwrap_or(3001);
                    info!("📋 Admin panel will be available at: http://localhost:{}/admin/login", admin_port);
                } else {
                    info!("🔧 Admin panel disabled by default");
                    info!(
                        "📋 To enable: set 'enabled: true' in {} admin section",
                        config::get_config_file_path()
                    );
                }
                info!("🔑 Generated admin password: {}", password);
                info!("📝 To change the password later:");
                info!("   1. Run: cargo run -- set-password");
            }
        }
        Commands::Start => {
            let config_path = config::get_config_file_path();
            if !tokio::fs::try_exists(&config_path).await.unwrap_or(false) {
                info!("Config file not found. Creating new configuration...");
                let base_url = std::env::var("RYANSEND_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:3000".to_string());
                let port = std::env::var("RYANSEND_PORT")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(3000);
                if let Some(password) = init_config(base_url, port).await? {
                    let admin_enabled = std::env::var("RYANSEND_DEFAULT_ADMIN_PANEL")
                        .map(|v| v.parse().unwrap_or(false))
                        .unwrap_or(false);

                    if admin_enabled {
                        info!("🔧 Admin panel enabled by default!");
                        let admin_port = std::env::var("RYANSEND_ADMIN_PORT")
                            .ok()
                            .and_then(|port_str| port_str.parse().ok())
                            .unwrap_or(3001);
                        info!("📋 Admin panel will be available at: http://localhost:{}/admin/login", admin_port);
                    } else {
                        info!("🔧 Admin panel disabled by default");
                        info!(
                            "📋 To enable: set 'enabled: true' in {} admin section",
                            config::get_config_file_path()
                        );
                    }
                    info!("🔑 Generated admin password: {}", password);
                    info!("📝 To change the password later:");
                    info!("   1. Run: cargo run -- set-password");
                }
            }

            let config = load_config().await?;
            info!(
                "Loaded config - base_url: {}, port: {}",
                config.base_url, config.port
            );

            // Start both main server and admin server concurrently
            let main_server = run_server(config.clone());
            let admin_server = run_admin_server(config.clone());

            tokio::try_join!(main_server, admin_server)?;
        }
        Commands::Share { path, expires_in } => {
            let config = load_config().await?;
            log::debug!(
                "Loaded config - base_url: {}, port: {}",
                config.base_url,
                config.port
            );

            match generate_url(&config, &path, expires_in).await {
                Ok(download_url) => {
                    println!("Share URL: {}", download_url);
                    println!("Token expires in {} seconds", expires_in);
                    info!(
                        "Generated share token for file: {} (expires in {}s)",
                        path.display(),
                        expires_in
                    );
                }
                Err(e) => {
                    error!("Error generating token: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::SetPassword => {
            println!("Enter new admin password:");
            match rpassword::read_password() {
                Ok(password) => {
                    if password.trim().is_empty() {
                        error!("Password cannot be empty");
                        std::process::exit(1);
                    }
                    if password.len() < 15 {
                        error!("Password must be at least 15 characters long");
                        std::process::exit(1);
                    }
                    match update_admin_password(&password).await {
                        Ok(()) => {
                            info!("Password updated successfully");
                        }
                        Err(e) => {
                            error!("Error updating password: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    error!("Error reading password: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
