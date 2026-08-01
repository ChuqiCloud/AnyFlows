use std::{
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    LoginCredentials, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole, UserInvitationError, UserInvitationReadFuture, UserInvitationRebate,
    UserInvitationService, UserInvitationSummary,
};
use af_domain::{GroupId, Quota, UserId};
use axum::{
    body::{Body, to_bytes},
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CACHE_CONTROL},
    },
};
use serde_json::Value;
use tower::ServiceExt as _;

use crate::invitations::build_user_invitation_router;

const USER_TOKEN: &str = "invitation-user-token";

struct FakeSessions;

impl SessionAuthenticator for FakeSessions {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async move {
            if token != USER_TOKEN {
                return Err(SessionAuthenticationError::InvalidSession);
            }
            Ok(SessionAuthentication::new(
                SessionPrincipal::new(UserId::new(7).unwrap(), SessionRole::User),
                GroupId::new(3).unwrap(),
                4_000_000_000,
            ))
        })
    }
}

struct FakeInvitations {
    calls: AtomicUsize,
}

impl UserInvitationService for FakeInvitations {
    fn get<'a>(&'a self, principal: SessionPrincipal) -> UserInvitationReadFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.user_id() != UserId::new(7).unwrap() {
                return Err(UserInvitationError::InvalidSession);
            }
            Ok(UserInvitationSummary::from_parts(
                "af-AAAAAAAAAAAAAAAAAAAAAA".to_owned(),
                3,
                2,
                Quota::new(40).unwrap(),
                Quota::new(60).unwrap(),
                vec![UserInvitationRebate::from_parts(
                    Quota::new(20).unwrap(),
                    1_800_000_000,
                )],
            ))
        })
    }
}

#[tokio::test]
async fn invitation_route_is_session_scoped_and_omits_invitee_identity() {
    let service = Arc::new(FakeInvitations {
        calls: AtomicUsize::new(0),
    });
    let service_port: Arc<dyn UserInvitationService> = service.clone();
    let session_port: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessions);
    let app = build_user_invitation_router(service_port, session_port);

    let unauthorized = app
        .clone()
        .oneshot(
            Request::get("/api/account/invitations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(service.calls.load(Ordering::Relaxed), 0);

    let response = app
        .oneshot(
            Request::get("/api/account/invitations")
                .header(AUTHORIZATION, format!("Bearer {USER_TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["invite_code"], "af-AAAAAAAAAAAAAAAAAAAAAA");
    assert_eq!(body["invited_count"], 3);
    assert_eq!(body["credited_count"], 2);
    assert_eq!(body["recent_rebates"][0]["quota_amount"], 20);
    let rendered = body.to_string();
    for forbidden in ["invitee", "username", "email", "user_id"] {
        assert!(!rendered.contains(forbidden), "{rendered}");
    }
    assert_eq!(service.calls.load(Ordering::Relaxed), 1);
}
