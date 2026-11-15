//! Where products are stored.

use async_trait::async_trait;
use eaglet::prelude::*;
use sqlx::SqlitePool;

use super::dto::{NewProduct, Product};

/// Storage for products.
#[async_trait]
pub trait ProductsRepo: Send + Sync {
    /// A page of products, newest first.
    ///
    /// # Errors
    /// When the database fails.
    async fn list(&self, limit: u32, offset: u32) -> Result<Vec<Product>, HttpError>;

    /// One product.
    ///
    /// # Errors
    /// When the database fails.
    async fn get(&self, id: i64) -> Result<Option<Product>, HttpError>;

    /// Store a new product for `owner_id`.
    ///
    /// # Errors
    /// When the database fails.
    async fn insert(&self, owner_id: i64, input: NewProduct) -> Result<Product, HttpError>;

    /// Save a changed product.
    ///
    /// # Errors
    /// When the database fails.
    async fn save(&self, product: &Product) -> Result<(), HttpError>;

    /// Remove a product.
    ///
    /// # Errors
    /// When the database fails.
    async fn delete(&self, id: i64) -> Result<(), HttpError>;
}

/// Products in SQLite.
#[injectable]
pub struct SqlProductsRepo {
    pool: Arc<SqlitePool>,
}

type Row = (i64, i64, String, i64);

fn product((id, owner_id, name, price_cents): Row) -> Product {
    Product {
        id,
        owner_id,
        name,
        price_cents,
    }
}

#[async_trait]
impl ProductsRepo for SqlProductsRepo {
    async fn list(&self, limit: u32, offset: u32) -> Result<Vec<Product>, HttpError> {
        let rows = sqlx::query_as::<_, Row>(
            "select id, owner_id, name, price_cents from products order by id desc limit ? offset ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
        .map_err(HttpError::internal)?;
        Ok(rows.into_iter().map(product).collect())
    }

    async fn get(&self, id: i64) -> Result<Option<Product>, HttpError> {
        let row = sqlx::query_as::<_, Row>(
            "select id, owner_id, name, price_cents from products where id = ?",
        )
        .bind(id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(HttpError::internal)?;
        Ok(row.map(product))
    }

    async fn insert(&self, owner_id: i64, input: NewProduct) -> Result<Product, HttpError> {
        let id = sqlx::query_scalar::<_, i64>(
            "insert into products (owner_id, name, price_cents) values (?, ?, ?) returning id",
        )
        .bind(owner_id)
        .bind(&input.name)
        .bind(input.price_cents)
        .fetch_one(&*self.pool)
        .await
        .map_err(HttpError::internal)?;
        Ok(Product {
            id,
            owner_id,
            name: input.name,
            price_cents: input.price_cents,
        })
    }

    async fn save(&self, p: &Product) -> Result<(), HttpError> {
        sqlx::query("update products set name = ?, price_cents = ? where id = ?")
            .bind(&p.name)
            .bind(p.price_cents)
            .bind(p.id)
            .execute(&*self.pool)
            .await
            .map_err(HttpError::internal)?;
        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<(), HttpError> {
        sqlx::query("delete from products where id = ?")
            .bind(id)
            .execute(&*self.pool)
            .await
            .map_err(HttpError::internal)?;
        Ok(())
    }
}
