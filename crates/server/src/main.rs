mod api;
mod controllers;
mod service;
mod state;

use crate::state::AppState;
use nats_utils::NatsConfig;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "logs_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 1. Connect to NATS
    // We assume the config file is at the root or we can load defaults.
    // Ideally, we check for a file or env vars.
    // For now, let's try to load "nats_config.json" or "../../nats_config.json"
    let config_path = if std::path::Path::new("nats_config.json").exists() {
        "nats_config.json"
    } else if std::path::Path::new("../../nats_config.json").exists() {
        "../../nats_config.json"
    } else {
        "nats_config.json" // Default to fail if not found
    };

    info!("Loading NATS config from: {}", config_path);

    // We can also manually build config if file doesn't exist to be robust,
    // but the requirement implies utilizing nats-utils.
    let nats_config = match NatsConfig::load_from_file(config_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                "Failed to load config file: {}. Using default local NATS connection.",
                e
            );
            NatsConfig {
                url: "nats://localhost:4222".to_string(),
                username: Some("nats".to_string()),
                password: Some("nats".to_string()),
            }
        }
    };

    let nats_client = nats_config
        .connect()
        .await
        .expect("Failed to connect to NATS");
    info!("Connected to NATS server at {}", nats_config.url);

    // 2. Initialize App State
    let app_state = AppState::new(nats_client);

    // 3. Setup Router
    // We merge the API router with the static file fallback
    let fallback_assets_dir = std::path::Path::new("frontend/dist");
    let crate_assets_dir = std::path::Path::new("../../frontend/dist");

    let final_dir = if fallback_assets_dir.exists() {
        fallback_assets_dir
    } else {
        crate_assets_dir
    };

    info!("Serving static assets from: {:?}", final_dir);

    let app = api::router::create_router()
        .with_state(app_state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .fallback_service(ServeDir::new(final_dir));

    // 4. Start Server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
