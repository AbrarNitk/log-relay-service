use serde::Deserialize;

/// Top-level service configuration — loaded from a JSON config file.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub nats: NatsSettings,
    pub relay: RelaySettings,
}

/// NATS connection and JetStream settings.
#[derive(Debug, Clone, Deserialize)]
pub struct NatsSettings {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub stream_name: String,
    // todo: filter-subject-pattern
}

/// Relay tuning knobs for the StreamManager.
#[derive(Debug, Clone, Deserialize)]
pub struct RelaySettings {
    /// Seconds of inactivity before an idle stream is cleaned up.
    pub idle_timeout_secs: u64,
    /// Capacity of the bounded broadcast channel per run_id.
    pub broadcast_capacity: usize,
    /// SSE keep-alive interval in seconds.
    pub sse_keepalive_secs: u64,
}

impl Config {
    /// Load config from a JSON file. Panics with a clear message if missing or malformed.
    pub fn load_from_file(path: &str) -> Self {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read config file '{}': {}", path, e));

        serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse config file '{}': {}", path, e))
    }
}
