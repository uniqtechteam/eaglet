//! SPEC-HTTP-001 FR-006: the OpenAPI 3.1 document and Swagger UI.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]
#![allow(
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    clippy::unused_self,
    dead_code,
    unused_variables,
    reason = "the handlers only need the right signatures for the docs"
)]

use eaglet::axum::body::Body;
use eaglet::axum::http::Request;
use eaglet::prelude::*;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower::ServiceExt;
use utoipa::ToSchema;
use validator::Validate;

#[derive(Serialize, Deserialize, ToSchema)]
struct User {
    id: u64,
    name: String,
    address: Address,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct Address {
    city: String,
}

#[derive(Deserialize, Validate, ToSchema)]
struct NewUser {
    #[validate(email)]
    email: String,
}

#[derive(Deserialize, Validate, ToSchema)]
struct ListQuery {
    limit: u32,
    cursor: Option<String>,
}

/// A body type without `ToSchema` still works; the docs show "any value".
#[derive(Deserialize)]
struct Untyped {
    anything: String,
}

#[injectable]
struct UsersController;

#[controller("/users")]
impl UsersController {
    /// Find one user.
    ///
    /// Returns 404 when there is no user with that id.
    #[get("/:id")]
    async fn find(&self, Path(id): Path<u64>) -> Result<Json<User>, HttpError> {
        Err(HttpError::not_found("none"))
    }

    /// List users.
    #[get("/")]
    async fn list(&self, Valid(Query(q)): Valid<Query<ListQuery>>) -> Json<Vec<User>> {
        Json(Vec::new())
    }

    #[post("/")]
    async fn create(&self, Valid(Json(body)): Valid<Json<NewUser>>) -> (StatusCode, Json<User>) {
        (
            StatusCode::CREATED,
            Json(User {
                id: 1,
                name: body.email,
                address: Address {
                    city: "Hanoi".into(),
                },
            }),
        )
    }

    #[put("/:id/files/:name")]
    async fn upload(
        &self,
        Path((id, name)): Path<(u64, String)>,
        Json(body): Json<Untyped>,
    ) -> StatusCode {
        StatusCode::NO_CONTENT
    }

    #[get("/:id/avatar/*rest")]
    async fn avatar(&self) -> &'static str {
        "png"
    }
}

#[module(controllers = [UsersController])]
struct AppModule;

fn validate_against_the_official_schema(doc: &Value) {
    let schema: Value = serde_json::from_str(include_str!("data/openapi-3.1-schema.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator.iter_errors(doc).map(|e| e.to_string()).collect();
    assert_eq!(errors, Vec::<String>::new(), "{doc:#}");
}

#[test]
fn the_schema_check_catches_a_broken_document() {
    let schema: Value = serde_json::from_str(include_str!("data/openapi-3.1-schema.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let broken = serde_json::json!({ "openapi": "3.1.0", "info": { "title": "x" }, "paths": {} });
    assert!(!validator.is_valid(&broken), "info.version is required");
}

async fn document() -> Value {
    let app = App::new::<AppModule>()
        .await
        .unwrap()
        .with_info("Users API", "1.2.0");
    serde_json::to_value(app.openapi()).unwrap()
}

#[tokio::test]
async fn ac_003_the_document_is_valid_openapi_3_1_and_lists_every_route() {
    let doc = document().await;
    validate_against_the_official_schema(&doc);
    assert_eq!(doc["openapi"], "3.1.0");
    assert_eq!(doc["info"]["title"], "Users API");
    assert_eq!(doc["info"]["version"], "1.2.0");
    let paths = doc["paths"].as_object().unwrap();
    let mut listed: Vec<String> = paths
        .iter()
        .flat_map(|(p, item)| {
            item.as_object()
                .unwrap()
                .keys()
                .map(move |m| format!("{m} {p}"))
        })
        .collect();
    listed.sort();
    assert_eq!(
        listed,
        [
            "get /users",
            "get /users/{id}",
            "get /users/{id}/avatar/{rest}",
            "post /users",
            "put /users/{id}/files/{name}",
        ]
    );
}

#[tokio::test]
async fn dtos_become_components_with_their_nested_types() {
    let doc = document().await;
    let schemas = doc["components"]["schemas"].as_object().unwrap();
    for name in ["User", "Address", "NewUser", "HttpError"] {
        assert!(schemas.contains_key(name), "missing {name}");
    }
    let find = &doc["paths"]["/users/{id}"]["get"];
    assert_eq!(
        find["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/User"
    );
    assert_eq!(
        find["responses"]["default"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/HttpError"
    );
    let create = &doc["paths"]["/users"]["post"];
    assert_eq!(
        create["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/NewUser"
    );
    assert!(create["responses"]["2XX"].is_object());
}

#[tokio::test]
async fn parameters_summaries_and_tags_come_from_the_code() {
    let doc = document().await;
    let find = &doc["paths"]["/users/{id}"]["get"];
    assert_eq!(find["summary"], "Find one user.");
    assert_eq!(
        find["description"],
        "Returns 404 when there is no user with that id."
    );
    assert_eq!(find["operationId"], "UsersController_find");
    assert_eq!(find["tags"][0], "Users");
    assert_eq!(find["parameters"][0]["name"], "id");
    assert_eq!(find["parameters"][0]["in"], "path");
    assert_eq!(find["parameters"][0]["schema"]["type"], "integer");

    let upload = &doc["paths"]["/users/{id}/files/{name}"]["put"];
    assert_eq!(upload["parameters"][1]["name"], "name");
    assert_eq!(upload["parameters"][1]["schema"]["type"], "string");
    assert!(
        upload["requestBody"].is_object(),
        "a body without ToSchema is still listed"
    );

    let list = &doc["paths"]["/users"]["get"];
    let params: Vec<(&str, bool)> = list["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap(),
                p["required"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(params, [("cursor", false), ("limit", true)]);
}

async fn fetch(router: eaglet::axum::Router, uri: &str) -> (StatusCode, String, String) {
    let res = router
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let content_type = res.headers()["content-type"].to_str().unwrap().to_owned();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        content_type,
        String::from_utf8(body.to_vec()).unwrap(),
    )
}

#[tokio::test]
async fn swagger_ui_and_the_json_are_served_at_docs() {
    let router = App::new::<AppModule>()
        .await
        .unwrap()
        .with_info("Users <API>", "1.0.0")
        .swagger("/docs")
        .into_router();
    let (status, content_type, html) = fetch(router.clone(), "/docs").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/html"));
    assert!(html.contains("swagger-ui-dist@5.34.1/swagger-ui-bundle.js"));
    assert!(html.contains(r#"integrity="sha384-"#));
    assert!(html.contains(r#"url: "/docs/openapi.json""#));
    assert!(html.contains("<title>Users &lt;API&gt;</title>"));

    let (status, content_type, json) = fetch(router, "/docs/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/json");
    let doc: Value = serde_json::from_str(&json).unwrap();
    validate_against_the_official_schema(&doc);
}

#[tokio::test]
async fn without_swagger_there_is_no_docs_route() {
    let router = App::new::<AppModule>().await.unwrap().into_router();
    let (status, _, _) = fetch(router, "/docs").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
