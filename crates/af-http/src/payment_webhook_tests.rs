use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_billing::{
    PaymentWebhookRequest, PaymentWebhookVerificationError, PaymentWebhookVerifier,
    TopupPaymentEventFuture, TopupPaymentEventPort, TopupPaymentEventWrite, TopupWebhookOutcome,
    TopupWebhookProcessor, VerifiedTopupPaymentEvent,
};
use af_domain::TopupOrderStatus;
use af_domain::{TopupOrderId, TopupPaymentEventType};
use axum::{
    body::Body,
    http::{HeaderValue, Request, StatusCode},
};
use tower::ServiceExt;

use super::build_payment_webhook_router;

#[tokio::test]
async fn configured_route_acknowledges_idempotent_outcome() {
    let calls = Arc::new(AtomicUsize::new(0));
    let processor = processor(
        Arc::new(RecordingVerifier),
        Arc::new(RecordingPort {
            calls: Arc::clone(&calls),
            outcome: TopupWebhookOutcome::Acknowledged(TopupOrderStatus::Paid),
        }),
    );
    let router = build_payment_webhook_router("stripe", Some(Arc::new(processor)));
    let response = router.oneshot(webhook_request("stripe")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn disabled_or_unknown_provider_never_reaches_processor() {
    let disabled = build_payment_webhook_router("stripe", None);
    let response = disabled.oneshot(webhook_request("stripe")).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let configured = build_payment_webhook_router(
        "stripe",
        Some(Arc::new(processor(
            Arc::new(RecordingVerifier),
            Arc::new(RecordingPort {
                calls: Arc::new(AtomicUsize::new(0)),
                outcome: TopupWebhookOutcome::NotFound,
            }),
        ))),
    );
    let response = configured.oneshot(webhook_request("paypal")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_header_is_rejected_before_verifier() {
    let router = build_payment_webhook_router(
        "stripe",
        Some(Arc::new(processor(
            Arc::new(RecordingVerifier),
            Arc::new(RecordingPort {
                calls: Arc::new(AtomicUsize::new(0)),
                outcome: TopupWebhookOutcome::NotFound,
            }),
        ))),
    );
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/payment/webhook/stripe")
        .header("stripe-signature", "valid")
        .body(Body::from("payload"))
        .unwrap();
    request.headers_mut().insert(
        "x-invalid",
        HeaderValue::from_bytes(&[0xff]).expect("测试头应保留非 UTF-8 字节"),
    );
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

fn processor(
    verifier: Arc<dyn PaymentWebhookVerifier>,
    port: Arc<dyn TopupPaymentEventPort>,
) -> TopupWebhookProcessor {
    TopupWebhookProcessor::new(verifier, port)
}

fn webhook_request(provider: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/api/payment/webhook/{provider}"))
        .header("stripe-signature", "valid")
        .body(Body::from("payload"))
        .unwrap()
}

struct RecordingVerifier;

impl PaymentWebhookVerifier for RecordingVerifier {
    fn provider(&self) -> &str {
        "stripe"
    }

    fn verify(
        &self,
        request: &PaymentWebhookRequest<'_>,
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError> {
        if request
            .headers()
            .iter()
            .any(|header| header.value() == "valid")
        {
            VerifiedTopupPaymentEvent::new(
                TopupOrderId::new([0x44; 16]).unwrap(),
                "evt-http".to_owned(),
                Some("trade-http".to_owned()),
                TopupPaymentEventType::Succeeded,
                [0xaa; 32],
            )
        } else {
            Err(PaymentWebhookVerificationError::InvalidSignature)
        }
    }
}

struct RecordingPort {
    calls: Arc<AtomicUsize>,
    outcome: TopupWebhookOutcome,
}

impl TopupPaymentEventPort for RecordingPort {
    fn accept<'a>(&'a self, _write: TopupPaymentEventWrite) -> TopupPaymentEventFuture<'a> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let outcome = self.outcome;
        Box::pin(async move { Ok(outcome) })
    }
}
