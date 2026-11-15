//! An eaglet example shop: people sign up, sign in with a JWT, and sell products that only they
//! can change. Storage is SQLite through SQLx; every repository is a trait, so tests swap in fakes.

pub mod app_module;
pub mod auth;
pub mod config;
pub mod db;
pub mod products;
