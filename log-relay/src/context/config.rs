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
    /// The JetStream stream name to subscribe to (e.g. "LOGS").
    pub stream_name: String,
    /// Subject filter pattern for archiving. Defaults to `logs.>`.
    pub filter_subject: Option<String>,
}

/// Relay tuning knobs for the StreamManager.
#[derive(Debug, Clone, Deserialize)]
pub struct RelaySettings {
    /// Seconds of inactivity (no NATS messages) before an idle stream is cleaned up.
    pub idle_timeout_secs: u64,
    /// Capacity of the bounded broadcast channel per run_id.
    /// Larger values tolerate slow consumers at the cost of memory.
    pub broadcast_capacity: usize,
    /// SSE keep-alive interval in seconds (HTTP layer ping, independent of heartbeat events).
    pub sse_keepalive_secs: u64,
    /// How often to emit a `Heartbeat` SSE event when no log messages are arriving.
    pub heartbeat_interval_secs: u64,
    /// The JetStream stream name used by StreamManager when creating consumers.
    pub stream_name: String,
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
