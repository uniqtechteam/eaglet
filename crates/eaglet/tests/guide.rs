//! The code shown in `docs/guide.md`. Every Rust block in the guide and the README is copied from
//! this file, the other tests or `examples/shop`; `tests/docs.rs` checks that it still is.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use eaglet::axum::http::{HeaderValue, header};
use eaglet::axum::response::Response;
use eaglet::prelude::*;
use eaglet::testing::TestApp;
use serde::Deserialize;
use serde_json::{Value, json};
use validator::Validate;

// ------------------------------------------------------------------ providers

/// Sends mail. A trait, so tests can swap in a fake.
pub trait Mailer: Send + Sync {
    /// Send `text` to `to`.
    ///
    /// # Errors
    /// When the mail server refuses it.
    fn send(&self, to: &str, text: &str) -> Result<(), HttpError>;
}

/// Where mail comes from.
pub struct MailSettings {
    from: String,
}

#[injectable]
struct SmtpMailer {
    settings: Arc<MailSettings>,
}

impl Mailer for SmtpMailer {
    fn send(&self, to: &str, text: &str) -> Result<(), HttpError> {
        tracing::info!(from = %self.settings.from, to, "{text}");
        Ok(())
    }
}

#[module(
    providers = [
        ProviderDef::value(MailSettings { from: "shop@example.com".into() }),
        provide!(dyn Mailer => SmtpMailer),
    ],
    exports = [dyn Mailer]
)]
struct MailModule;

#[injectable]
struct Greeter {
    mailer: Arc<dyn Mailer>,
    #[inject(default)]
    sent: AtomicU64,
}

impl Greeter {
    fn welcome(&self, email: &str) -> Result<u64, HttpError> {
        self.mailer.send(email, "Welcome to the shop")?;
        Ok(self.sent.fetch_add(1, Ordering::Relaxed) + 1)
    }
}

/// A connection pool, made by an async function at startup.
struct Pool {
    size: u32,
}

#[allow(clippy::unused_async, reason = "stands in for a real connection")]
async fn connect(size: u32) -> Result<Pool, std::io::Error> {
    Ok(Pool { size })
}

#[module(
    providers = [ProviderDef::factory(|| async { connect(5).await })],
    exports = [Pool]
)]
struct DbModule;

#[module(providers = [Greeter])]
struct ForgotTheImport;

#[tokio::test]
async fn a_missing_import_stops_the_app_at_startup() {
    let err = App::new::<ForgotTheImport>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "Greeter needs dyn Mailer, which ForgotTheImport does not provide or import"
    );
}

#[tokio::test]
async fn values_factories_and_trait_providers_are_built_once() {
    let app = App::new::<AppModule>().await.unwrap();
    assert_eq!(app.provider::<Pool>().unwrap().size, 5);
    let greeter = app.provider::<Greeter>().unwrap();
    assert_eq!(greeter.welcome("ana@example.com").unwrap(), 1);
    assert_eq!(
        app.provider::<Greeter>()
            .unwrap()
            .welcome("bo@example.com")
            .unwrap(),
        2
    );
}

// ------------------------------------------------------------------ guards

/// A partner who calls the API with a key.
#[derive(Debug, Clone)]
pub struct Partner {
    name: String,
}

/// API keys, looked up per request.
pub struct ApiKeys(HashMap<String, Partner>);

impl ApiKeys {
    #[allow(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "stands in for a database lookup"
    )]
    async fn find(&self, key: &str) -> Option<Partner> {
        self.0.get(key).cloned()
    }
}

#[injectable]
struct ApiKeyGuard {
    keys: Arc<ApiKeys>,
}

impl Guard for ApiKeyGuard {
    async fn can_activate(&self, parts: &mut Parts) -> Result<(), HttpError> {
        let key = parts
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| HttpError::unauthorized("send an x-api-key header"))?;
        let partner = self
            .keys
            .find(key)
            .await
            .ok_or_else(|| HttpError::unauthorized("unknown API key"))?;
        parts.extensions.insert(partner);
        Ok(())
    }
}

// ------------------------------------------------------------------ a controller with all of it

/// `GET /reports/search?q=lamp`.
#[derive(Debug, Deserialize, Validate)]
struct Search {
    #[validate(length(min = 2, message = "type at least 2 characters"))]
    q: String,
}

