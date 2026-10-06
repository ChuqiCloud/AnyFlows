//! Optional HTTP extensions used by private distributions.
//!
//! The public core owns authentication and the common API surface. Extensions
//! contribute already-authenticated route trees and a small capability
//! descriptor, which keeps enterprise implementations out of the core router
//! composition code while allowing the distribution to assemble both parts.

use std::sync::Arc;

use axum::{Json, Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Metadata exposed by an extension to the distribution and frontend shells.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
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

    /// Returns the OpenAPI fragment owned by this extension.
    ///
    /// The distribution merges the returned documents into the core document
    /// before exporting it or passing it to an API explorer.
    fn openapi_document(&self) -> Option<utoipa::openapi::OpenApi> {
        None
    }
}

/// Collection of optional extensions assembled by a distribution.
#[derive(Clone, Default)]
pub struct HttpExtensions {
    extensions: Vec<Arc<dyn HttpExtension>>,
}

impl HttpExtensions {
    pub fn register(&mut self, extension: Arc<dyn HttpExtension>) {
        self.extensions.push(extension);
    }

    /// Appends extensions assembled by a distribution-owned factory.
    pub fn extend(&mut self, other: Self) {
        self.extensions.extend(other.extensions);
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

    pub fn openapi_documents(&self) -> Vec<utoipa::openapi::OpenApi> {
        self.extensions
            .iter()
            .filter_map(|extension| extension.openapi_document())
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

/// Builds the public capability catalog consumed by both frontend shells.
pub fn build_extension_catalog_router(descriptors: Vec<HttpExtensionDescriptor>) -> Router {
    Router::new()
        .route("/api/extensions", get(list_extension_catalog))
        .with_state(Arc::new(descriptors))
}

#[utoipa::path(
    get,
    path = "/api/extensions",
    responses((status = 200, body = [HttpExtensionDescriptor]))
)]
pub(crate) async fn list_extension_catalog(
    State(descriptors): State<Arc<Vec<HttpExtensionDescriptor>>>,
) -> Json<Vec<HttpExtensionDescriptor>> {
    Json(descriptors.as_ref().clone())
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt as _;

    use super::*;

    #[tokio::test]
    async fn catalog_returns_registered_extension_metadata() {
        let mut extensions = HttpExtensions::default();
        extensions.register(Arc::new(TestExtension));
        let cloned = extensions.clone();

        assert_eq!(
            cloned.descriptors(),
            vec![HttpExtensionDescriptor::new(
                "enterprise",
                "Enterprise",
                ["organizations", "sso"],
            )]
        );

        let response = build_extension_catalog_router(extensions.descriptors())
            .oneshot(
                Request::builder()
                    .uri("/api/extensions")
                    .body(Body::empty())
                    .expect("valid catalog request"),
            )
            .await
            .expect("catalog route should respond");
        assert!(response.status().is_success());
        let body = to_bytes(response.into_body(), 8 * 1024)
            .await
            .expect("catalog body should be readable");
        let payload: Vec<HttpExtensionDescriptor> =
            serde_json::from_slice(&body).expect("catalog body should be JSON");
        assert_eq!(payload, cloned.descriptors());
    }

    struct TestExtension;

    impl HttpExtension for TestExtension {
        fn descriptor(&self) -> HttpExtensionDescriptor {
            HttpExtensionDescriptor::new("enterprise", "Enterprise", ["organizations", "sso"])
        }
    }
}
