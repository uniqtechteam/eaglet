//! Providers and the container that holds them once they are built.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::sync::Arc;

use crate::StartupError;

/// A provider type, identified by its `TypeId`, with a readable name for messages.
#[derive(Clone, Copy, Debug)]
pub struct Dep {
    /// Identity of the type.
    pub id: TypeId,
    /// Full type name (`std::any::type_name`).
    pub name: &'static str,
}

impl Dep {
    /// The dependency on `T` (which may be a trait object such as `dyn UsersRepo`).
    #[must_use]
    pub fn of<T: ?Sized + 'static>() -> Self {
        Self {
            id: TypeId::of::<T>(),
            name: std::any::type_name::<T>(),
        }
    }

    /// The name without module paths, for example `UsersService` or `dyn UsersRepo`.
    #[must_use]
    pub fn short_name(&self) -> String {
        short_type_name(self.name)
    }
}

impl PartialEq for Dep {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Dep {}

impl Hash for Dep {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// `my_app::users::UsersService` → `UsersService`; `alloc::sync::Arc<a::B>` → `Arc<B>`.
#[must_use]
pub fn short_type_name(full: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        out.push_str(word.rsplit("::").next().unwrap_or(word));
        word.clear();
    };
    for c in full.chars() {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

type Built = Box<dyn Any + Send + Sync>;
type BuildFuture = Pin<Box<dyn Future<Output = Result<Built, StartupError>> + Send>>;
type BuildFn = Arc<dyn Fn(&Container) -> BuildFuture + Send + Sync>;

/// Everything built at startup, one shared instance per provider type.
#[derive(Default)]
pub struct Container {
    items: HashMap<TypeId, Built>,
}

impl Container {
    /// The built provider of type `T`.
    ///
    /// # Errors
    /// [`StartupError::NotBuilt`] when `T` has not been built (eaglet builds in dependency order, so
    /// this only happens if `T` was never declared).
    pub fn get<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, StartupError> {
        self.items
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<Arc<T>>())
            .cloned()
            .ok_or_else(|| StartupError::NotBuilt {
                name: short_type_name(std::any::type_name::<T>()),
            })
    }

    pub(crate) fn insert(&mut self, id: TypeId, value: Built) {
        self.items.insert(id, value);
    }
}

impl fmt::Debug for Container {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Container")
            .field("providers", &self.items.len())
            .finish()
    }
}

/// A struct whose fields are other providers. Implemented by `#[injectable]`.
pub trait Injectable: Sized + Send + Sync + 'static {
    /// The providers this one needs.
    fn deps() -> Vec<Dep>;
    /// Build it from providers that are already in the container.
    ///
    /// # Errors
    /// When a dependency is missing.
    fn build(container: &Container) -> Result<Self, StartupError>;
}

/// How to build one provider. Written in `#[module(providers = [...])]`.
#[derive(Clone)]
pub struct ProviderDef {
    /// What this provides.
    pub key: Dep,
    /// What it needs first.
    pub deps: Vec<Dep>,
    pub(crate) build: BuildFn,
}

impl fmt::Debug for ProviderDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderDef")
            .field("key", &self.key.short_name())
            .field(
                "deps",
                &self.deps.iter().map(Dep::short_name).collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

fn boxed<T: ?Sized + Send + Sync + 'static>(value: Arc<T>) -> Built {
    Box::new(value)
}

impl ProviderDef {
    /// An `#[injectable]` struct.
    #[must_use]
    pub fn of<T: Injectable>() -> Self {
        Self {
            key: Dep::of::<T>(),
            deps: T::deps(),
            build: Arc::new(|c| {
                let built = T::build(c).map(|v| boxed(Arc::new(v)));
                Box::pin(async move { built })
            }),
        }
    }

    /// A ready value, for example the app's configuration.
    #[must_use]
    pub fn value<T: Send + Sync + 'static>(value: T) -> Self {
        Self::shared(Arc::new(value))
    }

    /// A ready shared value, including trait objects: `ProviderDef::shared::<dyn Repo>(Arc::new(PgRepo))`.
    #[must_use]
    pub fn shared<T: ?Sized + Send + Sync + 'static>(value: Arc<T>) -> Self {
        Self {
            key: Dep::of::<T>(),
            deps: Vec::new(),
            build: Arc::new(move |_| {
                let value = Arc::clone(&value);
                Box::pin(async move { Ok(boxed(value)) })
            }),
        }
    }

    /// A value created by an async function at startup, for example a database pool.
    #[must_use]
    pub fn factory<T, F, Fut, E>(make: F) -> Self
    where
        T: Send + Sync + 'static,
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        E: fmt::Display,
    {
        Self {
            key: Dep::of::<T>(),
            deps: Vec::new(),
            build: Arc::new(move |_| {
                let fut = make();
                Box::pin(async move {
                    fut.await
                        .map(|v| boxed(Arc::new(v)))
                        .map_err(|e| StartupError::Factory {
                            provider: short_type_name(std::any::type_name::<T>()),
                            message: e.to_string(),
                        })
                })
            }),
        }
    }

    /// Provide trait `T` with the `#[injectable]` struct `C`. Use the `provide!` macro:
    /// `provide!(dyn UsersRepo => PgUsersRepo)`.
    #[must_use]
    pub fn bind<T: ?Sized + Send + Sync + 'static, C: Injectable>(
        coerce: fn(Arc<C>) -> Arc<T>,
    ) -> Self {
        Self {
            key: Dep::of::<T>(),
            deps: C::deps(),
            build: Arc::new(move |c| {
                let built = C::build(c).map(|v| boxed(coerce(Arc::new(v))));
                Box::pin(async move { built })
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::short_type_name;

    #[test]
    fn short_names_drop_module_paths() {
        assert_eq!(
            short_type_name("my_app::users::UsersService"),
            "UsersService"
        );
        assert_eq!(
            short_type_name("dyn my_app::users::UsersRepo"),
            "dyn UsersRepo"
        );
        assert_eq!(short_type_name("alloc::sync::Arc<my_app::Db>"), "Arc<Db>");
    }
}
