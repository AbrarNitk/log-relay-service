use crate::controllers::{logs::stream_logs, stream_control::stop_stream};
use crate::state::AppState;
use axum::{
    Router,
    routing::{delete, get},
};

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/stream-logs/{run_id}", get(stream_logs))
        .route("/streams/{run_id}", delete(stop_stream))
}
