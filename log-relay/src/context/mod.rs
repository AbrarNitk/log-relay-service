pub mod config;
use std::sync::Arc;

use async_nats::jetstream;
use config::Config;

use crate::service::StreamManager;

/// Application-wide shared state, injected into all Axum handlers via [`State`].
///
/// Cloning is cheap — all heavy state is reference-counted.
#[derive(Clone)]
pub struct Context {
    pub stream_manager: StreamManager,
    pub nats_client: async_nats::Client,
    pub config: Arc<Config>,
}

impl Context {
    pub async fn build(config: Config) -> Self {
        // ── NATS connection ────────────────────────────────────────────────
        let mut nats_options = async_nats::ConnectOptions::new();
        if let (Some(user), Some(pass)) = (&config.nats.username, &config.nats.password) {
            nats_options = nats_options.user_and_password(user.clone(), pass.clone());
        }

        let nats_client = async_nats::connect_with_options(&config.nats.url, nats_options)
            .await
            .expect("Failed to connect to NATS");

        // ── JetStream context ──────────────────────────────────────────────
        let js = jetstream::new(nats_client.clone());

        // ── StreamManager ──────────────────────────────────────────────────
        let relay_config = Arc::new(config.relay.clone());
        let stream_manager = StreamManager::new(js, relay_config);

        Self {
            stream_manager,
            nats_client,
            config: Arc::new(config),
        }
    }
}
