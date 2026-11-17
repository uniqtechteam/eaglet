//! Testing: build the real module tree, swap providers for fakes, and call routes in memory, with
//! no TCP port and no network.
//!
//! ```ignore
//! let app = TestApp::new::<AppModule>()
//!     .override_provider::<dyn UsersRepo>(Arc::new(FakeUsersRepo::with_one_user()))
//!     .build()
//!     .await?;
//! let res = app.get("/users/1").send().await;
//! assert_eq!(res.status(), StatusCode::OK);
//! assert_eq!(res.json::<User>().name, "Ana");
//! ```
//!
//! Turn it on for tests only: `eaglet = { version = "…", features = ["testing"] }` under
//! `[dev-dependencies]`.

#![allow(
    clippy::panic,
    clippy::expect_used,
    reason = "test helpers: a bad header or an unexpected body should stop the test with a clear message"
)]

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tower::ServiceExt;

use crate::{App, AppBuilder, Module, StartupError};

/// A built app for tests, with an in-memory HTTP client.
#[derive(Debug)]
pub struct TestApp {
    app: App,
    router: axum::Router,
}

/// Builds a [`TestApp`]; see [`TestApp::new`].
#[derive(Debug)]
pub struct TestAppBuilder {
    inner: AppBuilder,
}

impl TestApp {
    /// Start from the module tree under `M`, exactly as the real app does.
    #[must_use]
    #[allow(
        clippy::new_ret_no_self,
        reason = "a builder that reads in order: new, override, build"
    )]
    pub fn new<M: Module>() -> TestAppBuilder {
        TestAppBuilder {
            inner: App::builder::<M>(),
        }
    }

    /// The built provider of type `T`, for assertions.
    ///
    /// # Errors
    /// When `T` is not provided anywhere in the app.
    pub fn provider<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, StartupError> {
        self.app.provider::<T>()
    }

    /// A request with any method.
    #[must_use]
    pub fn request(&self, method: Method, path: &str) -> TestRequest {
        TestRequest {
            router: self.router.clone(),
            builder: Request::builder().method(method).uri(path),
            body: Body::empty(),
        }
    }

    /// `GET path`.
    #[must_use]
    pub fn get(&self, path: &str) -> TestRequest {
        self.request(Method::GET, path)
    }

    /// `POST path`.
    #[must_use]
    pub fn post(&self, path: &str) -> TestRequest {
        self.request(Method::POST, path)
    }

    /// `PUT path`.
    #[must_use]
    pub fn put(&self, path: &str) -> TestRequest {
        self.request(Method::PUT, path)
    }

    /// `PATCH path`.
    #[must_use]
    pub fn patch(&self, path: &str) -> TestRequest {
        self.request(Method::PATCH, path)
    }

    /// `DELETE path`.
    #[must_use]
    pub fn delete(&self, path: &str) -> TestRequest {
        self.request(Method::DELETE, path)
    }
}

impl TestAppBuilder {
    /// Use `value` wherever `T` is needed, for example a fake repository behind a trait:
    /// `.override_provider::<dyn UsersRepo>(Arc::new(FakeUsersRepo))`. The value must be a `T`,
    /// so a wrong type does not compile.
    #[must_use]
    pub fn override_provider<T: ?Sized + Send + Sync + 'static>(self, value: Arc<T>) -> Self {
        Self {
            inner: self.inner.override_provider::<T>(value),
        }
    }

    /// Like [`override_provider`](Self::override_provider), for a concrete provider type:
    /// `.override_value(FakeClock::at(noon))`.
    #[must_use]
    pub fn override_value<T: Send + Sync + 'static>(self, value: T) -> Self {
        self.override_provider::<T>(Arc::new(value))
    }

    /// Check and build the app, with the same startup checks as the real one.
    ///
    /// # Errors
    /// A [`StartupError`] that names the provider and module at fault.
    pub async fn build(self) -> Result<TestApp, StartupError> {
        let app = self.inner.build().await?;
        let router = app.router();
        Ok(TestApp { app, router })
    }
}

/// A request being built; finish it with [`TestRequest::send`].
#[derive(Debug)]
pub struct TestRequest {
    router: axum::Router,
    builder: axum::http::request::Builder,
    body: Body,
}

impl TestRequest {
    /// Add a header.
    ///
    /// # Panics
    /// When the name or value is not a valid header.
    #[must_use]
    pub fn header<K, V>(mut self, name: K, value: V) -> Self
    where
        K: TryInto<HeaderName>,
        V: TryInto<HeaderValue>,
    {
        let name = name
            .try_into()
            .unwrap_or_else(|_| panic!("TestRequest::header: not a valid header name"));
        let value = value
            .try_into()
            .unwrap_or_else(|_| panic!("TestRequest::header: not a valid value for {name}"));
        self.builder = self.builder.header(name, value);
        self
    }

    /// `Authorization: Bearer <token>`.
    #[must_use]
    pub fn bearer(self, token: &str) -> Self {
        self.header(header::AUTHORIZATION, format!("Bearer {token}"))
    }

    /// Send `value` as a JSON body.
    ///
    /// # Panics
    /// When `value` cannot be turned into JSON.
    #[must_use]
    pub fn json<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        let bytes = serde_json::to_vec(value).expect("TestRequest::json: the value is not JSON");
        self.body = Body::from(bytes);
        self.header(header::CONTENT_TYPE, "application/json")
    }

    /// Send raw bytes or text as the body.
    #[must_use]
    pub fn body(mut self, body: impl Into<Body>) -> Self {
        self.body = body.into();
        self
    }

    /// Run the request through the app in memory.
    ///
    /// # Panics
    /// When the method or path is not valid, or the body cannot be read.
    pub async fn send(self) -> TestResponse {
        let req = self
            .builder
            .body(self.body)
            .expect("TestRequest::send: the method or path is not valid");
        let Ok(res) = self.router.oneshot(req).await;
        let status = res.status();
        let headers = res.headers().clone();
        let body = res
            .into_body()
            .collect()
            .await
            .expect("TestRequest::send: the response body could not be read")
            .to_bytes();
        TestResponse {
            status,
            headers,
            body,
        }
    }
}

/// A finished response: status, headers and the whole body.
#[derive(Debug, Clone)]
pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl TestResponse {
    /// The status.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// All headers.
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// One header as text, if present.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// The body bytes.
    #[must_use]
    pub fn bytes(&self) -> &Bytes {
        &self.body
    }

    /// The body as text.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The body parsed as JSON.
    ///
    /// # Panics
    /// When the body is not the expected JSON; the message shows the body.
    #[must_use]
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| {
            panic!(
                "the response ({}) is not the expected JSON: {e}\nbody: {}",
                self.status,
                self.text()
            )
        })
    }
}
