use super::*;
use af_admin::{
    LoginCredentials, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_domain::{GroupId, UserId};
use axum::body::{Body, to_bytes};
use http::{Request, StatusCode, header::CACHE_CONTROL};
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;
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

struct Disabled(AtomicUsize);
impl SoftwareUpdateService for Disabled {
    fn status(&self) -> SoftwareUpdateFuture<'_> {
        Box::pin(async {
            Ok(SoftwareStatus {
                launch_mode: "direct".to_owned(),
                launcher_protocol: None,
                launcher_version: None,
                update_phase: "idle".to_owned(),
                launcher_error: None,
                edition: "custom".to_owned(),
                version: "1.0.0".to_owned(),
                core_version: "0.3.0".to_owned(),
                commit: None,
                build_time: None,
                target: "test".to_owned(),
                deployment: "binary".to_owned(),
                node_id: "node".to_owned(),
                check_enabled: false,
                install_enabled: false,
                install_blocked_reason: Some("host_managed".to_owned()),
                restart_required: false,
                checked_at: None,
                latest: None,
                update_available: false,
            })
        })
    }
    fn check(&self) -> SoftwareUpdateFuture<'_> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(SoftwareUpdateError::Disabled) })
    }
    fn install(&self, _: String, _: String) -> SoftwareUpdateFuture<'_> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(SoftwareUpdateError::Disabled) })
    }
}

#[tokio::test]
async fn all_software_routes_require_an_administrator_and_are_not_cached() {
    let service = Arc::new(Disabled(AtomicUsize::new(0)));
    let router = build_software_update_router(service.clone(), Arc::new(Sessions));
    for (path, method) in [
        ("/api/admin/software", "GET"),
        ("/api/admin/software/check", "POST"),
        ("/api/admin/software/install", "POST"),
    ] {
        for (token, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some("invalid-session"), StatusCode::UNAUTHORIZED),
            (Some("user-session"), StatusCode::FORBIDDEN),
        ] {
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json");
            if let Some(token) = token {
                request = request.header("authorization", format!("Bearer {token}"));
            }
            let response = router
                .clone()
                .oneshot(
                    request
                        .body(Body::from(r#"{"tag":"v1.0.0","expected_node_id":"node"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{method} {path}");
        }
    }
    assert_eq!(service.0.load(Ordering::SeqCst), 0);
    for path in ["/api/admin/software/check", "/api/admin/software/install"] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("authorization", "Bearer admin-session")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"tag":"v1.0.0","expected_node_id":"node"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    }
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/admin/software")
                .header("authorization", "Bearer admin-session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let value: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 16384).await.unwrap()).unwrap();
    assert_eq!(value["check_enabled"], false);
    assert_eq!(value["install_enabled"], false);
}
