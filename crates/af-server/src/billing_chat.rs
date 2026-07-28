use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_billing::{
    BillingLifecycleError, BillingPrechargeError, BillingPrechargePort, BillingRequestLifecycle,
    BillingRequestPlan, BillingSettlementPort, RefundSignalPort, RequestPricingSnapshotError,
    RequestPricingSnapshotSource, UsageRecordPort,
};
use af_domain::{AfError, BillingReservationId, GatewayPrincipal, GroupId};
use af_http::{ChatService, ChatServiceFuture};
use af_protocol::{CanonicalRequestEnvelope, Usage};
use af_relay::{
    OpenAiChatCompletionFuture, OpenAiChatCompletionHook, OpenAiChatResponse, UsageResolutionError,
    estimate_openai_chat_request_upper_bound,
};
use tokio::time::sleep;
use uuid::Uuid;

const MAX_PRECHARGE_ATTEMPTS: usize = 3;
const MAX_COMPLETION_ATTEMPTS: usize = 3;
const RETRY_DELAY: Duration = Duration::from_millis(20);
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

/// 已固定路由计划的一次异步执行结果。
pub type PlannedChatExecutionFuture =
    Pin<Box<dyn Future<Output = Result<OpenAiChatResponse, AfError>> + Send + 'static>>;

/// 在计费前完成候选、目标分组与运行时目标装配的请求级计划。
pub trait PlannedChatExecution: Send {
    /// 返回本计划全部候选共同的实际计费分组。
    fn target_group_id(&self) -> GroupId;

    /// 在预扣成功后消费计划并执行全部候选故障转移。
    fn execute(self: Box<Self>) -> PlannedChatExecutionFuture;
}

/// 为已认证 Chat 请求信封生成不可变路由计划的对象安全端口。
pub trait ChatRoutePlanner: Send + Sync {
    /// 从当前运行时快照固定候选并完成所有安全装配；本方法不得发送上游请求。
    fn plan(
        &self,
        principal: &GatewayPrincipal,
        request: CanonicalRequestEnvelope,
        request_id: &str,
    ) -> Result<Box<dyn PlannedChatExecution>, AfError>;
}

/// 已装配计费依赖的 Chat 服务装饰器。
///
/// 装饰器在调用上游前固定请求上界并完成一次幂等预扣；非流式响应直接完成结算，流式响应
/// 则把同一生命周期交给流结束回调。缺少真实定价或持久化端口时，构造方必须显式拒绝装配，
/// 本类型不会把未知价格当作免费。
pub struct BillingChatService {
    inner: Arc<dyn ChatRoutePlanner>,
    pricing_source: Arc<dyn RequestPricingSnapshotSource>,
    precharge: Option<Arc<dyn BillingPrechargePort>>,
    refund: Option<Arc<dyn RefundSignalPort>>,
    settlement: Option<Arc<dyn BillingSettlementPort>>,
    usage: Arc<dyn UsageRecordPort>,
}

impl BillingChatService {
    /// 使用请求级定价快照来源与计费端口创建装饰器。
    #[must_use]
    pub fn new(
        inner: Arc<dyn ChatRoutePlanner>,
        pricing_source: Arc<dyn RequestPricingSnapshotSource>,
        precharge: Option<Arc<dyn BillingPrechargePort>>,
        refund: Option<Arc<dyn RefundSignalPort>>,
        settlement: Option<Arc<dyn BillingSettlementPort>>,
        usage: Arc<dyn UsageRecordPort>,
    ) -> Self {
        Self {
            inner,
            pricing_source,
            precharge,
            refund,
            settlement,
            usage,
        }
    }

