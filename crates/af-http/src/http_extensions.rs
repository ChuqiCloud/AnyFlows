//! Optional HTTP extensions used by private distributions.
//!
//! The public core owns authentication and the common API surface. Extensions
//! contribute already-authenticated route trees and a small capability
//! descriptor, which keeps enterprise implementations out of the core router
//! composition code while allowing the distribution to assemble both parts.

use std::sync::Arc;

use axum::Router;

/// Metadata exposed by an extension to the distribution and frontend shells.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpExtensionDescriptor {
    /// Stable identifier used in capability payloads and release manifests.
    pub id: String,
    /// Human-readable display name for administration surfaces.
    pub name: String,
    /// Feature keys implemented by this extension.
    pub capabilities: Vec<String>,
}

impl HttpExtensionDescriptor {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            capabilities: capabilities.into_iter().map(Into::into).collect(),
        }
    }
}

/// Route trees contributed by registered extensions.
pub struct HttpExtensionRoutes {
    pub public: Router,
    pub management: Router,
    pub webhook: Router,
}

impl Default for HttpExtensionRoutes {
    fn default() -> Self {
        Self {
            public: Router::new(),
            management: Router::new(),
            webhook: Router::new(),
        }
    }
}

/// A self-contained HTTP extension.
///
/// Implementations must finish authentication and attach their own state
/// before returning a router. The core only merges the resulting `Router<()>`;
/// it does not need to know which services or database types the extension
/// uses.
pub trait HttpExtension: Send + Sync {
    fn descriptor(&self) -> HttpExtensionDescriptor;

    fn public_routes(&self) -> Router {
        Router::new()
    }

    fn management_routes(&self) -> Router {
        Router::new()
    }

    fn webhook_routes(&self) -> Router {
        Router::new()
    }
}

/// Collection of optional extensions assembled by a distribution.
#[derive(Default)]
pub struct HttpExtensions {
    extensions: Vec<Arc<dyn HttpExtension>>,
}

impl HttpExtensions {
    pub fn register(&mut self, extension: Arc<dyn HttpExtension>) {
        self.extensions.push(extension);
    }

    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
    }

    pub fn descriptors(&self) -> Vec<HttpExtensionDescriptor> {
        self.extensions
            .iter()
            .map(|extension| extension.descriptor())
            .collect()
    }

    pub fn into_routes(self) -> HttpExtensionRoutes {
        self.extensions
            .into_iter()
            .fold(HttpExtensionRoutes::default(), |mut routes, extension| {
                routes.public = routes.public.merge(extension.public_routes());
                routes.management = routes.management.merge(extension.management_routes());
                routes.webhook = routes.webhook.merge(extension.webhook_routes());
                routes
            })
    }
}
