//! The catalog rules: only the owner changes or removes a product.

use eaglet::prelude::*;

use super::dto::{NewProduct, Product, ProductChanges};
use super::repository::ProductsRepo;
use crate::auth::CurrentUser;

/// Lists, finds, creates, changes and removes products.
#[injectable]
pub struct ProductsService {
    repo: Arc<dyn ProductsRepo>,
}

impl ProductsService {
    /// A page of products.
    ///
    /// # Errors
    /// When the database fails.
    pub async fn list(&self, limit: u32, offset: u32) -> Result<Vec<Product>, HttpError> {
        self.repo.list(limit, offset).await
    }

    /// One product.
    ///
    /// # Errors
    /// `NOT_FOUND` when there is no product with that id.
    pub async fn find(&self, id: i64) -> Result<Product, HttpError> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| HttpError::not_found(format!("no product with id {id}")))
    }

    /// Put a product on sale for `seller`.
    ///
    /// # Errors
    /// When the database fails.
    pub async fn create(
        &self,
        seller: &CurrentUser,
        input: NewProduct,
    ) -> Result<Product, HttpError> {
        self.repo.insert(seller.id, input).await
    }

    /// Change a product; only its owner may.
    ///
    /// # Errors
    /// `NOT_FOUND` or `FORBIDDEN`.
    pub async fn update(
        &self,
        user: &CurrentUser,
        id: i64,
        changes: ProductChanges,
    ) -> Result<Product, HttpError> {
        let mut product = self.owned(user, id).await?;
        if let Some(name) = changes.name {
            product.name = name;
        }
        if let Some(price) = changes.price_cents {
            product.price_cents = price;
        }
        self.repo.save(&product).await?;
        Ok(product)
    }

    /// Remove a product; only its owner may.
    ///
    /// # Errors
    /// `NOT_FOUND` or `FORBIDDEN`.
    pub async fn remove(&self, user: &CurrentUser, id: i64) -> Result<(), HttpError> {
        self.owned(user, id).await?;
        self.repo.delete(id).await
    }

    async fn owned(&self, user: &CurrentUser, id: i64) -> Result<Product, HttpError> {
        let product = self.find(id).await?;
        if product.owner_id == user.id {
            Ok(product)
        } else {
            Err(HttpError::forbidden(
                "only the seller can change this product",
            ))
        }
    }
}
