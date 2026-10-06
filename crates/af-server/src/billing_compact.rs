use std::{future::Future, pin::Pin, sync::Arc};

use af_billing::{
    BillingPrechargePort, BillingRequestLifecycle, BillingRequestPlan, BillingSettlementPort,
    RefundSignalPort, RequestPricingSnapshotSource, UsageRecordPort,
};
use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal, GroupId};
use af_http::{ResponsesCompactService, ResponsesCompactServiceFuture};
use af_protocol::{
    CanonicalResponsesCompactionRequest, ResponsesCompactionUsage, TokenCount, Usage, UsageDetails,
    UsageSemantics, UsageSource, openai_chat::MAX_OUTPUT_TOKENS, openai_responses_compact,
};
use af_relay::ResponsesCompactionResponse;

use crate::billing_chat::{
    complete_with_retries, map_lifecycle_error, map_snapshot_error, new_reservation_id,
    precharge_with_retries,
};

/// 已固定路由计划的一次 Responses Compact 异步执行结果。
pub type PlannedResponsesCompactExecutionFuture = Pin<
    Box<
        dyn Future<Output = Result<crate::RoutedExecution<ResponsesCompactionResponse>, AfError>>
            + Send
            + 'static,
    >,
>;

/// 在计费前完成专用候选与目标分组装配的请求级计划。
pub trait PlannedResponsesCompactExecution: Send {
    /// 返回本计划全部候选共同的实际计费分组。
    fn target_group_id(&self) -> GroupId;

    /// 在唯一一次预扣成功后消费计划并执行专用 Compact 故障转移。
    fn execute(self: Box<Self>) -> PlannedResponsesCompactExecutionFuture;
}

/// 为已认证 Responses Compact 请求生成不可变专用路由计划的对象安全端口。
pub trait ResponsesCompactRoutePlanner: Send + Sync {
    /// 固定明确具备 Compact 资格的原生 Responses 候选；本方法不得发送上游请求。
    fn plan<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalResponsesCompactionRequest,
        request_id: &'a str,
    ) -> ResponsesCompactRoutePlanFuture<'a>;
}

/// Responses Compact 路由计划生成的异步返回类型。
pub type ResponsesCompactRoutePlanFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Box<dyn PlannedResponsesCompactExecution>, AfError>> + Send + 'a,
    >,
>;

/// 已装配单次计费依赖的 Responses Compact 服务装饰器。
pub struct BillingResponsesCompactService {
    inner: Arc<dyn ResponsesCompactRoutePlanner>,
    pricing_source: Arc<dyn RequestPricingSnapshotSource>,
    precharge: Option<Arc<dyn BillingPrechargePort>>,
    refund: Option<Arc<dyn RefundSignalPort>>,
    settlement: Option<Arc<dyn BillingSettlementPort>>,
    usage: Arc<dyn UsageRecordPort>,
    request_outcomes: Option<crate::RequestOutcomeRuntime>,
}

impl BillingResponsesCompactService {
    /// 使用请求级定价快照来源与既有计费端口创建装饰器。
    #[must_use]
    pub fn new(
        inner: Arc<dyn ResponsesCompactRoutePlanner>,
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
        request: CanonicalResponsesCompactionRequest,
        request_id: &str,
    ) -> Result<ResponsesCompactionResponse, AfError> {
        let outcome = crate::RequestOutcomeContext::for_principal(
            request_id,
            af_domain::Protocol::OpenAiResponses,
            af_domain::Operation::ResponsesCompact,
            request.model(),
            *principal,
        );
        outcome
            .finish(
                self.request_outcomes.as_ref(),
                self.call_billed(principal, user_concurrency, request, request_id)
                    .await,
            )
            .await
    }

