use std::{future::Future, pin::Pin, sync::Arc};

use af_billing::{
    BillingPrechargePort, BillingRequestLifecycle, BillingRequestPlan, BillingSettlementPort,
    RefundSignalPort, RequestPricingSnapshotSource, UsageRecordPort,
};
use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal, GroupId};
use af_http::{EmbeddingService, EmbeddingServiceFuture};
use af_protocol::{
    CanonicalEmbeddingRequest, TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource,
};
use af_relay::EmbeddingResponse;

use crate::billing_chat::{
    complete_with_retries, map_lifecycle_error, map_snapshot_error, new_reservation_id,
    precharge_with_retries,
};

/// 已固定路由计划的一次 Embeddings 异步执行结果。
pub type PlannedEmbeddingExecutionFuture = Pin<
    Box<
        dyn Future<Output = Result<crate::RoutedExecution<EmbeddingResponse>, AfError>>
            + Send
            + 'static,
    >,
>;

/// 在计费前完成候选与目标分组装配的 Embeddings 请求级计划。
pub trait PlannedEmbeddingExecution: Send {
    /// 返回本计划全部候选共同的实际计费分组。
    fn target_group_id(&self) -> GroupId;

    /// 在预扣成功后消费计划并执行全部候选故障转移。
    fn execute(self: Box<Self>) -> PlannedEmbeddingExecutionFuture;
}

/// 为已认证 Embeddings 请求生成不可变路由计划的对象安全端口。
pub trait EmbeddingRoutePlanner: Send + Sync {
    /// 固定候选并完成安全装配；本方法不得发送上游请求。
    fn plan<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalEmbeddingRequest,
        request_id: &'a str,
    ) -> EmbeddingRoutePlanFuture<'a>;
}

/// Embeddings 路由计划生成的异步返回类型。
pub type EmbeddingRoutePlanFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn PlannedEmbeddingExecution>, AfError>> + Send + 'a>>;

/// 已装配计费依赖的 Embeddings 服务装饰器。
pub struct BillingEmbeddingService {
    inner: Arc<dyn EmbeddingRoutePlanner>,
    pricing_source: Arc<dyn RequestPricingSnapshotSource>,
    precharge: Option<Arc<dyn BillingPrechargePort>>,
    refund: Option<Arc<dyn RefundSignalPort>>,
    settlement: Option<Arc<dyn BillingSettlementPort>>,
    usage: Arc<dyn UsageRecordPort>,
    request_outcomes: Option<crate::RequestOutcomeRuntime>,
}

impl BillingEmbeddingService {
    /// 使用请求级定价快照来源与既有计费端口创建装饰器。
    #[must_use]
    pub fn new(
        inner: Arc<dyn EmbeddingRoutePlanner>,
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
        request: CanonicalEmbeddingRequest,
        request_id: &str,
    ) -> Result<EmbeddingResponse, AfError> {
        let outcome = crate::RequestOutcomeContext::for_principal(
            request_id,
            af_domain::Protocol::OpenAiEmbeddings,
            af_domain::Operation::Embedding,
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
        request: CanonicalEmbeddingRequest,
        request_id: &str,
    ) -> Result<crate::RoutedExecution<EmbeddingResponse>, AfError> {
        let upper_bound = embedding_usage_upper_bound(&request)?;
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
                af_domain::Protocol::OpenAiEmbeddings,
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
        let (body, usage) = response.into_parts();
        let _ = complete_with_retries(&mut lifecycle, usage)
            .await
            .map_err(map_lifecycle_error)?;
        Ok(crate::RoutedExecution::new(
            EmbeddingResponse::new(body, usage),
            channel_id,
        ))
    }
}

impl EmbeddingService for BillingEmbeddingService {
    fn embeddings<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalEmbeddingRequest,
        request_id: &'a str,
    ) -> EmbeddingServiceFuture<'a> {
        Box::pin(self.call(principal, user_concurrency, request, request_id))
    }
}

