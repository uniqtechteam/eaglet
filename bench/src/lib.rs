//! The same three endpoints written twice, once with plain Axum and once with eaglet, so the
//! difference in throughput and latency is only what eaglet adds on the request path.
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /users/:id` | path parameter in, a small JSON object out |
//! | `POST /users` | JSON body checked with `validator`, 201 with the created user, 422 when invalid |
//! | `GET /me` | a bearer-token guard runs first, the handler reads the user it put on the request |
//!
//! Both versions call the same [`shared::Users`] through an `Arc`, which is how a plain
//! Axum app would hold its state and how eaglet hands providers to controllers.

#![forbid(unsafe_code)]

pub mod eaglet_app;
pub mod plain;
pub mod shared;
