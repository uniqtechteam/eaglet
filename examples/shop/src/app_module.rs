//! The root module.

use eaglet::prelude::*;

use crate::auth::AuthModule;
use crate::config::ConfigModule;
use crate::db::DbModule;
use crate::products::ProductsModule;

/// The root of the module tree.
#[module(imports = [ConfigModule, DbModule, AuthModule, ProductsModule])]
pub struct AppModule;
