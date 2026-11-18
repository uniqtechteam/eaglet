//! A handler that panics answers 500 in the envelope and leaks nothing about the panic.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]
#![allow(
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    clippy::unused_self,
    clippy::panic,
    reason = "test handlers are written the way real controllers are, and this one panics"
)]

use eaglet::prelude::*;
use eaglet::testing::TestApp;
use serde_json::Value;

#[injectable]
struct BoomController;

#[controller("/boom")]
impl BoomController {
    #[get("/")]
    async fn boom(&self) -> &'static str {
        panic!("the database password is hunter2")
    }

    #[get("/after-await")]
    async fn after_await(&self) -> &'static str {
        tokio::task::yield_now().await;
        panic!("the database password is hunter2")
    }
}

#[module(controllers = [BoomController])]
struct AppModule;

#[tokio::test]
async fn a_panic_is_a_500_in_the_envelope() {
    let app = TestApp::new::<AppModule>().build().await.unwrap();
    for path in ["/boom", "/boom/after-await"] {
        let res = app.get(path).send().await;
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR, "{path}");
        assert!(!res.text().contains("hunter2"), "{path}");
        let body: Value = res.json();
        assert_eq!(body["code"], "INTERNAL_ERROR");
        assert_eq!(body["message"], "Something went wrong on our side");
        assert_eq!(
            body["traceId"].as_str(),
            res.header("x-request-id"),
            "{path}"
        );
    }
}
