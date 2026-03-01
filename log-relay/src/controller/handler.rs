use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};

use crate::context::Context;
use crate::controller::types::{StreamCreateQuery, StreamPolicies};

// ── SSE Stream Handler ────────────────────────────────────────────────────────

/// `GET /stream-logs/:run_id[?delivery_policy=…&replay_policy=…&…]`
///
/// Opens an SSE stream for `run_id`. The first `connected` event emits the
/// unique `stream_id` (ULID) the client must store for pause/resume/terminate.
#[tracing::instrument(name = "controller::logs_stream_start", fields(run_id), skip_all)]
pub async fn logs_stream_start(
    State(ctx): State<Context>,
    Path(run_id): Path<String>,
    Query(query): Query<StreamCreateQuery>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    let policies: StreamPolicies = query.try_into()?;
    let sse = crate::service::create_stream(&ctx, &run_id, policies).await;
    Ok(sse)
}

// ── Control Handlers (addressed by stream_id) ─────────────────────────────────

/// `POST /stream-logs/:run_id/:stream_id/pause`
#[tracing::instrument(name = "controller::logs_stream_pause", fields(stream_id), skip_all)]
pub async fn logs_stream_pause(
    State(ctx): State<Context>,
    Path((_run_id, stream_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match ctx.stream_manager.get(&stream_id) {
        Some(s) => {
            s.pause().await;
            StatusCode::NO_CONTENT
        }
        None => StatusCode::NOT_FOUND,
    }
}

/// `POST /stream-logs/:run_id/:stream_id/resume`
#[tracing::instrument(name = "controller::logs_stream_resume", fields(stream_id), skip_all)]
pub async fn logs_stream_resume(
    State(ctx): State<Context>,
    Path((_run_id, stream_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match ctx.stream_manager.get(&stream_id) {
        Some(s) => {
            s.resume().await;
            StatusCode::NO_CONTENT
        }
        None => StatusCode::NOT_FOUND,
    }
}

/// `DELETE /stream-logs/:run_id/:stream_id`
#[tracing::instrument(
    name = "controller::logs_stream_terminate",
    fields(stream_id),
    skip_all
)]
pub async fn logs_stream_terminate(
    State(ctx): State<Context>,
    Path((_run_id, stream_id)): Path<(String, String)>,
) -> impl IntoResponse {
    ctx.stream_manager.terminate(&stream_id).await;
    StatusCode::NO_CONTENT
}

/// `GET /stream-logs/:run_id/status`
///
/// Returns the number of active sessions and their stream_ids for `run_id`.
#[tracing::instrument(name = "controller::logs_stream_status", fields(run_id), skip_all)]
pub async fn logs_stream_status(
    State(ctx): State<Context>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    let sessions = ctx.stream_manager.sessions_for(&run_id);
    let body = serde_json::json!({
        "run_id": run_id,
        "session_count": sessions.len(),
        "stream_ids": sessions,
    });
    (StatusCode::OK, axum::Json(body)).into_response()
}
