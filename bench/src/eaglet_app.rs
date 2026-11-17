//! The eaglet version: one module, two controllers, one guard.

#![allow(
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    reason = "handlers are written the way real controllers are, async and on &self"
)]

use eaglet::axum::http::header;
use eaglet::prelude::*;

use crate::shared::{NewUser, User, Users};

/// Lets requests with `Authorization: Bearer bench-token` through and puts the user on them.
#[injectable]
pub struct AuthGuard {
    users: Arc<Users>,
}

impl Guard for AuthGuard {
    fn can_activate(
        &self,
        parts: &mut Parts,
    ) -> impl Future<Output = Result<(), HttpError>> + Send {
        let header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        let result = match self.users.by_token(header) {
            Some(user) => {
                parts.extensions.insert(user);
                Ok(())
            }
            None => Err(HttpError::unauthorized("sign in first")),
        };
        std::future::ready(result)
    }
}

/// `/users`.
#[injectable]
pub struct UsersController {
    users: Arc<Users>,
}

#[controller("/users")]
impl UsersController {
    /// One user.
    #[get("/:id")]
    async fn find(&self, Path(id): Path<u64>) -> Json<User> {
        Json(self.users.find(id))
    }

    /// Create a user from a validated body.
    #[post("/")]
    async fn create(&self, Valid(Json(input)): Valid<Json<NewUser>>) -> (StatusCode, Json<User>) {
        (StatusCode::CREATED, Json(self.users.create(input)))
    }
}

/// `/me`, behind [`AuthGuard`].
#[injectable]
pub struct MeController;

#[controller("/me")]
#[guard(AuthGuard)]
impl MeController {
    /// The signed-in user.
    #[get("/")]
    async fn me(&self, Extension(user): Extension<User>) -> Json<User> {
        Json(user)
    }
}

/// The whole app.
#[module(
    controllers = [UsersController, MeController],
    providers = [Users, AuthGuard]
)]
pub struct BenchModule;

/// Build the app and hand back its Axum router.
///
/// # Errors
/// When the module tree does not resolve (it does; this is the startup check).
pub async fn router() -> Result<axum::Router, StartupError> {
    Ok(App::new::<BenchModule>().await?.into_router())
}
