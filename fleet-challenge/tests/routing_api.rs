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
