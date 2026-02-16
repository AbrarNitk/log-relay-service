mod api;
mod config;
mod controllers;
mod service;
mod state;

use crate::config::Config;
use crate::service::stream_manager::StreamManager;
use crate::state::AppState;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // 1. Tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "logs_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::load_from_file("/home/ak/github/abrarnitk/logs-stream/config.json");
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

    // 4. Create StreamManager
    let stream_manager = StreamManager::new(nats_client, config.relay.clone());

    // 5. Build AppState
    let app_state = AppState::new(stream_manager, config);

    // 6. Setup Router with static file fallback
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

    // 7. Start Server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
