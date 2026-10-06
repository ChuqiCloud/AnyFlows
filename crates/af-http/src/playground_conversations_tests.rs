use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    LoginCredentials, PlaygroundConversationDeleteFuture, PlaygroundConversationError,
    PlaygroundConversationListFuture, PlaygroundConversationReadFuture,
    PlaygroundConversationSaveCommand, PlaygroundConversationSaveFuture,
    PlaygroundConversationService, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole,
};
use af_domain::{GroupId, UserId};
use axum::body::{Body, to_bytes};
use http::{Request, StatusCode, header::AUTHORIZATION};
use tower::ServiceExt;

use crate::playground_conversations::build_playground_conversation_router;

struct AcceptSession;

impl SessionAuthenticator for AcceptSession {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, _token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async {
            Ok(SessionAuthentication::new(
                SessionPrincipal::new(UserId::new(7).unwrap(), SessionRole::User),
                GroupId::new(3).unwrap(),
                u64::MAX,
            ))
        })
    }
}

#[derive(Default)]
struct RejectConversationService {
    save_calls: AtomicUsize,
    list_calls: AtomicUsize,
    read_calls: AtomicUsize,
    delete_calls: AtomicUsize,
}

impl PlaygroundConversationService for RejectConversationService {
    fn save(
        &self,
        _principal: SessionPrincipal,
        _command: PlaygroundConversationSaveCommand,
    ) -> PlaygroundConversationSaveFuture<'_> {
        self.save_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(PlaygroundConversationError::LimitReached) })
    }

    fn list(&self, _principal: SessionPrincipal) -> PlaygroundConversationListFuture<'_> {
        self.list_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(PlaygroundConversationError::Internal) })
    }

    fn read(
        &self,
        _principal: SessionPrincipal,
        _conversation_id: String,
    ) -> PlaygroundConversationReadFuture<'_> {
        self.read_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(PlaygroundConversationError::NotFound) })
    }

    fn delete(
        &self,
        _principal: SessionPrincipal,
        _conversation_id: String,
    ) -> PlaygroundConversationDeleteFuture<'_> {
        self.delete_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(PlaygroundConversationError::NotFound) })
    }
}

#[tokio::test]
async fn all_history_routes_require_session_and_return_no_store_errors() {
    let service = Arc::new(RejectConversationService::default());
    let router = build_playground_conversation_router(service.clone(), Arc::new(AcceptSession));

    let unauthenticated = router
        .clone()
        .oneshot(json_request("PUT", item_path(), valid_body(), false))
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(service.save_calls.load(Ordering::Relaxed), 0);

    let limited = router
        .clone()
        .oneshot(json_request("PUT", item_path(), valid_body(), true))
        .await
        .unwrap();
    assert_eq!(limited.status(), StatusCode::CONFLICT);
    assert_eq!(limited.headers()["cache-control"], "no-store");
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(limited.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body["code"], "playground_conversation_limit_reached");

    let list = router
        .clone()
        .oneshot(authenticated_request(
            "GET",
            "/api/playground/conversations",
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(list.headers()["cache-control"], "no-store");

    let read = router
        .clone()
        .oneshot(authenticated_request("GET", item_path()))
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::NOT_FOUND);

    let deleted = router
        .oneshot(authenticated_request("DELETE", item_path()))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NOT_FOUND);
    assert_eq!(service.list_calls.load(Ordering::Relaxed), 1);
    assert_eq!(service.read_calls.load(Ordering::Relaxed), 1);
    assert_eq!(service.delete_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn save_rejects_invalid_revision_and_oversized_body_before_service() {
    let service = Arc::new(RejectConversationService::default());
    let router = build_playground_conversation_router(service.clone(), Arc::new(AcceptSession));
    let invalid = router
        .clone()
        .oneshot(json_request(
            "PUT",
            item_path(),
            valid_body().replace("\"revision\":null", "\"revision\":0"),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let oversized = router
        .oneshot(json_request(
            "PUT",
            item_path(),
            format!("{{\"padding\":\"{}\"}}", "x".repeat(700 * 1024)),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(service.save_calls.load(Ordering::Relaxed), 0);
}

fn json_request(method: &str, uri: &str, body: String, authenticated: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if authenticated {
        builder = builder.header(AUTHORIZATION, "Bearer session-token");
    }
    builder.body(Body::from(body)).unwrap()
}

fn authenticated_request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, "Bearer session-token")
        .body(Body::empty())
        .unwrap()
}

fn valid_body() -> String {
    serde_json::json!({
        "revision": null,
        "sessions": [{
            "model": "gpt-5",
            "messages": [
                {"role": "user", "content": "question"},
                {"role": "assistant", "content": "answer"}
            ]
        }]
    })
    .to_string()
}

fn item_path() -> &'static str {
    "/api/playground/conversations/11111111111111111111111111111111"
}
