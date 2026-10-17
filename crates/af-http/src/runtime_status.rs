//! Administrator-only, precomputed operational snapshots.

use std::{future::Future, pin::Pin, sync::Arc};

use af_admin::SessionAuthenticator;
use axum::{Router, extract::State, middleware, response::Response, routing::get};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_session::no_store_json,
};

/// A bounded dependency check. No endpoint or credential is exposed.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct RuntimeDependency {
    pub kind: String,
    /// starting, healthy, unavailable, or not_configured.
    pub status: String,
    pub latency_ms: Option<u64>,
}

/// One resource sample; CPU is expressed in cores, not host percentages.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct RuntimeResourcePoint {
    pub sampled_at: i64,
    pub cpu_cores: Option<f64>,
    pub memory_bytes: Option<u64>,
}

/// One running process. The instance ID changes on every restart.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct RuntimeNode {
    pub node_id: String,
    pub instance_id: String,
    pub version: String,
    pub os: String,
    pub started_at: i64,
    pub sampled_at: i64,
    pub uptime_seconds: u64,
    /// starting, online, stale, or unknown when cluster observation failed.
    pub status: String,
    pub cpu_cores: Option<f64>,
    pub memory_bytes: Option<u64>,
    /// Container CPU quota, or host logical CPUs when no quota is present.
    pub cpu_capacity_cores: Option<f64>,
    pub host_memory_bytes: Option<u64>,
    pub host_memory_used_bytes: Option<u64>,
    pub container_memory_limit_bytes: Option<u64>,
    pub container_memory_bytes: Option<u64>,
    pub database: RuntimeDependency,
    pub redis: RuntimeDependency,
    /// At most 60 ten-second samples of this process.
    pub history: Vec<RuntimeResourcePoint>,
}

/// Persisted upstream prompt-cache usage; zero does not imply explicit reporting.
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct RuntimePromptCache {
    pub sampled_at: Option<i64>,
    pub period_start: i64,
    pub period_end: i64,
    pub status: String,
    /// Public usage ledger only; extension-owned service-account ledgers excluded.
    pub scope: String,
    pub upstream_requests: i64,
    pub estimated_requests: i64,
    pub positive_cache_requests: i64,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

/// A small operational snapshot. Shared dependencies are reported per observer.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RuntimeStatus {
    pub sampled_at: i64,
    pub local_instance_id: String,
    /// local, healthy, starting, unavailable, or capacity_reached.
    pub cluster_status: String,
    /// Last successful registry observation, retained on Redis failure.
    pub cluster_observed_at: Option<i64>,
    pub nodes: Vec<RuntimeNode>,
    pub prompt_cache: RuntimePromptCache,
}

pub type RuntimeStatusFuture<'a> = Pin<Box<dyn Future<Output = RuntimeStatus> + Send + 'a>>;

/// Implementations read in-memory state and must not probe infrastructure per request.
pub trait RuntimeStatusReader: Send + Sync {
    fn snapshot(&self) -> RuntimeStatusFuture<'_>;
}

/// Mount operational details behind both session and administrator authorization.
pub fn build_runtime_status_router(
    reader: Arc<dyn RuntimeStatusReader>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    Router::new()
        .route("/api/admin/runtime-status", get(read))
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(authenticator),
            authenticate_management_session,
        ))
        .with_state(reader)
}

async fn read(State(reader): State<Arc<dyn RuntimeStatusReader>>) -> Response {
    no_store_json(reader.snapshot().await)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use af_admin::{
        LoginCredentials, SessionAuthentication, SessionAuthenticationError,
        SessionAuthenticationFuture, SessionLoginFuture, SessionPrincipal, SessionRole,
    };
    use af_domain::{GroupId, UserId};
    use axum::body::{Body, to_bytes};
    use http::{Request, StatusCode, header::CACHE_CONTROL};
    use tower::ServiceExt;

    use super::*;

    struct Sessions;
    impl SessionAuthenticator for Sessions {
        fn login<'a>(&'a self, _: &'a LoginCredentials) -> SessionLoginFuture<'a> {
            Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
        }

        fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
            Box::pin(async move {
                let role = match token {
                    "admin-session" => SessionRole::Admin,
                    "user-session" => SessionRole::User,
                    _ => return Err(SessionAuthenticationError::InvalidSession),
                };
                Ok(SessionAuthentication::new(
                    SessionPrincipal::new(UserId::new(1).unwrap(), role),
                    GroupId::new(1).unwrap(),
                    u64::MAX,
                ))
            })
        }
    }

    struct Snapshot(AtomicUsize);
    impl RuntimeStatusReader for Snapshot {
        fn snapshot(&self) -> RuntimeStatusFuture<'_> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                RuntimeStatus {
                    sampled_at: 1_000,
                    local_instance_id: "sample".to_owned(),
                    cluster_status: "local".to_owned(),
                    cluster_observed_at: None,
                    nodes: vec![],
                    prompt_cache: RuntimePromptCache::default(),
                }
            })
        }
    }

    #[tokio::test]
    async fn operational_snapshot_requires_admin_and_is_never_cached() {
        let snapshot = Arc::new(Snapshot(AtomicUsize::new(0)));
        let router = build_runtime_status_router(snapshot.clone(), Arc::new(Sessions));
        for (token, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some("invalid-session"), StatusCode::UNAUTHORIZED),
            (Some("user-session"), StatusCode::FORBIDDEN),
        ] {
            let mut request = Request::builder().uri("/api/admin/runtime-status");
            if let Some(token) = token {
                request = request.header("authorization", format!("Bearer {token}"));
            }
            let response = router
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
        assert_eq!(snapshot.0.load(Ordering::SeqCst), 0);
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/admin/runtime-status")
                    .header("authorization", "Bearer admin-session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 16_384).await.unwrap()).unwrap();
        assert_eq!(value["cluster_status"], "local");
        assert_eq!(snapshot.0.load(Ordering::SeqCst), 1);
    }
}
