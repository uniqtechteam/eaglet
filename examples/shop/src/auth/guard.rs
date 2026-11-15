//! `AuthGuard`: lets requests with a valid `Authorization: Bearer <token>` through.

use eaglet::axum::http::header;
use eaglet::prelude::*;

use super::jwt::JwtService;

/// Put `#[guard(AuthGuard)]` on a controller or route; the handler reads the user with
/// `Extension<CurrentUser>`.
#[injectable]
pub struct AuthGuard {
    jwt: Arc<JwtService>,
}

impl AuthGuard {
    fn check(&self, parts: &mut Parts) -> Result<(), HttpError> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| {
                HttpError::unauthorized("sign in first: send Authorization: Bearer <token>")
            })?;
        let user = self.jwt.verify(token)?;
        parts.extensions.insert(user);
        Ok(())
    }
}

// The check never waits, so it is wrapped in a ready future; a guard that looks something up
// can be written as `async fn can_activate` instead.
impl Guard for AuthGuard {
    fn can_activate(
        &self,
        parts: &mut Parts,
    ) -> impl Future<Output = Result<(), HttpError>> + Send {
        std::future::ready(self.check(parts))
    }
}
