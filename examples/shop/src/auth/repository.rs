//! Where users are stored.

use async_trait::async_trait;
use eaglet::prelude::*;
use sqlx::SqlitePool;

/// A stored user.
#[derive(Debug, Clone)]
pub struct User {
    /// The id.
    pub id: i64,
    /// The email, unique.
    pub email: String,
    /// The Argon2 hash of the password.
    pub password_hash: String,
}

/// Storage for users.
#[async_trait]
pub trait UsersRepo: Send + Sync {
    /// Store a new user.
    ///
    /// # Errors
    /// `CONFLICT` when the email is taken.
    async fn create(&self, email: &str, password_hash: &str) -> Result<User, HttpError>;

    /// The user with this email.
    ///
    /// # Errors
    /// When the database fails.
    async fn by_email(&self, email: &str) -> Result<Option<User>, HttpError>;
}

/// Users in SQLite.
#[injectable]
pub struct SqlUsersRepo {
    pool: Arc<SqlitePool>,
}

#[async_trait]
impl UsersRepo for SqlUsersRepo {
    async fn create(&self, email: &str, password_hash: &str) -> Result<User, HttpError> {
        let id = sqlx::query_scalar::<_, i64>(
            "insert into users (email, password_hash) values (?, ?) returning id",
        )
        .bind(email)
        .bind(password_hash)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            if e.as_database_error()
                .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
            {
                HttpError::conflict("that email is already registered")
            } else {
                HttpError::internal(e)
            }
        })?;
        Ok(User {
            id,
            email: email.to_owned(),
            password_hash: password_hash.to_owned(),
        })
    }

    async fn by_email(&self, email: &str) -> Result<Option<User>, HttpError> {
        let row = sqlx::query_as::<_, (i64, String, String)>(
            "select id, email, password_hash from users where email = ?",
        )
        .bind(email)
        .fetch_optional(&*self.pool)
        .await
        .map_err(HttpError::internal)?;
        Ok(row.map(|(id, email, password_hash)| User {
            id,
            email,
            password_hash,
        }))
    }
}
