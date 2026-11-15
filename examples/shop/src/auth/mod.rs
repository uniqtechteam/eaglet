//! Accounts: sign up, sign in with a JWT, and `AuthGuard` for routes that need a signed-in user.

mod controller;
mod dto;
mod guard;
mod jwt;
mod repository;
mod service;

use eaglet::prelude::*;

use crate::config::ConfigModule;
use crate::db::DbModule;

pub use controller::AuthController;
pub use dto::{CurrentUser, SignIn, SignUp, Token};
pub use guard::AuthGuard;
pub use jwt::JwtService;
pub use repository::{SqlUsersRepo, User, UsersRepo};
pub use service::AuthService;

/// Accounts and sign-in. Exports `AuthGuard` for other modules' routes.
#[module(
    imports = [ConfigModule, DbModule],
    controllers = [AuthController],
    providers = [AuthService, JwtService, AuthGuard, provide!(dyn UsersRepo => SqlUsersRepo)],
    exports = [AuthGuard, JwtService]
)]
pub struct AuthModule;
