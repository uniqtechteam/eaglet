//! SPEC-HTTP-001 FR-001: controllers and routes.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]
#![allow(
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    clippy::unused_self,
    reason = "test handlers are written the way real controllers are, async and on &self"
)]
#![allow(
    dead_code,
    reason = "some test providers exist only to be wired, not read"
)]

use eaglet::axum::body::Body;
use eaglet::axum::http::{Request, StatusCode};
use eaglet::prelude::*;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
struct User {
    id: u64,
    name: String,
}

#[injectable]
struct UsersService;

impl UsersService {
    fn find(&self, id: u64) -> User {
        User {
            id,
            name: format!("user {id}"),
        }
    }
}

#[injectable]
struct UsersController {
    users: Arc<UsersService>,
}

#[controller("/users")]
impl UsersController {
    #[get("/:id")]
    async fn find(&self, Path(id): Path<u64>) -> Json<User> {
        Json(self.users.find(id))
    }

    #[get("/")]
    async fn list(&self) -> Json<Vec<User>> {
        Json(vec![self.users.find(1), self.users.find(2)])
    }

    #[post("/")]
    async fn create(&self, Json(user): Json<User>) -> (StatusCode, Json<User>) {
        (StatusCode::CREATED, Json(user))
    }

    #[delete("/:id")]
    fn remove(&self, Path(_id): Path<u64>) -> StatusCode {
        StatusCode::NO_CONTENT
    }

    /// Not a route: plain methods stay plain.
    fn helper(&self) -> &'static str {
        "helper"
    }
}

#[injectable]
struct FilesController;

#[controller("/files")]
impl FilesController {
    #[get("/*path")]
    async fn file(&self, Path(path): Path<String>) -> String {
        path
    }
}

#[module(controllers = [UsersController], providers = [UsersService])]
struct UsersModule;

#[module(imports = [UsersModule], controllers = [FilesController])]
struct AppModule;

async fn call(
    app: eaglet::axum::Router,
    method: &str,
    uri: &str,
    body: Option<&str>,
) -> (StatusCode, String) {
    let mut req = Request::builder().method(method).uri(uri);
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let res = app
        .oneshot(req.body(Body::from(body.unwrap_or("").to_owned())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn router() -> eaglet::axum::Router {
    App::new::<AppModule>().await.unwrap().into_router()
}

#[tokio::test]
async fn routes_use_injected_providers_and_axum_extractors() {
    let (status, body) = call(router().await, "GET", "/users/7", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<User>(&body).unwrap(),
        User {
            id: 7,
            name: "user 7".into()
        }
    );
}

#[tokio::test]
async fn several_methods_share_a_path() {
    let app = router().await;
    let (status, body) = call(app.clone(), "GET", "/users", None).await;
    assert_eq!(
        (
            status,
            serde_json::from_str::<Vec<User>>(&body).unwrap().len()
        ),
        (StatusCode::OK, 2)
    );
    let (status, body) = call(
        app.clone(),
        "POST",
        "/users",
        Some(r#"{"id":3,"name":"new"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body.contains("\"new\""));
    let (status, _) = call(app, "DELETE", "/users/3", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn wildcards_and_controllers_from_imported_modules() {
    let (status, body) = call(router().await, "GET", "/files/a/b.txt", None).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "a/b.txt"));
}

#[tokio::test]
async fn unknown_routes_are_404() {
    let (status, _) = call(router().await, "GET", "/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[test]
fn plain_methods_stay_callable() {
    let c = UsersController {
        users: Arc::new(UsersService),
    };
    assert_eq!(c.helper(), "helper");
}

// a controller needs its dependencies to be visible like any provider

#[injectable]
struct LonelyController {
    users: Arc<UsersService>,
}

#[controller("/lonely")]
impl LonelyController {
    #[get("/")]
    async fn hello(&self) -> &'static str {
        "hi"
    }
}

#[module(controllers = [LonelyController])]
struct LonelyModule;

#[tokio::test]
async fn a_controller_with_a_missing_dependency_fails_at_startup() {
    let err = App::new::<LonelyModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "LonelyController needs UsersService, which LonelyModule does not provide or import"
    );
}
