mod routes;

use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // 1. Tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "service=debug,log_relay=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = log_relay::context::config::Config::load_from_file(
        "/home/ak/github/abrarnitk/logs-stream/config.json",
    );

    // 4. Create StreamManager
    // let stream_manager = StreamManager::new(nats_client, config.relay.clone());

    // 5. Build AppState
    let ctx = log_relay::context::Context::build(config).await;

    // 6. Setup Router with static file fallback
    let assets_dir = std::path::Path::new("frontend/dist");

    tracing::info!("Serving static assets from: {:?}", assets_dir);

    let app = crate::routes::create_router(ctx)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .fallback_service(ServeDir::new(assets_dir));

    // 7. Start Server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
