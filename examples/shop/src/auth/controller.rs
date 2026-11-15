//! The `/auth` routes.

use eaglet::prelude::*;

use super::dto::{CurrentUser, SignIn, SignUp, Token};
use super::guard::AuthGuard;
use super::service::AuthService;

/// Sign-up, sign-in and the current user.
#[injectable]
pub struct AuthController {
    auth: Arc<AuthService>,
}

#[controller("/auth")]
impl AuthController {
    /// Create an account; the answer is a token, so the new user is signed in.
    #[post("/sign-up")]
    async fn sign_up(
        &self,
        Valid(Json(input)): Valid<Json<SignUp>>,
    ) -> Result<(StatusCode, Json<Token>), HttpError> {
        Ok((StatusCode::CREATED, Json(self.auth.sign_up(input).await?)))
    }

    /// Sign in with email and password.
    #[post("/sign-in")]
    async fn sign_in(
        &self,
        Valid(Json(input)): Valid<Json<SignIn>>,
    ) -> Result<Json<Token>, HttpError> {
        self.auth.sign_in(input).await.map(Json)
    }

    /// The signed-in user.
    #[get("/me")]
    #[guard(AuthGuard)]
    fn me(&self, Extension(user): Extension<CurrentUser>) -> Json<CurrentUser> {
        Json(user)
    }
}
