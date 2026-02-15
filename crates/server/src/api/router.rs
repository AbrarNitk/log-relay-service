use crate::controllers::logs::stream_logs;
use crate::state::AppState;
use axum::{Router, routing::get};
use std::sync::Arc;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/logs/{run_id}", get(stream_logs))
}
