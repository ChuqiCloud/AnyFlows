use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use af_adapter::Bytes;
use af_billing::{
    BillingLifecycleError, BillingPrechargeError, BillingPrechargePort, BillingRequestLifecycle,
    BillingRequestPlan, BillingSettlementPort, BillingUsageContext, BillingUsageDimensions,
    BillingUsageObservation, BillingUsageTiming, RefundSignalPort, RequestPricingSnapshotError,
    RequestPricingSnapshotSource, UsageRecordPort,
};
use af_domain::{
    AfError, BillingContractPriceSnapshot, BillingReservationId, ChannelId, ConcurrencyLimit,
    GatewayPrincipal, GroupId, Protocol,
};
use af_http::{ChatService, ChatServiceFuture};
use af_protocol::{CanonicalRequestEnvelope, TokenCount, Usage};
use af_relay::{
    ChatResponse, GenerationCompletionFuture, GenerationCompletionHook, GenerationStream,
    OpenAiChatUsageHandle, RelayDiagnosticInput, UsageResolutionError,
    estimate_openai_request_upper_bound,
};
use tokio::time::sleep;
use uuid::Uuid;

const MAX_PRECHARGE_ATTEMPTS: usize = 3;
const MAX_COMPLETION_ATTEMPTS: usize = 3;
const RETRY_DELAY: Duration = Duration::from_millis(20);
const DEFAULT_MAX_OUTPUT_TOKENS: i64 = 8_192;

/// 已固定路由计划的一次异步执行结果。
pub type PlannedChatExecutionFuture = Pin<
    Box<
        dyn Future<Output = Result<crate::RoutedExecution<ChatResponse>, AfError>> + Send + 'static,
    >,
>;

/// 路由计划生成的异步返回类型；Redis 粘性查找必须在计费预扣前完成。
pub type ChatRoutePlanFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn PlannedChatExecution>, AfError>> + Send + 'a>>;

/// 在计费前完成候选、目标分组与运行时目标装配的请求级计划。
pub trait PlannedChatExecution: Send {
    /// 返回本计划全部候选共同的实际计费分组。
    fn target_group_id(&self) -> GroupId;

    /// 返回路由映射后用于定价解析的 Canonical 模型。
    fn pricing_model(&self) -> &str;

    /// 返回经过路由期受控改写后的 Canonical 请求，计费只能以此请求估算与结算。
    fn canonical_request(&self) -> &CanonicalRequestEnvelope;

    /// 在预扣成功后消费计划并执行全部候选故障转移。
    fn execute(self: Box<Self>) -> PlannedChatExecutionFuture;
}

/// 计费完成后交还给公开服务边界的完整或流式结果。
enum BilledChatExecution {
    Full {
        body: Bytes,
        usage: Usage,
        channel_id: ChannelId,
    },
    Stream {
        body: Box<dyn GenerationStream>,
        usage: OpenAiChatUsageHandle,
        channel_id: ChannelId,
        completion: Box<BillingStreamCompletionHook>,
    },
}

