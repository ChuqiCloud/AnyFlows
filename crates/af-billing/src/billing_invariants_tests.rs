//! 跨组件计费安全不变量回归。

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use af_domain::{BillingReservationId, Quota, QuotaError};
use af_protocol::{TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource};
use rust_decimal::Decimal;

use crate::{
    BillingMode, BillingSession, PricingContext, PricingError, PricingRatio, PricingRatios,
    PricingResolver, RatioPricingResolver, RefundSignalOutcome, RefundSignalPort, TokenPrices,
    quota_math::{QuotaClampKind, QuotaMathOperation},
};

#[derive(Default)]
struct RecordingRefundPort {
    attempts: AtomicUsize,
    received: Mutex<Vec<BillingReservationId>>,
}

impl RecordingRefundPort {
    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::Acquire)
    }

    fn received(&self) -> Vec<BillingReservationId> {
        self.received
            .lock()
            .expect("测试退款记录锁不能损坏")
            .clone()
    }
}

impl RefundSignalPort for RecordingRefundPort {
    fn try_signal_refund(&self, reservation_id: BillingReservationId) -> RefundSignalOutcome {
        self.attempts.fetch_add(1, Ordering::AcqRel);
        self.received
            .lock()
            .expect("测试退款记录锁不能损坏")
            .push(reservation_id);
        RefundSignalOutcome::Accepted
    }
}

#[test]
fn ratio_component_sum_overflow_is_rejected_with_audit_marker() {
    let resolver = overflowing_component_sum_resolver();
    let usage = usage(1, 1);

    let error = resolver.resolve(&PricingContext::new(&usage)).unwrap_err();

    assert_pricing_overflow(error);
}

#[test]
fn ratio_accepts_the_exact_maximum_quota_without_saturation() {
    let resolver = RatioPricingResolver::metered(
        prices([
            Decimal::from(2),
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        ]),
        unit_ratios(),
    );
    let usage = usage(i64::MAX, 0);

    let price = resolver.resolve(&PricingContext::new(&usage)).unwrap();

    assert_eq!(price.quota(), Quota::new(i64::MAX).unwrap());
    assert_eq!(price.billing_mode(), BillingMode::PerToken);
    assert!(!price.is_free());
}

#[test]
fn pricing_failure_releases_the_exact_reserved_session_once() {
    let resolver = overflowing_component_sum_resolver();
    let usage = usage(1, 1);
    let reservation_id = reservation_id(71);
    let port = Arc::new(RecordingRefundPort::default());

    let error = resolve_reserved_request(
        &resolver,
        &PricingContext::new(&usage),
        reservation_id,
        port.clone(),
    )
    .unwrap_err();

    assert_pricing_overflow(error);
    assert_eq!(port.attempts(), 1);
    assert_eq!(port.received(), vec![reservation_id]);
}

fn resolve_reserved_request(
    resolver: &dyn PricingResolver,
    context: &PricingContext<'_>,
    reservation_id: BillingReservationId,
    port: Arc<dyn RefundSignalPort>,
) -> Result<(), PricingError> {
    let _session = BillingSession::from_reserved(reservation_id, port);
    let _price = resolver.resolve(context)?;
    Ok(())
}

fn overflowing_component_sum_resolver() -> RatioPricingResolver {
    RatioPricingResolver::metered(
        prices([
            Decimal::MAX,
            Decimal::MAX,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        ]),
        unit_ratios(),
    )
}

fn prices(values: [Decimal; 5]) -> TokenPrices {
    TokenPrices::new(values[0], values[1], values[2], values[3], values[4]).unwrap()
}

fn unit_ratios() -> PricingRatios {
    PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE)
}

fn usage(input_tokens: i64, output_tokens: i64) -> Usage {
    Usage::new(
        TokenCount::new(input_tokens).unwrap(),
        TokenCount::new(output_tokens).unwrap(),
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap()
}

fn reservation_id(marker: u8) -> BillingReservationId {
    BillingReservationId::new([marker; 16]).expect("测试幂等标识必须非零")
}

fn assert_pricing_overflow(error: PricingError) {
    let PricingError::Math(error) = error else {
        panic!("定价溢出必须通过额度数学错误返回");
    };
    assert_eq!(error.quota_error(), QuotaError::Overflow);
    let clamp = error.clamp().expect("定价溢出必须携带审计标记");
    assert_eq!(clamp.operation(), QuotaMathOperation::PricingResolution);
    assert_eq!(clamp.kind(), QuotaClampKind::Overflow);
}
