use std::{future::Future, pin::Pin, sync::Arc};

use af_billing::{
    BillingPrechargePort, BillingRequestLifecycle, BillingRequestPlan, BillingSettlementPort,
    RefundSignalPort, RequestPricingSnapshotSource, UsageRecordPort,
};
use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal, GroupId};
use af_http::{ImageService, ImageServiceFuture};
use af_protocol::{
    CanonicalImageGenerationRequest, ImageQuality, ImageSize, MAX_IMAGE_PIXELS, TokenCount, Usage,
    UsageDetails, UsageSemantics, UsageSource,
};
use af_relay::ImageResponse;

use crate::billing_chat::{
    complete_with_retries, map_lifecycle_error, map_snapshot_error, new_reservation_id,
    precharge_with_retries,
};

const HIGH_QUALITY_PIXELS_PER_TOKEN: u64 = 128;
const MEDIUM_QUALITY_PIXELS_PER_TOKEN: u64 = 512;
const LOW_QUALITY_PIXELS_PER_TOKEN: u64 = 2_048;

/// 已固定路由计划的一次 Images 异步执行结果。
pub type PlannedImageExecutionFuture = Pin<
    Box<
        dyn Future<Output = Result<crate::RoutedExecution<ImageResponse>, AfError>>
            + Send
            + 'static,
    >,
>;

/// 在计费前完成候选与目标分组装配的 Images 请求级计划。
pub trait PlannedImageExecution: Send {
    /// 返回本计划全部候选共同的实际计费分组。
    fn target_group_id(&self) -> GroupId;

    /// 在预扣成功后消费计划并执行全部候选故障转移。
    fn execute(self: Box<Self>) -> PlannedImageExecutionFuture;
}

/// 为已认证 Images 请求生成不可变路由计划的对象安全端口。
pub trait ImageRoutePlanner: Send + Sync {
    /// 固定候选并完成安全装配；本方法不得发送上游请求。
    fn plan<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalImageGenerationRequest,
        request_id: &'a str,
    ) -> ImageRoutePlanFuture<'a>;
}

/// Images 路由计划生成的异步返回类型。
pub type ImageRoutePlanFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn PlannedImageExecution>, AfError>> + Send + 'a>>;

/// 已装配计费依赖的 Images 服务装饰器。
pub struct BillingImageService {
    inner: Arc<dyn ImageRoutePlanner>,
    pricing_source: Arc<dyn RequestPricingSnapshotSource>,
    precharge: Option<Arc<dyn BillingPrechargePort>>,
    refund: Option<Arc<dyn RefundSignalPort>>,
    settlement: Option<Arc<dyn BillingSettlementPort>>,
    usage: Arc<dyn UsageRecordPort>,
    request_outcomes: Option<crate::RequestOutcomeRuntime>,
}

impl BillingImageService {
    /// 使用请求级定价快照来源与既有计费端口创建装饰器。
    #[must_use]
    pub fn new(
        inner: Arc<dyn ImageRoutePlanner>,
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
        request: CanonicalImageGenerationRequest,
        request_id: &str,
    ) -> Result<ImageResponse, AfError> {
        let outcome = crate::RequestOutcomeContext::for_principal(
            request_id,
            af_domain::Protocol::OpenAiImages,
            af_domain::Operation::Image,
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
        request: CanonicalImageGenerationRequest,
        request_id: &str,
    ) -> Result<crate::RoutedExecution<ImageResponse>, AfError> {
        let upper_bound = image_usage_upper_bound(&request)?;
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
                af_domain::Protocol::OpenAiImages,
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
        let (body, upstream_usage) = response.into_parts();
        let settlement_usage = match upstream_usage {
            Some(usage) if usage_within_upper_bound(&usage, &upper_bound)? => usage,
            Some(_) => return Err(AfError::Internal),
            // 上游缺失 usage 时保留公开响应事实，但不能把已经产生的图片成本退款。
            None => upper_bound,
        };
        let _ = complete_with_retries(&mut lifecycle, settlement_usage)
            .await
            .map_err(map_lifecycle_error)?;
        Ok(crate::RoutedExecution::new(
            ImageResponse::new(body, upstream_usage),
            channel_id,
        ))
    }
}

impl ImageService for BillingImageService {
    fn generate<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalImageGenerationRequest,
        request_id: &'a str,
    ) -> ImageServiceFuture<'a> {
        Box::pin(self.call(principal, user_concurrency, request, request_id))
    }
}

