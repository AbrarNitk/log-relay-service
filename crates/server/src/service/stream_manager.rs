use anyhow::{Context, Result};
use async_nats::jetstream;
use bytes::Bytes;
use dashmap::DashMap;
use futures::StreamExt;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::config::RelaySettings;

// ---------------------------------------------------------------------------
// SharedStream
// ---------------------------------------------------------------------------

/// A shared stream that fans out NATS messages to N SSE clients via a bounded
/// `broadcast` channel. When the last `Arc<SharedStream>` is dropped the
/// `CancellationToken` is cancelled, which signals the background NATS pump
/// task to exit.
pub struct SharedStream {
    pub run_id: String,

    /// Bounded broadcast channel — sends raw `Bytes` from NATS.
    /// Receivers are created per SSE client via `sender.subscribe()`.
    pub sender: broadcast::Sender<Bytes>,

    /// Cancellation token to signal the background pump to stop.
    pub shutdown: CancellationToken,

    /// Weak back-reference for self-removal from the map on Drop.
    streams_map: Arc<DashMap<String, Weak<SharedStream>>>,
}

impl Drop for SharedStream {
    fn drop(&mut self) {
        // Cancel the background NATS pump task.
        self.shutdown.cancel();
        // Remove ourselves from the map (idempotent — may already be gone
        // if the idle-timeout or stop_stream removed us first).
        self.streams_map.remove(&self.run_id);
        info!(run_id = %self.run_id, "SharedStream dropped — consumer cleaned up");
    }
}

// ---------------------------------------------------------------------------
// StreamManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of shared NATS consumers keyed by `run_id`.
///
/// Multiple SSE clients requesting the same `run_id` share a single NATS
/// consumer and `broadcast` channel. When the last client disconnects (or the
/// idle timeout fires), the consumer is cleaned up automatically.
#[derive(Clone)]
pub struct StreamManager {
    /// run_id → Weak<SharedStream>.  Using Weak so the map alone doesn't
    /// keep a stream alive.
    streams: Arc<DashMap<String, Weak<SharedStream>>>,

    /// JetStream context — wraps the single NATS TCP connection.
    js: jetstream::Context,

    /// Relay tuning knobs.
    config: Arc<RelaySettings>,
}

impl StreamManager {
    /// Create a new `StreamManager` from a connected NATS client.
    pub fn new(client: async_nats::Client, config: RelaySettings) -> Self {
        let js = jetstream::new(client);
        Self {
            streams: Arc::new(DashMap::new()),
            js,
            config: Arc::new(config),
        }
    }

    // ----- public API -------------------------------------------------------

    /// Get an existing shared stream or create a new one.
    ///
    /// **Race-free**: uses `DashMap::entry()` so concurrent requests for the
    /// same `run_id` are serialized within the shared lock.
    pub async fn get_or_create_stream(&self, run_id: &str) -> Result<Arc<SharedStream>> {
        // Fast path (read-only, no shared write-lock).
        if let Some(entry) = self.streams.get(run_id) {
            if let Some(arc) = entry.upgrade() {
                info!(run_id, "Reusing existing shared stream");
                return Ok(arc);
            }
        }

        // Slow path — entry API holds the shared lock.
        match self.streams.entry(run_id.to_string()) {
            dashmap::mapref::entry::Entry::Occupied(mut occ) => {
                if let Some(arc) = occ.get().upgrade() {
                    info!(run_id, "Reusing existing shared stream (race resolved)");
                    return Ok(arc);
                }
                // Dead Weak — replace it with a fresh stream.
                let shared = self.create_shared_stream(run_id).await?;
                let arc = Arc::new(shared);
                occ.insert(Arc::downgrade(&arc));
                self.spawn_nats_pump(run_id, Arc::clone(&arc));
                Ok(arc)
            }
            dashmap::mapref::entry::Entry::Vacant(vac) => {
                let shared = self.create_shared_stream(run_id).await?;
                let arc = Arc::new(shared);
                vac.insert(Arc::downgrade(&arc));
                self.spawn_nats_pump(run_id, Arc::clone(&arc));
                Ok(arc)
            }
        }
    }

    /// Forcefully stop a stream by `run_id`. Used by the `DELETE /streams/{run_id}` API.
    pub fn stop_stream(&self, run_id: &str) -> bool {
        if let Some((_, weak)) = self.streams.remove(run_id) {
            if let Some(arc) = weak.upgrade() {
                arc.shutdown.cancel();
                info!(run_id, "Stream stopped via API");
                return true;
            }
        }
        false
    }

