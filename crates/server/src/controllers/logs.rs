use crate::service::nats::LogsService;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    response::{
        IntoResponse,
        sse::{Event, Sse},
    },
};
use futures::StreamExt; // provides .map
use std::convert::Infallible;
use tracing::{error, info};

pub async fn stream_logs(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    info!("Starting log stream for run_id: {}", run_id);

    let logs_service = LogsService::new((*state.nats_client).clone());
    let stream_result = logs_service.stream_logs(run_id.clone()).await;

    let stream: std::pin::Pin<Box<dyn futures::Stream<Item = Result<Event, Infallible>> + Send>> =
        match stream_result {
            Ok(s) => {
                // Map the service stream to SSE events
                Box::pin(s.map(|msg_res| match msg_res {
                    Ok(log_line) => Ok(Event::default().data(log_line)),
                    Err(e) => {
                        error!("Stream processing error: {}", e);
                        Ok(Event::default().event("error").data(e.to_string()))
                    }
                }))
            }
            Err(e) => {
                error!("Failed to initialize log stream: {}", e);
                let error_event = Event::default()
                    .event("error")
                    .data(format!("Failed to connect to log stream: {}", e));

                // Create a stream that emits one error value and ends
                Box::pin(futures::stream::once(async move { Ok(error_event) }))
            }
        };

    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}
