//! The application: the checked module tree, the built providers and the HTTP router.

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use crate::module::resolve;
use crate::{Container, Module, ModuleDef, ProviderDef, StartupError};

/// A built eaglet application.
pub struct App {
    container: Arc<Container>,
    router: axum::Router,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("container", &self.container)
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

    /// The Axum router, to serve it yourself or nest it into another router.
    pub fn into_router(self) -> axum::Router {
        self.router
    }

    /// Serve the app on `addr` until the process stops.
    ///
    /// # Errors
    /// When the address cannot be bound or the server fails.
    pub async fn listen(self, addr: impl tokio::net::ToSocketAddrs) -> std::io::Result<()> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, self.router).await
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
        let mut router = axum::Router::new();
        for build_routes in resolved.routers {
            router = router.merge(build_routes(&container)?);
        }
        Ok(App {
            container: Arc::new(container),
            router,
        })
    }
}
