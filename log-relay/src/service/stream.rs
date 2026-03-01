use std::collections::HashSet;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

use super::events::StreamEvent;

// ── Internal commands sent from the public API to the background task ───────

/// Commands the public `ManagedStream` API can send into the background pump.
#[derive(Debug)]
pub(super) enum StreamCommand {
    /// Temporarily stop forwarding NATS messages to SSE clients.
    /// Messages continue to accumulate in NATS; nothing is lost.
    Pause,
    /// Resume forwarding after a previous `Pause`.
    Resume,
    /// Clean shutdown: broadcasts `Terminated(ForceStopped)` then exits.
    Terminate,
}

// ── ManagedStream ────────────────────────────────────────────────────────────

/// A handle to one live SSE session — a single NATS consumer identified by
/// its unique ULID `stream_id`.
///
/// **Key design points**
/// - Primary key is `stream_id` (ULID), not `run_id`. Every `create_stream`
///   call produces a new `stream_id`, so different sessions with different
///   policies never collide, and `terminate(stream_id)` is always precise.
/// - The `StreamManager` also maintains a secondary `run_id → {stream_id}`
///   index so callers can ask "how many streams are active for run_id X?"
///
/// **Ownership / lifetime model**
/// - `StreamManager` holds `Weak<ManagedStream>` in its primary map.
/// - Every SSE handler holds one `Arc<ManagedStream>`.
/// - When the last `Arc` is dropped, `Drop` cancels the background task and
///   cleans both map entries — no timer, no GC, no polling required.
pub struct ManagedStream {
    /// Unique per-session identifier (ULID string). Returned to the browser
    /// in the `Connected` SSE event so it can address pause/resume/terminate
    /// precisely.
    pub stream_id: String,

    /// The `run_id` this session is streaming logs for (used as NATS subject
    /// filter: `logs.*.*.*.{run_id}`).
    pub run_id: String,

    /// Broadcast sender. Kept alive here so new subscribers can join at any
    /// time while the pump is running.
    pub(super) sender: broadcast::Sender<StreamEvent>,

    /// Command channel into the background pump task.
    pub(super) control_tx: mpsc::Sender<StreamCommand>,

    /// Shared cancellation token. Cancelling it signals the pump to exit.
    pub(super) shutdown_token: CancellationToken,

    /// Back-reference to the primary map for Drop-time self-removal.
    pub(super) by_stream_id: Arc<DashMap<String, std::sync::Weak<ManagedStream>>>,

    /// Back-reference to the secondary index for Drop-time self-removal.
    pub(super) by_run_id: Arc<DashMap<String, HashSet<String>>>,

    /// Live subscriber count, for observability.
    pub(super) subscriber_count: AtomicU32,
}

impl ManagedStream {
    /// Subscribe to the event stream. Each caller gets an independent receiver
    /// positioned at the *current* broadcast tail.
    pub fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.subscriber_count.fetch_add(1, Ordering::Relaxed);
        self.sender.subscribe()
    }

    /// Number of active SSE clients currently subscribed.
    pub fn subscriber_count(&self) -> u32 {
        self.subscriber_count.load(Ordering::Relaxed)
    }

    /// Pause the NATS pump (no messages forwarded while paused).
    pub async fn pause(&self) {
        if let Err(e) = self.control_tx.send(StreamCommand::Pause).await {
            tracing::warn!(stream_id = %self.stream_id, "Failed to send Pause: {e}");
        }
    }

    /// Resume forwarding after a `pause()`.
    pub async fn resume(&self) {
        if let Err(e) = self.control_tx.send(StreamCommand::Resume).await {
            tracing::warn!(stream_id = %self.stream_id, "Failed to send Resume: {e}");
        }
    }

    /// Terminate this specific stream session immediately.
    pub async fn terminate(&self) {
        if let Err(e) = self.control_tx.send(StreamCommand::Terminate).await {
            tracing::warn!(stream_id = %self.stream_id, "Failed to send Terminate: {e}");
        }
        self.shutdown_token.cancel();
    }
}

/// When the last `Arc<ManagedStream>` is dropped, clean up both indices
/// and cancel the background pump task automatically.
impl Drop for ManagedStream {
    fn drop(&mut self) {
        tracing::info!(
            stream_id = %self.stream_id,
            run_id = %self.run_id,
            "ManagedStream dropped — cancelling pump and cleaning indices"
        );

        self.shutdown_token.cancel();

        // Remove from the primary stream_id index.
        self.by_stream_id.remove(&self.stream_id);

        // Remove stream_id from the run_id secondary index.
        self.by_run_id.alter(&self.run_id, |_, mut ids| {
            ids.remove(&self.stream_id);
            ids
        });
        // If no more streams for this run_id, remove the run_id entry entirely.
        self.by_run_id
            .remove_if(&self.run_id, |_, ids| ids.is_empty());
    }
}
