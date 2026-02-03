use anyhow::{anyhow, Result};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Information about a file being shared via tunnel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelFileInfo {
    pub id: String,
    pub name: String,
    pub size: u64,
}

/// Response from server after announcing a tunnel file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelAnnounceResponse {
    pub download_url: String,
    pub expires_in: u64,
}

/// Response to poll request
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PollResponse {
    /// No action needed, poll again
    Wait,
    /// Start uploading the file
    Upload { file_id: String },
}

/// Represents an active tunnel connection with a client
pub struct TunnelConnection {
    pub file_info: TunnelFileInfo,
    /// Notifier for when upload is requested
    pub upload_notify: Arc<tokio::sync::Notify>,
}

impl TunnelConnection {
    /// Notify client to start uploading
    pub fn notify_upload_requested(&self) {
        self.upload_notify.notify_waiters();
    }
}

/// Manages all active tunnel connections
pub struct TunnelManager {
    connections: Arc<RwLock<HashMap<String, Arc<TunnelConnection>>>>,
    /// Stores byte senders for active uploads - these pipe directly to download responses
    upload_senders: Arc<RwLock<HashMap<String, mpsc::Sender<Result<bytes::Bytes>>>>>,
}

impl TunnelManager {
    /// Create a new tunnel manager
    pub fn new() -> Self {
        info!("Tunnel manager initialized");

        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            upload_senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new tunnel connection
    pub async fn register_tunnel(&self, file_info: TunnelFileInfo) -> Result<String> {
        let file_id = file_info.id.clone();

        let tunnel_conn = Arc::new(TunnelConnection {
            file_info: file_info.clone(),
            upload_notify: Arc::new(tokio::sync::Notify::new()),
        });

        // Store the connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(file_id.clone(), tunnel_conn);
        }

        info!(
            "Registered tunnel for file: {} ({} bytes, id: {})",
            file_info.name, file_info.size, file_id
        );

        Ok(file_id)
    }

    /// Client polls for upload requests
    pub async fn poll_for_upload(&self, file_id: &str) -> Result<PollResponse> {
        let conn = {
            let connections = self.connections.read().await;
            connections.get(file_id).cloned()
        };

        match conn {
            Some(conn) => {
                // Wait for upload notification with timeout
                match tokio::time::timeout(
                    tokio::time::Duration::from_secs(30),
                    conn.upload_notify.notified(),
                )
                .await
                {
                    Ok(_) => Ok(PollResponse::Upload {
                        file_id: file_id.to_string(),
                    }),
                    Err(_) => {
                        // Timeout - return Wait to have client poll again
                        Ok(PollResponse::Wait)
                    }
                }
            }
            None => Err(anyhow!("Tunnel connection not found")),
        }
    }

    /// Get a tunnel connection by file ID
    pub async fn get_connection(&self, file_id: &str) -> Option<Arc<TunnelConnection>> {
        let connections = self.connections.read().await;
        connections.get(file_id).cloned()
    }

    /// Request file upload and get byte stream receiver
    /// This is called by the download handler to set up the streaming pipeline
    pub async fn request_file_stream(
        &self,
        file_id: &str,
    ) -> Result<mpsc::Receiver<Result<bytes::Bytes>>> {
        let conn = self
            .get_connection(file_id)
            .await
            .ok_or_else(|| anyhow!("Tunnel connection not found"))?;

        let (tx, rx) = mpsc::channel(16);

        // Store the sender for the upload handler to use
        {
            let mut senders = self.upload_senders.write().await;
            senders.insert(file_id.to_string(), tx);
        }

        // Notify client to start uploading
        conn.notify_upload_requested();

        Ok(rx)
    }

    /// Get the sender for streaming upload bytes
    /// This is called by the upload handler to send bytes as they arrive
    pub async fn get_upload_sender(
        &self,
        file_id: &str,
    ) -> Option<mpsc::Sender<Result<bytes::Bytes>>> {
        let senders = self.upload_senders.read().await;
        senders.get(file_id).cloned()
    }

    /// Signal end of upload and cleanup
    pub async fn finish_upload(&self, file_id: &str) {
        let mut senders = self.upload_senders.write().await;
        senders.remove(file_id);
        info!("Upload finished and cleaned up for file: {}", file_id);
    }

    /// Remove a tunnel connection
    #[allow(dead_code)]
    pub async fn remove_tunnel(&self, file_id: &str) {
        let mut connections = self.connections.write().await;
        connections.remove(file_id);
        info!("Removed tunnel connection for file: {}", file_id);
    }

    /// List all active tunnel connections
    #[allow(dead_code)]
    pub async fn list_connections(&self) -> Vec<TunnelFileInfo> {
        let connections = self.connections.read().await;
        connections
            .values()
            .map(|conn| conn.file_info.clone())
            .collect()
    }
}

/// Client-side tunnel for sharing files
pub struct TunnelClient {
    base_url: String,
    secret: String,
}

impl TunnelClient {
    /// Create a new tunnel client
    pub fn new(server_url: String, secret: String) -> Self {
        info!("Tunnel client initialized for server: {}", server_url);

        Self {
            base_url: server_url.trim_end_matches('/').to_string(),
            secret,
        }
    }

    /// Announce file to server and start polling for upload requests
    /// Returns the download URL provided by the server
    pub async fn announce_and_serve(
        &self,
        file_path: &Path,
        file_id: String,
    ) -> Result<TunnelAnnounceResponse> {
        // Get file metadata
        let metadata = tokio::fs::metadata(file_path).await?;
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("Invalid file name"))?
            .to_string();
        let file_size = metadata.len();

