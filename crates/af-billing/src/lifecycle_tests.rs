use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use af_domain::{
    BillingReservationId, GatewayPrincipal, GroupId, OrganizationId, OrganizationServiceAccountId,
    OrganizationServiceAccountKey, OrganizationServiceAccountLocator,
    OrganizationServiceAccountRuntimeIdentity, Quota, TokenId, UserId,
};
use af_protocol::{AudioDuration, TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource};
use rust_decimal::Decimal;

use crate::{
    BillingLifecycleError, BillingLifecycleState, BillingMode, BillingRequestPlan,
    BillingSettlementError, BillingSettlementFuture, BillingSettlementPort, BillingUsageDimensions,
    BillingUsageRecord, PriceData, PricingContext, PricingError, PricingRatio, PricingRatios,
    PricingResolver, RatioPricingResolver, RefundSignalOutcome, RefundSignalPort, TokenPrices,
    UsageRecordOutcome, UsageRecordPort,
};

#[derive(Default)]
struct ModeChangingResolver {
    calls: AtomicUsize,
}

impl PricingResolver for ModeChangingResolver {
    fn resolve(&self, context: &PricingContext<'_>) -> Result<PriceData, PricingError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            RatioPricingResolver::free().resolve(context)
        } else {
            metered_resolver().resolve(context)
        }
    }
}

#[derive(Default)]
struct RecordingSettlementPort {
    results: Mutex<VecDeque<Result<(), BillingSettlementError>>>,
    requests: Mutex<Vec<crate::SettlementRequest>>,
}

impl RecordingSettlementPort {
    fn with_results(results: impl IntoIterator<Item = Result<(), BillingSettlementError>>) -> Self {
        Self {
            results: Mutex::new(results.into_iter().collect()),
            ..Self::default()
        }
    }

    fn requests(&self) -> Vec<crate::SettlementRequest> {
        self.requests
            .lock()
            .expect("测试结算请求锁不能损坏")
            .clone()
    }
}

impl BillingSettlementPort for RecordingSettlementPort {
    fn settle<'a>(&'a self, request: crate::SettlementRequest) -> BillingSettlementFuture<'a> {
        Box::pin(async move {
            self.requests
                .lock()
                .expect("测试结算请求锁不能损坏")
                .push(request);
            self.results
                .lock()
                .expect("测试结算结果锁不能损坏")
                .pop_front()
                .unwrap_or(Ok(()))
        })
    }
}

#[derive(Default)]
struct RecordingUsagePort {
    outcomes: Mutex<VecDeque<UsageRecordOutcome>>,
    records: Mutex<Vec<BillingUsageRecord>>,
}

impl RecordingUsagePort {
    fn with_outcomes(outcomes: impl IntoIterator<Item = UsageRecordOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into_iter().collect()),
            ..Self::default()
        }
    }

    fn records(&self) -> Vec<BillingUsageRecord> {
        self.records.lock().expect("测试用量记录锁不能损坏").clone()
    }
}

impl UsageRecordPort for RecordingUsagePort {
    fn try_record(&self, record: BillingUsageRecord) -> UsageRecordOutcome {
        let outcome = self
            .outcomes
            .lock()
            .expect("测试用量结果锁不能损坏")
            .pop_front()
            .unwrap_or(UsageRecordOutcome::Accepted);
        if outcome == UsageRecordOutcome::Accepted {
            self.records
                .lock()
                .expect("测试用量记录锁不能损坏")
                .push(record);
        }
        outcome
    }
}

#[derive(Default)]
struct PanickingUsagePort {
    calls: AtomicUsize,
    records: Mutex<Vec<BillingUsageRecord>>,
}

impl PanickingUsagePort {
    fn calls(&self) -> usize {
        self.calls.load(Ordering::Acquire)
    }

    fn records(&self) -> Vec<BillingUsageRecord> {
        self.records
            .lock()
            .expect("测试 panic 用量记录锁不能损坏")
            .clone()
    }
}

