use std::{future::Future, pin::Pin, sync::Arc};

use af_billing::{
    BillingPrechargePort, BillingRequestLifecycle, BillingRequestPlan, BillingSettlementPort,
    RefundSignalPort, RequestPricingSnapshotSource, UsageRecordPort,
};
use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal, GroupId};
use af_http::{RerankService, RerankServiceFuture};
use af_protocol::{
    CanonicalRerankRequest, RerankUsage, TokenCount, Usage, UsageDetails, UsageSemantics,
    UsageSource,
};
use af_relay::RerankResponse;

use crate::billing_chat::{
    complete_with_retries, map_lifecycle_error, map_snapshot_error, new_reservation_id,
    precharge_with_retries,
};

/// 已固定路由计划的一次 Rerank 异步执行结果。
pub type PlannedRerankExecutionFuture = Pin<
    Box<
        dyn Future<Output = Result<crate::RoutedExecution<RerankResponse>, AfError>>
            + Send
            + 'static,
    >,
>;

/// 在计费前完成候选与目标分组装配的 Rerank 请求级计划。
pub trait PlannedRerankExecution: Send {
    /// 返回本计划全部候选共同的实际计费分组。
    fn target_group_id(&self) -> GroupId;

    /// 在预扣成功后消费计划并执行全部候选故障转移。
    fn execute(self: Box<Self>) -> PlannedRerankExecutionFuture;
}

/// 为已认证 Rerank 请求生成不可变路由计划的对象安全端口。
pub trait RerankRoutePlanner: Send + Sync {
    /// 固定候选并完成安全装配；本方法不得发送上游请求。
    fn plan<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRerankRequest,
        request_id: &'a str,
    ) -> RerankRoutePlanFuture<'a>;
}

/// Rerank 路由计划生成的异步返回类型。
pub type RerankRoutePlanFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn PlannedRerankExecution>, AfError>> + Send + 'a>>;

/// 已装配计费依赖的 Rerank 服务装饰器。
pub struct BillingRerankService {
    inner: Arc<dyn RerankRoutePlanner>,
    pricing_source: Arc<dyn RequestPricingSnapshotSource>,
    precharge: Option<Arc<dyn BillingPrechargePort>>,
    refund: Option<Arc<dyn RefundSignalPort>>,
    settlement: Option<Arc<dyn BillingSettlementPort>>,
    usage: Arc<dyn UsageRecordPort>,
    request_outcomes: Option<crate::RequestOutcomeRuntime>,
}

impl BillingRerankService {
    /// 使用请求级定价快照来源与既有计费端口创建装饰器。
    #[must_use]
    pub fn new(
        inner: Arc<dyn RerankRoutePlanner>,
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
        request: CanonicalRerankRequest,
        request_id: &str,
    ) -> Result<RerankResponse, AfError> {
        let outcome = crate::RequestOutcomeContext::for_principal(
            request_id,
            af_domain::Protocol::JinaRerank,
            af_domain::Operation::Rerank,
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
        request: CanonicalRerankRequest,
        request_id: &str,
    ) -> Result<crate::RoutedExecution<RerankResponse>, AfError> {
        let upper_bound = rerank_usage_upper_bound(&request)?;
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
                af_domain::Protocol::JinaRerank,
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
        let settlement_usage = rerank_settlement_usage(usage, upper_bound);
        let _ = complete_with_retries(&mut lifecycle, settlement_usage)
            .await
            .map_err(map_lifecycle_error)?;
        Ok(crate::RoutedExecution::new(
            RerankResponse::new(body, usage),
            channel_id,
        ))
    }
}

impl RerankService for BillingRerankService {
    fn rerank<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRerankRequest,
        request_id: &'a str,
    ) -> RerankServiceFuture<'a> {
        Box::pin(self.call(principal, user_concurrency, request, request_id))
    }
}

async fn start_lifecycle(
    service: &BillingRerankService,
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

fn rerank_usage_upper_bound(request: &CanonicalRerankRequest) -> Result<Usage, AfError> {
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

/// 上游缺少真实 token usage 时保留公开空值，但结算使用已冻结的 UTF-8 字节上界。
fn rerank_settlement_usage(usage: Option<RerankUsage>, upper_bound: Usage) -> Usage {
    usage
        .and_then(RerankUsage::token_usage)
        .unwrap_or(upper_bound)
}

#[cfg(test)]
mod tests {
    use af_protocol::{RerankDocument, RerankSearchUnits, rerank_v1};

    use super::*;

    #[test]
    fn upper_bound_uses_query_and_document_utf8_bytes_with_zero_output() {
        let request = rerank_v1::parse_request(
            br#"{"model":"rerank-test","query":"a","documents":["\u4e2d\u6587"]}"#,
        )
        .unwrap();
        let usage = rerank_usage_upper_bound(&request).unwrap();

        assert_eq!(usage.input_tokens().get(), 7);
        assert_eq!(usage.output_tokens(), TokenCount::ZERO);
    }

    #[test]
    fn missing_or_search_unit_only_usage_keeps_conservative_settlement() {
        let request = CanonicalRerankRequest::new(
            "rerank-test".to_owned(),
            "query".to_owned(),
            vec![RerankDocument::Text("document".to_owned())],
            None,
            false,
        )
        .unwrap();
        let upper_bound = rerank_usage_upper_bound(&request).unwrap();
        let search_units =
            RerankUsage::new(None, Some(RerankSearchUnits::new(1).unwrap())).unwrap();

        assert_eq!(rerank_settlement_usage(None, upper_bound), upper_bound);
        assert_eq!(
            rerank_settlement_usage(Some(search_units), upper_bound),
            upper_bound
        );
    }
}
