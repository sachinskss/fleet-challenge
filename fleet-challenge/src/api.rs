use crate::models::Layout;
use crate::routing::plan_route;
use crate::state::AppState;
use crate::validation::validate_layout;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct ValidationResult {
    valid: bool,
    errors: Vec<crate::validation::ValidationError>,
}

pub async fn health() -> &'static str {
    "ok"
}
pub async fn root() -> impl IntoResponse {
    Json(serde_json::json!({
        "service": "fleet-challenge",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": [
            "GET /health",
            "POST /api/v1/layout/validate",
            "POST /api/v1/route"
        ]
    }))
}

/// POST /api/v1/layout/validate
/// example request body:
/// {   "id": "layout1", "nodes": [...], "edges": [...] }
pub async fn validate_handler(
    State(state): State<AppState>,
    Json(layout): Json<Layout>,
) -> impl IntoResponse {
    match validate_layout(&layout) {
        Ok(graph) => {
            let mut guard = state.last_valid_graph.write().await;
            *guard = Some(graph);
            (
                StatusCode::OK,
                Json(ValidationResult {
                    valid: true,
                    errors: Vec::new(),
                }),
            )
        }
        Err(errors) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ValidationResult {
                valid: false,
                errors,
            }),
        ),
    }
}
/// Represents a request to plan a route between two nodes in the graph.
#[derive(Debug, Deserialize)]
pub struct RouteRequest {
    start: String,
    goal: String,
}
/// Represents the result of a route planning operation, including the total distance and the sequence of nodes and edges in the path.
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// POST /api/v1/route
/// example request body:
/// { "start": "node1", "goal": "node2" }
pub async fn route_handler(
    State(state): State<AppState>,
    Json(req): Json<RouteRequest>,
) -> impl IntoResponse {
    let guard = state.last_valid_graph.read().await;
    let graph = match guard.as_ref() {
        Some(graph) => graph,
        None => {
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "no valid layout has been submitted to /api/v1/layout/validate yet"
                        .to_string(),
                }),
            )
                .into_response();
        }
    };

    match plan_route(graph, &req.start, &req.goal) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(err) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: err.to_string(),
            }),
        )
            .into_response(),
    }
}