impl UsageRecordPort for PanickingUsagePort {
    fn try_record(&self, record: BillingUsageRecord) -> UsageRecordOutcome {
        if self.calls.fetch_add(1, Ordering::AcqRel) == 0 {
            panic!("测试用量端口异常");
        }
        self.records
            .lock()
            .expect("测试 panic 用量记录锁不能损坏")
            .push(record);
        UsageRecordOutcome::Accepted
    }
}

#[derive(Default)]
struct RecordingRefundPort {
    reservations: Mutex<Vec<BillingReservationId>>,
}

impl RecordingRefundPort {
    fn reservations(&self) -> Vec<BillingReservationId> {
        self.reservations
            .lock()
            .expect("测试退款记录锁不能损坏")
            .clone()
    }
}

impl RefundSignalPort for RecordingRefundPort {
    fn try_signal_refund(&self, reservation_id: BillingReservationId) -> RefundSignalOutcome {
        self.reservations
            .lock()
            .expect("测试退款记录锁不能损坏")
            .push(reservation_id);
        RefundSignalOutcome::Accepted
    }
}

#[tokio::test]
async fn free_request_skips_reservation_and_settlement_but_records_usage_once() {
    let estimated = usage(20, 100);
    let actual = usage(20, 7);
    let plan = BillingRequestPlan::prepare(Arc::new(RatioPricingResolver::free()), &estimated)
        .expect("免费定价必须可准备");
    assert_eq!(plan.billing_mode(), BillingMode::Free);
    assert_eq!(plan.precharge_quota(), None);

    let usage_port = Arc::new(RecordingUsagePort::default());
    let event_id = reservation_id(4);
    let mut lifecycle = plan
        .start_free(principal(), event_id, usage_port.clone())
        .expect("免费计划必须进入生命周期");
    let first = lifecycle.complete(actual).await.unwrap();
    let replay = lifecycle.complete(actual).await.unwrap();

    assert_eq!(first, replay);
    assert_eq!(first.billing_mode(), BillingMode::Free);
    assert_eq!(first.quota(), Quota::ZERO);
    assert_eq!(first.usage(), actual);
    assert_eq!(first.usage_record().event_id(), event_id);
    assert_eq!(usage_port.records(), vec![first.usage_record()]);
}

#[tokio::test]
async fn service_account_identity_is_carried_into_billing_completion() {
    let measured = usage(20, 7);
    let identity = OrganizationServiceAccountRuntimeIdentity::new(
        OrganizationServiceAccountLocator::new(
            OrganizationId::new(44).unwrap(),
            OrganizationServiceAccountId::new(55).unwrap(),
        ),
        OrganizationServiceAccountKey::new([1; 16]).unwrap(),
        OrganizationServiceAccountKey::new([2; 16]).unwrap(),
    );
    let usage_port = Arc::new(RecordingUsagePort::default());
    let plan = BillingRequestPlan::prepare(Arc::new(RatioPricingResolver::free()), &measured)
        .expect("免费定价必须可准备");
    let mut lifecycle = plan
        .start_free(principal(), reservation_id(6), usage_port.clone())
        .unwrap()
        .with_service_account_identity(identity);

    let completion = lifecycle.complete(measured).await.unwrap();

    assert_eq!(completion.service_account_identity(), Some(identity));
    assert_eq!(
        completion.usage_record().service_account_identity(),
        Some(identity)
    );
    assert_eq!(
        usage_port.records()[0].service_account_identity(),
        Some(identity)
    );
}