async fn no_store(mut res: Response) -> Response {
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

#[injectable]
struct ReportsController {
    greeter: Arc<Greeter>,
}

#[controller("/reports")]
#[guard(ApiKeyGuard)]
#[layer(eaglet::axum::middleware::map_response(no_store))]
impl ReportsController {
    /// Search the reports.
    #[get("/search")]
    fn search(
        &self,
        Extension(partner): Extension<Partner>,
        Valid(Query(search)): Valid<Query<Search>>,
    ) -> String {
        format!("{} searched for {}", partner.name, search.q)
    }

    /// Send the partner a welcome mail.
    #[post("/welcome")]
    fn welcome(&self, Extension(partner): Extension<Partner>) -> Result<String, HttpError> {
        let count = self.greeter.welcome(&partner.name)?;
        Ok(format!("mail number {count}"))
    }

    /// Reports beyond the plan are refused with a code clients can branch on.
    #[get("/export")]
    fn export(&self) -> Result<String, HttpError> {
        let error = HttpError::new(
            StatusCode::PAYMENT_REQUIRED,
            "PLAN_LIMIT",
            "needs the team plan",
        );
        Err(error.with_details(json!({ "plan": "free", "needs": "team" })))
    }
}

fn keys() -> ApiKeys {
    let partner = Partner {
        name: "acme".into(),
    };
    ApiKeys(HashMap::from([("k1".to_owned(), partner)]))
}

#[module(
    imports = [MailModule, DbModule],
    controllers = [ReportsController],
    providers = [Greeter, ApiKeyGuard, ProviderDef::value(keys())]
)]
struct AppModule;

// ------------------------------------------------------------------ testing

/// Remembers the mail instead of sending it.
#[derive(Default)]
struct FakeMailer {
    sent: Mutex<Vec<String>>,
}

impl Mailer for FakeMailer {
    fn send(&self, to: &str, _text: &str) -> Result<(), HttpError> {
        self.sent.lock().unwrap().push(to.to_owned());
        Ok(())
    }
}

#[tokio::test]
async fn the_guard_runs_before_the_handler() {
    let app = TestApp::new::<AppModule>().build().await.unwrap();

    let res = app.get("/reports/search?q=lamp").send().await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(res.json::<Value>()["message"], "send an x-api-key header");

    let res = app
        .get("/reports/search?q=lamp")
        .header("x-api-key", "k1")
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.text(), "acme searched for lamp");
    assert_eq!(res.header("cache-control"), Some("no-store"));
}

#[tokio::test]
async fn a_short_query_is_a_validation_error() {
    let app = TestApp::new::<AppModule>().build().await.unwrap();
    let res = app
        .get("/reports/search?q=x")
        .header("x-api-key", "k1")
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = res.json::<Value>();
    assert_eq!(body["code"], "VALIDATION_FAILED");
    assert_eq!(
        body["details"]["fields"]["q"],
        json!(["type at least 2 characters"])
    );
    assert_eq!(body["traceId"], res.header("x-request-id").unwrap());
}

#[tokio::test]
async fn a_custom_error_keeps_its_code_and_details() {
    let app = TestApp::new::<AppModule>().build().await.unwrap();
    let res = app
        .get("/reports/export")
        .header("x-api-key", "k1")
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::PAYMENT_REQUIRED);
    let body = res.json::<Value>();
    assert_eq!(body["code"], "PLAN_LIMIT");
    assert_eq!(body["details"]["needs"], "team");
}

#[tokio::test]
async fn a_fake_mailer_replaces_the_real_one() {
    let mailer = Arc::new(FakeMailer::default());
    let app = TestApp::new::<AppModule>()
        .override_provider::<dyn Mailer>(mailer.clone())
        .build()
        .await
        .unwrap();

    let res = app
        .post("/reports/welcome")
        .header("x-api-key", "k1")
        .send()
        .await;

    assert_eq!(res.text(), "mail number 1");
    assert_eq!(*mailer.sent.lock().unwrap(), ["acme"]);
}

// ------------------------------------------------------------------ OpenAPI

#[tokio::test]
async fn the_document_lists_the_routes_with_their_summaries() {
    let app = App::new::<AppModule>()
        .await
        .unwrap()
        .with_info("Reports API", "1.2.0");
    let doc = serde_json::to_value(app.openapi()).unwrap();
    assert_eq!(doc["info"]["title"], "Reports API");
    assert_eq!(
        doc["paths"]["/reports/search"]["get"]["summary"],
        "Search the reports."
    );
}