    async fn call_billed(
        &self,
        principal: &GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalResponsesCompactionRequest,
        request_id: &str,
    ) -> Result<crate::RoutedExecution<ResponsesCompactionResponse>, AfError> {
        let upper_bound = compact_usage_upper_bound(&request)?;
        let model = request.model().to_owned();
        let planned = self
            .inner
            .plan(principal, user_concurrency, request, request_id)
            .await?;
        let snapshot = self
            .pricing_source
            .capture_for_request(
                &model,
                principal.group_id(),
                planned.target_group_id(),
                principal
                    .organization_principal()
                    .map(|organization| organization.organization_id()),
                af_domain::Protocol::OpenAiResponses,
                crate::utc_time::current_utc_day_second().ok_or(AfError::Internal)?,
            )
            .await
            .map_err(map_snapshot_error)?;
        let plan = BillingRequestPlan::prepare(snapshot.resolver(), &upper_bound)
            .map_err(map_lifecycle_error)?
            .with_contract_price(snapshot.contract_price());
        let reservation_id = new_reservation_id()?;
        let mut lifecycle = start_lifecycle(self, plan, *principal, reservation_id).await?;

        let routed = planned.execute().await?;
        let channel_id = routed.channel_id();
        let response = routed.into_value();
        let (body, compact_usage) = response.into_parts();
        let usage = normalize_compact_usage(compact_usage)?;
        if !usage_within_upper_bound(&usage, &upper_bound)? {
            return Err(AfError::Internal);
        }
        let _ = complete_with_retries(&mut lifecycle, usage)
            .await
            .map_err(map_lifecycle_error)?;
        Ok(crate::RoutedExecution::new(
            ResponsesCompactionResponse::new(body, compact_usage),
            channel_id,
        ))
    }
}

impl ResponsesCompactService for BillingResponsesCompactService {
    fn compact<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalResponsesCompactionRequest,
        request_id: &'a str,
    ) -> ResponsesCompactServiceFuture<'a> {
        Box::pin(self.call(principal, user_concurrency, request, request_id))
    }
}

async fn start_lifecycle(
    service: &BillingResponsesCompactService,
    plan: BillingRequestPlan,
    principal: GatewayPrincipal,
    reservation_id: af_domain::BillingReservationId,
) -> Result<BillingRequestLifecycle, AfError> {
    let contract_price = plan.contract_price();
    match plan.precharge_quota() {
        None => plan
            .start_free(principal, reservation_id, service.usage.clone())
            .map_err(map_lifecycle_error),
        Some(amount) => {
            let precharge = service.precharge.as_ref().ok_or(AfError::Internal)?;
            let refund = service.refund.as_ref().ok_or(AfError::Internal)?;
            let settlement = service.settlement.as_ref().ok_or(AfError::Internal)?;
            precharge_with_retries(precharge, reservation_id, principal, amount, contract_price)
                .await?;
            match plan.start_reserved(
                principal,
                reservation_id,
                refund.clone(),
                settlement.clone(),
                service.usage.clone(),
            ) {
                Ok(lifecycle) => Ok(lifecycle),
                Err(error) => {
                    let _ = refund.try_signal_refund(reservation_id);
                    Err(map_lifecycle_error(error))
                }
            }
        }
    }
}

fn compact_usage_upper_bound(
    request: &CanonicalResponsesCompactionRequest,
) -> Result<Usage, AfError> {
    // 远程响应引用隐藏了完整上下文，无法在预扣前得到可靠输入上界。
    if request.previous_response_id().is_some() {
        return Err(AfError::InvalidRequest);
    }
    let value = openai_responses_compact::build_request(request).map_err(|_| AfError::Internal)?;
    let encoded = serde_json::to_vec(&value).map_err(|_| AfError::Internal)?;
    let encoded_tokens = i64::try_from(encoded.len()).map_err(|_| AfError::Internal)?;
    let media_floor = if request
        .input()
        .and_then(|input| input.items())
        .is_some_and(|items| items.iter().any(|item| contains_media(item.as_value())))
    {
        MAX_OUTPUT_TOKENS
    } else {
        0
    };
    let input_tokens = encoded_tokens.max(media_floor);
    Usage::new(
        TokenCount::new(input_tokens).map_err(|_| AfError::Internal)?,
        TokenCount::new(MAX_OUTPUT_TOKENS).map_err(|_| AfError::Internal)?,
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Estimated,
        UsageSemantics::Inclusive,
    )
    .map_err(|_| AfError::Internal)
}