#[tokio::test]
async fn completed_usage_replay_must_match_audio_duration_dimension() {
    let measured = usage(20, 7);
    let plan = BillingRequestPlan::prepare(Arc::new(RatioPricingResolver::free()), &measured)
        .expect("免费定价必须可准备");
    let usage_port = Arc::new(RecordingUsagePort::default());
    let mut lifecycle = plan
        .start_free(principal(), reservation_id(9), usage_port.clone())
        .unwrap();
    let duration = AudioDuration::from_nanoseconds(1_500_000_000).unwrap();
    let dimensions = BillingUsageDimensions::with_audio_duration(duration);

    let completion = lifecycle
        .complete_with_dimensions(measured, dimensions)
        .await
        .unwrap();
    assert_eq!(completion.dimensions(), dimensions);
    assert_eq!(
        lifecycle
            .complete_with_dimensions(measured, dimensions)
            .await,
        Ok(completion)
    );
    assert_eq!(
        lifecycle
            .complete_with_dimensions(
                measured,
                BillingUsageDimensions::with_audio_duration(
                    AudioDuration::from_nanoseconds(2_000_000_000).unwrap(),
                ),
            )
            .await,
        Err(BillingLifecycleError::CompletionConflict)
    );
    assert_eq!(usage_port.records(), vec![completion.usage_record()]);
}

#[tokio::test]
async fn free_plan_still_reports_billing_mode_changes_before_port_errors() {
    let measured = usage(20, 7);
    let plan = BillingRequestPlan::prepare(Arc::new(ModeChangingResolver::default()), &measured)
        .expect("首次免费定价必须可准备");
    let mut lifecycle = plan
        .start_free(
            principal(),
            reservation_id(10),
            Arc::new(RecordingUsagePort::default()),
        )
        .unwrap();

    assert_eq!(
        lifecycle.complete(measured).await,
        Err(BillingLifecycleError::BillingModeChanged)
    );
}

#[tokio::test]
async fn zero_cost_per_token_reserves_one_and_settles_actual_zero() {
    let estimated = usage(12, 64);
    let actual = usage(12, 1);
    let plan = BillingRequestPlan::prepare(Arc::new(zero_metered_resolver()), &estimated)
        .expect("零价按量定价必须可准备");
    assert_eq!(plan.billing_mode(), BillingMode::PerToken);
    assert_eq!(plan.precharge_quota(), Some(Quota::new(1).unwrap()));

    let settlement = Arc::new(RecordingSettlementPort::default());
    let usage_port = Arc::new(RecordingUsagePort::default());
    let refund = Arc::new(RecordingRefundPort::default());
    let reservation_id = reservation_id(1);
    let mut lifecycle = plan
        .start_reserved(
            principal(),
            reservation_id,
            refund.clone(),
            settlement.clone(),
            usage_port.clone(),
        )
        .expect("按量计划必须使用已确认预留启动");

    let completion = lifecycle.complete(actual).await.unwrap();
    let requests = settlement.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].reservation_id(), reservation_id);
    assert_eq!(requests[0].actual_quota(), Quota::ZERO);
    assert_eq!(completion.quota(), Quota::ZERO);
    assert_eq!(usage_port.records(), vec![completion.usage_record()]);
    drop(lifecycle);
    assert!(refund.reservations().is_empty());
}

