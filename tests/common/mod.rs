use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

pub async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    user: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri).header("x-user-id", user);
    let body = if let Some(v) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(v.to_string())
    } else {
        Body::empty()
    };
    app.clone().oneshot(builder.body(body).unwrap()).await.unwrap()
}

pub async fn post(app: &Router, uri: &str, user: &str, body: &Value) -> axum::response::Response {
    send(app, Method::POST, uri, user, Some(body.clone())).await
}

pub async fn get(app: &Router, uri: &str, user: &str) -> axum::response::Response {
    send(app, Method::GET, uri, user, None).await
}

pub async fn read_json(res: axum::response::Response) -> Value {
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    assert!(
        status == StatusCode::OK || status == StatusCode::CREATED,
        "expected 200/201, got {status}: {}",
        String::from_utf8_lossy(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}
