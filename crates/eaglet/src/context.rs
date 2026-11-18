//! Per-request context: the trace id, and turning every error response into the envelope.
//!
//! Every request goes through this layer, so it is written to cost as little as possible on the
//! happy path: the id lives in a fixed buffer (no `String`, no formatting), the inner future is
//! polled in place (no boxing), and the response body is only read when an error has to become
//! the envelope.

use std::cell::Cell;
use std::convert::Infallible;
use std::future::Future;
use std::hash::{BuildHasher, RandomState};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tower::{Layer, Service};

use crate::HttpError;
use crate::http_error::code_for;

/// The header that carries the trace id in and out.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// [`REQUEST_ID_HEADER`] as a header name, checked once at compile time.
const REQUEST_ID: HeaderName = HeaderName::from_static(REQUEST_ID_HEADER);

/// Error bodies longer than this are not copied into the envelope.
const MAX_ERROR_BODY: usize = 4096;

/// Incoming ids longer than this are replaced with a fresh one.
const MAX_ID_LEN: usize = 64;

/// A trace id. A fresh one is just a number until something needs its text; one sent by a proxy
/// keeps the header value it came in, so it goes back out without a copy.
#[derive(Clone)]
enum TraceId {
    Fresh(u64),
    Incoming(HeaderValue),
}

impl TraceId {
    /// A trace id sent by a proxy is kept when it looks like an id and not like an attack.
    fn incoming(req: &Request) -> Option<Self> {
        let value = req.headers().get(&REQUEST_ID)?;
        let bytes = value.as_bytes();
        let ok = !bytes.is_empty()
            && bytes.len() <= MAX_ID_LEN
            && bytes
                .iter()
                .all(|&b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        ok.then(|| Self::Incoming(value.clone()))
    }

    /// A short random id, unique enough to find one request in the logs.
    fn fresh() -> Self {
        Self::Fresh(next_random())
    }

    /// The id as text: 16 hex digits for a fresh one.
    fn to_text(&self) -> String {
        match self {
            Self::Fresh(n) => String::from_utf8_lossy(&hex(*n)).into_owned(),
            Self::Incoming(value) => String::from_utf8_lossy(value.as_bytes()).into_owned(),
        }
    }

    /// The value for the `x-request-id` response header.
    fn into_header(self) -> Option<HeaderValue> {
        match self {
            Self::Fresh(n) => HeaderValue::from_bytes(&hex(n)).ok(),
            Self::Incoming(value) => Some(value),
        }
    }
}

/// `n` as 16 lowercase hex digits.
fn hex(mut n: u64) -> [u8; 16] {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0; 16];
    for slot in out.iter_mut().rev() {
        *slot = DIGITS[usize::try_from(n & 0xf).unwrap_or_default()];
        n >>= 4;
    }
    out
}

/// `SplitMix64` over a per-thread state seeded once from the OS-randomised `RandomState`. Each
/// thread walks its own sequence, so there is no shared counter for cores to fight over.
fn next_random() -> u64 {
    thread_local! {
        static STATE: Cell<u64> = Cell::new(RandomState::new().hash_one(0_u64));
    }
    STATE.with(|state| {
        let s = state.get().wrapping_add(0x9e37_79b9_7f4a_7c15);
        state.set(s);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    })
}

tokio::task_local! {
    static TRACE_ID: TraceId;
}

/// The trace id of the request being handled, when called inside one.
#[must_use]
pub fn trace_id() -> Option<String> {
    TRACE_ID.try_with(TraceId::to_text).ok()
}

/// The layer every eaglet app runs first.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RequestContextLayer;

impl<S> Layer<S> for RequestContextLayer {
    type Service = RequestContext<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestContext { inner }
    }
}

/// Gives every request a trace id, turns error responses into the envelope and sends the id back
/// in `x-request-id`.
#[derive(Clone, Debug)]
pub(crate) struct RequestContext<S> {
    inner: S,
}

impl<S> Service<Request> for RequestContext<S>
where
    S: Service<Request, Response = Response, Error = Infallible>,
    S::Future: Unpin,
{
    type Response = Response;
    type Error = Infallible;
    type Future = RequestFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let id = TraceId::incoming(&req).unwrap_or_else(TraceId::fresh);
        // axum's `Route` only clones itself here and runs nothing until it is polled, and every
        // poll below runs with the id in scope.
        let inner = self.inner.call(req);
        RequestFuture {
            id: Some(id),
            state: State::Inner(inner),
        }
    }
}

type BoxedResponse = Pin<Box<dyn Future<Output = Response> + Send>>;

enum State<F> {
    /// The route is running.
    Inner(F),
    /// The route answered with an error that is being turned into the envelope.
    Envelope(BoxedResponse),
}

/// The response future of [`RequestContext`]. Every poll runs with the trace id in scope.
pub(crate) struct RequestFuture<F> {
    id: Option<TraceId>,
    state: State<F>,
}

impl<F> Future for RequestFuture<F>
where
    F: Future<Output = Result<Response, Infallible>> + Unpin,
{
    type Output = Result<Response, Infallible>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let Some(id) = this.id.clone() else {
            // polled again after it finished; there is nothing left to give
            return Poll::Pending;
        };
        let done = TRACE_ID.sync_scope(id, || {
            loop {
                let res = match &mut this.state {
                    State::Inner(fut) => {
                        // a panicking handler answers 500 instead of dropping the connection
                        match catch_unwind(AssertUnwindSafe(|| Pin::new(fut).poll(cx))) {
                            Ok(Poll::Ready(Ok(res))) => res,
                            Ok(Poll::Ready(Err(never))) => match never {},
                            Ok(Poll::Pending) => return Poll::Pending,
                            Err(_) => {
                                tracing::error!(
                                    trace_id = trace_id().as_deref(),
                                    "the handler panicked"
                                );
                                return Poll::Ready(
                                    HttpError::new(
                                        StatusCode::INTERNAL_SERVER_ERROR,
                                        "INTERNAL_ERROR",
                                        "Something went wrong on our side",
                                    )
                                    .into_response(),
                                );
                            }
                        }
                    }
                    State::Envelope(fut) => match fut.as_mut().poll(cx) {
                        Poll::Ready(res) => return Poll::Ready(res),
                        Poll::Pending => return Poll::Pending,
                    },
                };
                if needs_envelope(&res) {
                    this.state = State::Envelope(Box::pin(into_envelope(res)));
                } else {
                    return Poll::Ready(res);
                }
            }
        });
        let Poll::Ready(mut res) = done else {
            return Poll::Pending;
        };
        if let Some(value) = this.id.take().and_then(TraceId::into_header) {
            res.headers_mut().insert(REQUEST_ID, value);
        }
        Poll::Ready(Ok(res))
    }
}

/// Error responses that are not JSON yet (Axum's 404, 405 and extractor rejections, or a plain
/// string from a handler) become the envelope. Success responses are never touched.
fn needs_envelope(res: &Response) -> bool {
    let status = res.status();
    if !(status.is_client_error() || status.is_server_error()) {
        return false;
    }
    let is_json = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    !is_json
}

/// Rebuild an error response as the envelope, keeping its status and headers such as `Allow`.
async fn into_envelope(res: Response) -> Response {
    let (parts, body) = res.into_parts();
    let status = parts.status;
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
    use super::TraceId;

    #[test]
    fn ids_are_hex_and_differ() {
        let a = TraceId::fresh().to_text();
        let b = TraceId::fresh().to_text();
        assert_eq!(a.len(), 16);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
