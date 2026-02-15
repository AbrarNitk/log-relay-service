use anyhow::{Context, Result};
use async_nats::Client;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct NatsConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl NatsConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path).context("Failed to read NATS config file")?;
        let config: NatsConfig =
            serde_json::from_str(&content).context("Failed to parse NATS config")?;
        Ok(config)
    }

    pub async fn connect(&self) -> Result<Client> {
        let mut options = async_nats::ConnectOptions::new();

        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            options = options.user_and_password(user.clone(), pass.clone());
        }

        let client = async_nats::connect_with_options(&self.url, options)
            .await
            .context("Failed to connect to NATS")?;

        Ok(client)
    }
}
