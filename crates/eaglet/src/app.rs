//! The application: the checked module tree, the built providers and the HTTP router.

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use crate::module::resolve;
use crate::openapi::RouteDoc;
use crate::{Container, Module, ModuleDef, ProviderDef, StartupError};

/// A built eaglet application.
pub struct App {
    container: Arc<Container>,
    routes: axum::Router,
    docs: Vec<RouteDoc>,
    title: String,
    version: String,
    #[cfg_attr(
        not(feature = "swagger"),
        allow(dead_code, reason = "set by .swagger()")
    )]
    swagger: Option<String>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("container", &self.container)
            .field("title", &self.title)
            .field("routes", &self.docs.len())
            .finish_non_exhaustive()
    }
}

impl App {
    /// Check the module tree under `M`, build every provider and assemble the router.
    ///
    /// # Errors
    /// A [`StartupError`] that names the provider and module at fault.
    pub async fn new<M: Module>() -> Result<Self, StartupError> {
        Self::builder::<M>().build().await
    }

    /// Start building an app, for example to override providers in tests.
    #[must_use]
    pub fn builder<M: Module>() -> AppBuilder {
        AppBuilder {
            root: M::definition,
            overrides: HashMap::new(),
        }
    }

    /// The built provider of type `T`.
    ///
    /// # Errors
    /// When `T` is not provided anywhere in the app.
    pub fn provider<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, StartupError> {
        self.container.get::<T>()
    }

    /// The title and version shown in the API docs (by default the root module's name and `0.1.0`).
    #[must_use]
    pub fn with_info(mut self, title: impl Into<String>, version: impl Into<String>) -> Self {
        self.title = title.into();
        self.version = version.into();
        self
    }

    /// Wrap every route of the app, and the 404 for unknown paths, in a Tower layer: a timeout,
    /// CORS, security headers, compression. Layers added later run first. They run inside the
    /// error envelope, so an error they answer with (a `408` from a timeout, say) gets the
    /// envelope and the trace id like any other. The Swagger UI pages are not wrapped.
    ///
    /// ```ignore
    /// let app = App::new::<AppModule>()
    ///     .await?
    ///     .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(10)));
    /// ```
    #[must_use]
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: tower::Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
        L::Service: tower::Service<axum::extract::Request> + Clone + Send + Sync + 'static,
        <L::Service as tower::Service<axum::extract::Request>>::Response:
            axum::response::IntoResponse + 'static,
        <L::Service as tower::Service<axum::extract::Request>>::Error:
            Into<std::convert::Infallible> + 'static,
        <L::Service as tower::Service<axum::extract::Request>>::Future: Send + 'static,
    {
        self.routes = self.routes.layer(layer);
        self
    }

    /// The OpenAPI 3.1 document for every route, for example to write it to a file and generate a
    /// client from it.
    #[cfg(feature = "openapi")]
    #[must_use]
    pub fn openapi(&self) -> utoipa::openapi::OpenApi {
        crate::openapi::document(&self.title, &self.version, &self.docs)
    }

    /// Serve Swagger UI at `path` and the OpenAPI document at `{path}/openapi.json`, for example
    /// `.swagger("/docs")`.
    #[cfg(feature = "swagger")]
    #[must_use]
    pub fn swagger(mut self, path: &str) -> Self {
        let path = format!("/{}", path.trim_matches('/'));
        self.swagger = Some(path);
        self
    }

    /// The finished router: the routes, the docs pages when switched on, and the request layer
    /// that adds trace ids and the error envelope.
    fn finish(&self) -> axum::Router {
        #[allow(unused_mut, reason = "only changed with the swagger feature")]
        let mut router = self.routes.clone();
        #[cfg(feature = "swagger")]
        if let Some(path) = &self.swagger {
            router = router.merge(self.docs_routes(path));
        }
        router.layer(crate::context::RequestContextLayer)
    }

