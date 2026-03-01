pub mod events;
pub mod manager;
pub mod stream;

pub use events::{EnrichedLog, StreamEndReason, StreamEvent};
pub use manager::StreamManager;
pub use stream::ManagedStream;

use std::convert::Infallible;
use std::time::Duration;

use axum::response::{Sse, sse::Event};
use events::StreamEvent as SE;
use tokio::sync::broadcast::error::RecvError;

use crate::{context::Context, controller::types::StreamPolicies};

/// Create a new stream session for `run_id` and return it as a fully configured
/// Axum SSE response. The returned `stream_id` (ULID) is embedded in the first
/// `connected` SSE event so the browser can address pause/resume/terminate.
#[tracing::instrument(name = "service::create_stream", skip_all, fields(run_id))]
pub async fn create_stream(
    ctx: &Context,
    run_id: &str,
    policies: StreamPolicies,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>> + use<>> {
    let (stream_id, managed) = ctx.stream_manager.create(run_id, &policies).await;
    let mut rx = managed.subscribe();
    let keepalive_secs = ctx.config.relay.sse_keepalive_secs;

    // Clone the token BEFORE moving `managed` into the stream closure.
    // We need a DropGuard inside the stream so the pump is cancelled the
    // instant Axum drops this future (client disconnect, server shutdown, etc.).
    // We cannot rely on Arc<ManagedStream>::drop() because the pump task holds
    // its own Arc clone — refcount only goes 2→1, drop() never fires.
    let shutdown_token = managed.shutdown_token.clone();

    let event_stream = async_stream::stream! {
        // DropGuard cancels `shutdown_token` (and therefore the pump) the
        // moment this future is dropped — loop exit OR Axum dropping mid-await.
        let _pump_guard = shutdown_token.drop_guard();
        let _managed = managed;

        // Yield Connected immediately — before any recv() — so the browser
        // always receives the stream_id regardless of pump-task timing.
        let connected_ev = SE::Connected(stream_id);
        if let Ok(ev) = Event::try_from(&connected_ev) {
            yield Ok::<Event, Infallible>(ev);
        }

        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Skip duplicate Connected events emitted by the pump task.
                    if matches!(event, SE::Connected(_)) { continue; }
                    let is_terminal = event.is_terminal();
                    if let Ok(sse_event) = Event::try_from(&event) {
                        yield Ok::<Event, Infallible>(sse_event);
                    }
                    if is_terminal { break; }
                }
                Err(RecvError::Lagged(n)) => {
                    let warning = SE::Warning(format!("Slow consumer: skipped {n} messages"));
                    if let Ok(ev) = Event::try_from(&warning) {
                        yield Ok(ev);
                    }
                }
                Err(RecvError::Closed) => {
                    tracing::info!("SSE broadcast channel closed — pump has exited");
                    break;
                }
            }
        }
        // _pump_guard drops here (or earlier if Axum drops the future mid-await)
        // → shutdown_token cancelled → pump select! wakes → pump exits → its Arc drops.
    };

    Sse::new(event_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(keepalive_secs))
            .text("keep-alive"),
    )
}