    /// Returns the number of currently active streams (for /health diagnostics).
    pub fn active_count(&self) -> usize {
        self.streams.len()
    }

    // ----- internals --------------------------------------------------------

    /// Build a `SharedStream` (does NOT start the pump — caller does that).
    async fn create_shared_stream(&self, run_id: &str) -> Result<SharedStream> {
        let (sender, _) = broadcast::channel(self.config.broadcast_capacity);
        let shutdown = CancellationToken::new();

        info!(
            run_id,
            capacity = self.config.broadcast_capacity,
            "Created new SharedStream"
        );

        Ok(SharedStream {
            run_id: run_id.to_string(),
            sender,
            shutdown,
            streams_map: Arc::clone(&self.streams),
        })
    }

    /// Spawn a background tokio task that pulls messages from NATS and
    /// broadcasts them to all SSE receivers.
    fn spawn_nats_pump(&self, run_id: &str, shared: Arc<SharedStream>) {
        let js = self.js.clone();
        let config = Arc::clone(&self.config);
        let run_id_owned = run_id.to_string();
        let streams_map = Arc::clone(&self.streams);

        tokio::spawn(async move {
            if let Err(e) = run_nats_pump(&js, &config, &run_id_owned, &shared).await {
                error!(run_id = %run_id_owned, "NATS pump error: {:#}", e);
            }

            // Cleanup: remove from map so no new clients can join a dead stream.
            streams_map.remove(&run_id_owned);
            info!(run_id = %run_id_owned, "NATS pump task exited");
        });
    }
}

// ---------------------------------------------------------------------------
// NATS pump — the inner loop
// ---------------------------------------------------------------------------

async fn run_nats_pump(
    js: &jetstream::Context,
    config: &RelaySettings,
    run_id: &str,
    shared: &SharedStream,
) -> Result<()> {
    let stream_name = "LOGS";
    let filter_subject = format!("logs.*.*.*.{}", run_id);

    info!(run_id, filter = %filter_subject, "Setting up NATS consumer");

    let stream = js
        .get_stream(stream_name)
        .await
        .context("Failed to get JetStream stream handle")?;

    // Ephemeral pull consumer — auto-deleted when inactive.
    let consumer_config = jetstream::consumer::pull::Config {
        filter_subject: filter_subject.clone(),
        deliver_policy: jetstream::consumer::DeliverPolicy::New,
        replay_policy: jetstream::consumer::ReplayPolicy::Instant,
        // NATS will delete this consumer after `inactive_threshold` of no pulls.
        inactive_threshold: Duration::from_secs(config.idle_timeout_secs + 30),
        ..Default::default()
    };

    let consumer = stream
        .create_consumer(consumer_config)
        .await
        .context("Failed to create ephemeral consumer")?;

    info!(run_id, "Ephemeral consumer created for {}", filter_subject);

    let mut messages = consumer
        .messages()
        .await
        .context("Failed to get message iterator")?;

    let idle_dur = Duration::from_secs(config.idle_timeout_secs);

    loop {
        tokio::select! {
            biased;

            // Shutdown signal — immediate exit.
            _ = shared.shutdown.cancelled() => {
                info!(run_id, "Shutdown signal received");
                break;
            }

            // Next message with idle timeout.
            msg = tokio::time::timeout(idle_dur, messages.next()) => {
                match msg {
                    // Message arrived within timeout.
                    Ok(Some(Ok(nats_msg))) => {
                        // Fire-and-forget ack for UI streaming (at-most-once is fine).
                        if let Err(e) = nats_msg.ack().await {
                            warn!(run_id, "Failed to ack message: {}", e);
                        }

                        // Zero-copy: broadcast the Bytes reference directly.
                        // If all receivers are gone, send returns Err — we ignore it
                        // because Drop-based cleanup will stop us shortly.
                        let _ = shared.sender.send(nats_msg.payload.clone());
                    }
                    // NATS stream error.
                    Ok(Some(Err(e))) => {
                        error!(run_id, "NATS stream error: {}", e);
                        // Continue — transient errors are recoverable.
                    }
                    // Stream ended (unlikely for JetStream pull, but handle it).
                    Ok(None) => {
                        info!(run_id, "NATS message stream ended");
                        break;
                    }
                    // Idle timeout — no messages for `idle_dur`.
                    Err(_) => {
                        info!(run_id, secs = config.idle_timeout_secs, "Idle timeout, closing stream");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
