use serde::Deserialize;

/// Top-level archiver configuration — loaded from a JSON config file.
#[derive(Debug, Clone, Deserialize)]
pub struct ArchiverConfig {
    pub nats: NatsSettings,
    pub openobserve: OpenObserveSettings,
    pub archiver: BatchSettings,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NatsSettings {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub stream_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenObserveSettings {
    pub url: String,
    pub org: String,
    pub stream: String,
    pub user: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchSettings {
    /// Number of messages to collect before sending a batch.
    pub batch_size: usize,
    /// Max seconds to wait before flushing a partial batch.
    pub batch_timeout_secs: u64,
}

impl ArchiverConfig {
    pub fn load_from_file(path: &str) -> Self {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read config file '{}': {}", path, e));
        serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse config file '{}': {}", path, e))
    }
}
