use crate::config::OpenObserveSettings;
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::Client;
use std::time::Duration;
use tracing::{error, info, warn};

/// OpenObserve HTTP client for bulk log ingestion.
pub struct OpenObserveClient {
    client: Client,
    endpoint: String,
    auth_header: String,
}

impl OpenObserveClient {
    pub fn new(settings: &OpenObserveSettings) -> Self {
        let endpoint = format!(
            "{}/api/{}/{}/_json",
            settings.url.trim_end_matches('/'),
            settings.org,
            settings.stream,
        );

        let credentials = format!("{}:{}", settings.user, settings.password);
        let auth_header = format!("Basic {}", BASE64.encode(credentials));

        info!(%endpoint, "OpenObserve client initialized");

        Self {
            client: Client::new(),
            endpoint,
            auth_header,
        }
    }

    /// Send a batch of JSON records to OpenObserve.
    /// Retries with exponential backoff on failure (never gives up).
    pub async fn send_batch(&self, records: &[serde_json::Value]) -> Result<()> {
        let mut backoff = Duration::from_secs(1);
        let max_backoff = Duration::from_secs(60);

        loop {
            match self.try_send(records).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    error!(
                        error = %e,
                        backoff_secs = backoff.as_secs(),
                        batch_size = records.len(),
                        "Failed to send batch to OpenObserve, retrying..."
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(max_backoff);
                }
            }
        }
    }

    async fn try_send(&self, records: &[serde_json::Value]) -> Result<()> {
        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(records)
            .send()
            .await
            .context("HTTP request to OpenObserve failed")?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = response.text().await.unwrap_or_default();
            warn!(%status, %body, "OpenObserve returned non-success status");
            anyhow::bail!("OpenObserve returned {}: {}", status, body)
        }
    }
}
