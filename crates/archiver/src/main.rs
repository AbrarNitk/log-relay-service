mod batch;
mod config;
mod ingest;

use config::ArchiverConfig;
use ingest::OpenObserveClient;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "archiver=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 2. Load config
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/home/ak/github/abrarnitk/logs-stream/config.json".to_string());

    info!("Loading config from: {}", config_path);
    let config = ArchiverConfig::load_from_file(&config_path);
    info!(?config, "Config loaded");

    // 3. Connect to NATS
    let mut nats_options = async_nats::ConnectOptions::new();
    if let (Some(user), Some(pass)) = (&config.nats.username, &config.nats.password) {
        nats_options = nats_options.user_and_password(user.clone(), pass.clone());
    }

    let nats_client = async_nats::connect_with_options(&config.nats.url, nats_options)
        .await
        .expect("Failed to connect to NATS");
    info!("Connected to NATS at {}", config.nats.url);

    // 4. Get JetStream context and bind/create durable consumer
    let js = async_nats::jetstream::new(nats_client);

    let stream = js
        .get_stream(&config.nats.stream_name)
        .await
        .expect("Failed to get NATS stream — is it created? Run start_nats.sh first.");

    info!(
        stream = %config.nats.stream_name,
        "Bound to NATS stream"
    );

    // Create or bind to a durable pull consumer named "archiver".
    let consumer = stream
        .get_or_create_consumer(
            "archiver",
            async_nats::jetstream::consumer::pull::Config {
                durable_name: Some("archiver".to_string()),
                filter_subject: "logs.>".to_string(),
                ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
                ..Default::default()
            },
        )
        .await
        .expect("Failed to create/bind archiver consumer");

    info!("Durable consumer 'archiver' ready");

    // 5. Initialize OpenObserve client
    let oo_client = OpenObserveClient::new(&config.openobserve);

    // 6. Run the batch loop (runs forever)
    info!("Starting archiver batch loop...");
    batch::run_batch_loop(consumer, &oo_client, &config.archiver).await?;

    Ok(())
}
