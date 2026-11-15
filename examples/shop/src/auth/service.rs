//! Sign-up and sign-in rules.

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use eaglet::prelude::*;
use password_hash::rand_core::OsRng;

use super::dto::{CurrentUser, SignIn, SignUp, Token};
use super::jwt::JwtService;
use super::repository::UsersRepo;

/// Creates accounts and signs people in.
#[injectable]
pub struct AuthService {
    users: Arc<dyn UsersRepo>,
    jwt: Arc<JwtService>,
}

/// Argon2 is slow on purpose, so it runs off the async threads.
async fn hash(password: String) -> Result<String, HttpError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(HttpError::internal)
    })
    .await
    .map_err(HttpError::internal)?
}

async fn matches(password: String, stored: String) -> Result<bool, HttpError> {
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&stored).map_err(HttpError::internal)?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .map_err(HttpError::internal)?
}

impl AuthService {
    /// Create an account and sign it in.
    ///
    /// # Errors
    /// `CONFLICT` when the email is taken.
    pub async fn sign_up(&self, input: SignUp) -> Result<Token, HttpError> {
        let email = input.email.trim().to_lowercase();
        let password_hash = hash(input.password).await?;
        let user = self.users.create(&email, &password_hash).await?;
        self.jwt.sign(&CurrentUser {
            id: user.id,
            email: user.email,
        })
    }

    /// Sign in with email and password.
    ///
    /// # Errors
    /// `UNAUTHORIZED` when the email or password is wrong (the message does not say which).
    pub async fn sign_in(&self, input: SignIn) -> Result<Token, HttpError> {
        let wrong = || HttpError::unauthorized("the email or password is wrong");
        let email = input.email.trim().to_lowercase();
        let user = self.users.by_email(&email).await?.ok_or_else(wrong)?;
        if !matches(input.password, user.password_hash).await? {
            return Err(wrong());
        }
        self.jwt.sign(&CurrentUser {
            id: user.id,
            email: user.email,
        })
    }
}
