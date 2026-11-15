//! `HttpError` and the JSON error envelope every failed request returns:
//! `{ "code", "message", "details", "traceId" }`.

use std::borrow::Cow;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::context;

/// An error a handler, guard or extractor returns. It becomes the JSON envelope with the matching
/// status. `code` is stable and meant for clients to branch on; `message` is for people.
///
/// ```ignore
/// async fn find(&self, Path(id): Path<u64>) -> Result<Json<User>, HttpError> {
///     self.users.find(id).map(Json).ok_or_else(|| HttpError::not_found("no user with that id"))
/// }
/// ```
#[derive(Debug, Clone)]
pub struct HttpError {
    status: StatusCode,
    code: Cow<'static, str>,
    message: Cow<'static, str>,
    details: Option<Value>,
}

impl HttpError {
    /// An error with any status, code and message.
    pub fn new(
        status: StatusCode,
        code: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    /// 400 `BAD_REQUEST`.
    pub fn bad_request(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "BAD_REQUEST", message)
    }

    /// 401 `UNAUTHORIZED`: no valid credentials.
    pub fn unauthorized(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", message)
    }

    /// 403 `FORBIDDEN`: signed in, but not allowed.
    pub fn forbidden(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "FORBIDDEN", message)
    }

    /// 404 `NOT_FOUND`.
    pub fn not_found(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "NOT_FOUND", message)
    }

    /// 409 `CONFLICT`.
    pub fn conflict(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::CONFLICT, "CONFLICT", message)
    }

    /// 422 `VALIDATION_FAILED`; put the field errors in [`HttpError::with_details`].
    pub fn validation(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_FAILED",
            message,
        )
    }

    /// 500 `INTERNAL_ERROR`. The cause is logged with the trace id and never sent to the client,
    /// which gets a generic message instead.
    pub fn internal(cause: impl std::fmt::Display) -> Self {
        tracing::error!(trace_id = context::trace_id().as_deref(), "{cause}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "Something went wrong on our side",
        )
    }

    /// Attach machine-readable details, for example field errors.
    #[must_use]
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// The HTTP status.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The stable code, for example `UNAUTHORIZED`.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The message for people.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The details, if any.
    #[must_use]
    pub fn details(&self) -> Option<&Value> {
        self.details.as_ref()
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.status.as_u16(),
            self.code,
            self.message
        )
    }
}

impl std::error::Error for HttpError {}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let body = json!({
            "code": self.code,
            "message": self.message,
            "details": self.details,
            "traceId": context::trace_id(),
        });
        (self.status, Json(body)).into_response()
    }
}

/// The code used for a bare status, for example `METHOD_NOT_ALLOWED` for 405.
pub(crate) fn code_for(status: StatusCode) -> Cow<'static, str> {
    match status {
        StatusCode::INTERNAL_SERVER_ERROR => "INTERNAL_ERROR".into(),
        StatusCode::UNPROCESSABLE_ENTITY => "VALIDATION_FAILED".into(),
        other => other
            .canonical_reason()
            .unwrap_or("ERROR")
            .to_uppercase()
            .replace([' ', '-'], "_")
            .into(),
    }
}