    async fn call(
        &self,
        principal: &GatewayPrincipal,
        request: CanonicalRequestEnvelope,
        request_id: &str,
    ) -> Result<OpenAiChatResponse, AfError> {
        let upper_bound = estimate_openai_chat_request_upper_bound(request.canonical())
            .map_err(map_upper_bound_error)?;
        let model = request.canonical().model.clone();
        let planned = self.inner.plan(principal, request, request_id)?;
        let source_group_id = principal.group_id();
        let target_group_id = planned.target_group_id();
        let snapshot = self
            .pricing_source
            .capture(
                &model,
                source_group_id,
                target_group_id,
                current_utc_day_second()?,
            )
            .map_err(map_snapshot_error)?;
        let plan = BillingRequestPlan::prepare(Arc::new(snapshot.resolver()), &upper_bound)
            .map_err(map_lifecycle_error)?;
        let reservation_id = new_reservation_id()?;
        let mut lifecycle = match plan.precharge_quota() {
            None => plan
                .start_free(*principal, reservation_id, self.usage.clone())
                .map_err(map_lifecycle_error)?,
            Some(amount) => {
                let precharge = self.precharge.as_ref().ok_or(AfError::Internal)?;
                let refund = self.refund.as_ref().ok_or(AfError::Internal)?;
                let settlement = self.settlement.as_ref().ok_or(AfError::Internal)?;
                precharge_with_retries(precharge, reservation_id, *principal, amount).await?;
                match plan.start_reserved(
                    *principal,
                    reservation_id,
                    refund.clone(),
                    settlement.clone(),
                    self.usage.clone(),
                ) {
                    Ok(lifecycle) => lifecycle,
                    Err(error) => {
                        let _ = refund.try_signal_refund(reservation_id);
                        return Err(map_lifecycle_error(error));
                    }
                }
            }
        };

        let response = planned.execute().await?;
        match response {
            OpenAiChatResponse::Full { body, usage } => {
                let usage = usage.map_err(|_| AfError::Internal)?;
                let _ = complete_with_retries(&mut lifecycle, usage)
                    .await
                    .map_err(map_lifecycle_error)?;
                Ok(OpenAiChatResponse::Full {
                    body,
                    usage: Ok(usage),
                })
            }
            OpenAiChatResponse::Stream { body, usage } => {
                let hook = BillingStreamCompletionHook {
                    lifecycle: Some(lifecycle),
                };
                let body = Box::new((*body).with_completion_hook(Box::new(hook)));
                Ok(OpenAiChatResponse::Stream { body, usage })
            }
        }
    }
}

impl ChatService for BillingChatService {
    fn chat_completions<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        request: CanonicalRequestEnvelope,
        request_id: &'a str,
    ) -> ChatServiceFuture<'a> {
        Box::pin(self.call(principal, request, request_id))
    }
}

/// 流式 usage 完成回调持有唯一计费生命周期；流提前结束时生命周期析构会触发既有退款兜底。
struct BillingStreamCompletionHook {
    lifecycle: Option<BillingRequestLifecycle>,
}

impl OpenAiChatCompletionHook for BillingStreamCompletionHook {
    fn on_complete(
        mut self: Box<Self>,
        usage: Result<Usage, UsageResolutionError>,
    ) -> OpenAiChatCompletionFuture {
        Box::pin(async move {
            let Some(mut lifecycle) = self.lifecycle.take() else {
                return;
            };
            let Ok(usage) = usage else {
                drop(lifecycle);
                return;
            };
            if complete_with_retries(&mut lifecycle, usage).await.is_err() {
                tracing::error!(
                    error_kind = "billing_stream_completion_failed",
                    "流式响应结束后的计费完成失败"
                );
            }
        })
    }
}

async fn precharge_with_retries(
    port: &Arc<dyn BillingPrechargePort>,
    reservation_id: BillingReservationId,
    principal: GatewayPrincipal,
    amount: af_domain::Quota,
) -> Result<(), AfError> {
    let mut attempts = 0;
    loop {
        match port.precharge(reservation_id, principal, amount).await {
            Ok(()) => return Ok(()),
            Err(BillingPrechargeError::OutcomeUnknown) if attempts + 1 < MAX_PRECHARGE_ATTEMPTS => {
                attempts += 1;
                sleep(RETRY_DELAY).await;
            }
            Err(error) => return Err(map_precharge_error(error)),
        }
    }
}

async fn complete_with_retries(
    lifecycle: &mut BillingRequestLifecycle,
    usage: Usage,
) -> Result<af_billing::BillingCompletion, BillingLifecycleError> {
    let mut attempts = 0;
    loop {
        match lifecycle.complete(usage).await {
            Ok(completion) => return Ok(completion),
            Err(error)
                if retryable_completion_error(error) && attempts + 1 < MAX_COMPLETION_ATTEMPTS =>
            {
                attempts += 1;
                sleep(RETRY_DELAY).await;
            }
            Err(error) => return Err(error),
        }
    }
}

fn retryable_completion_error(error: BillingLifecycleError) -> bool {
    matches!(
        error,
        BillingLifecycleError::Settlement(af_billing::BillingSettlementError::OutcomeUnknown)
            | BillingLifecycleError::UsageRecordSaturated
    )
}

fn new_reservation_id() -> Result<BillingReservationId, AfError> {
    BillingReservationId::new(Uuid::new_v4().into_bytes()).map_err(|_| AfError::Internal)
}

