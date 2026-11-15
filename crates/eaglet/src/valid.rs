//! `Valid<Json<T>>` and `Valid<Query<T>>`: parse, then check with `validator`.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use validator::{Validate, ValidationErrors, ValidationErrorsKind};

use crate::HttpError;
use crate::http_error::code_for;

/// Parse the request part, then run its `#[validate(...)]` rules. A body that cannot be parsed is a
/// 400 (or the status Axum gives it); a value that breaks a rule is a 422 `VALIDATION_FAILED` that
/// lists every field: `"details": { "fields": { "email": ["email"] } }`.
///
/// ```ignore
/// #[derive(Deserialize, Validate)]
/// struct SignUp {
///     #[validate(email)]
///     email: String,
/// }
///
/// #[post("/")]
/// async fn sign_up(&self, Valid(Json(body)): Valid<Json<SignUp>>) -> StatusCode { ... }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Valid<T>(pub T);

impl<T> std::ops::Deref for Valid<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<S, T> FromRequest<S> for Valid<Json<T>>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = HttpError;

    async fn from_request(req: Request, state: &S) -> Result<Self, HttpError> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|r| HttpError::new(r.status(), code_for(r.status()), r.body_text()))?;
        value.validate().map_err(|e| validation_error(&e))?;
        Ok(Self(Json(value)))
    }
}

impl<S, T> FromRequestParts<S> for Valid<Query<T>>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, HttpError> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|r| HttpError::new(r.status(), code_for(r.status()), r.body_text()))?;
        value.validate().map_err(|e| validation_error(&e))?;
        Ok(Self(Query(value)))
    }
}

/// Every broken rule, as `field path → messages` (the rule's message, or its code).
fn validation_error(errors: &ValidationErrors) -> HttpError {
    let mut fields = BTreeMap::new();
    flatten("", errors, &mut fields);
    let names: Vec<&str> = fields.keys().map(String::as_str).collect();
    let message = format!("some fields are not valid: {}", names.join(", "));
    HttpError::validation(message).with_details(json!({ "fields": fields }))
}

fn flatten(prefix: &str, errors: &ValidationErrors, out: &mut BTreeMap<String, Vec<Value>>) {
    for (name, kind) in errors.errors() {
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        };
        match kind {
            ValidationErrorsKind::Field(list) => {
                let messages = list
                    .iter()
                    .map(|e| Value::from(e.message.as_deref().unwrap_or(&e.code).to_owned()))
                    .collect();
                out.insert(path, messages);
            }
            ValidationErrorsKind::Struct(inner) => flatten(&path, inner, out),
            ValidationErrorsKind::List(items) => {
                for (i, inner) in items {
                    flatten(&format!("{path}[{i}]"), inner, out);
                }
            }
        }
    }
}
