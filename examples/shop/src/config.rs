//! Settings, read from the environment once at startup.

use std::fmt::Write as _;

use eaglet::prelude::*;
use password_hash::rand_core::{OsRng, RngCore};

/// Settings read from the environment.
#[derive(Debug, Clone)]
pub struct Config {
    /// `PORT`, 3000 by default.
    pub port: u16,
    /// `DATABASE_URL`, `sqlite://shop.db?mode=rwc` by default.
    pub database_url: String,
    /// `JWT_SECRET`. Without it a random secret is made at startup, so tokens stop working when the
    /// server restarts.
    pub jwt_secret: String,
}

impl Config {
    /// Read the settings from the environment.
    #[must_use]
    pub fn from_env() -> Self {
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
            tracing::warn!(
                "JWT_SECRET is not set; using a random secret until the server restarts"
            );
            let mut bytes = [0u8; 32];
            OsRng.fill_bytes(&mut bytes);
            bytes.iter().fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            })
        });
        Self {
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            database_url: database_url(),
            jwt_secret,
        }
    }
}

/// `DATABASE_URL`, `sqlite://shop.db?mode=rwc` by default.
#[must_use]
pub fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://shop.db?mode=rwc".to_owned())
}

/// Import this module to inject `Arc<Config>`.
#[module(providers = [ProviderDef::value(Config::from_env())], exports = [Config])]
pub struct ConfigModule;
