pub mod config;
use config::Config;
use std::sync::Arc;

#[derive(Clone)]
pub struct Context {
    // pub stream_manager: Arc<StreamManager>,
    pub nats_client: async_nats::Client,
    pub config: Arc<Config>,
}

impl Context {
    pub fn new(nats_client: async_nats::Client, config: Config) -> Self {
        Self {
            nats_client,
            config: Arc::new(config),
        }
    }

    pub async fn build(config: Config) -> Self {
        let mut nats_options = async_nats::ConnectOptions::new();
        if let (Some(user), Some(pass)) = (&config.nats.username, &config.nats.password) {
            nats_options = nats_options.user_and_password(user.clone(), pass.clone());
        }

        let nats_client = async_nats::connect_with_options(&config.nats.url, nats_options)
            .await
            .expect("Failed to connect to NATS");

        Self {
            nats_client,
            config: Arc::new(config),
        }
    }
}
