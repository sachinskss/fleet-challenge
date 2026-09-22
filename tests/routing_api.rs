mod common;

use axum::http::StatusCode;
use common::{invalid_layout, json, post, valid_layout};
use fleet_challenge::app;

async fn submit_valid_layout() -> axum::Router {
    let app = app();
    let response = post(app.clone(), "/api/v1/layout/validate", valid_layout()).await;
    assert_eq!(response.status(), StatusCode::OK);
    app
}

#[tokio::test]
async fn valid_layout_followed_by_route_returns_expected_path() {
    let app = submit_valid_layout().await;
    let response = post(
        app,
        "/api/v1/route",
        r#"{"start":"Node_BR","goal":"Node_BC"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(
        body["nodes"],
        serde_json::json!(["Node_BR", "Node_TR", "Node_BC"])
    );
    assert_eq!(body["edges"], serde_json::json!(["BR_2_TR", "TR_2_BC"]));
}

#[tokio::test]
async fn invalid_layout_does_not_replace_last_valid_layout() {
    let app = submit_valid_layout().await;
    let response = post(app.clone(), "/api/v1/layout/validate", invalid_layout()).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let response = post(
        app,
        "/api/v1/route",
        r#"{"start":"Node_BL","goal":"Node_BC"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["nodes"], serde_json::json!(["Node_BL", "Node_BC"]));
    assert_eq!(body["edges"], serde_json::json!(["BL_2_BC"]));
}

#[tokio::test]
async fn route_respects_edge_direction() {
    let app = submit_valid_layout().await;
    let response = post(
        app,
        "/api/v1/route",
        r#"{"start":"Node_BR","goal":"Node_BC"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(
        body["nodes"],
        serde_json::json!(["Node_BR", "Node_TR", "Node_BC"])
    );
    assert_eq!(body["edges"], serde_json::json!(["BR_2_TR", "TR_2_BC"]));
}

#[tokio::test]
async fn route_before_valid_layout_returns_service_unavailable() {
    let response = post(
        app(),
        "/api/v1/route",
        r#"{"start":"Node_BR","goal":"Node_BC"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "no_valid_layout");
}

#[tokio::test]
async fn unknown_node_returns_not_found_with_error_code() {
    let app = submit_valid_layout().await;
    let response = post(
        app,
        "/api/v1/route",
        r#"{"start":"Node_GHOST","goal":"Node_BC"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "unknown_start_node");
}

#[tokio::test]
async fn unknown_goal_returns_not_found_with_error_code() {
    let app = submit_valid_layout().await;
    let response = post(
        app,
        "/api/v1/route",
        r#"{"start":"Node_BC","goal":"Node_GHOST"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "unknown_goal_node");
}

#[tokio::test]
async fn malformed_route_json_returns_structured_bad_request() {
    let response = post(app(), "/api/v1/route", "{not-json").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "malformed_json");
}
