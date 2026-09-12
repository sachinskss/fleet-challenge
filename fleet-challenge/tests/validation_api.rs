mod common;

use axum::http::StatusCode;
use common::{invalid_layout, json, post, valid_layout};
use fleet_challenge::app;

#[tokio::test]
async fn valid_layout_returns_ok() {
    let response = post(app(), "/api/v1/layout/validate", valid_layout()).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["valid"], true);
    assert_eq!(body["errors"], serde_json::json!([]));
}

#[tokio::test]
async fn invalid_layout_returns_unprocessable_entity() {
    let response = post(app(), "/api/v1/layout/validate", invalid_layout()).await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json(response).await;
    assert_eq!(body["valid"], false);
    assert!(body["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|error| error["rule"] == "edge_references_existing_node"));
}
