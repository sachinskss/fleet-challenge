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
use tower_http::trace::TraceLayer;

pub fn app() -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/api/v1/layout/validate", post(validate_handler))
        .route("/api/v1/route", post(route_handler))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(AppState::new())
}

pub async fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fleet_challenge=info,tower_http=info".into()),
        )
        .init();

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("fleet-challenge listening on http://0.0.0.0:8080");
    axum::serve(listener, app()).await.unwrap();
}
