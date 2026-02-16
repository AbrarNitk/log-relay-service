use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Serialize;
use tracing::info;

#[derive(Serialize)]
struct StopStreamResponse {
    stopped: bool,
    run_id: String,
}

/// `DELETE /streams/{run_id}` — forcefully stop a stream.
pub async fn stop_stream(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    info!(run_id = %run_id, "Stop stream requested");
    let stopped = state.stream_manager.stop_stream(&run_id);

    let status = if stopped {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };

    (status, Json(StopStreamResponse { stopped, run_id }))
}
