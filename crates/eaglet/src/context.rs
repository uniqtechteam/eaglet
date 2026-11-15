//! Per-request context: the trace id, and turning every error response into the envelope.

use std::hash::{BuildHasher, RandomState};
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::Request;
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::HttpError;
use crate::http_error::code_for;

/// The header that carries the trace id in and out.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Error bodies longer than this are not copied into the envelope.
const MAX_ERROR_BODY: usize = 4096;

tokio::task_local! {
    static TRACE_ID: String;
}

/// The trace id of the request being handled, when called inside one.
#[must_use]
pub fn trace_id() -> Option<String> {
    TRACE_ID.try_with(Clone::clone).ok()
}

/// A short random id, unique enough to find one request in the logs.
fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}", RandomState::new().hash_one(n))
}

/// A trace id sent by a proxy is kept when it looks like an id and not like an attack.
fn incoming_id(req: &Request) -> Option<String> {
    let value = req.headers().get(REQUEST_ID_HEADER)?.to_str().ok()?;
    let ok = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then(|| value.to_owned())
}

/// The middleware every eaglet app runs first.
pub(crate) async fn request_context(req: Request, next: Next) -> Response {
    let id = incoming_id(&req).unwrap_or_else(new_id);
    let res = TRACE_ID.scope(id.clone(), next.run(req)).await;
    let mut res = TRACE_ID.scope(id.clone(), into_envelope(res)).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        res.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    res
}

/// Error responses that are not JSON yet (Axum's 404, 405 and extractor rejections, or a plain
/// string from a handler) become the envelope, keeping their status and headers such as `Allow`.
async fn into_envelope(res: Response) -> Response {
    let status = res.status();
    let is_json = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    if !(status.is_client_error() || status.is_server_error()) || is_json {
        return res;
    }
    let (parts, body) = res.into_parts();
    let text = axum::body::to_bytes(body, MAX_ERROR_BODY)
        .await
        .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
        .unwrap_or_default();
    let message = if status.is_server_error() {
        if !text.is_empty() {
            tracing::error!(trace_id = trace_id().as_deref(), "{text}");
        }
        "Something went wrong on our side".to_owned()
    } else if text.is_empty() {
        status.canonical_reason().unwrap_or("Error").to_owned()
    } else {
        text
    };
    let mut out = HttpError::new(status, code_for(status), message).into_response();
    for (name, value) in &parts.headers {
        if name != header::CONTENT_TYPE && name != header::CONTENT_LENGTH {
            out.headers_mut().insert(name.clone(), value.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::new_id;

    #[test]
    fn ids_are_hex_and_differ() {
        let a = new_id();
        let b = new_id();
        assert_eq!(a.len(), 16);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
