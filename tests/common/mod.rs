use axum::{body::Body, http::Request, response::Response, Router};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

pub fn valid_layout() -> &'static str {
    include_str!("../../test_data/valid_map.json")
}

pub fn invalid_layout() -> &'static str {
    include_str!("../../test_data/invalid_unknown_node_reference.json")
}

pub fn post_json(path: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

pub async fn post(app: Router, path: &str, body: &str) -> Response {
    app.oneshot(post_json(path, body)).await.unwrap()
}

pub async fn json(response: Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

