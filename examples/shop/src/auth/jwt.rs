//! Signing and checking tokens.

use std::time::{SystemTime, UNIX_EPOCH};

use eaglet::prelude::*;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use super::dto::{CurrentUser, Token};
use crate::config::Config;

/// How long a token is valid: one day.
const LIFETIME_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: i64,
    email: String,
    exp: u64,
}

/// Signs tokens with `JWT_SECRET` (HS256) and checks them.
#[injectable]
pub struct JwtService {
    config: Arc<Config>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl JwtService {
    /// A token for `user`.
    ///
    /// # Errors
    /// `INTERNAL_ERROR` when signing fails.
    pub fn sign(&self, user: &CurrentUser) -> Result<Token, HttpError> {
        let claims = Claims {
            sub: user.id,
            email: user.email.clone(),
            exp: now() + LIFETIME_SECS,
        };
        let key = EncodingKey::from_secret(self.config.jwt_secret.as_bytes());
        let token =
            encode(&Header::new(Algorithm::HS256), &claims, &key).map_err(HttpError::internal)?;
        Ok(Token {
            token,
            token_type: "Bearer".to_owned(),
            expires_in: LIFETIME_SECS,
        })
    }

    /// The user a token was signed for.
    ///
    /// # Errors
    /// `UNAUTHORIZED` when the token is broken, signed with another secret or expired.
    pub fn verify(&self, token: &str) -> Result<CurrentUser, HttpError> {
        let key = DecodingKey::from_secret(self.config.jwt_secret.as_bytes());
        let data =
            decode::<Claims>(token, &key, &Validation::new(Algorithm::HS256)).map_err(|_| {
                HttpError::unauthorized("the token is not valid or has expired; sign in again")
            })?;
        Ok(CurrentUser {
            id: data.claims.sub,
            email: data.claims.email,
        })
    }
}
