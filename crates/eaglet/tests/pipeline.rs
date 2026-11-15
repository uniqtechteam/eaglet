//! SPEC-HTTP-001 FR-002..005: guards, validation, the error envelope and layers.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]
#![allow(
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    clippy::unused_self,
    reason = "test handlers are written the way real controllers are, async and on &self"
)]

use std::sync::atomic::{AtomicUsize, Ordering};

use eaglet::axum::body::Body;
use eaglet::axum::http::{HeaderValue, Request, header};
use eaglet::axum::response::Response;
use eaglet::prelude::*;
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::{Value, json};
use tower::ServiceExt;
use validator::Validate;

// ------------------------------------------------------------------ the app under test

#[derive(Clone, Debug)]
struct User {
    name: String,
}

/// Lets a request through when it has `Authorization: Bearer good`, and tells the handler who it is.
#[injectable]
struct AuthGuard;

impl Guard for AuthGuard {
    async fn can_activate(&self, parts: &mut Parts) -> Result<(), HttpError> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        if token != Some("Bearer good") {
            return Err(HttpError::unauthorized("sign in first"));
        }
        parts.extensions.insert(User { name: "ana".into() });
        Ok(())
    }
}

#[injectable]
struct AdminGuard;

impl Guard for AdminGuard {
    async fn can_activate(&self, _parts: &mut Parts) -> Result<(), HttpError> {
        Err(HttpError::forbidden("admins only"))
    }
}

#[injectable]
struct Calls {
    #[inject(default)]
    count: AtomicUsize,
}

#[injectable]
struct AccountController {
    calls: Arc<Calls>,
}

async fn tag(mut res: Response) -> Response {
    res.headers_mut()
        .insert("x-layer", HeaderValue::from_static("account"));
    res
}

async fn tag_route(mut res: Response) -> Response {
    res.headers_mut()
        .insert("x-route-layer", HeaderValue::from_static("me"));
    res
}

#[controller("/account")]
#[guard(AuthGuard)]
#[layer(eaglet::axum::middleware::map_response(tag))]
impl AccountController {
    #[get("/me")]
    #[layer(eaglet::axum::middleware::map_response(tag_route))]
    async fn me(&self, Extension(user): Extension<User>) -> String {
        self.calls.count.fetch_add(1, Ordering::Relaxed);
        user.name
    }

    #[delete("/")]
    #[guard(AdminGuard)]
    async fn close(&self) -> StatusCode {
        self.calls.count.fetch_add(1, Ordering::Relaxed);
        StatusCode::NO_CONTENT
    }
}

#[derive(Deserialize, Validate)]
struct SignUp {
    #[validate(email)]
    email: String,
    #[validate(length(min = 8, message = "use at least 8 characters"))]
    password: String,
}

#[derive(Deserialize, Validate)]
struct Search {
    #[validate(range(min = 1, max = 100))]
    limit: u32,
}

#[injectable]
struct PublicController;

#[controller("/")]
impl PublicController {
    #[post("/sign-up")]
    async fn sign_up(&self, Valid(Json(body)): Valid<Json<SignUp>>) -> String {
        body.email
    }

    #[get("/search")]
    async fn search(&self, Valid(Query(q)): Valid<Query<Search>>) -> String {
        q.limit.to_string()
    }

    #[get("/items/:id")]
    async fn item(&self, Path(id): Path<u64>) -> Result<String, HttpError> {
        if id == 7 {
            Ok("seven".into())
        } else {
            Err(HttpError::not_found("no item with that id"))
        }
    }

    #[get("/boom")]
    async fn boom(&self) -> Result<String, HttpError> {
        Err(HttpError::internal("database password is hunter2"))
    }

    #[get("/trace")]
    async fn trace(&self) -> String {
        eaglet::trace_id().unwrap_or_default()
    }
}

#[module(
    controllers = [AccountController, PublicController],
    providers = [AuthGuard, AdminGuard, Calls]
)]
struct AppModule;

// ------------------------------------------------------------------ helpers

struct Reply {
    status: StatusCode,
    headers: eaglet::axum::http::HeaderMap,
    text: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap()
    }
}

