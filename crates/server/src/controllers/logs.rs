use crate::state::AppState;
use axum::{
    extract::{Path, State},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// SSE endpoint: `GET /stream-logs/{run_id}`
///
/// Returns a Server-Sent Events stream of log lines for the given `run_id`.
/// Multiple clients requesting the same `run_id` share a single NATS consumer.
pub async fn stream_logs(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    info!(run_id = %run_id, "Client requesting log stream");

    let shared = match state.stream_manager.get_or_create_stream(&run_id).await {
        Ok(s) => s,
        Err(e) => {
            error!(run_id = %run_id, "Failed to create stream: {:#}", e);
            let error_stream = async_stream::stream! {
                yield Ok::<_, Infallible>(
                    Event::default()
                        .event("error")
                        .data(format!("Failed to connect to log stream: {}", e))
                );
            };
            return Sse::new(error_stream)
                .keep_alive(
                    KeepAlive::new()
                        .interval(Duration::from_secs(state.config.relay.sse_keepalive_secs)),
                )
                .into_response();
        }
    };

    let mut rx = shared.sender.subscribe();
    let keepalive_secs = state.config.relay.sse_keepalive_secs;

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(bytes) => {
                    // Zero-copy path: convert Bytes → &str only at the SSE boundary.
                    let text = String::from_utf8_lossy(&bytes);
                    yield Ok::<_, Infallible>(Event::default().data(text.as_ref()));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(run_id = %run_id, skipped = n, "Client lagging behind");
                    yield Ok(Event::default()
                        .event("warning")
                        .data(format!("Skipped {} messages due to slow consumption", n)));
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!(run_id = %run_id, "Broadcast channel closed — ending SSE");
                    break;
                }
            }
        }
        // `shared` (Arc<SharedStream>) is dropped here when the stream ends.
        // If this was the last Arc, SharedStream::drop cancels the NATS pump.
        drop(shared);
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(keepalive_secs)))
        .into_response()
}