        let file_info = TunnelFileInfo {
            id: file_id.clone(),
            name: file_name.clone(),
            size: file_size,
        };

        info!(
            "Announcing file to server: {} ({} bytes, id: {})",
            file_name, file_size, file_id
        );

        // Announce file to server
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/tunnel/announce", self.base_url))
            .header("Authorization", format!("Bearer {}", self.secret))
            .json(&file_info)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to announce file: {} - {}",
                status,
                error_text
            ));
        }

        // Parse the response to get the download URL
        let announce_response: TunnelAnnounceResponse = response.json().await?;

        // Start polling and serving in background
        let file_path = file_path.to_path_buf();
        let base_url = self.base_url.clone();
        let secret = self.secret.clone();
        tokio::spawn(async move {
            if let Err(e) =
                Self::poll_and_serve(base_url, file_id, file_name, file_size, file_path, secret)
                    .await
            {
                error!("Error in poll and serve: {}", e);
            }
        });

        Ok(announce_response)
    }

    /// Poll for upload requests and stream file when requested
    /// Automatically re-announces tunnel if server forgets about it (e.g., after restart)
    async fn poll_and_serve(
        base_url: String,
        file_id: String,
        file_name: String,
        file_size: u64,
        file_path: std::path::PathBuf,
        secret: String,
    ) -> Result<()> {
        let client = reqwest::Client::new();
        let mut retry_delay_secs = 1u64; // Start with 1 second, exponentially increase on failures
        const MAX_RETRY_DELAY_SECS: u64 = 60; // Cap at 60 seconds

        loop {
            info!("📡 Long polling server (will hold for up to 30s)...");

            // Long poll for upload request - server holds connection open
            match client
                .get(format!("{}/tunnel/poll", base_url))
                .query(&[("file_id", &file_id)])
                .header("Authorization", format!("Bearer {}", secret))
                .timeout(tokio::time::Duration::from_secs(35)) // Slightly longer than server timeout
                .send()
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    if !status.is_success() {
                        // If server returns 404, it likely restarted and forgot about our tunnel
                        // Re-announce to re-establish the connection
                        if status == reqwest::StatusCode::NOT_FOUND {
                            info!("🔄 Server doesn't recognize tunnel (likely restarted). Re-announcing...");

                            let file_info = TunnelFileInfo {
                                id: file_id.clone(),
                                name: file_name.clone(),
                                size: file_size,
                            };

                            match client
                                .post(format!("{}/tunnel/announce", base_url))
                                .header("Authorization", format!("Bearer {}", secret))
                                .json(&file_info)
                                .send()
                                .await
                            {
                                Ok(resp) if resp.status().is_success() => {
                                    info!("✅ Tunnel re-announced successfully!");
                                }
                                Ok(resp) => {
                                    warn!("Failed to re-announce tunnel: {}", resp.status());
                                }
                                Err(e) => {
                                    warn!("Failed to re-announce tunnel: {}", e);
                                }
                            }
                        } else {
                            warn!("❌ Poll failed with status: {}", status);
                        }
                        warn!("Retrying in {} seconds...", retry_delay_secs);
                        tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay_secs))
                            .await;
                        // Exponential backoff with cap
                        retry_delay_secs = (retry_delay_secs * 2).min(MAX_RETRY_DELAY_SECS);
                        continue;
                    }

                    let poll_response: PollResponse = match response.json().await {
                        Ok(r) => r,
                        Err(e) => {
                            warn!("Failed to parse poll response: {}", e);
                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                            continue;
                        }
                    };

                    match poll_response {
                        PollResponse::Wait => {
                            // Server timed out, no download request - poll again
                            info!("⏱️  Poll timeout (no download), polling again...");
                            // Reset retry delay on successful poll
                            retry_delay_secs = 1;
                        }
                        PollResponse::Upload {
                            file_id: req_file_id,
                        } => {
                            if req_file_id == file_id {
                                info!("🚀 Download detected! Starting file stream...");
                                if let Err(e) = Self::stream_upload(
                                    &client, &base_url, &file_id, &file_path, &secret,
                                )
                                .await
                                {
                                    error!("Failed to stream file: {}", e);
                                } else {
                                    // Reset retry delay on successful upload
                                    retry_delay_secs = 1;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "❌ Poll request failed: {} - retrying in {}s",
                        e, retry_delay_secs
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay_secs)).await;
                    // Exponential backoff with cap
                    retry_delay_secs = (retry_delay_secs * 2).min(MAX_RETRY_DELAY_SECS);
                }
            }
        }
    }

    /// Stream entire file as POST body
    async fn stream_upload(
        client: &reqwest::Client,
        base_url: &str,
        file_id: &str,
        file_path: &Path,
        secret: &str,
    ) -> Result<()> {
        info!("Opening file for streaming: {}", file_path.display());

        // Read file into bytes - we'll use a streaming approach
        // Note: This reads in chunks internally for efficiency
        let file = tokio::fs::File::open(file_path).await?;
        let file_size = file.metadata().await?.len();

        info!("Streaming {} bytes to server...", file_size);

        // Create a stream from the file that reqwest can consume
        use futures::StreamExt;
        let stream = tokio_util::io::ReaderStream::new(file);

        // Convert the stream to bytes chunks
        let body_stream = stream.map(|result| result);

        // POST the file as a streaming body
        let response = client
            .post(format!("{}/tunnel/upload", base_url))
            .query(&[("file_id", file_id)])
            .header("Authorization", format!("Bearer {}", secret))
            .body(reqwest::Body::wrap_stream(body_stream))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("Upload failed: {}", response.status()));
        }

        info!("File stream complete: {} bytes", file_size);
        Ok(())
    }
}