async fn call(app: eaglet::axum::Router, req: Request<Body>) -> Reply {
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        text: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

fn post_json(uri: &str, body: &str) -> Request<Body> {
    Request::post(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn app() -> (eaglet::axum::Router, Arc<Calls>) {
    let app = App::new::<AppModule>().await.unwrap();
    let calls = app.provider::<Calls>().unwrap();
    (app.into_router(), calls)
}

// ------------------------------------------------------------------ guards (FR-002)

#[tokio::test]
async fn ac_001_a_guarded_route_without_a_token_is_401_and_the_handler_never_runs() {
    let (app, calls) = app().await;
    let reply = call(app, get("/account/me")).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    let body = reply.json();
    assert_eq!(body["code"], "UNAUTHORIZED");
    assert_eq!(body["message"], "sign in first");
    assert_eq!(calls.count.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn a_guard_passes_values_to_the_handler() {
    let (app, calls) = app().await;
    let req = Request::get("/account/me")
        .header(header::AUTHORIZATION, "Bearer good")
        .body(Body::empty())
        .unwrap();
    let reply = call(app, req).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.text, "ana");
    assert_eq!(calls.count.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn controller_guards_run_before_route_guards() {
    let (app, calls) = app().await;
    let without_token = call(
        app.clone(),
        Request::delete("/account").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(without_token.json()["code"], "UNAUTHORIZED");
    let req = Request::delete("/account")
        .header(header::AUTHORIZATION, "Bearer good")
        .body(Body::empty())
        .unwrap();
    let reply = call(app, req).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert_eq!(reply.json()["code"], "FORBIDDEN");
    assert_eq!(calls.count.load(Ordering::Relaxed), 0);
}

#[injectable]
struct LonelyController;

#[controller("/lonely")]
#[guard(AuthGuard)]
impl LonelyController {
    #[get("/")]
    async fn hi(&self) -> &'static str {
        "hi"
    }
}

#[module(controllers = [LonelyController])]
struct MissingGuardModule;

#[tokio::test]
async fn a_guard_that_is_not_provided_fails_at_startup() {
    let err = App::new::<MissingGuardModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "LonelyController needs AuthGuard, which MissingGuardModule does not provide or import"
    );
}

// ------------------------------------------------------------------ validation (FR-003)

#[tokio::test]
async fn ac_002_a_bad_email_is_422_and_lists_the_field() {
    let (app, _) = app().await;
    let reply = call(
        app,
        post_json("/sign-up", r#"{"email":"not-an-email","password":"short"}"#),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    let body = reply.json();
    assert_eq!(body["code"], "VALIDATION_FAILED");
    assert_eq!(
        body["details"],
        json!({ "fields": { "email": ["email"], "password": ["use at least 8 characters"] } })
    );
    assert_eq!(
        body["message"],
        "some fields are not valid: email, password"
    );
}

#[tokio::test]
async fn a_valid_body_reaches_the_handler() {
    let (app, _) = app().await;
    let reply = call(
        app,
        post_json(
            "/sign-up",
            r#"{"email":"ana@example.com","password":"long enough"}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.text, "ana@example.com");
}

#[tokio::test]
async fn broken_json_is_400_in_the_envelope() {
    let (app, _) = app().await;
    let reply = call(app, post_json("/sign-up", "{not json")).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(reply.json()["code"], "BAD_REQUEST");
}

#[tokio::test]
async fn query_strings_are_validated_too() {
    let (app, _) = app().await;
    let ok = call(app.clone(), get("/search?limit=5")).await;
    assert_eq!(ok.text, "5");
    let bad = call(app, get("/search?limit=500")).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        bad.json()["details"],
        json!({ "fields": { "limit": ["range"] } })
    );
}

// ------------------------------------------------------------------ the envelope (FR-004)

#[tokio::test]
async fn every_error_has_the_envelope_and_a_trace_id() {
    let (app, _) = app().await;
    let reply = call(app, get("/items/1")).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    let body = reply.json();
    assert_eq!(body["code"], "NOT_FOUND");
    assert_eq!(body["message"], "no item with that id");
    assert_eq!(body["details"], Value::Null);
    let trace = body["traceId"].as_str().unwrap();
    assert_eq!(trace.len(), 16);
    assert_eq!(reply.headers["x-request-id"], trace);
}

#[tokio::test]
async fn an_incoming_request_id_is_kept() {
    let (app, _) = app().await;
    let req = Request::get("/trace")
        .header("x-request-id", "edge-42")
        .body(Body::empty())
        .unwrap();
    let reply = call(app, req).await;
    assert_eq!(reply.text, "edge-42");
    assert_eq!(reply.headers["x-request-id"], "edge-42");
}

#[tokio::test]
async fn a_strange_request_id_is_replaced() {
    let (app, _) = app().await;
    let req = Request::get("/trace")
        .header("x-request-id", "<script>")
        .body(Body::empty())
        .unwrap();
    let reply = call(app, req).await;
    assert_eq!(reply.text.len(), 16);
}

#[tokio::test]
async fn internal_errors_hide_the_cause() {
    let (app, _) = app().await;
    let reply = call(app, get("/boom")).await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reply.json()["code"], "INTERNAL_ERROR");
    assert!(!reply.text.contains("hunter2"));
}

#[tokio::test]
async fn axum_rejections_and_unknown_routes_use_the_envelope() {
    let (app, _) = app().await;
    let bad_path = call(app.clone(), get("/items/abc")).await;
    assert_eq!(bad_path.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad_path.json()["code"], "BAD_REQUEST");

    let missing = call(app.clone(), get("/nowhere")).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert_eq!(missing.json()["code"], "NOT_FOUND");

    let wrong_method = call(app, Request::put("/items/7").body(Body::empty()).unwrap()).await;
    assert_eq!(wrong_method.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(wrong_method.json()["code"], "METHOD_NOT_ALLOWED");
    assert!(wrong_method.headers.contains_key(header::ALLOW));
}

// ------------------------------------------------------------------ layers (FR-005)

#[tokio::test]
async fn layers_wrap_the_controller_and_the_route() {
    let (app, _) = app().await;
    let req = Request::get("/account/me")
        .header(header::AUTHORIZATION, "Bearer good")
        .body(Body::empty())
        .unwrap();
    let reply = call(app.clone(), req).await;
    assert_eq!(reply.headers["x-layer"], "account");
    assert_eq!(reply.headers["x-route-layer"], "me");

    // controller layers run outside its guards, so even a rejected request passes through them
    let rejected = call(app, get("/account/me")).await;
    assert_eq!(rejected.headers["x-layer"], "account");
    assert!(!rejected.headers.contains_key("x-route-layer"));
}
