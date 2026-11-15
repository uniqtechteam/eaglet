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
mod context;
mod error;
mod guard;
mod http_error;
mod module;
#[cfg(feature = "validate")]
mod valid;

pub use app::{App, AppBuilder};
pub use container::{Container, Dep, Injectable, ProviderDef, short_type_name};
pub use context::{REQUEST_ID_HEADER, trace_id};
pub use eaglet_macros::{
    controller, delete, get, guard, injectable, layer, module, patch, post, put,
};
pub use error::StartupError;
pub use guard::Guard;
pub use http_error::HttpError;
pub use module::{Controller, ControllerDef, Module, ModuleDef, RouterFn};
#[cfg(feature = "validate")]
pub use valid::Valid;

/// Used by the code the macros generate; not a stable API.
#[doc(hidden)]
pub mod __private {
    pub use crate::guard::{guard_method, guard_router};
}

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
    #[cfg(feature = "validate")]
    pub use crate::Valid;
    pub use crate::{
        App, Guard, HttpError, ProviderDef, StartupError, controller, delete, get, guard,
        injectable, layer, module, patch, post, provide, put,
    };
    pub use axum::Json;
    pub use axum::extract::{Extension, Path, Query};
    pub use axum::http::StatusCode;
    pub use axum::http::request::Parts;
    pub use std::sync::Arc;
}
