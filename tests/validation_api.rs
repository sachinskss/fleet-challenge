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
    assert_eq!(body["error"]["code"], "invalid_layout");
    assert!(body["error"]["details"]["violations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|error| error["rule"] == "edge_references_existing_node"));
}

#[tokio::test]
async fn malformed_json_returns_structured_bad_request() {
    let response = post(app(), "/api/v1/layout/validate", "{not-json").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "malformed_json");
}
