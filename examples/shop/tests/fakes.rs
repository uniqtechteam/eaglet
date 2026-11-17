//! The NestJS way to test a service: swap its repository for a fake. No database is opened.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]

use std::sync::Mutex;

use async_trait::async_trait;
use eaglet::prelude::*;
use eaglet::testing::TestApp;
use serde_json::{Value, json};
use shop::app_module::AppModule;
use shop::auth::{CurrentUser, JwtService};
use shop::config::Config;
use shop::products::{NewProduct, Product, ProductsRepo};

/// Products in a `Vec`, with one product already for sale.
struct FakeProducts(Mutex<Vec<Product>>);

impl FakeProducts {
    fn with_a_lamp() -> Self {
        Self(Mutex::new(vec![Product {
            id: 7,
            owner_id: 1,
            name: "Lamp".into(),
            price_cents: 2500,
        }]))
    }
}

#[async_trait]
impl ProductsRepo for FakeProducts {
    async fn list(&self, _limit: u32, _offset: u32) -> Result<Vec<Product>, HttpError> {
        Ok(self.0.lock().unwrap().clone())
    }

    async fn get(&self, id: i64) -> Result<Option<Product>, HttpError> {
        Ok(self.0.lock().unwrap().iter().find(|p| p.id == id).cloned())
    }

    async fn insert(&self, owner_id: i64, input: NewProduct) -> Result<Product, HttpError> {
        let product = Product {
            id: 8,
            owner_id,
            name: input.name,
            price_cents: input.price_cents,
        };
        self.0.lock().unwrap().push(product.clone());
        Ok(product)
    }

    async fn save(&self, product: &Product) -> Result<(), HttpError> {
        let mut rows = self.0.lock().unwrap();
        if let Some(row) = rows.iter_mut().find(|p| p.id == product.id) {
            *row = product.clone();
        }
        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<(), HttpError> {
        self.0.lock().unwrap().retain(|p| p.id != id);
        Ok(())
    }
}

async fn app() -> TestApp {
    TestApp::new::<AppModule>()
        .override_provider::<dyn ProductsRepo>(Arc::new(FakeProducts::with_a_lamp()))
        // never connected to: the products repo is fake and these tests sign no one up
        .override_value(sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap())
        .override_value(Config {
            port: 0,
            database_url: String::new(),
            jwt_secret: "test-secret".into(),
        })
        .build()
        .await
        .unwrap()
}

/// A token for user `id`, signed by the app's own `JwtService`.
fn token_for(app: &TestApp, id: i64) -> String {
    let user = CurrentUser {
        id,
        email: format!("user{id}@example.com"),
    };
    app.provider::<JwtService>()
        .unwrap()
        .sign(&user)
        .unwrap()
        .token
}

#[tokio::test]
async fn the_fake_repo_serves_the_catalog() {
    let app = app().await;
    let list = app.get("/products").send().await.json::<Vec<Value>>();
    assert_eq!(list.len(), 1);
    assert_eq!(
        app.get("/products/7").send().await.json::<Value>()["name"],
        "Lamp"
    );
    assert_eq!(
        app.get("/products/9").send().await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn only_the_owner_may_change_a_product() {
    let app = app().await;
    let owner = token_for(&app, 1);
    let stranger = token_for(&app, 2);
    let res = app
        .patch("/products/7")
        .bearer(&stranger)
        .json(&json!({ "name": "Mine now" }))
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let res = app
        .patch("/products/7")
        .bearer(&owner)
        .json(&json!({ "name": "Desk lamp" }))
        .send()
        .await;
    assert_eq!(res.json::<Value>()["name"], "Desk lamp");
}

#[tokio::test]
async fn a_new_product_belongs_to_whoever_signed_in() {
    let app = app().await;
    let res = app
        .post("/products")
        .bearer(&token_for(&app, 5))
        .json(&json!({ "name": "Chair", "price_cents": 9900 }))
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(res.json::<Value>()["owner_id"], 5);
}

/// `ProductsModule` without its repository binding.
#[module(
    imports = [shop::auth::AuthModule, shop::db::DbModule],
    controllers = [shop::products::ProductsController],
    providers = [shop::products::ProductsService]
)]
struct ProductsModule;

#[tokio::test]
async fn a_missing_provider_stops_the_app_with_both_names() {
    let err = App::new::<ProductsModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "ProductsService needs dyn ProductsRepo, which ProductsModule does not provide or import"
    );
}
