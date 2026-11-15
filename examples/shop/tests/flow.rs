//! The whole shop against a real SQLite database in memory: sign up, sign in, sell, browse, and
//! the ownership rules.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]

use eaglet::prelude::*;
use eaglet::testing::TestApp;
use serde_json::{Value, json};
use shop::app_module::AppModule;
use shop::config::Config;

async fn app() -> TestApp {
    let pool = shop::db::connect("sqlite::memory:").await.unwrap();
    TestApp::new::<AppModule>()
        .override_value(pool)
        .override_value(Config {
            port: 0,
            database_url: "sqlite::memory:".into(),
            jwt_secret: "test-secret".into(),
        })
        .build()
        .await
        .unwrap()
}

async fn sign_up(app: &TestApp, email: &str) -> String {
    let res = app
        .post("/auth/sign-up")
        .json(&json!({ "email": email, "password": "correct horse" }))
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "{}", res.text());
    res.json::<Value>()["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn sign_up_sign_in_and_who_am_i() {
    let app = app().await;
    sign_up(&app, "Ana@Example.com").await;

    let wrong = app
        .post("/auth/sign-in")
        .json(&json!({ "email": "ana@example.com", "password": "wrong password" }))
        .send()
        .await;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong.json::<Value>()["message"],
        "the email or password is wrong"
    );

    let res = app
        .post("/auth/sign-in")
        .json(&json!({ "email": "ana@example.com", "password": "correct horse" }))
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let token = res.json::<Value>()["token"].as_str().unwrap().to_owned();

    let me = app.get("/auth/me").bearer(&token).send().await;
    assert_eq!(me.json::<Value>()["email"], "ana@example.com");
    assert_eq!(
        app.get("/auth/me").send().await.status(),
        StatusCode::UNAUTHORIZED
    );
    let forged = app.get("/auth/me").bearer("not.a.token").send().await;
    assert_eq!(forged.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_email_can_only_be_registered_once_and_must_be_valid() {
    let app = app().await;
    sign_up(&app, "bo@example.com").await;
    let again = app
        .post("/auth/sign-up")
        .json(&json!({ "email": "bo@example.com", "password": "another one" }))
        .send()
        .await;
    assert_eq!(again.status(), StatusCode::CONFLICT);
    let bad = app
        .post("/auth/sign-up")
        .json(&json!({ "email": "nope", "password": "short" }))
        .send()
        .await;
    assert_eq!(bad.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        bad.json::<Value>()["details"]["fields"],
        json!({ "email": ["email"], "password": ["use 8 to 200 characters"] })
    );
}

#[tokio::test]
async fn sellers_own_their_products() {
    let app = app().await;
    let ana = sign_up(&app, "ana@example.com").await;
    let bo = sign_up(&app, "bo@example.com").await;

    let anonymous = app
        .post("/products")
        .json(&json!({ "name": "Lamp", "price_cents": 2500 }))
        .send()
        .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let created = app
        .post("/products")
        .bearer(&ana)
        .json(&json!({ "name": "Lamp", "price_cents": 2500 }))
        .send()
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let id = created.json::<Value>()["id"].as_i64().unwrap();
    let url = format!("/products/{id}");

    let listed = app
        .get("/products?limit=10")
        .send()
        .await
        .json::<Vec<Value>>();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["name"], "Lamp");

    let stolen = app
        .patch(&url)
        .bearer(&bo)
        .json(&json!({ "price_cents": 1 }))
        .send()
        .await;
    assert_eq!(stolen.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        app.delete(&url).bearer(&bo).send().await.status(),
        StatusCode::FORBIDDEN
    );

    let changed = app
        .patch(&url)
        .bearer(&ana)
        .json(&json!({ "price_cents": 1999 }))
        .send()
        .await;
    assert_eq!(changed.json::<Value>()["price_cents"], 1999);
    assert_eq!(app.get(&url).send().await.json::<Value>()["name"], "Lamp");

    assert_eq!(
        app.delete(&url).bearer(&ana).send().await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(app.get(&url).send().await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bad_prices_and_page_sizes_are_rejected() {
    let app = app().await;
    let ana = sign_up(&app, "ana@example.com").await;
    let free = app
        .post("/products")
        .bearer(&ana)
        .json(&json!({ "name": "Gift", "price_cents": 0 }))
        .send()
        .await;
    assert_eq!(free.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let huge = app.get("/products?limit=1000").send().await;
    assert_eq!(huge.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_api_docs_list_every_route() {
    let pool = shop::db::connect("sqlite::memory:").await.unwrap();
    let app = App::builder::<AppModule>()
        .override_provider(Arc::new(pool))
        .build()
        .await
        .unwrap();
    let doc = serde_json::to_value(app.openapi()).unwrap();
    let mut routes: Vec<String> = doc["paths"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(path, item)| {
            item.as_object()
                .unwrap()
                .keys()
                .map(move |m| format!("{m} {path}"))
        })
        .collect();
    routes.sort();
    assert_eq!(
        routes,
        [
            "delete /products/{id}",
            "get /auth/me",
            "get /products",
            "get /products/{id}",
            "patch /products/{id}",
            "post /auth/sign-in",
            "post /auth/sign-up",
            "post /products",
        ]
    );
    assert!(doc["components"]["schemas"]["NewProduct"].is_object());
}
