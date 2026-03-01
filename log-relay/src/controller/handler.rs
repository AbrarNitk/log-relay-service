use crate::context::Context;

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
};

use crate::controller::types::{StreamCreateQuery, StreamPolicies};
use axum::http::StatusCode;

#[tracing::instrument(name = "controller::logs-stream-start", fields(run_id,), skip_all)]
pub async fn logs_stream_start(
    State(ctx): State<Context>,
    Path(run_id): Path<String>,
    Query(query): Query<StreamCreateQuery>,
) -> Result<axum::response::Response, (StatusCode, &'static str)> {
    let policies: StreamPolicies = query.try_into()?;
    Ok(crate::service::create_stream(&ctx, &run_id, &policies).await)
}

pub async fn logs_stream_pause() -> axum::response::Response {
    "log-stream".into_response()
}

pub async fn logs_stream_resume() -> axum::response::Response {
    "log-stream".into_response()
}

pub async fn logs_stream_terminate() -> axum::response::Response {
    "log-stream".into_response()
}

pub async fn logs_stream_status() -> axum::response::Response {
    "log-stream".into_response()
}
