//! The plain Axum version, written the way a careful Axum app would be: state in an `Arc`,
//! validation in the handler, the guard as a `route_layer`.

use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;
use validator::Validate;

use crate::shared::{NewUser, User, Users};

/// The error body, with the `code` and `message` fields eaglet also sends.
#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

fn error(status: StatusCode, code: &'static str, message: String) -> Response {
    (status, Json(ErrorBody { code, message })).into_response()
}

async fn find(State(users): State<Arc<Users>>, Path(id): Path<u64>) -> Json<User> {
    Json(users.find(id))
}

async fn create(State(users): State<Arc<Users>>, Json(input): Json<NewUser>) -> Response {
    match input.validate() {
        Ok(()) => (StatusCode::CREATED, Json(users.create(input))).into_response(),
        Err(e) => error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_FAILED",
            e.to_string(),
        ),
    }
}

async fn me(Extension(user): Extension<User>) -> Json<User> {
    Json(user)
}

async fn auth(State(users): State<Arc<Users>>, mut req: Request, next: Next) -> Response {
    let header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    match users.by_token(header) {
        Some(user) => {
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        None => error(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "sign in first".to_owned(),
        ),
    }
}

/// The router: `GET /users/{id}`, `POST /users`, `GET /me` (guarded).
pub fn router() -> Router {
    let users = Arc::new(Users);
    let guarded = Router::new()
        .route("/me", get(me))
        .route_layer(middleware::from_fn_with_state(Arc::clone(&users), auth));
    Router::new()
        .route("/users/{id}", get(find))
        .route("/users", post(create))
        .merge(guarded)
        .with_state(users)
}