async fn start_lifecycle(
    service: &BillingEmbeddingService,
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

fn embedding_usage_upper_bound(request: &CanonicalEmbeddingRequest) -> Result<Usage, AfError> {
    let input_tokens = i64::try_from(request.total_text_bytes()).map_err(|_| AfError::Internal)?;
    Usage::new(
        TokenCount::new(input_tokens).map_err(|_| AfError::Internal)?,
        TokenCount::ZERO,
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
    use af_protocol::{EmbeddingInput, openai_embeddings};
    use rust_decimal::Decimal;

    use super::*;

    #[test]
    fn upper_bound_uses_total_utf8_bytes_and_zero_output() {
        let request = openai_embeddings::parse_request(
            br#"{"model":"embedding-test","input":["a","\u4e2d\u6587"]}"#,
        )
        .unwrap();
        let usage = embedding_usage_upper_bound(&request).unwrap();
        assert_eq!(usage.input_tokens().get(), 7);
        assert_eq!(usage.output_tokens(), TokenCount::ZERO);
        assert!(matches!(request.input(), EmbeddingInput::Texts(_)));
    }

    #[tokio::test]
    async fn one_precharge_covers_execution_and_settles_actual_input_once() {
        let fixture = billing_fixture(true).await;
        let response = fixture
            .service
            .embeddings(&principal(), None, request(), "embedding-request-success")
            .await
            .unwrap();
        let (_, usage) = response.into_parts();

        assert_eq!(usage.input_tokens().get(), 2);
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
        assert!(precharges[0].1.units() > settlements[0].actual_quota().units());
        assert!(fixture.refund.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_execution_refunds_the_single_reservation_without_settlement() {
        let fixture = billing_fixture(false).await;
        let error = fixture
            .service
            .embeddings(&principal(), None, request(), "embedding-request-failure")
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

    struct PlannerStub {
        target_group_id: GroupId,
        succeed: bool,
        executions: Arc<Mutex<usize>>,
    }

    impl EmbeddingRoutePlanner for PlannerStub {
        fn plan<'a>(
            &'a self,
            _principal: &'a GatewayPrincipal,
            _user_concurrency: Option<ConcurrencyLimit>,
            _request: CanonicalEmbeddingRequest,
            _request_id: &'a str,
        ) -> EmbeddingRoutePlanFuture<'a> {
            let target_group_id = self.target_group_id;
            let succeed = self.succeed;
            let executions = Arc::clone(&self.executions);
            Box::pin(async move {
                Ok(Box::new(ExecutionStub {
                    target_group_id,
                    succeed,
                    executions,
                }) as Box<dyn PlannedEmbeddingExecution>)
            })
        }
    }

    struct ExecutionStub {
        target_group_id: GroupId,
        succeed: bool,
        executions: Arc<Mutex<usize>>,
    }

    impl PlannedEmbeddingExecution for ExecutionStub {
        fn target_group_id(&self) -> GroupId {
            self.target_group_id
        }

        fn execute(self: Box<Self>) -> PlannedEmbeddingExecutionFuture {
            Box::pin(async move {
                *self.executions.lock().unwrap() += 1;
                if !self.succeed {
                    return Err(AfError::Internal);
                }
                Ok(crate::RoutedExecution::new(
                    EmbeddingResponse::new(Bytes::from_static(b"{}"), embedding_usage(2)),
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
        service: BillingEmbeddingService,
        precharge: Arc<PrechargeStub>,
        refund: Arc<RefundStub>,
        settlement: Arc<SettlementStub>,
        usage: Arc<UsageStub>,
        executions: Arc<Mutex<usize>>,
    }

    async fn billing_fixture(succeed: bool) -> BillingFixture {
        let precharge = Arc::new(PrechargeStub::default());
        let refund = Arc::new(RefundStub::default());
        let settlement = Arc::new(SettlementStub::default());
        let usage = Arc::new(UsageStub::default());
        let executions = Arc::new(Mutex::new(0));
        let service = BillingEmbeddingService::new(
            Arc::new(PlannerStub {
                target_group_id: GroupId::new(3).unwrap(),
                succeed,
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

    fn request() -> CanonicalEmbeddingRequest {
        openai_embeddings::parse_request(
            br#"{"model":"embedding-test","input":["a","\u4e2d\u6587"]}"#,
        )
        .unwrap()
    }

    fn embedding_usage(input_tokens: i64) -> Usage {
        Usage::new(
            TokenCount::new(input_tokens).unwrap(),
            TokenCount::ZERO,
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
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        )
        .unwrap();
        let model_prices = ModelPriceCache::load(Arc::new(StaticModelPriceSource {
            records: vec![
                ModelPriceSourceRecord::new(
                    "embedding-test".to_owned(),
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
