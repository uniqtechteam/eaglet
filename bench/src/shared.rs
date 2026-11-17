//! What both versions share: the DTOs, the service and the token check.

use serde::{Deserialize, Serialize};
use validator::Validate;

/// The token `GET /me` accepts.
pub const TOKEN: &str = "bench-token";

/// A user as the API returns it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    /// The id.
    pub id: u64,
    /// Display name.
    pub name: String,
    /// Contact email.
    pub email: String,
}

/// The body of `POST /users`.
#[derive(Debug, Deserialize, Validate)]
pub struct NewUser {
    /// Display name, 1 to 64 characters.
    #[validate(length(min = 1, max = 64))]
    pub name: String,
    /// Must look like an email.
    #[validate(email)]
    pub email: String,
}

/// The work behind the routes. Kept tiny on purpose: the bench measures the framework, not this.
/// `#[injectable]` only adds the impl eaglet uses to build it; plain Axum ignores it.
#[eaglet::injectable]
#[derive(Debug, Default)]
pub struct Users;

impl Users {
    /// The user with `id`.
    #[must_use]
    pub fn find(&self, id: u64) -> User {
        User {
            id,
            name: format!("user {id}"),
            email: format!("user{id}@example.com"),
        }
    }

    /// Store a new user (here: give it an id).
    #[must_use]
    pub fn create(&self, input: NewUser) -> User {
        User {
            id: 1,
            name: input.name,
            email: input.email,
        }
    }

    /// The user a bearer token belongs to, if any.
    #[must_use]
    pub fn by_token(&self, authorization: Option<&str>) -> Option<User> {
        (authorization?.strip_prefix("Bearer ")? == TOKEN).then(|| self.find(7))
    }
}
