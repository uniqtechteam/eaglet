//! What the auth routes accept and return.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

/// The body of `POST /auth/sign-up`.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SignUp {
    /// Used to sign in.
    #[validate(email)]
    pub email: String,
    /// At least 8 characters.
    #[validate(length(min = 8, max = 200, message = "use 8 to 200 characters"))]
    pub password: String,
}

/// The body of `POST /auth/sign-in`.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SignIn {
    /// The email used to sign up.
    #[validate(email)]
    pub email: String,
    /// The password.
    #[validate(length(min = 1, max = 200))]
    pub password: String,
}

/// A signed token; send it as `Authorization: Bearer <token>`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Token {
    /// The JWT.
    pub token: String,
    /// Always `Bearer`.
    pub token_type: String,
    /// Seconds until it expires.
    pub expires_in: u64,
}

/// The signed-in user, put on the request by `AuthGuard`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CurrentUser {
    /// The user's id.
    pub id: i64,
    /// The user's email.
    pub email: String,
}
