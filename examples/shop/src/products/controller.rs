//! The `/products` routes. Reading is public; changing needs a signed-in user.

use eaglet::prelude::*;

use super::dto::{ListQuery, NewProduct, Product, ProductChanges};
use super::service::ProductsService;
use crate::auth::{AuthGuard, CurrentUser};

/// The HTTP side of the catalog.
#[injectable]
pub struct ProductsController {
    products: Arc<ProductsService>,
}

#[controller("/products")]
impl ProductsController {
    /// List products, newest first.
    #[get("/")]
    async fn list(
        &self,
        Valid(Query(q)): Valid<Query<ListQuery>>,
    ) -> Result<Json<Vec<Product>>, HttpError> {
        let page = self
            .products
            .list(q.limit.unwrap_or(20), q.offset.unwrap_or(0))
            .await?;
        Ok(Json(page))
    }

    /// Find one product.
    #[get("/:id")]
    async fn find(&self, Path(id): Path<i64>) -> Result<Json<Product>, HttpError> {
        self.products.find(id).await.map(Json)
    }

    /// Put a product on sale. Needs a signed-in user, who becomes its seller.
    #[post("/")]
    #[guard(AuthGuard)]
    async fn create(
        &self,
        Extension(user): Extension<CurrentUser>,
        Valid(Json(input)): Valid<Json<NewProduct>>,
    ) -> Result<(StatusCode, Json<Product>), HttpError> {
        let product = self.products.create(&user, input).await?;
        Ok((StatusCode::CREATED, Json(product)))
    }

    /// Change a product. Only its seller may.
    #[patch("/:id")]
    #[guard(AuthGuard)]
    async fn update(
        &self,
        Extension(user): Extension<CurrentUser>,
        Path(id): Path<i64>,
        Valid(Json(changes)): Valid<Json<ProductChanges>>,
    ) -> Result<Json<Product>, HttpError> {
        self.products.update(&user, id, changes).await.map(Json)
    }

    /// Remove a product. Only its seller may.
    #[delete("/:id")]
    #[guard(AuthGuard)]
    async fn remove(
        &self,
        Extension(user): Extension<CurrentUser>,
        Path(id): Path<i64>,
    ) -> Result<StatusCode, HttpError> {
        self.products.remove(&user, id).await?;
        Ok(StatusCode::NO_CONTENT)
    }
}
