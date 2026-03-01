use std::fmt;

use axum::response::sse::Event;
use serde::Serialize;

/// Events that flow from the background NATS fetcher task through the broadcast
/// channel to every SSE client connected to a given `run_id`.
///
/// Each variant maps to a distinct SSE `event:` type so the browser can easily
/// discriminate without parsing the data field.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Emitted once when the consumer is first created and connected to NATS.
    /// Carries the unique `stream_id` (ULID) the browser must use to address
    /// pause, resume, and terminate operations for this session.
    Connected(String),

    /// Emitted periodically (configurable interval) to prove the SSE pipe is
    /// still open even when there are no log messages arriving.
    Heartbeat,

    /// A real log message, enriched with metadata parsed from the NATS subject.
    Log(EnrichedLog),

    /// Non-fatal advisory — e.g., the broadcast channel lagged and some
    /// messages were skipped for this particular receiver.
    Warning(String),

    /// A fatal error from the NATS layer (stream or consumer errors).
    Error(String),

    /// The stream has ended. Receiving this tells the SSE handler to close the
    /// HTTP response.
    Terminated(StreamEndReason),
}

impl StreamEvent {
    /// Returns the SSE `event:` name for this variant.
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::Connected(_) => "connected",
            Self::Heartbeat => "heartbeat",
            Self::Log(_) => "log",
            Self::Warning(_) => "warning",
            Self::Error(_) => "error",
            Self::Terminated(_) => "terminated",
        }
    }

    /// Returns whether this event signals the end of the stream.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminated(_))
    }
}

/// Convert a `StreamEvent` into an Axum SSE `Event`.
///
/// The conversion lives here — as close to the type definition as possible —
/// so that the controller layer is purely a thin routing concern.
impl TryFrom<&StreamEvent> for Event {
    type Error = serde_json::Error;

    fn try_from(event: &StreamEvent) -> Result<Self, Self::Error> {
        let ev = Event::default().event(event.event_name());

        let ev = match event {
            StreamEvent::Connected(stream_id) => {
                ev.data(serde_json::json!({ "stream_id": stream_id }).to_string())
            }
            StreamEvent::Heartbeat => ev.data("heartbeat"),
            StreamEvent::Warning(msg) => ev.data(msg.as_str()),
            StreamEvent::Error(msg) => ev.data(msg.as_str()),
            StreamEvent::Terminated(reason) => ev.data(reason.to_string()),
            StreamEvent::Log(log) => {
                let data = serde_json::to_string(log)?;
                ev.id(log.sequence_id.to_string()).data(data)
            }
        };

        Ok(ev)
    }
}

// ── EnrichedLog ───────────────────────────────────────────────────────────────

/// An enriched log payload. `raw_data` holds the original NATS bytes; all
/// other fields are derived from the NATS metadata and subject.
///
/// Subject layout: `logs.<namespace>.<app>.<component>.<run_id>`
#[derive(Debug, Clone, Serialize)]
pub struct EnrichedLog {
    /// JetStream stream sequence number — doubles as the SSE `id:` field.
    pub sequence_id: u64,

    /// Wall-clock publish time as Unix milliseconds.
    pub timestamp_ms: i64,

    /// The `<namespace>` segment of the NATS subject.
    pub namespace: String,

    /// The `<app>` segment of the NATS subject.
    pub app: String,

    /// The `<component>` segment of the NATS subject.
    pub component: String,

    /// The `<run_id>` segment.
    pub run_id: String,

    /// The raw message payload decoded as UTF-8. Non-UTF-8 payloads are
    /// represented as `"<binary>"`.
    ///
    /// We store a `String` here (decoded once) rather than `Bytes` so that
    /// `serde_json::to_string(log)` works without a custom serializer.
    #[serde(rename = "message")]
    pub raw_data: String,
}

// ── StreamEndReason ───────────────────────────────────────────────────────────

/// Why a managed stream came to an end.
#[derive(Debug, Clone)]
pub enum StreamEndReason {
    /// The application published an explicit "end" signal on the log subject.
    ApplicationEnded,
    /// No messages arrived for the configured idle-timeout window.
    IdleTimeout,
    /// A caller invoked `ManagedStream::terminate()` explicitly.
    ForceStopped,
    /// The background NATS consumer encountered an unrecoverable error.
    NatsError(String),
}

/// Human-readable representation — used directly as the SSE data payload for
/// `Terminated` events. Implements `Display` so `.to_string()` just works.
impl fmt::Display for StreamEndReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ApplicationEnded => f.write_str("application_ended"),
            Self::IdleTimeout => f.write_str("idle_timeout"),
            Self::ForceStopped => f.write_str("force_stopped"),
            Self::NatsError(msg) => write!(f, "nats_error: {msg}"),
        }
    }
}