fn contains_media(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(values) => values.iter().any(contains_media),
        serde_json::Value::Object(object) => {
            object
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| matches!(kind, "input_image" | "input_file"))
                || object.values().any(contains_media)
        }
        _ => false,
    }
}

fn normalize_compact_usage(usage: ResponsesCompactionUsage) -> Result<Usage, AfError> {
    // 官方只报告通用 cache_write_tokens，当前五类价格桶无法判断其缓存 TTL。
    if usage.cache_write_tokens() != TokenCount::ZERO {
        return Err(AfError::Internal);
    }
    Usage::new(
        usage.input_tokens(),
        usage.output_tokens(),
        UsageDetails::new(
            usage.cached_tokens(),
            TokenCount::ZERO,
            TokenCount::ZERO,
            usage.reasoning_tokens(),
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .map_err(|_| AfError::Internal)
}

fn usage_within_upper_bound(actual: &Usage, upper_bound: &Usage) -> Result<bool, AfError> {
    let actual_total = actual
        .checked_total_tokens()
        .map_err(|_| AfError::Internal)?;
    let upper_total = upper_bound
        .checked_total_tokens()
        .map_err(|_| AfError::Internal)?;
    Ok(
        actual.input_tokens().get() <= upper_bound.input_tokens().get()
            && actual.output_tokens().get() <= upper_bound.output_tokens().get()
            && actual_total.get() <= upper_total.get(),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use af_adapter::Bytes;
    use af_billing::{
        BillingMode, BillingPrechargeFuture, BillingSettlementFuture, BillingUsageRecord,
        CachedRequestPricingSnapshotSource, GroupPricingCache, GroupPricingSource,
        GroupPricingSourceCatalog, GroupPricingSourceFuture, GroupPricingSourceRecord,
        ModelPriceCache, ModelPriceSource, ModelPriceSourceFuture, ModelPriceSourceRecord,
        PricingRatio, RefundSignalOutcome, SettlementRequest, TokenPrices, UsageRecordOutcome,
    };
    use af_domain::{BillingReservationId, Quota, TokenId, UserId};
    use rust_decimal::Decimal;

    use super::*;

    #[tokio::test]
    async fn one_precharge_covers_execution_and_settles_actual_usage_once() {
        let fixture = billing_fixture(compact_usage(2, 1, 0, 1, 0), true).await;
        let response = fixture
            .service
            .compact(&principal(), None, request(), "compact-request-success")
            .await
            .unwrap();
        let (_, public_usage) = response.into_parts();

        assert_eq!(public_usage.input_tokens().get(), 2);
        assert_eq!(*fixture.plans.lock().unwrap(), 1);
        assert_eq!(*fixture.executions.lock().unwrap(), 1);
        let precharges = fixture.precharge.calls.lock().unwrap();
        let settlements = fixture.settlement.calls.lock().unwrap();
        let records = fixture.usage.records.lock().unwrap();
        assert_eq!(precharges.len(), 1);
        assert_eq!(settlements.len(), 1);
        assert_eq!(records.len(), 1);
        assert_eq!(precharges[0].0, settlements[0].reservation_id());
        assert_eq!(records[0].event_id(), settlements[0].reservation_id());
        assert_eq!(records[0].usage().input_tokens().get(), 2);
        assert_eq!(records[0].usage().output_tokens().get(), 1);
        assert!(precharges[0].1.units() > settlements[0].actual_quota().units());
        assert!(fixture.refund.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cache_write_usage_refunds_without_settlement() {
        let fixture = billing_fixture(compact_usage(2, 0, 1, 1, 0), true).await;
        let error = fixture
            .service
            .compact(&principal(), None, request(), "compact-request-cache-write")
            .await
            .unwrap_err();

        assert_eq!(error, AfError::Internal);
        assert_eq!(*fixture.executions.lock().unwrap(), 1);
        let precharges = fixture.precharge.calls.lock().unwrap();
        let refunds = fixture.refund.calls.lock().unwrap();
        assert_eq!(precharges.len(), 1);
        assert_eq!(refunds.as_slice(), &[precharges[0].0]);
        assert!(fixture.settlement.calls.lock().unwrap().is_empty());
        assert!(fixture.usage.records.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_execution_refunds_the_single_reservation() {
        let fixture = billing_fixture(compact_usage(2, 0, 0, 1, 0), false).await;
        let error = fixture
            .service
            .compact(&principal(), None, request(), "compact-request-failure")
            .await
            .unwrap_err();

        assert_eq!(error, AfError::Internal);
        let precharges = fixture.precharge.calls.lock().unwrap();
        let refunds = fixture.refund.calls.lock().unwrap();
        assert_eq!(precharges.len(), 1);
        assert_eq!(refunds.as_slice(), &[precharges[0].0]);
        assert!(fixture.settlement.calls.lock().unwrap().is_empty());
        assert!(fixture.usage.records.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn previous_response_reference_is_rejected_before_planning() {
        let fixture = billing_fixture(compact_usage(2, 0, 0, 1, 0), true).await;
        let request = openai_responses_compact::parse_request(
            br#"{"model":"compact-test","previous_response_id":"resp_private"}"#,
        )
        .unwrap();
        let error = fixture
            .service
            .compact(
                &principal(),
                None,
                request,
                "compact-request-remote-context",
            )
            .await
            .unwrap_err();

        assert_eq!(error, AfError::InvalidRequest);
        assert_eq!(*fixture.plans.lock().unwrap(), 0);
        assert_eq!(*fixture.executions.lock().unwrap(), 0);
        assert!(fixture.precharge.calls.lock().unwrap().is_empty());
    }

    struct PlannerStub {
        target_group_id: GroupId,
        response_usage: ResponsesCompactionUsage,
        succeed: bool,
        plans: Arc<Mutex<usize>>,
        executions: Arc<Mutex<usize>>,
    }

    impl ResponsesCompactRoutePlanner for PlannerStub {
        fn plan<'a>(
            &'a self,
            _principal: &'a GatewayPrincipal,
            _user_concurrency: Option<ConcurrencyLimit>,
            _request: CanonicalResponsesCompactionRequest,
            _request_id: &'a str,
        ) -> ResponsesCompactRoutePlanFuture<'a> {
            let target_group_id = self.target_group_id;
            let response_usage = self.response_usage;
            let succeed = self.succeed;
            let plans = Arc::clone(&self.plans);
            let executions = Arc::clone(&self.executions);
            Box::pin(async move {
                *plans.lock().unwrap() += 1;
                Ok(Box::new(ExecutionStub {
                    target_group_id,
                    response_usage,
                    succeed,
                    executions,
                })
                    as Box<dyn PlannedResponsesCompactExecution>)
            })
        }
    }

    struct ExecutionStub {
        target_group_id: GroupId,
        response_usage: ResponsesCompactionUsage,
        succeed: bool,
        executions: Arc<Mutex<usize>>,
    }

    impl PlannedResponsesCompactExecution for ExecutionStub {
        fn target_group_id(&self) -> GroupId {
            self.target_group_id
        }

        fn execute(self: Box<Self>) -> PlannedResponsesCompactExecutionFuture {
            Box::pin(async move {
                *self.executions.lock().unwrap() += 1;
                if !self.succeed {
                    return Err(AfError::Internal);
                }
                Ok(crate::RoutedExecution::new(
                    ResponsesCompactionResponse::new(
                        Bytes::from_static(b"{}"),
                        self.response_usage,
                    ),
                    af_domain::ChannelId::new(1).unwrap(),
                ))
            })
        }
    }

    #[derive(Default)]
    struct PrechargeStub {
        calls: Mutex<Vec<(BillingReservationId, Quota)>>,
    }

    impl BillingPrechargePort for PrechargeStub {
        fn precharge<'a>(
            &'a self,
            reservation_id: BillingReservationId,
            _principal: GatewayPrincipal,
            amount: Quota,
        ) -> BillingPrechargeFuture<'a> {
            Box::pin(async move {
                self.calls.lock().unwrap().push((reservation_id, amount));
                Ok(())
            })
        }
    }

    #[derive(Default)]
    struct RefundStub {
        calls: Mutex<Vec<BillingReservationId>>,
    }

    impl RefundSignalPort for RefundStub {
        fn try_signal_refund(&self, reservation_id: BillingReservationId) -> RefundSignalOutcome {
            self.calls.lock().unwrap().push(reservation_id);
            RefundSignalOutcome::Accepted
        }
    }

    #[derive(Default)]
    struct SettlementStub {
        calls: Mutex<Vec<SettlementRequest>>,
    }

    impl BillingSettlementPort for SettlementStub {
        fn settle<'a>(&'a self, request: SettlementRequest) -> BillingSettlementFuture<'a> {
            Box::pin(async move {
                self.calls.lock().unwrap().push(request);
                Ok(())
            })
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

    struct BillingFixture {
        service: BillingResponsesCompactService,
        precharge: Arc<PrechargeStub>,
        refund: Arc<RefundStub>,
        settlement: Arc<SettlementStub>,
        usage: Arc<UsageStub>,
        plans: Arc<Mutex<usize>>,
        executions: Arc<Mutex<usize>>,
    }

    async fn billing_fixture(
        response_usage: ResponsesCompactionUsage,
        succeed: bool,
    ) -> BillingFixture {
        let precharge = Arc::new(PrechargeStub::default());
        let refund = Arc::new(RefundStub::default());
        let settlement = Arc::new(SettlementStub::default());
        let usage = Arc::new(UsageStub::default());
        let plans = Arc::new(Mutex::new(0));
        let executions = Arc::new(Mutex::new(0));
        let service = BillingResponsesCompactService::new(
            Arc::new(PlannerStub {
                target_group_id: GroupId::new(3).unwrap(),
                response_usage,
                succeed,
                plans: Arc::clone(&plans),
                executions: Arc::clone(&executions),
            }),
            Arc::new(metered_snapshot_source().await),
            Some(precharge.clone()),
            Some(refund.clone()),
            Some(settlement.clone()),
            usage.clone(),
        );
        BillingFixture {
            service,
            precharge,
            refund,
            settlement,
            usage,
            plans,
            executions,
        }
    }

    fn principal() -> GatewayPrincipal {
        GatewayPrincipal::new(
            TokenId::new(1).unwrap(),
            UserId::new(2).unwrap(),
            GroupId::new(3).unwrap(),
        )
    }

    fn request() -> CanonicalResponsesCompactionRequest {
        openai_responses_compact::parse_request(
            br#"{"model":"compact-test","input":"compact this context"}"#,
        )
        .unwrap()
    }

    fn compact_usage(
        input_tokens: i64,
        cached_tokens: i64,
        cache_write_tokens: i64,
        output_tokens: i64,
        reasoning_tokens: i64,
    ) -> ResponsesCompactionUsage {
        ResponsesCompactionUsage::new(
            input_tokens,
            cached_tokens,
            cache_write_tokens,
            output_tokens,
            reasoning_tokens,
            input_tokens + output_tokens,
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
                    "compact-test".to_owned(),
                    BillingMode::PerToken,
                    prices,
                    1,
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
}
