//! Both versions must answer every bench request the same way, or the numbers compare different work.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(router: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn get(uri: &str, token: Option<&str>) -> Request<Body> {
    let mut req = Request::get(uri);
    if let Some(token) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    req.body(Body::empty()).unwrap()
}

fn post(body: &Value) -> Request<Body> {
    Request::post("/users")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn routers() -> [Router; 2] {
    [
        eaglet_bench::plain::router(),
        eaglet_bench::eaglet_app::router().await.unwrap(),
    ]
}

#[tokio::test]
async fn get_user_returns_the_same_json() {
    for router in routers().await {
        let (status, body) = call(&router, get("/users/42", None)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({ "id": 42, "name": "user 42", "email": "user42@example.com" })
        );
    }
}

#[tokio::test]
async fn post_user_validates_then_creates() {
    for router in routers().await {
        let good = json!({ "name": "Ana", "email": "ana@example.com" });
        let (status, body) = call(&router, post(&good)).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(
            body,
            json!({ "id": 1, "name": "Ana", "email": "ana@example.com" })
        );

        let bad = json!({ "name": "Ana", "email": "not-an-email" });
        let (status, body) = call(&router, post(&bad)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["code"], "VALIDATION_FAILED");
    }
}

#[tokio::test]
async fn me_needs_the_token() {
    for router in routers().await {
        let (status, body) = call(&router, get("/me", None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["code"], "UNAUTHORIZED");

        let (status, body) = call(&router, get("/me", Some("bench-token"))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"], 7);
    }
}
