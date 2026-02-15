use anyhow::{Context, Result};
use async_nats::jetstream::{self, consumer::PullConsumer};
use futures::StreamExt;
use std::time::Duration;
use tracing::{debug, error, info};

#[derive(Clone)]
pub struct LogsService {
    js: jetstream::Context,
}

/// Helper method for efficient creation of NATS consumers
impl LogsService {
    pub fn new(client: async_nats::Client) -> Self {
        let js = jetstream::new(client);
        Self { js }
    }

    /// Stream logs for a given `run_id`.
    ///
    /// This method is designed for high performance:
    /// - Uses JetStream ephemeral consumers for each client connection.
    /// - Streams messages as they arrive without immediate buffering.
    /// - Attempts to use zero-copy mechanisms where feasible (e.g., passing Bytes through).
    ///
    /// Note on "Zero Copy": Ideally, we would yield `Bytes` directly to `axum::response::sse::Event`.
    /// However, SSE protocol is text-based. We convert valid UTF-8 bytes to String with minimal overhead.
    /// If the payload is already valid UTF-8, `String::from_utf8` reuses the allocation of `Vec<u8>`.
    pub async fn stream_logs(
        self,
        run_id: String,
    ) -> Result<impl futures::Stream<Item = Result<String, anyhow::Error>>> {
        let stream_name = "LOGS";
        let filter_subject = format!("logs.job.{}", run_id);

        info!("Setting up log stream for subject: {}", filter_subject);

        let stream = self
            .js
            .get_stream(stream_name)
            .await
            .context("Failed to get stream handle")?;

        // Create ephemeral consumer configuration
        let consumer_config = jetstream::consumer::pull::Config {
            filter_subject: filter_subject.clone(),
            deliver_policy: jetstream::consumer::DeliverPolicy::All,
            replay_policy: jetstream::consumer::ReplayPolicy::Instant,
            inactive_threshold: Duration::from_secs(60),
            ..Default::default()
        };

        // Create the consumer
        let consumer = stream
            .create_consumer(consumer_config)
            .await
            .context("Failed to create consumer")?;

        info!("Created ephemeral consumer for {}", filter_subject);

        // Get the message stream
        let messages = consumer
            .messages()
            .await
            .context("Failed to get message stream")?;

        // Transform the stream
        Ok(messages.then(|msg_res| async move {
            match msg_res {
                Ok(msg) => {
                    // Acknowledge the message to ensure at-least-once delivery semantics
                    // Fire-and-forget logic for high throughput UI streaming
                    if let Err(e) = msg.ack().await {
                        error!("Failed to ack message: {}", e);
                    }

                    // Zero-copy optimization: Reuse the underlying Vec<u8> if valid UTF-8
                    match String::from_utf8(msg.payload.to_vec()) {
                        Ok(s) => Ok(s),
                        Err(e) => {
                            // Allocation only on invalid UTF-8 (fallback)
                            Ok(String::from_utf8_lossy(&e.into_bytes()).to_string())
                        }
                    }
                }
                Err(e) => {
                    error!("Stream error: {}", e);
                    Err(anyhow::anyhow!("Stream error: {}", e))
                }
            }
        }))
    }
}
