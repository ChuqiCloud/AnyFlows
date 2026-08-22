use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_domain::{TopupOrderId, TopupOrderStatus, TopupPaymentEventType};

use super::{
    PaymentWebhookEventRouter, PaymentWebhookHandlerError, PaymentWebhookHandlerOutcome,
    PaymentWebhookHeader, PaymentWebhookInputError, PaymentWebhookProcessor, PaymentWebhookRequest,
    PaymentWebhookRouteFuture, PaymentWebhookVerificationError, PaymentWebhookVerifier,
    TopupPaymentEventFuture, TopupPaymentEventPort, TopupWebhookOutcome, TopupWebhookProcessor,
    TopupWebhookProcessorError, VerifiedTopupPaymentEvent,
};

const RECEIVED_AT: u64 = 1_900_000_000;

#[tokio::test]
async fn rejected_signature_never_reaches_persistence_port() {
    let calls = Arc::new(AtomicUsize::new(0));
    let processor = TopupWebhookProcessor::new(
        Arc::new(FakeVerifier {
            valid: false,
            provider: "stripe",
        }),
        Arc::new(RecordingPort {
            calls: calls.clone(),
            outcome: TopupWebhookOutcome::Applied(TopupOrderStatus::Paid),
        }),
    );
    let headers = [PaymentWebhookHeader::new("stripe-signature", "invalid").unwrap()];
    let request = PaymentWebhookRequest::new(br#"{"type":"paid"}"#, &headers, RECEIVED_AT).unwrap();

    assert_eq!(
        processor.process(request).await,
        Err(TopupWebhookProcessorError::VerificationRejected)
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn verified_event_reaches_port_once_and_returns_closed_outcome() {
    let calls = Arc::new(AtomicUsize::new(0));
    let processor = TopupWebhookProcessor::new(
        Arc::new(FakeVerifier {
            valid: true,
            provider: "stripe",
        }),
        Arc::new(RecordingPort {
            calls: calls.clone(),
            outcome: TopupWebhookOutcome::Applied(TopupOrderStatus::Paid),
        }),
    );
    let headers = [PaymentWebhookHeader::new("stripe-signature", "valid").unwrap()];
    let request = PaymentWebhookRequest::new(br#"{"type":"paid"}"#, &headers, RECEIVED_AT).unwrap();

    assert_eq!(
        processor.process(request).await,
        Ok(TopupWebhookOutcome::Applied(TopupOrderStatus::Paid))
    );
    assert_eq!(calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn invalid_provider_contract_fails_closed_before_persistence() {
    let calls = Arc::new(AtomicUsize::new(0));
    let processor = TopupWebhookProcessor::new(
        Arc::new(FakeVerifier {
            valid: true,
            provider: "Stripe",
        }),
        Arc::new(RecordingPort {
            calls: calls.clone(),
            outcome: TopupWebhookOutcome::Applied(TopupOrderStatus::Paid),
        }),
    );
    let headers = [PaymentWebhookHeader::new("x-signature", "valid").unwrap()];
    let request = PaymentWebhookRequest::new(b"payload", &headers, RECEIVED_AT).unwrap();

    assert_eq!(
        processor.process(request).await,
        Err(TopupWebhookProcessorError::Invariant)
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn verified_non_terminal_event_is_acknowledged_without_persistence() {
    let calls = Arc::new(AtomicUsize::new(0));
    let processor = TopupWebhookProcessor::new(
        Arc::new(NonTerminalVerifier),
        Arc::new(RecordingPort {
            calls: calls.clone(),
            outcome: TopupWebhookOutcome::Applied(TopupOrderStatus::Failed),
        }),
    );
    let headers = [PaymentWebhookHeader::new("stripe-signature", "valid").unwrap()];
    let request = PaymentWebhookRequest::new(b"payload", &headers, RECEIVED_AT).unwrap();

    assert_eq!(
        processor.process(request).await,
        Ok(TopupWebhookOutcome::IgnoredNonTerminal)
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn unified_processor_routes_only_verified_facts() {
    let calls = Arc::new(AtomicUsize::new(0));
    let processor = PaymentWebhookProcessor::new(
        Arc::new(FakeVerifier {
            valid: true,
            provider: "stripe",
        }),
        Arc::new(RecordingRouter {
            calls: Arc::clone(&calls),
            outcome: PaymentWebhookHandlerOutcome::Applied,
        }),
    );
    let headers = [PaymentWebhookHeader::new("stripe-signature", "valid").unwrap()];
    let request = PaymentWebhookRequest::new(b"payload", &headers, RECEIVED_AT).unwrap();

    assert_eq!(
        processor.process(request).await,
        Ok(PaymentWebhookHandlerOutcome::Applied)
    );
    assert_eq!(calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn unified_processor_does_not_route_non_terminal_events() {
    let calls = Arc::new(AtomicUsize::new(0));
    let processor = PaymentWebhookProcessor::new(
        Arc::new(NonTerminalVerifier),
        Arc::new(RecordingRouter {
            calls: Arc::clone(&calls),
            outcome: PaymentWebhookHandlerOutcome::Applied,
        }),
    );
    let headers = [PaymentWebhookHeader::new("stripe-signature", "valid").unwrap()];
    let request = PaymentWebhookRequest::new(b"payload", &headers, RECEIVED_AT).unwrap();

    assert_eq!(
        processor.process(request).await,
        Ok(PaymentWebhookHandlerOutcome::IgnoredNonTerminal)
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[test]
fn request_and_verified_event_debug_output_are_redacted() {
    let header = PaymentWebhookHeader::new("x-signature", "signature-secret").unwrap();
    let headers = [header];
    let request = PaymentWebhookRequest::new(b"payload-secret", &headers, RECEIVED_AT).unwrap();
    let verified = VerifiedTopupPaymentEvent::new(
        order_id(),
        "provider-event-secret".to_owned(),
        Some("trade-secret".to_owned()),
        TopupPaymentEventType::Succeeded,
        1234,
        "CNY".to_owned(),
        "alipay".to_owned(),
        [0xaa; 32],
    )
    .unwrap();

    for (rendered, secret) in [
        (format!("{header:?}"), "signature-secret"),
        (format!("{request:?}"), "payload-secret"),
        (format!("{verified:?}"), "provider-event-secret"),
        (format!("{verified:?}"), "trade-secret"),
    ] {
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains(secret));
    }
}

#[test]
fn request_bounds_reject_ambiguous_or_oversized_inputs() {
    assert_eq!(
        PaymentWebhookRequest::new(b"", &[], RECEIVED_AT).unwrap_err(),
        PaymentWebhookInputError::EmptyPayload
    );
    assert_eq!(
        PaymentWebhookHeader::new("bad header", "value").unwrap_err(),
        PaymentWebhookInputError::InvalidHeader
    );
    assert_eq!(
        PaymentWebhookHeader::new("x-signature", "bad\r\nvalue").unwrap_err(),
        PaymentWebhookInputError::InvalidHeader
    );
    for value in [
        "bad\0value",
        "bad\u{000b}value",
        "bad\u{001f}value",
        "bad\u{007f}value",
    ] {
        assert_eq!(
            PaymentWebhookHeader::new("x-signature", value).unwrap_err(),
            PaymentWebhookInputError::InvalidHeader
        );
    }
    assert_eq!(
        PaymentWebhookHeader::new("x-signature", "part-a\tpart-b")
            .unwrap()
            .value(),
        "part-a\tpart-b"
    );
}

struct FakeVerifier {
    valid: bool,
    provider: &'static str,
}

impl PaymentWebhookVerifier for FakeVerifier {
    fn provider(&self) -> &str {
        self.provider
    }

    fn verify(
        &self,
        request: &PaymentWebhookRequest<'_>,
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError> {
        if !self.valid
            || request
                .headers()
                .iter()
                .all(|header| header.value() != "valid")
        {
            return Err(PaymentWebhookVerificationError::InvalidSignature);
        }
        VerifiedTopupPaymentEvent::new(
            order_id(),
            "evt-paid".to_owned(),
            Some("trade-paid".to_owned()),
            TopupPaymentEventType::Succeeded,
            1234,
            "USD".to_owned(),
            "card".to_owned(),
            [0xbb; 32],
        )
    }
}

struct RecordingPort {
    calls: Arc<AtomicUsize>,
    outcome: TopupWebhookOutcome,
}

struct RecordingRouter {
    calls: Arc<AtomicUsize>,
    outcome: PaymentWebhookHandlerOutcome,
}

impl PaymentWebhookEventRouter for RecordingRouter {
    fn route<'a>(
        &'a self,
        _provider: String,
        _event: VerifiedTopupPaymentEvent,
        _payload_sha256: [u8; 32],
        _received_at: u64,
    ) -> PaymentWebhookRouteFuture<'a> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let outcome = self.outcome;
        Box::pin(async move { Ok::<_, PaymentWebhookHandlerError>(outcome) })
    }
}

struct NonTerminalVerifier;

impl PaymentWebhookVerifier for NonTerminalVerifier {
    fn provider(&self) -> &str {
        "stripe"
    }

    fn verify(
        &self,
        _request: &PaymentWebhookRequest<'_>,
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError> {
        Err(PaymentWebhookVerificationError::NonTerminalEvent)
    }
}

impl TopupPaymentEventPort for RecordingPort {
    fn accept<'a>(&'a self, _write: af_db::TopupPaymentEventWrite) -> TopupPaymentEventFuture<'a> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move { Ok(self.outcome) })
    }
}

fn order_id() -> TopupOrderId {
    TopupOrderId::new([0x44; 16]).expect("测试订单标识必须非零")
}