    #[cfg(feature = "swagger")]
    fn docs_routes(&self, path: &str) -> axum::Router {
        use axum::http::header;
        use axum::routing::get;

        let base = if path == "/" { "" } else { path };
        let json_path = format!("{base}/openapi.json");
        let json = self.openapi().to_json().unwrap_or_else(|_| "{}".to_owned());
        let html = crate::openapi::swagger_html(&self.title, &json_path);
        let json_page = move || {
            let json = json.clone();
            async move { ([(header::CONTENT_TYPE, "application/json")], json) }
        };
        let html_page = move || {
            let html = html.clone();
            async move { axum::response::Html(html) }
        };
        axum::Router::new()
            .route(path, get(html_page))
            .route(&json_path, get(json_page))
    }

    /// A copy of the router, for the in-memory test client.
    #[cfg(feature = "testing")]
    pub(crate) fn router(&self) -> axum::Router {
        self.finish()
    }

    /// The Axum router, to serve it yourself or mount it inside another router.
    pub fn into_router(self) -> axum::Router {
        self.finish()
    }

    /// Serve the app on `addr` until Ctrl-C or SIGTERM, then let the requests in flight finish
    /// (for up to 30 seconds) before returning.
    ///
    /// A client gets 30 seconds to send a request's headers, which is also how long a kept-alive
    /// connection may sit idle, so slow or stuck clients cannot hold connections open. Handlers
    /// can read the peer address with `ConnectInfo<SocketAddr>`.
    ///
    /// # Errors
    /// When the address cannot be bound.
    pub async fn listen(self, addr: impl tokio::net::ToSocketAddrs) -> std::io::Result<()> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        self.serve(listener, crate::serve::stop_signal()).await
    }

    /// Like [`App::listen`], on a listener you bound yourself and until `stop` finishes instead of
    /// a signal.
    ///
    /// ```no_run
    /// # async fn run(app: eaglet::App) -> std::io::Result<()> {
    /// let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    /// let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    /// # drop(stop);
    /// app.serve(listener, async { stopped.await.ok(); }).await
    /// # }
    /// ```
    ///
    /// # Errors
    /// Reserved for server failures; a stop is `Ok(())`.
    pub async fn serve(
        self,
        listener: tokio::net::TcpListener,
        stop: impl Future<Output = ()> + Send,
    ) -> std::io::Result<()> {
        crate::serve::serve(listener, self.into_router(), stop).await
    }
}

/// Builds an [`App`], optionally with providers replaced.
pub struct AppBuilder {
    root: fn() -> ModuleDef,
    overrides: HashMap<TypeId, ProviderDef>,
}

impl std::fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppBuilder")
            .field("overrides", &self.overrides.len())
            .finish_non_exhaustive()
    }
}

impl AppBuilder {
    /// Use `value` wherever `T` is needed, instead of the declared provider. The declared provider's
    /// dependencies are still checked.
    #[must_use]
    pub fn override_provider<T: ?Sized + Send + Sync + 'static>(mut self, value: Arc<T>) -> Self {
        self.overrides
            .insert(TypeId::of::<T>(), ProviderDef::shared(value));
        self
    }

    /// Check, build and assemble the app.
    ///
    /// # Errors
    /// A [`StartupError`] that names the provider and module at fault.
    pub async fn build(self) -> Result<App, StartupError> {
        let resolved = resolve(self.root, &self.overrides)?;
        let mut container = Container::default();
        for provider in resolved.providers {
            let built = (provider.build)(&container).await?;
            container.insert(provider.key.id, built);
        }
        let mut routes = axum::Router::new();
        for build_routes in resolved.routers {
            routes = routes.merge(build_routes(&container)?);
        }
        Ok(App {
            container: Arc::new(container),
            routes,
            docs: resolved.docs.iter().flat_map(|docs| docs()).collect(),
            title: (self.root)().name.to_owned(),
            version: "0.1.0".to_owned(),
            swagger: None,
        })
    }
}
