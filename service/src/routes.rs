use axum::{
    Router,
    routing::{delete, get},
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
        .route("/health", get(|| async { "OK" }))
        .route("/stream-logs/{run_id}", get(logs_stream_start))
        .route("/streams/{run_id}", delete(logs_stream_terminate))
        .with_state(ctx)
}
