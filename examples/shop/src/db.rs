//! The SQLite pool and the tables. Tests build their own pool with [`connect`] and
//! `sqlite::memory:`.

use eaglet::prelude::*;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

use crate::config::database_url;

const SCHEMA: &str = "
create table if not exists users (
    id integer primary key autoincrement,
    email text not null unique,
    password_hash text not null
);
create table if not exists products (
    id integer primary key autoincrement,
    owner_id integer not null references users(id),
    name text not null,
    price_cents integer not null
);
";

/// Open a pool and create the tables when they are missing.
///
/// # Errors
/// When the database cannot be opened or the tables cannot be created.
pub async fn connect(url: &str) -> Result<SqlitePool, sqlx::Error> {
    // one connection keeps a `sqlite::memory:` database shared; raise it for a file database
    let max = if url.contains(":memory:") { 1 } else { 5 };
    let pool = SqlitePoolOptions::new()
        .max_connections(max)
        .connect(url)
        .await?;
    sqlx::raw_sql(SCHEMA).execute(&pool).await?;
    Ok(pool)
}

/// Import this module to inject `Arc<SqlitePool>`.
#[module(
    providers = [ProviderDef::factory(|| async { connect(&database_url()).await })],
    exports = [SqlitePool]
)]
pub struct DbModule;
