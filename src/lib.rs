pub mod api;
pub mod graph;
pub mod models;
pub mod routing;
pub mod state;
pub mod validation;

use api::{health, root, route_handler, validate_handler};
use axum::{
    routing::{get, post},
    Router,
};
use state::AppState;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

const MAX_REQUEST_BODY_BYTES: usize = 1_048_576;

pub fn app() -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/api/v1/layout/validate", post(validate_handler))
        .route("/api/v1/route", post(route_handler))
        .layer(TraceLayer::new_for_http())
        // Permissive CORS is useful for local development; production should restrict origins.
        .layer(CorsLayer::permissive())
        .layer(RequestBodyLimitLayer::new(MAX_REQUEST_BODY_BYTES))
        .with_state(AppState::new())
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fleet_challenge=info,tower_http=info".into()),
        )
        .init();

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    let bind_address = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&bind_address).await?;
    tracing::info!("fleet-challenge listening on http://127.0.0.1:{port}");
    axum::serve(listener, app()).await?;
    Ok(())
}
