use crate::config::BatchSettings;
use crate::ingest::OpenObserveClient;
use anyhow::Result;
use async_nats::jetstream::consumer::pull;
use futures::StreamExt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

/// Parsed subject metadata from `logs.<ns>.<app>.<component>.<run_id>`.
struct SubjectMeta {
    namespace: String,
    application: String,
    component: String,
    run_id: String,
}

/// Parse a NATS subject of the form `logs.<ns>.<app>.<component>.<run_id>`.
fn parse_subject(subject: &str) -> Option<SubjectMeta> {
    let parts: Vec<&str> = subject.splitn(5, '.').collect();
    if parts.len() == 5 && parts[0] == "logs" {
        Some(SubjectMeta {
            namespace: parts[1].to_string(),
            application: parts[2].to_string(),
            component: parts[3].to_string(),
            run_id: parts[4].to_string(),
        })
    } else {
        None
    }
}

fn now_epoch_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

/// Run the batch loop: pull messages → parse → enrich → POST → ack.
pub async fn run_batch_loop(
    consumer: async_nats::jetstream::consumer::Consumer<pull::Config>,
    oo_client: &OpenObserveClient,
    settings: &BatchSettings,
) -> Result<()> {
    let batch_timeout = Duration::from_secs(settings.batch_timeout_secs);

    info!(
        batch_size = settings.batch_size,
        batch_timeout_secs = settings.batch_timeout_secs,
        "Starting batch loop"
    );

    loop {
        // Pull a batch of messages from the consumer.
        let mut messages = consumer
            .fetch()
            .max_messages(settings.batch_size)
            .expires(batch_timeout)
            .messages()
            .await?;

        let mut records: Vec<serde_json::Value> = Vec::with_capacity(settings.batch_size);
        let mut pending_acks: Vec<async_nats::jetstream::Message> =
            Vec::with_capacity(settings.batch_size);

        while let Some(msg_result) = messages.next().await {
            match msg_result {
                Ok(msg) => {
                    let subject = msg.subject.as_str().to_string();
                    let payload = String::from_utf8_lossy(&msg.payload).to_string();

                    let record = if let Some(meta) = parse_subject(&subject) {
                        serde_json::json!({
                            "_timestamp": now_epoch_micros(),
                            "namespace": meta.namespace,
                            "application": meta.application,
                            "component": meta.component,
                            "run_id": meta.run_id,
                            "log": payload,
                        })
                    } else {
                        // Unknown subject format — still ingest with raw subject.
                        warn!(%subject, "Could not parse subject into 4-part hierarchy");
                        serde_json::json!({
                            "_timestamp": now_epoch_micros(),
                            "subject": subject,
                            "log": payload,
                        })
                    };

                    records.push(record);
                    pending_acks.push(msg);
                }
                Err(e) => {
                    error!("Error receiving message from consumer: {}", e);
                }
            }
        }

        if records.is_empty() {
            debug!("No messages in this batch cycle, continuing...");
            continue;
        }

        let count = records.len();
        info!(count, "Sending batch to OpenObserve");

        // POST to OpenObserve — retries with backoff internally.
        oo_client.send_batch(&records).await?;

        // Ack all messages only after successful ingestion.
        for msg in pending_acks {
            if let Err(e) = msg.ack().await {
                warn!("Failed to ack message: {}", e);
            }
        }

        info!(count, "Batch ingested and acknowledged successfully");
    }
}