#[tokio::test]
async fn settlement_retry_requires_the_frozen_usage_and_dimensions() {
    let measured = usage(1_000_000, 2_000_000);
    let plan = BillingRequestPlan::prepare(Arc::new(metered_resolver()), &measured).unwrap();
    let settlement = Arc::new(RecordingSettlementPort::with_results([
        Err(BillingSettlementError::OutcomeUnknown),
        Ok(()),
    ]));
    let usage_port = Arc::new(RecordingUsagePort::default());
    let refund = Arc::new(RecordingRefundPort::default());
    let reservation_id = reservation_id(2);
    let mut lifecycle = plan
        .start_reserved(
            principal(),
            reservation_id,
            refund.clone(),
            settlement.clone(),
            usage_port.clone(),
        )
        .unwrap();
    let dimensions = BillingUsageDimensions::with_audio_duration(
        AudioDuration::from_nanoseconds(1_500_000_000).unwrap(),
    );

    assert_eq!(
        lifecycle
            .complete_with_dimensions(measured, dimensions)
            .await,
        Err(BillingLifecycleError::Settlement(
            BillingSettlementError::OutcomeUnknown
        ))
    );
    assert_eq!(lifecycle.state(), BillingLifecycleState::SettlementPending);
    assert!(usage_port.records().is_empty());
    assert_eq!(
        lifecycle
            .complete_with_dimensions(
                measured,
                BillingUsageDimensions::with_audio_duration(
                    AudioDuration::from_nanoseconds(2_000_000_000).unwrap(),
                ),
            )
            .await,
        Err(BillingLifecycleError::CompletionConflict)
    );
    assert_eq!(
        lifecycle
            .complete_with_dimensions(estimated_usage(1_000_000, 2_000_000), dimensions)
            .await,
        Err(BillingLifecycleError::CompletionConflict)
    );
    assert_eq!(settlement.requests().len(), 1);
    let completion = lifecycle
        .complete_with_dimensions(measured, dimensions)
        .await
        .unwrap();
    assert_eq!(settlement.requests().len(), 2);
    assert_eq!(settlement.requests()[0], settlement.requests()[1]);
    assert_eq!(usage_port.records(), vec![completion.usage_record()]);
    assert_eq!(lifecycle.state(), BillingLifecycleState::Completed);
    drop(lifecycle);
    assert!(refund.reservations().is_empty());
}

#[tokio::test]
async fn usage_queue_retry_never_repeats_persistent_settlement() {
    let measured = usage(1_000_000, 1_000_000);
    let plan = BillingRequestPlan::prepare(Arc::new(metered_resolver()), &measured).unwrap();
    let settlement = Arc::new(RecordingSettlementPort::default());
    let usage_port = Arc::new(RecordingUsagePort::with_outcomes([
        UsageRecordOutcome::Saturated,
        UsageRecordOutcome::Accepted,
    ]));
    let mut lifecycle = plan
        .start_reserved(
            principal(),
            reservation_id(3),
            Arc::new(RecordingRefundPort::default()),
            settlement.clone(),
            usage_port.clone(),
        )
        .unwrap();

    assert_eq!(
        lifecycle.complete(measured).await,
        Err(BillingLifecycleError::UsageRecordSaturated)
    );
    assert_eq!(lifecycle.state(), BillingLifecycleState::UsagePending);
    let completion = lifecycle.complete(measured).await.unwrap();

    assert_eq!(settlement.requests().len(), 1);
    assert_eq!(usage_port.records(), vec![completion.usage_record()]);
    assert_eq!(lifecycle.state(), BillingLifecycleState::Completed);
}

#[tokio::test]
async fn closed_usage_record_can_retry_without_repeating_settlement() {
    let measured = usage(1_000_000, 1_000_000);
    let plan = BillingRequestPlan::prepare(Arc::new(metered_resolver()), &measured).unwrap();
    let settlement = Arc::new(RecordingSettlementPort::default());
    let usage_port = Arc::new(RecordingUsagePort::with_outcomes([
        UsageRecordOutcome::Closed,
        UsageRecordOutcome::Accepted,
    ]));
    let refund = Arc::new(RecordingRefundPort::default());
    let mut lifecycle = plan
        .start_reserved(
            principal(),
            reservation_id(12),
            refund.clone(),
            settlement.clone(),
            usage_port.clone(),
        )
        .unwrap();

    assert_eq!(
        lifecycle.complete(measured).await,
        Err(BillingLifecycleError::UsageRecordClosed)
    );
    assert_eq!(lifecycle.state(), BillingLifecycleState::UsagePending);
    let completion = lifecycle.complete(measured).await.unwrap();

    assert_eq!(settlement.requests().len(), 1);
    assert_eq!(usage_port.records(), vec![completion.usage_record()]);
    assert!(refund.reservations().is_empty());
}

