use clap::Parser;
use nats_utils::NatsConfig;
use std::time::Duration;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    run_id: String,

    #[arg(short, long, default_value = "default")]
    namespace: String,

    #[arg(short, long, default_value = "app")]
    application: String,

    #[arg(long, default_value = "worker")]
    component: String,

    #[arg(short, long, default_value_t = 10)]
    pressure: u64, // Logs per second

    #[arg(long, default_value = "nats_config.json")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    info!(
        "Starting log generator for run_id: {} (subject: logs.{}.{}.{}.{})",
        args.run_id, args.namespace, args.application, args.component, args.run_id
    );

    // Load config
    let config = NatsConfig::load_from_file(&args.config)?;
    let client = config.connect().await?;

    let subject = format!(
        "logs.{}.{}.{}.{}",
        args.namespace, args.application, args.component, args.run_id
    );
    let interval = Duration::from_micros(1_000_000 / args.pressure);

    let mut count: u64 = 0;
    loop {
        let log_msg = format!(
            "Log entry #{} for run {} [ns={}, app={}, comp={}]",
            count, args.run_id, args.namespace, args.application, args.component
        );

        match client.publish(subject.clone(), log_msg.into()).await {
            Ok(_) => info!("Published: {}", count),
            Err(e) => error!("Failed to publish: {}", e),
        }

        count += 1;
        tokio::time::sleep(interval).await;
    }
}
