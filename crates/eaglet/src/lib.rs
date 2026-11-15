//! eaglet brings the `NestJS` structure to Rust web apps: modules, providers, controllers and guards,
//! on top of plain Axum.
//!
//! ```ignore
//! use eaglet::prelude::*;
//!
//! #[injectable]
//! struct Clock;
//!
//! #[injectable]
//! struct Greeter {
//!     clock: Arc<Clock>,
//! }
//!
//! #[module(providers = [Clock, Greeter])]
//! struct AppModule;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let app = App::new::<AppModule>().await?;
//! # Ok(()) }
//! ```

extern crate self as eaglet;

mod app;
mod container;
mod error;
mod module;

pub use app::{App, AppBuilder};
pub use container::{Container, Dep, Injectable, ProviderDef, short_type_name};
pub use eaglet_macros::{injectable, module};
pub use error::StartupError;
pub use module::{Module, ModuleDef};

/// Axum, re-exported so apps and generated code use the same version.
pub use axum;

/// The eaglet version, shown by `eaglet --version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Provide a trait with an `#[injectable]` struct: `provide!(dyn UsersRepo => PgUsersRepo)`.
#[macro_export]
macro_rules! provide {
    (dyn $trait:path => $concrete:ty) => {
        $crate::ProviderDef::bind::<dyn $trait, $concrete>(
            |inner: ::std::sync::Arc<$concrete>| -> ::std::sync::Arc<dyn $trait> { inner },
        )
    };
}

/// Everything an app usually needs: `use eaglet::prelude::*;`.
pub mod prelude {
    pub use crate::{App, ProviderDef, StartupError, injectable, module, provide};
    pub use std::sync::Arc;
}
