use axum::{
    Router,
    routing::{delete, get, post},
};
use log_relay::{
    context::Context,
    controller::handler::{
        logs_stream_pause, logs_stream_resume, logs_stream_start, logs_stream_status,
        logs_stream_terminate,
    },
};

pub fn create_router<S>(ctx: Context) -> Router<S> {
    Router::new()
        // ── Health ─────────────────────────────────────────────────────────
        .route("/health", get(|| async { "OK" }))
        // ── SSE stream creation  ───────────────────────────────────────────
        // GET /stream-logs/:run_id[?delivery_policy=…&replay_policy=…&…]
        // On connect, the first SSE `connected` event carries the stream_id.
        .route("/stream-logs/{run_id}", get(logs_stream_start))
        // ── Per-session status (lists all stream_ids for the run_id) ───────
        // GET /stream-logs/:run_id/status
        .route("/stream-logs/{run_id}/status", get(logs_stream_status))
        // ── Per-session control  (all addressed by stream_id) ─────────────
        // POST   /stream-logs/:run_id/:stream_id/pause
        .route(
            "/stream-logs/{run_id}/{stream_id}/pause",
            post(logs_stream_pause),
        )
        // POST   /stream-logs/:run_id/:stream_id/resume
        .route(
            "/stream-logs/{run_id}/{stream_id}/resume",
            post(logs_stream_resume),
        )
        // DELETE /stream-logs/:run_id/:stream_id
        .route(
            "/stream-logs/{run_id}/{stream_id}",
            delete(logs_stream_terminate),
        )
        .with_state(ctx)
}
