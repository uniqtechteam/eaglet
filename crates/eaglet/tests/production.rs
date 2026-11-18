//! The production setup from the guide: a timeout, a security header and CORS around every route.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]
#![allow(
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    clippy::unused_self,
    reason = "test handlers are written the way real controllers are, async and on &self"
)]

use std::time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;

use eaglet::axum::body::Body;
use eaglet::axum::http::{HeaderValue, Request, header};
use eaglet::axum::response::Response;
use eaglet::prelude::*;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

#[injectable]
struct PingController;

#[controller("/")]
impl PingController {
    #[get("/ping")]
    async fn ping(&self) -> &'static str {
        "pong"
    }

    #[get("/slow")]
    async fn slow(&self) -> &'static str {
        tokio::time::sleep(Duration::from_secs(5)).await;
        "late"
    }
}

#[module(controllers = [PingController])]
struct AppModule;

async fn production_app() -> Result<App, Box<dyn std::error::Error>> {
    let app = App::new::<AppModule>()
        .await?
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(15),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(CorsLayer::new().allow_origin(["https://app.example.com".parse()?]));
    Ok(app)
}

async fn get(app: App, path: &str) -> (Response, Value) {
    let req = Request::get(path)
        .header(header::ORIGIN, "https://app.example.com")
        .body(Body::empty())
        .unwrap();
    let res = app.into_router().oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (Response::from_parts(parts, Body::empty()), json)
}

#[tokio::test]
async fn every_route_gets_the_security_header_and_cors() {
    for path in ["/ping", "/nope"] {
        let (res, _) = get(production_app().await.unwrap(), path).await;
        let headers = res.headers();
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff", "{path}");
        assert_eq!(
            headers[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "https://app.example.com",
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_timeout_is_a_408_in_the_envelope() {
    let app = App::new::<AppModule>()
        .await
        .unwrap()
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_millis(50),
        ));
    let (res, body) = get(app, "/slow").await;
    assert_eq!(res.status(), StatusCode::REQUEST_TIMEOUT);
    assert_eq!(body["code"], "REQUEST_TIMEOUT");
    assert!(body["traceId"].is_string());
}
