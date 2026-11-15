//! What the product routes accept and return.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

/// A product for sale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Product {
    /// Set by the server.
    pub id: i64,
    /// The user who sells it.
    pub owner_id: i64,
    /// The display name.
    pub name: String,
    /// The price in cents, so no rounding happens.
    pub price_cents: i64,
}

/// The body of `POST /products`.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct NewProduct {
    /// 1 to 200 characters.
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    /// At least 1 cent.
    #[validate(range(min = 1, max = 100_000_000))]
    pub price_cents: i64,
}

/// The body of `PATCH /products/:id`; fields left out stay as they are.
#[derive(Debug, Default, Deserialize, Validate, ToSchema)]
pub struct ProductChanges {
    /// 1 to 200 characters.
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,
    /// At least 1 cent.
    #[validate(range(min = 1, max = 100_000_000))]
    pub price_cents: Option<i64>,
}

/// `GET /products?limit=20&offset=0`.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ListQuery {
    /// 1 to 100, 20 by default.
    #[validate(range(min = 1, max = 100))]
    pub limit: Option<u32>,
    /// How many to skip, 0 by default.
    pub offset: Option<u32>,
}
