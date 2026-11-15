//! SPEC-TEST-001: `TestApp`, provider overrides and the in-memory client.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]
#![allow(
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    reason = "test handlers are written the way real controllers are"
)]

use std::collections::HashMap;
use std::time::Instant;

use eaglet::prelude::*;
use eaglet::testing::TestApp;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
}

trait UsersRepo: Send + Sync {
    fn find(&self, id: u64) -> Option<User>;
}

/// The "real" repository, which would talk to a database.
#[injectable]
struct PgUsersRepo;

impl UsersRepo for PgUsersRepo {
    fn find(&self, _id: u64) -> Option<User> {
        None
    }
}

struct FakeUsersRepo(HashMap<u64, User>);

impl FakeUsersRepo {
    fn with_one_user() -> Self {
        let ana = User {
            id: 1,
            name: "Ana".into(),
        };
        Self(HashMap::from([(1, ana)]))
    }
}

impl UsersRepo for FakeUsersRepo {
    fn find(&self, id: u64) -> Option<User> {
        self.0.get(&id).cloned()
    }
}

#[derive(Debug)]
struct Greeting(String);

#[injectable]
struct UsersController {
    repo: Arc<dyn UsersRepo>,
    greeting: Arc<Greeting>,
}

#[controller("/users")]
impl UsersController {
    #[get("/:id")]
    async fn find(&self, Path(id): Path<u64>) -> Result<Json<User>, HttpError> {
        self.repo
            .find(id)
            .map(Json)
            .ok_or_else(|| HttpError::not_found("no user with that id"))
    }

    #[post("/")]
    async fn create(&self, Json(user): Json<User>) -> (StatusCode, Json<User>) {
        (StatusCode::CREATED, Json(user))
    }

    #[get("/hello")]
    async fn hello(&self) -> String {
        self.greeting.0.clone()
    }
}

#[module(
    controllers = [UsersController],
    providers = [provide!(dyn UsersRepo => PgUsersRepo), ProviderDef::value(Greeting("hi".into()))]
)]
struct AppModule;

async fn app_with_fake() -> TestApp {
    TestApp::new::<AppModule>()
        .override_provider::<dyn UsersRepo>(Arc::new(FakeUsersRepo::with_one_user()))
        .build()
        .await
        .unwrap()
}

#[tokio::test]
async fn ac_001_an_overridden_repo_serves_its_one_user() {
    let app = app_with_fake().await;
    let res = app.get("/users/1").send().await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.json::<User>(),
        User {
            id: 1,
            name: "Ana".into()
        }
    );
    let missing = app.get("/users/2").send().await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing.json::<serde_json::Value>()["code"], "NOT_FOUND");
}

#[tokio::test]
async fn without_the_override_the_real_provider_is_used() {
    let app = TestApp::new::<AppModule>().build().await.unwrap();
    assert_eq!(
        app.get("/users/1").send().await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn json_bodies_headers_and_concrete_overrides() {
    let app = TestApp::new::<AppModule>()
        .override_value(Greeting("hello from the test".into()))
        .build()
        .await
        .unwrap();
    let user = User {
        id: 9,
        name: "Bo".into(),
    };
    let res = app
        .post("/users")
        .header("x-request-id", "test-1")
        .json(&user)
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(res.json::<User>(), user);
    assert_eq!(res.header("x-request-id"), Some("test-1"));
    assert_eq!(
        app.get("/users/hello").send().await.text(),
        "hello from the test"
    );
    assert_eq!(app.provider::<Greeting>().unwrap().0, "hello from the test");
}

#[tokio::test]
async fn ac_002_a_hundred_requests_take_well_under_a_second() {
    let app = app_with_fake().await;
    let start = Instant::now();
    for _ in 0..100 {
        assert_eq!(app.get("/users/1").send().await.status(), StatusCode::OK);
    }
    assert!(start.elapsed().as_millis() < 1000);
}