fn current_utc_day_second() -> Result<u32, AfError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AfError::Internal)?;
    let second = elapsed.as_secs() % SECONDS_PER_DAY;
    u32::try_from(second).map_err(|_| AfError::Internal)
}

fn map_upper_bound_error(error: UsageResolutionError) -> AfError {
    match error {
        UsageResolutionError::UnsupportedContent => AfError::InvalidRequest,
        UsageResolutionError::Interrupted
        | UsageResolutionError::InvalidSequence
        | UsageResolutionError::Overflow
        | UsageResolutionError::EstimationFailed => AfError::Internal,
    }
}

fn map_snapshot_error(_error: RequestPricingSnapshotError) -> AfError {
    AfError::Internal
}

fn map_precharge_error(error: BillingPrechargeError) -> AfError {
    match error {
        BillingPrechargeError::InsufficientQuota => AfError::InsufficientQuota,
        BillingPrechargeError::OutcomeUnknown
        | BillingPrechargeError::Conflict
        | BillingPrechargeError::Unavailable
        | BillingPrechargeError::Invariant => AfError::Internal,
        _ => AfError::Internal,
    }
}

fn map_lifecycle_error(error: BillingLifecycleError) -> AfError {
    match error {
        BillingLifecycleError::Settlement(
            af_billing::BillingSettlementError::InsufficientQuota,
        ) => AfError::InsufficientQuota,
        BillingLifecycleError::InvalidActivation
        | BillingLifecycleError::BillingModeChanged
        | BillingLifecycleError::CompletionConflict
        | BillingLifecycleError::Pricing(_)
        | BillingLifecycleError::Session(_)
        | BillingLifecycleError::Settlement(_)
        | BillingLifecycleError::UsageRecordSaturated
        | BillingLifecycleError::UsageRecordClosed
        | BillingLifecycleError::UsageRecordConflict
        | BillingLifecycleError::UsageRecordPanicked => AfError::Internal,
        _ => AfError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use af_adapter::Bytes;
    use af_billing::{
        BillingMode, BillingPrechargeFuture, BillingPrechargePort, BillingSettlementFuture,
        BillingSettlementPort, BillingUsageRecord, CachedRequestPricingSnapshotSource,
        GroupModelRatioSourceRecord, GroupPricingCache, GroupPricingSource,
        GroupPricingSourceCatalog, GroupPricingSourceFuture, GroupPricingSourceRecord,
        ModelPriceCache, ModelPriceSource, ModelPriceSourceFuture, ModelPriceSourceRecord,
        PricingRatio, RefundSignalOutcome, RequestPricingSnapshot, TokenPrices, UsageRecordOutcome,
    };
    use af_domain::{GroupId, Operation, Quota, Role, TokenId, UserId};
    use af_http::ChatService;
    use af_protocol::{
        CanonicalRequest, CanonicalRequestEnvelope, ContentBlock, Message, TokenCount, Usage,
    };
    use rust_decimal::Decimal;

    use super::*;

    struct FullStub {
        target_group_id: GroupId,
    }

    impl ChatRoutePlanner for FullStub {
        fn plan(
            &self,
            _principal: &GatewayPrincipal,
            _request: CanonicalRequestEnvelope,
            _request_id: &str,
        ) -> Result<Box<dyn PlannedChatExecution>, AfError> {
            Ok(Box::new(FullExecution {
                target_group_id: self.target_group_id,
            }))
        }
    }

    struct FullExecution {
        target_group_id: GroupId,
    }

    impl PlannedChatExecution for FullExecution {
        fn target_group_id(&self) -> GroupId {
            self.target_group_id
        }

        fn execute(self: Box<Self>) -> PlannedChatExecutionFuture {
            Box::pin(async move {
                Ok(OpenAiChatResponse::Full {
                    body: Bytes::from_static(b"{}"),
                    usage: Ok(usage()),
                })
            })
        }
    }

    #[derive(Default)]
    struct PrechargeStub {
        calls: Mutex<Vec<BillingReservationId>>,
    }

    impl BillingPrechargePort for PrechargeStub {
        fn precharge<'a>(
            &'a self,
            id: BillingReservationId,
            _principal: GatewayPrincipal,
            _amount: Quota,
        ) -> BillingPrechargeFuture<'a> {
            Box::pin(async move {
                self.calls.lock().unwrap().push(id);
                Ok(())
            })
        }
    }

    #[derive(Default)]
    struct SettlementStub {
        calls: Mutex<Vec<af_billing::SettlementRequest>>,
    }

    impl BillingSettlementPort for SettlementStub {
        fn settle<'a>(
            &'a self,
            request: af_billing::SettlementRequest,
        ) -> BillingSettlementFuture<'a> {
            Box::pin(async move {
                self.calls.lock().unwrap().push(request);
                Ok(())
            })
        }
    }

    struct RefundStub;

    impl RefundSignalPort for RefundStub {
        fn try_signal_refund(&self, _reservation_id: BillingReservationId) -> RefundSignalOutcome {
            RefundSignalOutcome::Accepted
        }
    }

    #[derive(Default)]
    struct UsageStub {
        records: Mutex<Vec<BillingUsageRecord>>,
    }

    impl UsageRecordPort for UsageStub {
        fn try_record(&self, record: BillingUsageRecord) -> UsageRecordOutcome {
            self.records.lock().unwrap().push(record);
            UsageRecordOutcome::Accepted
        }
    }

    #[tokio::test]
    async fn full_response_completes_the_same_lifecycle_once() {
        let usage_port = Arc::new(UsageStub::default());
        let precharge = Arc::new(PrechargeStub::default());
        let settlement = Arc::new(SettlementStub::default());
        let service = BillingChatService::new(
            Arc::new(FullStub {
                target_group_id: GroupId::new(3).unwrap(),
            }),
            Arc::new(metered_snapshot_source().await),
            Some(precharge.clone()),
            Some(Arc::new(RefundStub)),
            Some(settlement.clone()),
            usage_port.clone(),
        );
        let response = service
            .chat_completions(&principal(), request(), "request-1")
            .await
            .unwrap();
        assert!(matches!(response, OpenAiChatResponse::Full { .. }));
        assert_eq!(precharge.calls.lock().unwrap().len(), 1);
        assert_eq!(settlement.calls.lock().unwrap().len(), 1);
        assert_eq!(usage_port.records.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn pricing_capture_uses_the_group_pinned_by_the_route_plan() {
        let pricing = Arc::new(TrackingPricingSource {
            inner: metered_snapshot_source_with_target(GroupId::new(4).unwrap()).await,
            calls: Mutex::new(Vec::new()),
        });
        let service = BillingChatService::new(
            Arc::new(FullStub {
                target_group_id: GroupId::new(4).unwrap(),
            }),
            pricing.clone(),
            Some(Arc::new(PrechargeStub::default())),
            Some(Arc::new(RefundStub)),
            Some(Arc::new(SettlementStub::default())),
            Arc::new(UsageStub::default()),
        );

        service
            .chat_completions(&principal(), request(), "request-target-group")
            .await
            .unwrap();

        assert_eq!(
            pricing.calls.lock().unwrap().as_slice(),
            &[(GroupId::new(3).unwrap(), GroupId::new(4).unwrap())]
        );
    }

    #[tokio::test]
    async fn one_precharge_covers_the_entire_failover_execution() {
        let attempts = Arc::new(Mutex::new(0_usize));
        let precharge = Arc::new(PrechargeStub::default());
        let settlement = Arc::new(SettlementStub::default());
        let service = BillingChatService::new(
            Arc::new(SimulatedFailoverStub {
                attempts: attempts.clone(),
            }),
            Arc::new(metered_snapshot_source().await),
            Some(precharge.clone()),
            Some(Arc::new(RefundStub)),
            Some(settlement.clone()),
            Arc::new(UsageStub::default()),
        );

        service
            .chat_completions(&principal(), request(), "request-failover")
            .await
            .unwrap();

        assert_eq!(*attempts.lock().unwrap(), 2);
        assert_eq!(precharge.calls.lock().unwrap().len(), 1);
        assert_eq!(settlement.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn reservation_ids_are_non_zero_and_not_reused_by_generator() {
        let first = new_reservation_id().unwrap();
        let second = new_reservation_id().unwrap();
        assert_ne!(first, second);
        assert_ne!(first.bytes(), [0; 16]);
        assert_ne!(second.bytes(), [0; 16]);
    }

    fn principal() -> GatewayPrincipal {
        GatewayPrincipal::new(
            TokenId::new(1).unwrap(),
            UserId::new(2).unwrap(),
            GroupId::new(3).unwrap(),
        )
    }

    fn request() -> CanonicalRequestEnvelope {
        CanonicalRequest::new(
            Operation::Chat,
            "gpt-test".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("hello".to_owned())],
            )],
            false,
        )
        .into()
    }

    fn usage() -> Usage {
        Usage::new(
            TokenCount::new(2).unwrap(),
            TokenCount::new(1).unwrap(),
            af_protocol::UsageDetails::new(
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
            ),
            af_protocol::UsageSource::Estimated,
            af_protocol::UsageSemantics::Inclusive,
        )
        .unwrap()
    }

    #[derive(Clone)]
    struct StaticModelPriceSource {
        records: Vec<ModelPriceSourceRecord>,
    }

    impl ModelPriceSource for StaticModelPriceSource {
        fn load<'a>(&'a self) -> ModelPriceSourceFuture<'a> {
            let records = self.records.clone();
            Box::pin(async move { Ok(records) })
        }
    }

    #[derive(Clone)]
    struct StaticGroupPricingSource {
        catalog: GroupPricingSourceCatalog,
    }

    impl GroupPricingSource for StaticGroupPricingSource {
        fn load<'a>(&'a self) -> GroupPricingSourceFuture<'a> {
            let catalog = self.catalog.clone();
            Box::pin(async move { Ok(catalog) })
        }
    }

    async fn metered_snapshot_source() -> CachedRequestPricingSnapshotSource {
        metered_snapshot_source_with_target(GroupId::new(3).unwrap()).await
    }

    async fn metered_snapshot_source_with_target(
        target_group_id: GroupId,
    ) -> CachedRequestPricingSnapshotSource {
        let prices = TokenPrices::new(
            Decimal::new(1, 6),
            Decimal::new(1, 6),
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        )
        .unwrap();
        let model_prices = ModelPriceCache::load(Arc::new(StaticModelPriceSource {
            records: vec![
                ModelPriceSourceRecord::new(
                    "gpt-test".to_owned(),
                    BillingMode::PerToken,
                    prices,
                    1,
                )
                .unwrap(),
            ],
        }))
        .await
        .unwrap();
        let source_group_id = GroupId::new(3).unwrap();
        let mut groups = vec![GroupPricingSourceRecord::new(
            source_group_id,
            PricingRatio::ONE,
            None,
        )];
        if target_group_id != source_group_id {
            groups.push(GroupPricingSourceRecord::new(
                target_group_id,
                PricingRatio::new(1_200_000).unwrap(),
                None,
            ));
        }
        let ratios = (target_group_id != source_group_id)
            .then(|| {
                GroupModelRatioSourceRecord::new(
                    source_group_id,
                    target_group_id,
                    PricingRatio::new(750_000).unwrap(),
                )
            })
            .into_iter()
            .collect();
        let group_pricing = GroupPricingCache::load(Arc::new(StaticGroupPricingSource {
            catalog: GroupPricingSourceCatalog::new(groups, ratios),
        }))
        .await
        .unwrap();
        CachedRequestPricingSnapshotSource::new(model_prices, group_pricing)
    }

    struct TrackingPricingSource {
        inner: CachedRequestPricingSnapshotSource,
        calls: Mutex<Vec<(GroupId, GroupId)>>,
    }

    struct SimulatedFailoverStub {
        attempts: Arc<Mutex<usize>>,
    }

    impl ChatRoutePlanner for SimulatedFailoverStub {
        fn plan(
            &self,
            principal: &GatewayPrincipal,
            _request: CanonicalRequestEnvelope,
            _request_id: &str,
        ) -> Result<Box<dyn PlannedChatExecution>, AfError> {
            Ok(Box::new(SimulatedFailoverExecution {
                target_group_id: principal.group_id(),
                attempts: self.attempts.clone(),
            }))
        }
    }

    struct SimulatedFailoverExecution {
        target_group_id: GroupId,
        attempts: Arc<Mutex<usize>>,
    }

    impl PlannedChatExecution for SimulatedFailoverExecution {
        fn target_group_id(&self) -> GroupId {
            self.target_group_id
        }

        fn execute(self: Box<Self>) -> PlannedChatExecutionFuture {
            Box::pin(async move {
                // 两次内部尝试模拟 RelayStateMachine 完成一次故障转移。
                *self.attempts.lock().unwrap() = 2;
                Ok(OpenAiChatResponse::Full {
                    body: Bytes::from_static(b"{}"),
                    usage: Ok(usage()),
                })
            })
        }
    }

    impl RequestPricingSnapshotSource for TrackingPricingSource {
        fn capture(
            &self,
            model: &str,
            source_group_id: GroupId,
            target_group_id: GroupId,
            current_second: u32,
        ) -> Result<RequestPricingSnapshot, RequestPricingSnapshotError> {
            self.calls
                .lock()
                .unwrap()
                .push((source_group_id, target_group_id));
            self.inner
                .capture(model, source_group_id, target_group_id, current_second)
        }
    }
}