async fn start_lifecycle(
    service: &BillingImageService,
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

fn image_usage_upper_bound(request: &CanonicalImageGenerationRequest) -> Result<Usage, AfError> {
    let input_tokens = i64::try_from(request.prompt_bytes()).map_err(|_| AfError::Internal)?;
    let options = request.options();
    let pixels = match options.size() {
        Some(ImageSize::Exact(dimensions)) => u64::from(dimensions.width())
            .checked_mul(u64::from(dimensions.height()))
            .ok_or(AfError::Internal)?,
        Some(ImageSize::Auto) | None => u64::from(MAX_IMAGE_PIXELS),
    };
    let pixels_per_token = match options.quality() {
        Some(ImageQuality::Low) => LOW_QUALITY_PIXELS_PER_TOKEN,
        Some(ImageQuality::Medium) => MEDIUM_QUALITY_PIXELS_PER_TOKEN,
        Some(ImageQuality::High | ImageQuality::Auto) | None => HIGH_QUALITY_PIXELS_PER_TOKEN,
    };
    let output_per_image = pixels.div_ceil(pixels_per_token);
    let output_tokens = output_per_image
        .checked_mul(u64::from(options.effective_count().get()))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(AfError::Internal)?;
    usage(input_tokens, output_tokens, UsageSource::Estimated)
}

fn usage_within_upper_bound(actual: &Usage, upper_bound: &Usage) -> Result<bool, AfError> {
    let actual_input = actual
        .checked_input_tokens()
        .map_err(|_| AfError::Internal)?;
    let upper_input = upper_bound
        .checked_input_tokens()
        .map_err(|_| AfError::Internal)?;
    Ok(actual_input <= upper_input && actual.output_tokens() <= upper_bound.output_tokens())
}

fn usage(input_tokens: i64, output_tokens: i64, source: UsageSource) -> Result<Usage, AfError> {
    Usage::new(
        TokenCount::new(input_tokens).map_err(|_| AfError::Internal)?,
        TokenCount::new(output_tokens).map_err(|_| AfError::Internal)?,
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
    use af_protocol::openai_images;
    use rust_decimal::Decimal;

    use super::*;

    #[test]
    fn upper_bound_is_checked_by_prompt_pixels_quality_and_count() {
        let high = request_with(r#""n":2,"size":"1024x1536","quality":"high","#);
        let low = request_with(r#""n":2,"size":"1024x1536","quality":"low","#);
        let auto = request_with(r#""n":10,"size":"auto","quality":"auto","#);

        assert_eq!(
            image_usage_upper_bound(&high).unwrap().input_tokens().get(),
            2
        );
        assert_eq!(
            image_usage_upper_bound(&high)
                .unwrap()
                .output_tokens()
                .get(),
            24_576
        );
        assert_eq!(
            image_usage_upper_bound(&low).unwrap().output_tokens().get(),
            1_536
        );
        assert_eq!(
            image_usage_upper_bound(&auto)
                .unwrap()
                .output_tokens()
                .get(),
            648_000
        );
    }

    #[tokio::test]
    async fn upstream_usage_settles_actual_once() {
        let fixture =
            billing_fixture(Some(usage(2, 272, UsageSource::Upstream).unwrap()), true).await;
        let response = fixture
            .service
            .generate(&principal(), None, request(), "image-request-success")
            .await
            .unwrap();
        let (_, public_usage) = response.into_parts();

        assert_eq!(public_usage.unwrap().output_tokens().get(), 272);
        assert_eq!(fixture.precharge.calls.lock().unwrap().len(), 1);
        let settlements = fixture.settlement.calls.lock().unwrap();
        assert_eq!(settlements.len(), 1);
        let records = fixture.usage.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].usage().output_tokens().get(), 272);
        assert!(fixture.refund.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_upstream_usage_settles_the_reserved_upper_bound() {
        let fixture = billing_fixture(None, true).await;
        let response = fixture
            .service
            .generate(&principal(), None, request(), "image-request-estimated")
            .await
            .unwrap();
        let (_, public_usage) = response.into_parts();

        assert!(public_usage.is_none());
        let settlements = fixture.settlement.calls.lock().unwrap();
        assert_eq!(settlements.len(), 1);
        let records = fixture.usage.records.lock().unwrap();
        assert_eq!(records[0].usage().source(), UsageSource::Estimated);
        assert_eq!(
            records[0].usage().output_tokens().get(),
            image_usage_upper_bound(&request())
                .unwrap()
                .output_tokens()
                .get()
        );
        assert!(fixture.refund.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn upstream_usage_above_the_reserved_upper_bound_fails_closed() {
        let fixture =
            billing_fixture(Some(usage(2, 513, UsageSource::Upstream).unwrap()), true).await;
        let error = fixture
            .service
            .generate(&principal(), None, request(), "image-request-overflow")
            .await
            .unwrap_err();

        assert_eq!(error, AfError::Internal);
        assert_eq!(fixture.precharge.calls.lock().unwrap().len(), 1);
        assert_eq!(fixture.refund.calls.lock().unwrap().len(), 1);
        assert!(fixture.settlement.calls.lock().unwrap().is_empty());
        assert!(fixture.usage.records.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_execution_refunds_without_settlement() {
        let fixture = billing_fixture(None, false).await;
        let error = fixture
            .service
            .generate(&principal(), None, request(), "image-request-failure")
            .await
            .unwrap_err();

        assert_eq!(error, AfError::Internal);
        assert_eq!(fixture.precharge.calls.lock().unwrap().len(), 1);
        assert_eq!(fixture.refund.calls.lock().unwrap().len(), 1);
        assert!(fixture.settlement.calls.lock().unwrap().is_empty());
        assert!(fixture.usage.records.lock().unwrap().is_empty());
    }

    struct PlannerStub {
        target_group_id: GroupId,
        response_usage: Option<Usage>,
        succeed: bool,
    }

    impl ImageRoutePlanner for PlannerStub {
        fn plan<'a>(
            &'a self,
            _principal: &'a GatewayPrincipal,
            _user_concurrency: Option<ConcurrencyLimit>,
            _request: CanonicalImageGenerationRequest,
            _request_id: &'a str,
        ) -> ImageRoutePlanFuture<'a> {
            let target_group_id = self.target_group_id;
            let response_usage = self.response_usage;
            let succeed = self.succeed;
            Box::pin(async move {
                Ok(Box::new(ExecutionStub {
                    target_group_id,
                    response_usage,
                    succeed,
                }) as Box<dyn PlannedImageExecution>)
            })
        }
    }

    struct ExecutionStub {
        target_group_id: GroupId,
        response_usage: Option<Usage>,
        succeed: bool,
    }

    impl PlannedImageExecution for ExecutionStub {
        fn target_group_id(&self) -> GroupId {
            self.target_group_id
        }

        fn execute(self: Box<Self>) -> PlannedImageExecutionFuture {
            Box::pin(async move {
                if !self.succeed {
                    return Err(AfError::Internal);
                }
                Ok(crate::RoutedExecution::new(
                    ImageResponse::new(Bytes::from_static(b"{}"), self.response_usage),
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
        service: BillingImageService,
        precharge: Arc<PrechargeStub>,
        refund: Arc<RefundStub>,
        settlement: Arc<SettlementStub>,
        usage: Arc<UsageStub>,
    }

    async fn billing_fixture(response_usage: Option<Usage>, succeed: bool) -> BillingFixture {
        let precharge = Arc::new(PrechargeStub::default());
        let refund = Arc::new(RefundStub::default());
        let settlement = Arc::new(SettlementStub::default());
        let usage = Arc::new(UsageStub::default());
        let service = BillingImageService::new(
            Arc::new(PlannerStub {
                target_group_id: GroupId::new(3).unwrap(),
                response_usage,
                succeed,
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
        }
    }

    fn principal() -> GatewayPrincipal {
        GatewayPrincipal::new(
            TokenId::new(1).unwrap(),
            UserId::new(2).unwrap(),
            GroupId::new(3).unwrap(),
        )
    }

    fn request() -> CanonicalImageGenerationRequest {
        request_with(r#""size":"1024x1024","quality":"low","#)
    }

    fn request_with(fields: &str) -> CanonicalImageGenerationRequest {
        let body =
            format!(r#"{{"model":"image-test","prompt":"hi",{fields}"output_format":"png"}}"#);
        openai_images::parse_request(body.as_bytes()).unwrap()
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
                    "image-test".to_owned(),
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
