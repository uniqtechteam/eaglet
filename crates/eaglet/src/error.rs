//! Startup errors: problems in the module graph, reported before the server takes a request.

use std::fmt;

/// Something in the module graph cannot work. Every message names the provider and the module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupError {
    /// A provider depends on something its module neither provides nor imports.
    MissingProvider {
        /// The provider that has the dependency.
        provider: String,
        /// What it needs.
        needs: String,
        /// The module it is declared in.
        module: String,
    },
    /// Providers depend on each other in a circle; `path` starts and ends with the same name.
    Cycle {
        /// The circle, for example `["A", "B", "A"]`.
        path: Vec<String>,
    },
    /// The same provider type is declared by two modules.
    Duplicate {
        /// The provider.
        provider: String,
        /// The first module that declares it.
        first: String,
        /// The second module that declares it.
        second: String,
    },
    /// A module exports something it does not provide or import.
    BadExport {
        /// The exporting module.
        module: String,
        /// What it tries to export.
        export: String,
    },
    /// An async factory returned an error.
    Factory {
        /// The provider the factory builds.
        provider: String,
        /// The factory's error message.
        message: String,
    },
    /// A provider was asked for before it was built.
    NotBuilt {
        /// The provider.
        name: String,
    },
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingProvider {
                provider,
                needs,
                module,
            } => {
                write!(
                    f,
                    "{provider} needs {needs}, which {module} does not provide or import"
                )
            }
            Self::Cycle { path } => {
                write!(
                    f,
                    "providers depend on each other in a circle: {}",
                    path.join(" → ")
                )
            }
            Self::Duplicate {
                provider,
                first,
                second,
            } => write!(
                f,
                "{provider} is provided by both {first} and {second}; provide it in one module and export it"
            ),
            Self::BadExport { module, export } => {
                write!(
                    f,
                    "{module} exports {export}, which it does not provide or import"
                )
            }
            Self::Factory { provider, message } => {
                write!(f, "could not create {provider}: {message}")
            }
            Self::NotBuilt { name } => {
                write!(
                    f,
                    "{name} was asked for before it was built (this is a bug in eaglet)"
                )
            }
        }
    }
}

impl std::error::Error for StartupError {}
