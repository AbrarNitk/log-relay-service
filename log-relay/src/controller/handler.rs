use axum::response::IntoResponse;

pub async fn logs_stream_start() -> axum::response::Response {
    "log-stream".into_response()
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
