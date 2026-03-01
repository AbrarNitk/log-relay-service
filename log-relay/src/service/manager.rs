use std::collections::HashSet;
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_nats::jetstream;
use dashmap::DashMap;
use futures::StreamExt;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use ulid::Ulid;

use crate::{context::config::RelaySettings, controller::types::StreamPolicies};

use super::events::{EnrichedLog, StreamEndReason, StreamEvent};
use super::stream::{ManagedStream, StreamCommand};

// ── StreamManager ─────────────────────────────────────────────────────────────

/// Central registry of all active SSE sessions.
///
/// Two indices are maintained:
/// - **`by_stream_id`** (primary): `stream_id (ULID) → Weak<ManagedStream>`.
///   Used for precise per-session operations: pause, resume, terminate.
/// - **`by_run_id`** (secondary): `run_id → HashSet<stream_id>`.
///   Used to query how many sessions are streaming a given `run_id`.
///
/// Every call to `create` generates a fresh ULID `stream_id`, so different
/// clients (with different policies) never share state accidentally.
#[derive(Clone)]
pub struct StreamManager {
    by_stream_id: Arc<DashMap<String, Weak<ManagedStream>>>,
    by_run_id: Arc<DashMap<String, HashSet<String>>>,
    js: jetstream::Context,
    config: Arc<RelaySettings>,
}

impl StreamManager {
    pub fn new(js: jetstream::Context, config: Arc<RelaySettings>) -> Self {
        Self {
            by_stream_id: Arc::new(DashMap::new()),
            by_run_id: Arc::new(DashMap::new()),
            js,
            config,
        }
    }

    // ── Public API ─────────────────────────────────────────────────────────

    /// Create a brand-new managed stream session for `run_id` with the given
    /// consumer policies. Returns `(stream_id, Arc<ManagedStream>)`.
    ///
    /// A fresh ULID `stream_id` is generated on every call — sessions are
    /// always independent regardless of policies or `run_id`.
    #[tracing::instrument(name = "stream_manager::create", skip(self, policies), fields(run_id))]
    pub async fn create(
        &self,
        run_id: &str,
        policies: &StreamPolicies,
    ) -> (String, Arc<ManagedStream>) {
        let stream_id = Ulid::new().to_string();
        tracing::info!(run_id, %stream_id, "Creating new stream session");

        let stream = self.build_and_spawn(run_id, &stream_id, policies).await;

        // Register in the primary index.
        self.by_stream_id
            .insert(stream_id.clone(), Arc::downgrade(&stream));

        // Register in the secondary run_id index.
        self.by_run_id
            .entry(run_id.to_string())
            .or_insert_with(HashSet::new)
            .insert(stream_id.clone());

        (stream_id, stream)
    }

    /// Look up an active stream by its `stream_id`.
    /// Returns `None` if the session has already ended.
    pub fn get(&self, stream_id: &str) -> Option<Arc<ManagedStream>> {
        self.by_stream_id.get(stream_id).and_then(|w| w.upgrade())
    }

    /// Forcefully terminate a specific session by `stream_id`.
    #[tracing::instrument(name = "stream_manager::terminate", skip(self))]
    pub async fn terminate(&self, stream_id: &str) {
        if let Some(stream) = self.get(stream_id) {
            stream.terminate().await;
            tracing::info!(%stream_id, "Session force-terminated");
        }
    }

    /// Number of active sessions streaming a given `run_id`.
    pub fn session_count_for(&self, run_id: &str) -> usize {
        self.by_run_id.get(run_id).map(|ids| ids.len()).unwrap_or(0)
    }

