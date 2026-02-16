use crate::config::Config;
use crate::service::stream_manager::StreamManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub stream_manager: Arc<StreamManager>,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(stream_manager: StreamManager, config: Config) -> Self {
        Self {
            stream_manager: Arc::new(stream_manager),
            config: Arc::new(config),
        }
    }
}
