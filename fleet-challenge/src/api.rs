use crate::models::Layout;
use crate::routing::{plan_route, RouteError};
use crate::state::AppState;
use crate::validation::validate_layout;
use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
struct ValidationResult {
    valid: bool,
    errors: Vec<crate::validation::ValidationError>,
}

#[derive(Debug, Serialize)]
struct ApiError {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: ApiError,
}

fn error_response(
    status: StatusCode,
    code: &'static str,
    message: String,
) -> axum::response::Response {
    (
        status,
        Json(ErrorResponse {
            error: ApiError {
                code,
                message,
                details: None,
            },
        }),
    )
        .into_response()
}

fn json_error(rejection: JsonRejection) -> axum::response::Response {
    error_response(
        StatusCode::BAD_REQUEST,
        "malformed_json",
        rejection.body_text(),
    )
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
    payload: Result<Json<Layout>, JsonRejection>,
) -> impl IntoResponse {
    let Json(layout) = match payload {
        Ok(payload) => payload,
        Err(rejection) => return json_error(rejection),
    };

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
                .into_response()
        }
        Err(errors) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: ApiError {
                    code: "invalid_layout",
                    message: "layout validation failed".into(),
                    details: Some(serde_json::json!({ "violations": errors })),
                },
            }),
        )
            .into_response(),
    }
}
/// Represents a request to plan a route between two nodes in the graph.
#[derive(Debug, Deserialize)]
pub struct RouteRequest {
    start: String,
    goal: String,
}
/// POST /api/v1/route
/// example request body:
/// { "start": "node1", "goal": "node2" }
pub async fn route_handler(
    State(state): State<AppState>,
    payload: Result<Json<RouteRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Json(req) = match payload {
        Ok(payload) => payload,
        Err(rejection) => return json_error(rejection),
    };

    let guard = state.last_valid_graph.read().await;
    let graph = match guard.as_ref() {
        Some(graph) => graph,
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "no_valid_layout",
                "no valid layout has been submitted to /api/v1/layout/validate yet".into(),
            );
        }
    };

    match plan_route(graph, &req.start, &req.goal) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(err @ (RouteError::UnknownStart(_) | RouteError::UnknownGoal(_))) => {
            error_response(StatusCode::NOT_FOUND, err.code(), err.to_string())
        }
        Err(err @ RouteError::NoRoute(_, _)) => error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            err.code(),
            err.to_string(),
        ),
    }
}
