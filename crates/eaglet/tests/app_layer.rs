//! `App::layer`: a layer around every route, inside the error envelope.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]
#![allow(
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    clippy::unused_self,
    reason = "test handlers are written the way real controllers are, async and on &self"
)]

use eaglet::axum::body::Body;
use eaglet::axum::http::{HeaderValue, Request};
use eaglet::axum::middleware::{from_fn, map_response};
use eaglet::axum::response::{IntoResponse, Response};
use eaglet::prelude::*;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

#[injectable]
struct PingController;

#[controller("/ping")]
impl PingController {
    #[get("/")]
    async fn ping(&self) -> &'static str {
        "pong"
    }
}

#[module(controllers = [PingController])]
struct AppModule;

async fn nosniff(mut res: Response) -> Response {
    res.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    res
}

async fn send(app: eaglet::axum::Router, path: &str) -> (Response, Value) {
    let req = Request::get(path).body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (Response::from_parts(parts, Body::empty()), json)
}

#[tokio::test]
async fn a_layer_wraps_routes_and_the_not_found_answer() {
    let app = App::new::<AppModule>()
        .await
        .unwrap()
        .layer(map_response(nosniff))
        .into_router();
    for path in ["/ping", "/nope"] {
        let (res, _) = send(app.clone(), path).await;
        assert_eq!(res.headers()["x-content-type-options"], "nosniff", "{path}");
    }
}

#[tokio::test]
async fn an_error_from_a_layer_gets_the_envelope() {
    let app = App::new::<AppModule>()
        .await
        .unwrap()
        .layer(from_fn(
            |_req: eaglet::axum::extract::Request, _next: eaglet::axum::middleware::Next| async {
                StatusCode::REQUEST_TIMEOUT.into_response()
            },
        ))
        .into_router();
    let (res, body) = send(app, "/ping").await;
    assert_eq!(res.status(), StatusCode::REQUEST_TIMEOUT);
    assert_eq!(body["code"], "REQUEST_TIMEOUT");
    assert_eq!(
        body["traceId"].as_str().unwrap(),
        res.headers()["x-request-id"]
    );
}