#[tokio::test]
async fn usage_record_conflict_keeps_completion_frozen_without_rebilling() {
    let measured = usage(1_000_000, 1_000_000);
    let plan = BillingRequestPlan::prepare(Arc::new(metered_resolver()), &measured).unwrap();
    let settlement = Arc::new(RecordingSettlementPort::default());
    let usage_port = Arc::new(RecordingUsagePort::with_outcomes([
        UsageRecordOutcome::Conflict,
        UsageRecordOutcome::Conflict,
    ]));
    let refund = Arc::new(RecordingRefundPort::default());
    let mut lifecycle = plan
        .start_reserved(
            principal(),
            reservation_id(13),
            refund.clone(),
            settlement.clone(),
            usage_port,
        )
        .unwrap();

    assert_eq!(
        lifecycle.complete(measured).await,
        Err(BillingLifecycleError::UsageRecordConflict)
    );
    assert_eq!(lifecycle.state(), BillingLifecycleState::UsagePending);
    assert_eq!(
        lifecycle.complete(measured).await,
        Err(BillingLifecycleError::UsageRecordConflict)
    );
    assert_eq!(settlement.requests().len(), 1);
    assert!(refund.reservations().is_empty());
}

#[tokio::test]
async fn panicking_usage_record_port_is_caught_and_can_retry_without_rebilling() {
    let measured = usage(1_000_000, 1_000_000);
    let plan = BillingRequestPlan::prepare(Arc::new(metered_resolver()), &measured).unwrap();
    let settlement = Arc::new(RecordingSettlementPort::default());
    let usage_port = Arc::new(PanickingUsagePort::default());
    let refund = Arc::new(RecordingRefundPort::default());
    let mut lifecycle = plan
        .start_reserved(
            principal(),
            reservation_id(14),
            refund.clone(),
            settlement.clone(),
            usage_port.clone(),
        )
        .unwrap();

    assert_eq!(
        lifecycle.complete(measured).await,
        Err(BillingLifecycleError::UsageRecordPanicked)
    );
    assert_eq!(lifecycle.state(), BillingLifecycleState::UsagePending);
    let completion = lifecycle.complete(measured).await.unwrap();

    assert_eq!(usage_port.calls(), 2);
    assert_eq!(usage_port.records(), vec![completion.usage_record()]);
    assert_eq!(settlement.requests().len(), 1);
    assert!(refund.reservations().is_empty());
}

fn principal() -> GatewayPrincipal {
    GatewayPrincipal::new(
        TokenId::new(11).unwrap(),
        UserId::new(22).unwrap(),
        GroupId::new(33).unwrap(),
    )
}

fn reservation_id(marker: u8) -> BillingReservationId {
    BillingReservationId::new([marker; 16]).expect("测试预留标识必须非零")
}

fn usage(input: i64, output: i64) -> Usage {
    usage_with_source(input, output, UsageSource::Upstream)
}

fn estimated_usage(input: i64, output: i64) -> Usage {
    usage_with_source(input, output, UsageSource::Estimated)
}

fn usage_with_source(input: i64, output: i64, source: UsageSource) -> Usage {
    Usage::new(
        TokenCount::new(input).unwrap(),
        TokenCount::new(output).unwrap(),
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        source,
        UsageSemantics::Inclusive,
    )
    .unwrap()
}

fn zero_metered_resolver() -> RatioPricingResolver {
    RatioPricingResolver::metered(
        TokenPrices::new(
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        )
        .unwrap(),
        unit_ratios(),
    )
}

fn metered_resolver() -> RatioPricingResolver {
    RatioPricingResolver::metered(
        TokenPrices::new(
            Decimal::ONE,
            Decimal::from(2),
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        )
        .unwrap(),
        unit_ratios(),
    )
}

fn unit_ratios() -> PricingRatios {
    PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE)
}