/// 为已认证 Chat 请求信封生成不可变路由计划的对象安全端口。
pub trait ChatRoutePlanner: Send + Sync {
    /// 从当前运行时快照固定候选并完成所有安全装配；本方法不得发送上游请求。
    fn plan<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &'a str,
        diagnostic: RelayDiagnosticInput,
    ) -> ChatRoutePlanFuture<'a>;
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
    request_outcomes: Option<crate::RequestOutcomeRuntime>,
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
            request_outcomes: None,
        }
    }

    /// 注入同步模型请求终态的只追加观测运行时。
    #[must_use]
    pub fn with_request_outcomes(mut self, runtime: crate::RequestOutcomeRuntime) -> Self {
        self.request_outcomes = Some(runtime);
        self
    }

    async fn call(
        &self,
        principal: &GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &str,
        diagnostic: RelayDiagnosticInput,
    ) -> Result<ChatResponse, AfError> {
        let outcome = crate::RequestOutcomeContext::for_principal(
            request_id,
            response_protocol,
            request.canonical().operation,
            request.requested_model(),
            *principal,
        );
        match self
            .call_billed(
                principal,
                user_concurrency,
                request,
                response_protocol,
                request_id,
                diagnostic,
            )
            .await
        {
            Ok(BilledChatExecution::Full {
                body,
                usage,
                channel_id,
            }) => {
                outcome
                    .record_success(self.request_outcomes.as_ref(), channel_id)
                    .await;
                Ok(ChatResponse::Full {
                    body,
                    usage: Ok(usage),
                })
            }
            Ok(BilledChatExecution::Stream {
                body,
                usage,
                channel_id,
                mut completion,
            }) => {
                completion.outcome = Some(StreamRequestOutcome {
                    context: outcome,
                    runtime: self.request_outcomes.clone(),
                    channel_id,
                });
                Ok(ChatResponse::Stream {
                    body: body.with_completion_hook(completion),
                    usage,
                })
            }
            Err(error) => {
                outcome
                    .record_failure(self.request_outcomes.as_ref(), &error, None)
                    .await;
                Err(error)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn call_billed(
        &self,
        principal: &GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &str,
        diagnostic: RelayDiagnosticInput,
    ) -> Result<BilledChatExecution, AfError> {
        let started_at = Instant::now();
        // 缺省值必须同时约束预扣和上游输出，避免预留额度低于平台真实成本。
        let request = request.with_default_max_output_tokens(
            TokenCount::new(DEFAULT_MAX_OUTPUT_TOKENS).expect("网关默认输出上限必须是合法正整数"),
        );
        let planned = self
            .inner
            .plan(
                principal,
                user_concurrency,
                request,
                response_protocol,
                request_id,
                diagnostic,
            )
            .await?;
        // 路由计划可能已按受控正文档案重建消息；预扣与后续结算必须使用同一份事实。
        let canonical = planned.canonical_request().canonical();
        let observation_context = BillingUsageContext::new(
            request_id,
            planned.canonical_request().requested_model(),
            response_protocol,
            canonical.operation,
            canonical.stream,
            canonical.reasoning.and_then(|value| value.effort()),
            canonical.reasoning.and_then(|value| value.budget_tokens()),
        )
        .map_err(|_| AfError::Internal)?;
        let upper_bound =
            estimate_openai_request_upper_bound(canonical).map_err(map_upper_bound_error)?;
        let source_group_id = principal.group_id();
        let target_group_id = planned.target_group_id();
        let pricing_model = planned.pricing_model().to_owned();
        let snapshot = self
            .pricing_source
            .capture_for_request(
                &pricing_model,
                source_group_id,
                target_group_id,
                principal
                    .organization_principal()
                    .map(|organization| organization.organization_id()),
                response_protocol,
                crate::utc_time::current_utc_day_second().ok_or(AfError::Internal)?,
            )
            .await
            .map_err(map_snapshot_error)?;
        let plan = BillingRequestPlan::prepare(snapshot.resolver(), &upper_bound)
            .map_err(map_lifecycle_error)?
            .with_contract_price(snapshot.contract_price());
        let reservation_id = new_reservation_id()?;
        let mut lifecycle = match plan.precharge_quota() {
            None => plan
                .start_free(*principal, reservation_id, self.usage.clone())
                .map_err(map_lifecycle_error)?,
            Some(amount) => {
                let precharge = self.precharge.as_ref().ok_or(AfError::Internal)?;
                let refund = self.refund.as_ref().ok_or(AfError::Internal)?;
                let settlement = self.settlement.as_ref().ok_or(AfError::Internal)?;
                precharge_with_retries(
                    precharge,
                    reservation_id,
                    *principal,
                    amount,
                    snapshot.contract_price(),
                )
                .await?;
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

        let routed = planned.execute().await?;
        let channel_id = routed.channel_id();
        let response = routed.into_value();
        match response {
            ChatResponse::Full { body, usage } => {
                let usage = usage.map_err(|_| AfError::Internal)?;
                let observation = call_observation(observation_context, None, started_at.elapsed());
                let _ = complete_with_observation_retries(
                    &mut lifecycle,
                    usage,
                    BillingUsageDimensions::empty(),
                    observation,
                )
                .await
                .map_err(map_lifecycle_error)?;
                Ok(BilledChatExecution::Full {
                    body,
                    usage,
                    channel_id,
                })
            }
            ChatResponse::Stream { body, usage } => {
                let completion = Box::new(BillingStreamCompletionHook {
                    lifecycle: Some(lifecycle),
                    context: observation_context,
                    started_at,
                    first_token: started_at.elapsed(),
                    outcome: None,
                });
                Ok(BilledChatExecution::Stream {
                    body,
                    usage,
                    channel_id,
                    completion,
                })
            }
        }
    }
}

impl ChatService for BillingChatService {
    fn chat_completions<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &'a str,
        diagnostic: RelayDiagnosticInput,
    ) -> ChatServiceFuture<'a> {
        Box::pin(self.call(
            principal,
            user_concurrency,
            request,
            response_protocol,
            request_id,
            diagnostic,
        ))
    }
}

/// 流式 usage 完成回调持有唯一计费生命周期；流提前结束时生命周期析构会触发既有退款兜底。
struct BillingStreamCompletionHook {
    lifecycle: Option<BillingRequestLifecycle>,
    context: BillingUsageContext,
    started_at: Instant,
    first_token: Duration,
    outcome: Option<StreamRequestOutcome>,
}

/// 流式计费与请求终态共享的一次性低敏感度上下文。
struct StreamRequestOutcome {
    context: crate::RequestOutcomeContext,
    runtime: Option<crate::RequestOutcomeRuntime>,
    channel_id: ChannelId,
}

impl StreamRequestOutcome {
    fn spawn_success(self) -> Option<tokio::task::JoinHandle<()>> {
        let runtime = tokio::runtime::Handle::try_current().ok()?;
        Some(runtime.spawn(async move {
            self.context
                .record_success(self.runtime.as_ref(), self.channel_id)
                .await;
        }))
    }

    fn spawn_failure(self, error: AfError) -> Option<tokio::task::JoinHandle<()>> {
        let runtime = tokio::runtime::Handle::try_current().ok()?;
        Some(runtime.spawn(async move {
            self.context
                .record_failure(self.runtime.as_ref(), &error, Some(self.channel_id))
                .await;
        }))
    }
}

/// 确保流回调在终态判定前被取消时仍写入一次结果未知事实。
struct StreamRequestOutcomeGuard {
    outcome: Option<StreamRequestOutcome>,
}

impl StreamRequestOutcomeGuard {
    const fn new(outcome: Option<StreamRequestOutcome>) -> Self {
        Self { outcome }
    }

    async fn record_success(mut self) {
        let Some(outcome) = self.outcome.take() else {
            return;
        };
        if let Some(task) = outcome.spawn_success() {
            let _ = task.await;
        }
    }

    async fn record_failure(mut self, error: AfError) {
        let Some(outcome) = self.outcome.take() else {
            return;
        };
        if let Some(task) = outcome.spawn_failure(error) {
            let _ = task.await;
        }
    }
}

impl Drop for StreamRequestOutcomeGuard {
    fn drop(&mut self) {
        let Some(outcome) = self.outcome.take() else {
            return;
        };
        if outcome
            .spawn_failure(AfError::RequestOutcomeUnknown)
            .is_none()
        {
            tracing::error!(
                error_kind = "request_outcome_stream_drop_without_runtime",
                "流式请求提前释放且当前没有可用异步运行时"
            );
        }
    }
}

impl GenerationCompletionHook for BillingStreamCompletionHook {
    fn on_complete(
        mut self: Box<Self>,
        usage: Result<Usage, UsageResolutionError>,
    ) -> GenerationCompletionFuture {
        let outcome = StreamRequestOutcomeGuard::new(self.outcome.take());
        Box::pin(async move {
            let Some(mut lifecycle) = self.lifecycle.take() else {
                outcome.record_failure(AfError::Internal).await;
                return;
            };
            let Ok(usage) = usage else {
                drop(lifecycle);
                outcome.record_failure(AfError::RequestOutcomeUnknown).await;
                return;
            };
            let observation = call_observation(
                self.context,
                Some(self.first_token),
                self.started_at.elapsed(),
            );
            match complete_with_observation_retries(
                &mut lifecycle,
                usage,
                BillingUsageDimensions::empty(),
                observation,
            )
            .await
            {
                Ok(_) => {
                    outcome.record_success().await;
                }
                Err(error) => {
                    tracing::error!(
                        error_kind = "billing_stream_completion_failed",
                        "流式响应结束后的计费完成失败"
                    );
                    outcome.record_failure(map_lifecycle_error(error)).await;
                }
            }
        })
    }
}

impl Drop for BillingStreamCompletionHook {
    fn drop(&mut self) {
        let Some(outcome) = self.outcome.take() else {
            return;
        };
        // Drop 只移交一次性收尾任务，不在析构路径执行数据库 IO。
        if outcome
            .spawn_failure(AfError::RequestOutcomeUnknown)
            .is_none()
        {
            tracing::error!(
                error_kind = "request_outcome_stream_drop_without_runtime",
                "流式请求提前释放且当前没有可用异步运行时"
            );
        }
    }
}

pub(crate) async fn precharge_with_retries(
    port: &Arc<dyn BillingPrechargePort>,
    reservation_id: BillingReservationId,
    principal: GatewayPrincipal,
    amount: af_domain::Quota,
    contract_price: Option<BillingContractPriceSnapshot>,
) -> Result<(), AfError> {
    let mut attempts = 0;
    loop {
        match port
            .precharge_with_snapshot(reservation_id, principal, amount, contract_price)
            .await
        {
            Ok(()) => return Ok(()),
            Err(BillingPrechargeError::OutcomeUnknown) if attempts + 1 < MAX_PRECHARGE_ATTEMPTS => {
                attempts += 1;
                sleep(RETRY_DELAY).await;
            }
            Err(error) => return Err(map_precharge_error(error)),
        }
    }
}

pub(crate) async fn complete_with_retries(
    lifecycle: &mut BillingRequestLifecycle,
    usage: Usage,
) -> Result<af_billing::BillingCompletion, BillingLifecycleError> {
    complete_with_dimensions_retries(
        lifecycle,
        usage,
        af_billing::BillingUsageDimensions::empty(),
    )
    .await
}

pub(crate) async fn complete_with_dimensions_retries(
    lifecycle: &mut BillingRequestLifecycle,
    usage: Usage,
    dimensions: af_billing::BillingUsageDimensions,
) -> Result<af_billing::BillingCompletion, BillingLifecycleError> {
    complete_with_observation_retries(
        lifecycle,
        usage,
        dimensions,
        BillingUsageObservation::default(),
    )
    .await
}

pub(crate) async fn complete_with_observation_retries(
    lifecycle: &mut BillingRequestLifecycle,
    usage: Usage,
    dimensions: BillingUsageDimensions,
    observation: BillingUsageObservation,
) -> Result<af_billing::BillingCompletion, BillingLifecycleError> {
    let mut attempts = 0;
    loop {
        match lifecycle
            .complete_with_observation(usage, dimensions, observation)
            .await
        {
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

fn call_observation(
    context: BillingUsageContext,
    first_token: Option<Duration>,
    duration: Duration,
) -> BillingUsageObservation {
    let Ok(timing) = BillingUsageTiming::new(first_token, duration) else {
        tracing::error!(
            error_kind = "billing_usage_timing_overflow",
            "调用日志耗时无法安全持久化，本次计费继续完成"
        );
        return BillingUsageObservation::default();
    };
    BillingUsageObservation::new(context, timing)
}

fn retryable_completion_error(error: BillingLifecycleError) -> bool {
    matches!(
        error,
        BillingLifecycleError::Settlement(af_billing::BillingSettlementError::OutcomeUnknown)
            | BillingLifecycleError::UsageRecordSaturated
    )
}

pub(crate) fn new_reservation_id() -> Result<BillingReservationId, AfError> {
    BillingReservationId::new(Uuid::new_v4().into_bytes()).map_err(|_| AfError::Internal)
}

fn map_upper_bound_error(error: UsageResolutionError) -> AfError {
    match error {
        UsageResolutionError::UnsupportedContent
        | UsageResolutionError::StatefulContextUnsupported => AfError::InvalidRequest,
        UsageResolutionError::Interrupted
        | UsageResolutionError::InvalidSequence
        | UsageResolutionError::Overflow
        | UsageResolutionError::EstimationFailed => AfError::Internal,
    }
}

pub(crate) fn map_snapshot_error(_error: RequestPricingSnapshotError) -> AfError {
    AfError::Internal
}

fn map_precharge_error(error: BillingPrechargeError) -> AfError {
    match error {
        BillingPrechargeError::InsufficientQuota => AfError::InsufficientQuota,
        BillingPrechargeError::RateLimited { retry_after } => {
            AfError::QuotaWindowLimited { retry_after }
        }
        BillingPrechargeError::OutcomeUnknown
        | BillingPrechargeError::Conflict
        | BillingPrechargeError::Unavailable
        | BillingPrechargeError::Invariant => AfError::Internal,
        _ => AfError::Internal,
    }
}

pub(crate) fn map_lifecycle_error(error: BillingLifecycleError) -> AfError {
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

    use af_adapter::{Bytes, HeaderMap};
    use af_billing::{
        BillingMode, BillingPrechargeFuture, BillingPrechargePort, BillingSettlementFuture,
        BillingSettlementPort, BillingUsageRecord, CachedRequestPricingSnapshotSource,
        GroupModelRatioSourceRecord, GroupPricingCache, GroupPricingSource,
        GroupPricingSourceCatalog, GroupPricingSourceFuture, GroupPricingSourceRecord,
        ModelPriceCache, ModelPriceSource, ModelPriceSourceFuture, ModelPriceSourceRecord,
        PricingRatio, RefundSignalOutcome, RequestPricingSnapshot, TokenPrices, UsageRecordOutcome,
    };
    use af_db::RequestOutcomeRepository;
    use af_domain::{
        ChannelId, GroupId, Operation, Quota, QuotaWindowRetryAfter, Role, TokenId, UserId,
    };
    use af_http::ChatService;
    use af_protocol::{
        CanonicalRequest, CanonicalRequestEnvelope, ContentBlock, Message, TokenCount, Usage,
        openai_responses,
    };
    use rust_decimal::Decimal;
    use sea_orm::{ConnectionTrait, Statement};

    use super::*;
    use crate::test_database::SqliteTestDatabase;

    #[test]
    fn recoverable_window_precharge_maps_to_downstream_rate_limit() {
        let retry_after = QuotaWindowRetryAfter::from_seconds(37).unwrap();
        assert_eq!(
            map_precharge_error(BillingPrechargeError::RateLimited { retry_after }),
            AfError::QuotaWindowLimited { retry_after }
        );
    }

    #[tokio::test]
    async fn stream_outcome_guard_records_each_terminal_path_once() {
        let database = SqliteTestDatabase::new("stream-outcome").await;
        let backend = database.seed().get_database_backend();
        database
            .seed()
            .execute(Statement::from_string(
                backend,
                "INSERT INTO channels (id, name, \"type\", protocol, model_mapping, param_override, header_override, settings) VALUES (1, '流式终态测试渠道', 'openai', 'openai_chat', '{}', '{}', '{}', '{}')",
            ))
            .await
            .unwrap();
        let runtime = crate::RequestOutcomeRuntime::new(RequestOutcomeRepository::new(
            database.pool().clone(),
        ));

        StreamRequestOutcomeGuard::new(stream_outcome(&runtime, "request-stream-success"))
            .record_success()
            .await;
        StreamRequestOutcomeGuard::new(stream_outcome(&runtime, "request-stream-failure"))
            .record_failure(AfError::Internal)
            .await;
        drop(StreamRequestOutcomeGuard::new(stream_outcome(
            &runtime,
            "request-stream-interrupted",
        )));

        let rows = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let rows = database
                    .seed()
                    .query_all(Statement::from_string(
                        backend,
                        "SELECT request_id, outcome, error_kind FROM request_outcome_logs ORDER BY request_id",
                    ))
                    .await
                    .unwrap();
                if rows.len() == 3 {
                    break rows;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let facts = rows
            .iter()
            .map(|row| {
                (
                    row.try_get::<String>("", "request_id").unwrap(),
                    row.try_get::<i16>("", "outcome").unwrap(),
                    row.try_get::<Option<String>>("", "error_kind").unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            facts,
            vec![
                (
                    "request-stream-failure".to_owned(),
                    2,
                    Some("internal".to_owned()),
                ),
                (
                    "request-stream-interrupted".to_owned(),
                    2,
                    Some("outcome_unknown".to_owned()),
                ),
                ("request-stream-success".to_owned(), 1, None),
            ]
        );
        database.close().await;
    }

    fn stream_outcome(
        runtime: &crate::RequestOutcomeRuntime,
        request_id: &str,
    ) -> Option<StreamRequestOutcome> {
        Some(StreamRequestOutcome {
            context: crate::RequestOutcomeContext::new(
                request_id,
                Protocol::OpenAiChat,
                Operation::Chat,
                "gpt-test",
            ),
            runtime: Some(runtime.clone()),
            channel_id: ChannelId::new(1).unwrap(),
        })
    }

    struct FullStub {
        target_group_id: GroupId,
        pricing_model_override: Option<String>,
    }

    impl ChatRoutePlanner for FullStub {
        fn plan<'a>(
            &'a self,
            _principal: &'a GatewayPrincipal,
            _user_concurrency: Option<ConcurrencyLimit>,
            request: CanonicalRequestEnvelope,
            _response_protocol: Protocol,
            _request_id: &'a str,
            _diagnostic: RelayDiagnosticInput,
        ) -> ChatRoutePlanFuture<'a> {
            Box::pin(async move {
                Ok(Box::new(FullExecution {
                    target_group_id: self.target_group_id,
                    pricing_model: self
                        .pricing_model_override
                        .clone()
                        .unwrap_or_else(|| request.canonical().model.clone()),
                    request,
                }) as Box<dyn PlannedChatExecution>)
            })
        }
    }

    struct FullExecution {
        target_group_id: GroupId,
        pricing_model: String,
        request: CanonicalRequestEnvelope,
    }

    struct ProtocolTrackingStub {
        protocols: Arc<Mutex<Vec<Protocol>>>,
        target_group_id: GroupId,
    }

    struct RequestTrackingStub {
        requests: Arc<Mutex<Vec<(i64, Option<Protocol>)>>>,
        target_group_id: GroupId,
    }

    impl ChatRoutePlanner for ProtocolTrackingStub {
        fn plan<'a>(
            &'a self,
            _principal: &'a GatewayPrincipal,
            _user_concurrency: Option<ConcurrencyLimit>,
            request: CanonicalRequestEnvelope,
            response_protocol: Protocol,
            _request_id: &'a str,
            _diagnostic: RelayDiagnosticInput,
        ) -> ChatRoutePlanFuture<'a> {
            self.protocols.lock().unwrap().push(response_protocol);
            Box::pin(async move {
                Ok(Box::new(FullExecution {
                    target_group_id: self.target_group_id,
                    pricing_model: request.canonical().model.clone(),
                    request,
                }) as Box<dyn PlannedChatExecution>)
            })
        }
    }

    impl ChatRoutePlanner for RequestTrackingStub {
        fn plan<'a>(
            &'a self,
            _principal: &'a GatewayPrincipal,
            _user_concurrency: Option<ConcurrencyLimit>,
            request: CanonicalRequestEnvelope,
            _response_protocol: Protocol,
            _request_id: &'a str,
            _diagnostic: RelayDiagnosticInput,
        ) -> ChatRoutePlanFuture<'a> {
            let max_output_tokens = request
                .canonical()
                .sampling
                .max_output_tokens()
                .expect("计费入口必须在规划前补齐输出上限")
                .get();
            self.requests
                .lock()
                .unwrap()
                .push((max_output_tokens, request.source_protocol()));
            Box::pin(async move {
                Ok(Box::new(FullExecution {
                    target_group_id: self.target_group_id,
                    pricing_model: request.canonical().model.clone(),
                    request,
                }) as Box<dyn PlannedChatExecution>)
            })
        }
    }

    impl PlannedChatExecution for FullExecution {
        fn target_group_id(&self) -> GroupId {
            self.target_group_id
        }

        fn pricing_model(&self) -> &str {
            &self.pricing_model
        }

        fn canonical_request(&self) -> &CanonicalRequestEnvelope {
            &self.request
        }

        fn execute(self: Box<Self>) -> PlannedChatExecutionFuture {
            Box::pin(async move {
                Ok(crate::RoutedExecution::new(
                    ChatResponse::Full {
                        body: Bytes::from_static(b"{}"),
                        usage: Ok(usage()),
                    },
                    af_domain::ChannelId::new(1).unwrap(),
                ))
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
                pricing_model_override: None,
            }),
            Arc::new(metered_snapshot_source().await),
            Some(precharge.clone()),
            Some(Arc::new(RefundStub)),
            Some(settlement.clone()),
            usage_port.clone(),
        );
        let response = service
            .chat_completions(
                &principal(),
                None,
                request(),
                Protocol::OpenAiChat,
                "request-1",
                diagnostic(),
            )
            .await
            .unwrap();
        assert!(matches!(response, ChatResponse::Full { .. }));
        assert_eq!(precharge.calls.lock().unwrap().len(), 1);
        assert_eq!(settlement.calls.lock().unwrap().len(), 1);
        assert_eq!(usage_port.records.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn expression_pricing_uses_the_same_precharge_settlement_and_usage_chain() {
        let usage_port = Arc::new(UsageStub::default());
        let precharge = Arc::new(PrechargeStub::default());
        let settlement = Arc::new(SettlementStub::default());
        let service = BillingChatService::new(
            Arc::new(FullStub {
                target_group_id: GroupId::new(3).unwrap(),
                pricing_model_override: None,
            }),
            Arc::new(expression_snapshot_source().await),
            Some(precharge.clone()),
            Some(Arc::new(RefundStub)),
            Some(settlement.clone()),
            usage_port.clone(),
        );

        service
            .chat_completions(
                &principal(),
                None,
                request(),
                Protocol::OpenAiChat,
                "request-expression",
                diagnostic(),
            )
            .await
            .unwrap();

        assert_eq!(precharge.calls.lock().unwrap().len(), 1);
        assert_eq!(settlement.calls.lock().unwrap().len(), 1);
        assert_eq!(usage_port.records.lock().unwrap().len(), 1);
        assert_eq!(
            usage_port.records.lock().unwrap()[0].billing_mode(),
            BillingMode::PerToken
        );
    }

    #[tokio::test]
    async fn response_protocol_reaches_the_route_plan_without_changing_billing() {
        let protocols = Arc::new(Mutex::new(Vec::new()));
        let settlement = Arc::new(SettlementStub::default());
        let service = BillingChatService::new(
            Arc::new(ProtocolTrackingStub {
                protocols: Arc::clone(&protocols),
                target_group_id: GroupId::new(3).unwrap(),
            }),
            Arc::new(metered_snapshot_source().await),
            Some(Arc::new(PrechargeStub::default())),
            Some(Arc::new(RefundStub)),
            Some(settlement.clone()),
            Arc::new(UsageStub::default()),
        );

        service
            .chat_completions(
                &principal(),
                None,
                request(),
                Protocol::Anthropic,
                "request-anthropic",
                diagnostic(),
            )
            .await
            .unwrap();
        service
            .chat_completions(
                &principal(),
                None,
                responses_request(),
                Protocol::OpenAiResponses,
                "request-responses",
                diagnostic(),
            )
            .await
            .unwrap();

        assert_eq!(
            protocols.lock().unwrap().as_slice(),
            &[Protocol::Anthropic, Protocol::OpenAiResponses]
        );
        assert_eq!(settlement.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn missing_output_limit_reaches_route_plan_as_rebuilt_default() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let service = BillingChatService::new(
            Arc::new(RequestTrackingStub {
                requests: requests.clone(),
                target_group_id: GroupId::new(3).unwrap(),
            }),
            Arc::new(metered_snapshot_source().await),
            Some(Arc::new(PrechargeStub::default())),
            Some(Arc::new(RefundStub)),
            Some(Arc::new(SettlementStub::default())),
            Arc::new(UsageStub::default()),
        );

        service
            .chat_completions(
                &principal(),
                None,
                responses_request(),
                Protocol::OpenAiResponses,
                "request-default-output-limit",
                diagnostic(),
            )
            .await
            .unwrap();

        assert_eq!(requests.lock().unwrap().as_slice(), &[(8_192, None)]);
    }

    #[tokio::test]
    async fn stateless_encrypted_reasoning_reaches_precharge() {
        let precharge = Arc::new(PrechargeStub::default());
        let settlement = Arc::new(SettlementStub::default());
        let service = BillingChatService::new(
            Arc::new(FullStub {
                target_group_id: GroupId::new(3).unwrap(),
                pricing_model_override: None,
            }),
            Arc::new(metered_snapshot_source().await),
            Some(precharge.clone()),
            Some(Arc::new(RefundStub)),
            Some(settlement.clone()),
            Arc::new(UsageStub::default()),
        );

        service
            .chat_completions(
                &principal(),
                None,
                encrypted_reasoning_responses_request(),
                Protocol::OpenAiResponses,
                "request-encrypted-reasoning",
                diagnostic(),
            )
            .await
            .unwrap();

        assert_eq!(precharge.calls.lock().unwrap().len(), 1);
        assert_eq!(settlement.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn pricing_capture_uses_model_and_group_pinned_by_the_route_plan() {
        let pricing = Arc::new(TrackingPricingSource {
            inner: metered_snapshot_source_with_target(GroupId::new(4).unwrap()).await,
            calls: Mutex::new(Vec::new()),
        });
        let service = BillingChatService::new(
            Arc::new(FullStub {
                target_group_id: GroupId::new(4).unwrap(),
                pricing_model_override: Some("gpt-routed".to_owned()),
            }),
            pricing.clone(),
            Some(Arc::new(PrechargeStub::default())),
            Some(Arc::new(RefundStub)),
            Some(Arc::new(SettlementStub::default())),
            Arc::new(UsageStub::default()),
        );

        service
            .chat_completions(
                &principal(),
                None,
                request(),
                Protocol::OpenAiChat,
                "request-target-group",
                diagnostic(),
            )
            .await
            .unwrap();

        assert_eq!(
            pricing.calls.lock().unwrap().as_slice(),
            &[(
                "gpt-routed".to_owned(),
                GroupId::new(3).unwrap(),
                GroupId::new(4).unwrap(),
            )]
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
            .chat_completions(
                &principal(),
                None,
                request(),
                Protocol::OpenAiChat,
                "request-failover",
                diagnostic(),
            )
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

    fn diagnostic() -> RelayDiagnosticInput {
        RelayDiagnosticInput::capture(
            "POST",
            "/v1/chat/completions",
            &HeaderMap::new(),
            &Bytes::new(),
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

    fn responses_request() -> CanonicalRequestEnvelope {
        openai_responses::parse_request_envelope(Bytes::from_static(
            br#"{"model":"gpt-test","input":"hello","store":false}"#,
        ))
        .unwrap()
    }

    fn encrypted_reasoning_responses_request() -> CanonicalRequestEnvelope {
        openai_responses::parse_request_envelope(Bytes::from_static(
            br#"{
                "model":"gpt-test",
                "input":[
                    {
                        "type":"reasoning",
                        "encrypted_content":"encrypted-reasoning-canary",
                        "summary":[{"type":"summary_text","text":"brief summary"}]
                    },
                    {"role":"user","content":[{"type":"input_text","text":"continue"}]}
                ],
                "include":["reasoning.encrypted_content"],
                "store":false
            }"#,
        ))
        .unwrap()
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

    async fn expression_snapshot_source() -> CachedRequestPricingSnapshotSource {
        let model_prices = ModelPriceCache::load(Arc::new(StaticModelPriceSource {
            records: vec![
                ModelPriceSourceRecord::expression(
                    "gpt-test".to_owned(),
                    af_billing::BillingExpressionDefinition::new(
                        r#"tier("base", p * 2 + c * 3)"#.to_owned(),
                    )
                    .unwrap(),
                    2,
                )
                .unwrap(),
            ],
        }))
        .await
        .unwrap();
        let group_id = GroupId::new(3).unwrap();
        let group_pricing = GroupPricingCache::load(Arc::new(StaticGroupPricingSource {
            catalog: GroupPricingSourceCatalog::new(
                vec![GroupPricingSourceRecord::new(
                    group_id,
                    PricingRatio::ONE,
                    None,
                )],
                Vec::new(),
            ),
        }))
        .await
        .unwrap();
        CachedRequestPricingSnapshotSource::new(model_prices, group_pricing)
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
                ModelPriceSourceRecord::new(
                    "gpt-routed".to_owned(),
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
        calls: Mutex<Vec<(String, GroupId, GroupId)>>,
    }

    struct SimulatedFailoverStub {
        attempts: Arc<Mutex<usize>>,
    }

    impl ChatRoutePlanner for SimulatedFailoverStub {
        fn plan<'a>(
            &'a self,
            principal: &'a GatewayPrincipal,
            _user_concurrency: Option<ConcurrencyLimit>,
            request: CanonicalRequestEnvelope,
            _response_protocol: Protocol,
            _request_id: &'a str,
            _diagnostic: RelayDiagnosticInput,
        ) -> ChatRoutePlanFuture<'a> {
            let target_group_id = principal.group_id();
            let attempts = self.attempts.clone();
            Box::pin(async move {
                Ok(Box::new(SimulatedFailoverExecution {
                    target_group_id,
                    pricing_model: request.canonical().model.clone(),
                    request,
                    attempts,
                }) as Box<dyn PlannedChatExecution>)
            })
        }
    }

    struct SimulatedFailoverExecution {
        target_group_id: GroupId,
        pricing_model: String,
        request: CanonicalRequestEnvelope,
        attempts: Arc<Mutex<usize>>,
    }

    impl PlannedChatExecution for SimulatedFailoverExecution {
        fn target_group_id(&self) -> GroupId {
            self.target_group_id
        }

        fn pricing_model(&self) -> &str {
            &self.pricing_model
        }

        fn canonical_request(&self) -> &CanonicalRequestEnvelope {
            &self.request
        }

        fn execute(self: Box<Self>) -> PlannedChatExecutionFuture {
            Box::pin(async move {
                // 两次内部尝试模拟 RelayStateMachine 完成一次故障转移。
                *self.attempts.lock().unwrap() = 2;
                Ok(crate::RoutedExecution::new(
                    ChatResponse::Full {
                        body: Bytes::from_static(b"{}"),
                        usage: Ok(usage()),
                    },
                    af_domain::ChannelId::new(1).unwrap(),
                ))
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
                .push((model.to_owned(), source_group_id, target_group_id));
            self.inner
                .capture(model, source_group_id, target_group_id, current_second)
        }
    }
}