    /// All active `stream_id`s for a given `run_id`.
    pub fn sessions_for(&self, run_id: &str) -> Vec<String> {
        self.by_run_id
            .get(run_id)
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Total number of active stream sessions across all `run_id`s.
    pub fn total_active(&self) -> usize {
        self.by_stream_id.len()
    }

    // ── Internal helpers ───────────────────────────────────────────────────

    /// Construct the `ManagedStream` and spawn the background NATS pump task.
    async fn build_and_spawn(
        &self,
        run_id: &str,
        stream_id: &str,
        policies: &StreamPolicies,
    ) -> Arc<ManagedStream> {
        let (tx, _) = broadcast::channel::<StreamEvent>(self.config.broadcast_capacity);
        let (ctrl_tx, ctrl_rx) = mpsc::channel::<StreamCommand>(8);
        let shutdown = CancellationToken::new();

        let stream = Arc::new(ManagedStream {
            stream_id: stream_id.to_string(),
            run_id: run_id.to_string(),
            sender: tx,
            control_tx: ctrl_tx,
            shutdown_token: shutdown.clone(),
            by_stream_id: Arc::clone(&self.by_stream_id),
            by_run_id: Arc::clone(&self.by_run_id),
            subscriber_count: std::sync::atomic::AtomicU32::new(0),
        });

        self.spawn_pump(
            run_id,
            stream_id,
            Arc::clone(&stream),
            ctrl_rx,
            shutdown,
            policies,
        )
        .await;

        stream
    }

    /// Spawn the Tokio task that polls NATS and fans out enriched events.
    async fn spawn_pump(
        &self,
        run_id: &str,
        stream_id: &str,
        stream: Arc<ManagedStream>,
        mut ctrl_rx: mpsc::Receiver<StreamCommand>,
        shutdown: CancellationToken,
        policies: &StreamPolicies,
    ) {
        let js = self.js.clone();
        let config = Arc::clone(&self.config);
        let run_id = run_id.to_string();
        let stream_id = stream_id.to_string();
        let filter_subject = format!("logs.*.*.*.{}", run_id);
        let deliver_policy = policies.deliver_policy;
        let replay_policy = policies.replay_policy;
        let nats_stream_name = config.stream_name.clone();

        tokio::spawn(async move {
            tracing::info!(%stream_id, %run_id, %filter_subject, "NATS pump started");

            // ── Create the ephemeral ordered consumer ─────────────────────────
            // We use pull::OrderedConfig (not push::) to avoid the idle-heartbeat
            // negotiation that push consumers require — pull ordered consumers
            // call .messages() the same way and work with all NATS server versions.
            let consumer_config = jetstream::consumer::pull::OrderedConfig {
                filter_subject: filter_subject.clone(),
                deliver_policy,
                replay_policy,
                ..Default::default()
            };

            let consumer = match js.get_stream(&nats_stream_name).await {
                Ok(js_stream) => match js_stream.create_consumer(consumer_config).await {
                    Ok(c) => c,
                    Err(e) => {
                        let msg = format!("Failed to create NATS consumer: {e}");
                        tracing::error!(%stream_id, "{msg}");
                        let _ = stream.sender.send(StreamEvent::Error(msg.clone()));
                        let _ = stream
                            .sender
                            .send(StreamEvent::Terminated(StreamEndReason::NatsError(msg)));
                        return;
                    }
                },
                Err(e) => {
                    let msg = format!("NATS stream '{}' not found: {e}", nats_stream_name);
                    tracing::error!(%stream_id, "{msg}");
                    let _ = stream.sender.send(StreamEvent::Error(msg.clone()));
                    let _ = stream
                        .sender
                        .send(StreamEvent::Terminated(StreamEndReason::NatsError(msg)));
                    return;
                }
            };

            // Signal SSE clients we are live and include the stream_id.
            let _ = stream
                .sender
                .send(StreamEvent::Connected(stream_id.clone()));

            let mut messages = match consumer.messages().await {
                Ok(m) => m,
                Err(e) => {
                    let msg = format!("Failed to open message stream: {e}");
                    tracing::error!(%stream_id, "{msg}");
                    let _ = stream.sender.send(StreamEvent::Error(msg.clone()));
                    let _ = stream
                        .sender
                        .send(StreamEvent::Terminated(StreamEndReason::NatsError(msg)));
                    return;
                }
            };

            let idle_timeout = Duration::from_secs(config.idle_timeout_secs);
            let heartbeat_interval = Duration::from_secs(config.heartbeat_interval_secs);
            let mut paused = false;
            let mut heartbeat_ticker = tokio::time::interval(heartbeat_interval);
            heartbeat_ticker.reset();

            let end_reason = loop {
                tokio::select! {
                    biased;

                    // ── Shutdown signal ───────────────────────────────────────
                    _ = shutdown.cancelled() => {
                        tracing::info!(%stream_id, "Pump shutdown via cancellation token");
                        break StreamEndReason::ForceStopped;
                    }

                    // ── Control command ───────────────────────────────────────
                    Some(cmd) = ctrl_rx.recv() => match cmd {
                        StreamCommand::Pause     => { paused = true;  tracing::info!(%stream_id, "Paused");   }
                        StreamCommand::Resume    => { paused = false; tracing::info!(%stream_id, "Resumed");  }
                        StreamCommand::Terminate => { break StreamEndReason::ForceStopped; }
                    },

                    // ── Heartbeat ─────────────────────────────────────────────
                    _ = heartbeat_ticker.tick() => {
                        if !paused {
                            let _ = stream.sender.send(StreamEvent::Heartbeat);
                        }
                    }

                    // ── NATS message (with idle timeout) ──────────────────────
                    result = tokio::time::timeout(idle_timeout, messages.next()) => {
                        match result {
                            Err(_) => {
                                tracing::info!(%stream_id, idle_secs = config.idle_timeout_secs, "Idle timeout");
                                break StreamEndReason::IdleTimeout;
                            }
                            Ok(None) => {
                                tracing::info!(%stream_id, "NATS stream closed");
                                break StreamEndReason::ApplicationEnded;
                            }
                            Ok(Some(Err(e))) => {
                                tracing::error!(%stream_id, "NATS error: {e}");
                                let _ = stream.sender.send(StreamEvent::Error(e.to_string()));
                                // Transient — don't kill the stream.
                            }
                            Ok(Some(Ok(msg))) => {
                                tracing::debug!(
                                    %stream_id,
                                    subject = %msg.subject,
                                    "NATS message received"
                                );
                                if paused {
                                    let _ = msg.ack().await;
                                    continue;
                                }

                                let enriched = enrich_message(&run_id, &msg);
                                tracing::debug!(
                                    %stream_id,
                                    seq = enriched.sequence_id,
                                    message = %enriched.raw_data,
                                    "Broadcasting log event"
                                );
                                let receiver_count = stream.sender.receiver_count();
                                if receiver_count == 0 {
                                    tracing::warn!(%stream_id, "No SSE subscribers — log event will be lost");
                                }
                                let _ = stream.sender.send(StreamEvent::Log(enriched));

                                if let Err(e) = msg.ack().await {
                                    tracing::warn!(%stream_id, "ACK failed: {e}");
                                }

                                heartbeat_ticker.reset();
                            }
                        }
                    }
                }
            };

            // ── Teardown ──────────────────────────────────────────────────────
            tracing::info!(%stream_id, reason = ?end_reason, "Pump exiting");
            let _ = stream.sender.send(StreamEvent::Terminated(end_reason));
            // Both map entries are cleaned by ManagedStream::drop when the Arc is released.
        });
    }
}

// ── Message Enrichment ────────────────────────────────────────────────────────

/// Parse the NATS subject and message metadata into an [`EnrichedLog`].
///
/// Subject format: `logs.<namespace>.<app>.<component>.<run_id>`
///
/// The payload is decoded to UTF-8 once here; non-UTF-8 bytes become `"<binary>"`.
fn enrich_message(run_id: &str, msg: &async_nats::jetstream::Message) -> EnrichedLog {
    let mut parts = msg.subject.as_str().splitn(5, '.');
    let _ = parts.next(); // "logs"
    let namespace = parts.next().unwrap_or("").to_string();
    let app = parts.next().unwrap_or("").to_string();
    let component = parts.next().unwrap_or("").to_string();

    let (sequence_id, timestamp_ms) = msg
        .info()
        .map(|i| {
            (
                i.stream_sequence,
                i.published.unix_timestamp_nanos() / 1_000_000 as i128,
            )
        })
        .unwrap_or((0, 0));

    let raw_data = std::str::from_utf8(&msg.payload)
        .unwrap_or("<binary>")
        .to_string();

    EnrichedLog {
        sequence_id,
        timestamp_ms: timestamp_ms as i64,
        namespace,
        app,
        component,
        run_id: run_id.to_string(),
        raw_data,
    }
}
