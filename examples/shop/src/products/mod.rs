//! Products: anyone can browse; signed-in users sell products that only they can change.

mod controller;
mod dto;
mod repository;
mod service;

use eaglet::prelude::*;

use crate::auth::AuthModule;
use crate::db::DbModule;

pub use controller::ProductsController;
pub use dto::{ListQuery, NewProduct, Product, ProductChanges};
pub use repository::{ProductsRepo, SqlProductsRepo};
pub use service::ProductsService;

/// The product catalog.
#[module(
    imports = [AuthModule, DbModule],
    controllers = [ProductsController],
    providers = [ProductsService, provide!(dyn ProductsRepo => SqlProductsRepo)]
)]
pub struct ProductsModule;
