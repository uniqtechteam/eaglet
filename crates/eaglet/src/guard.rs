//! Guards: checks that run before a route's handler and its body parsing.

use std::sync::Arc;

use axum::extract::Request;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;

use crate::HttpError;

/// A check that decides whether a request may reach the handler.
/// A guard is a provider: make it `#[injectable]`, list it in a module, and put
/// `#[guard(AuthGuard)]` on a controller (all its routes) or on one route. It runs before the body
/// is read, and may add values to `parts.extensions` for the handler (for example the signed-in
/// user, read with `Extension<User>`).
///
/// ```ignore
/// #[injectable]
/// struct AuthGuard { sessions: Arc<Sessions> }
///
/// impl Guard for AuthGuard {
///     async fn can_activate(&self, parts: &mut Parts) -> Result<(), HttpError> {
///         let user = self.sessions.user(parts).ok_or_else(|| HttpError::unauthorized("sign in first"))?;
///         parts.extensions.insert(user);
///         Ok(())
///     }
/// }
/// ```
pub trait Guard: Send + Sync + 'static {
    /// `Ok(())` lets the request through; an error is returned to the client as it is.
    fn can_activate(&self, parts: &mut Parts)
    -> impl Future<Output = Result<(), HttpError>> + Send;
}

async fn check<G: Guard>(guard: &G, req: Request, next: Next) -> Response {
    let (mut parts, body) = req.into_parts();
    match guard.can_activate(&mut parts).await {
        Ok(()) => next.run(Request::from_parts(parts, body)).await,
        Err(err) => err.into_response(),
    }
}

/// Used by `#[guard]` on a route.
#[doc(hidden)]
pub fn guard_method<G: Guard>(route: MethodRouter, guard: Arc<G>) -> MethodRouter {
    route.route_layer(axum::middleware::from_fn(
        move |req: Request, next: Next| {
            let guard = Arc::clone(&guard);
            async move { check(guard.as_ref(), req, next).await }
        },
    ))
}

/// Used by `#[guard]` on a controller.
#[doc(hidden)]
pub fn guard_router<G: Guard>(router: axum::Router, guard: Arc<G>) -> axum::Router {
    router.route_layer(axum::middleware::from_fn(
        move |req: Request, next: Next| {
            let guard = Arc::clone(&guard);
            async move { check(guard.as_ref(), req, next).await }
        },
    ))
}
