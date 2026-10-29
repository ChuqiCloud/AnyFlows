use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    ImageHistoryContent, ImageHistoryEntry, ImageHistoryError, ImageHistoryFuture,
    ImageHistoryService, ImageHistoryUpload, LoginCredentials, SessionAuthentication,
    SessionAuthenticationError, SessionAuthenticationFuture, SessionAuthenticator,
    SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_domain::{GroupId, UserId};
use axum::body::Body;
use http::{Request, StatusCode, header::AUTHORIZATION};
use tower::ServiceExt;

use crate::playground_image_history::build_playground_image_history_router;

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
struct RejectImageHistory {
    save_calls: AtomicUsize,
    list_calls: AtomicUsize,
}

impl ImageHistoryService for RejectImageHistory {
    fn save(
        &self,
        _principal: SessionPrincipal,
        _upload: ImageHistoryUpload,
    ) -> ImageHistoryFuture<'_, ImageHistoryEntry> {
        self.save_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(ImageHistoryError::Internal) })
    }

    fn list(&self, _principal: SessionPrincipal) -> ImageHistoryFuture<'_, Vec<ImageHistoryEntry>> {
        self.list_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(ImageHistoryError::Internal) })
    }

    fn read(
        &self,
        _principal: SessionPrincipal,
        _image_id: String,
        _thumbnail: bool,
    ) -> ImageHistoryFuture<'_, ImageHistoryContent> {
        Box::pin(async { Err(ImageHistoryError::Internal) })
    }

    fn delete(
        &self,
        _principal: SessionPrincipal,
        _image_id: String,
    ) -> ImageHistoryFuture<'_, ()> {
        Box::pin(async { Err(ImageHistoryError::Internal) })
    }

    fn clear(&self, _principal: SessionPrincipal) -> ImageHistoryFuture<'_, ()> {
        Box::pin(async { Err(ImageHistoryError::Internal) })
    }
}

#[tokio::test]
async fn image_history_routes_require_session_and_return_no_store_errors() {
    let service = Arc::new(RejectImageHistory::default());
    let router = build_playground_image_history_router(service.clone(), Arc::new(AcceptSession));

    let unauthenticated = router
        .clone()
        .oneshot(request("GET", "/api/playground/image-history", None, false))
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(service.list_calls.load(Ordering::Relaxed), 0);

    let unavailable = router
        .oneshot(request("GET", "/api/playground/image-history", None, true))
        .await
        .unwrap();
    assert_eq!(unavailable.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(unavailable.headers()["cache-control"], "no-store");
    assert_eq!(service.list_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn image_history_rejects_invalid_image_bytes_before_service() {
    let service = Arc::new(RejectImageHistory::default());
    let router = build_playground_image_history_router(service.clone(), Arc::new(AcceptSession));
    let boundary = "image-history-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nimage-model\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nA red bird\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"image.png\"\r\nContent-Type: image/png\r\n\r\nnot-an-image\r\n--{boundary}--\r\n"
    );
    let response = router
        .oneshot(request(
            "POST",
            "/api/playground/image-history",
            Some((format!("multipart/form-data; boundary={boundary}"), body)),
            true,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(service.save_calls.load(Ordering::Relaxed), 0);
}

fn request(
    method: &str,
    uri: &str,
    body: Option<(String, String)>,
    authenticated: bool,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if authenticated {
        builder = builder.header(AUTHORIZATION, "Bearer session-token");
    }
    let request_body = if let Some((content_type, body)) = body {
        builder = builder.header("content-type", content_type);
        Body::from(body)
    } else {
        Body::empty()
    };
    builder.body(request_body).unwrap()
}
