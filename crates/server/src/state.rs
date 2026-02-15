use async_nats::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub nats_client: Arc<Client>,
}

impl AppState {
    pub fn new(nats_client: Client) -> Self {
        Self {
            nats_client: Arc::new(nats_client),
        }
    }
}
