use std::{sync::Arc, time::Duration};

use af_billing::{
    BillingLifecycleError, BillingPrechargeError, BillingPrechargePort, BillingRequestLifecycle,
    BillingRequestPlan, BillingSettlementPort, PricingResolver, RefundSignalPort, UsageRecordPort,
};
use af_domain::{AfError, BillingReservationId, GatewayPrincipal};
use af_http::{ChatService, ChatServiceFuture};
use af_protocol::{CanonicalRequest, Usage};
use af_relay::{
    OpenAiChatCompletionHook, OpenAiChatResponse, UsageResolutionError,
    estimate_openai_chat_request_upper_bound,
};
use tokio::time::sleep;
use uuid::Uuid;

const MAX_PRECHARGE_ATTEMPTS: usize = 3;
const MAX_COMPLETION_ATTEMPTS: usize = 3;
const RETRY_DELAY: Duration = Duration::from_millis(20);

/// 已装配计费依赖的 Chat 服务装饰器。
///
/// 装饰器在调用上游前固定请求上界并完成一次幂等预扣；非流式响应直接完成结算，流式响应
/// 则把同一生命周期交给流结束回调。缺少真实定价或持久化端口时，构造方必须显式拒绝装配，
/// 本类型不会把未知价格当作免费。
pub struct BillingChatService {
    inner: Arc<dyn ChatService>,
    resolver: Arc<dyn PricingResolver>,
    precharge: Option<Arc<dyn BillingPrechargePort>>,
    refund: Option<Arc<dyn RefundSignalPort>>,
    settlement: Option<Arc<dyn BillingSettlementPort>>,
    usage: Arc<dyn UsageRecordPort>,
}

impl BillingChatService {
    /// 使用显式定价解析器与计费端口创建装饰器。
    #[must_use]
    pub fn new(
        inner: Arc<dyn ChatService>,
        resolver: Arc<dyn PricingResolver>,
        precharge: Option<Arc<dyn BillingPrechargePort>>,
        refund: Option<Arc<dyn RefundSignalPort>>,
        settlement: Option<Arc<dyn BillingSettlementPort>>,
        usage: Arc<dyn UsageRecordPort>,
    ) -> Self {
        Self {
            inner,
            resolver,
            precharge,
            refund,
            settlement,
            usage,
        }
    }

    async fn call(
        &self,
        principal: &GatewayPrincipal,
        request: CanonicalRequest,
        request_id: &str,
    ) -> Result<OpenAiChatResponse, AfError> {
        let upper_bound =
            estimate_openai_chat_request_upper_bound(&request).map_err(map_upper_bound_error)?;
        let plan = BillingRequestPlan::prepare(self.resolver.clone(), &upper_bound)
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

        let response = self
            .inner
            .chat_completions(principal, request, request_id)
            .await?;
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
        request: CanonicalRequest,
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
    fn on_complete(mut self: Box<Self>, usage: Result<Usage, UsageResolutionError>) {
        let Some(mut lifecycle) = self.lifecycle.take() else {
            return;
        };
        let Ok(usage) = usage else {
            drop(lifecycle);
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let task = async move {
                    if complete_with_retries(&mut lifecycle, usage).await.is_err() {
                        tracing::error!(
                            error_kind = "billing_stream_completion_failed",
                            "流式响应结束后的计费完成失败"
                        );
                    }
                };
                drop(handle.spawn(task));
            }
            Err(_) => drop(lifecycle),
        }
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

fn map_upper_bound_error(error: UsageResolutionError) -> AfError {
    match error {
        UsageResolutionError::UnsupportedContent => AfError::InvalidRequest,
        UsageResolutionError::Interrupted
        | UsageResolutionError::InvalidSequence
        | UsageResolutionError::Overflow
        | UsageResolutionError::EstimationFailed => AfError::Internal,
    }
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
        BillingPrechargeFuture, BillingPrechargePort, BillingSettlementFuture,
        BillingSettlementPort, BillingUsageRecord, PricingRatio, PricingRatios,
        RatioPricingResolver, RefundSignalOutcome, TokenPrices, UsageRecordOutcome,
    };
    use af_domain::{GroupId, Operation, Quota, Role, TokenId, UserId};
    use af_http::{ChatService, ChatServiceFuture};
    use af_protocol::{CanonicalRequest, ContentBlock, Message, TokenCount, Usage};
    use rust_decimal::Decimal;

    use super::*;

    struct FullStub;

    impl ChatService for FullStub {
        fn chat_completions<'a>(
            &'a self,
            _principal: &'a GatewayPrincipal,
            _request: CanonicalRequest,
            _request_id: &'a str,
        ) -> ChatServiceFuture<'a> {
            Box::pin(async {
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
            Arc::new(FullStub),
            Arc::new(metered_resolver()),
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

    fn request() -> CanonicalRequest {
        CanonicalRequest::new(
            Operation::Chat,
            "gpt-test".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("hello".to_owned())],
            )],
            false,
        )
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

    fn metered_resolver() -> RatioPricingResolver {
        RatioPricingResolver::metered(
            TokenPrices::new(
                Decimal::new(1, 6),
                Decimal::new(1, 6),
                Decimal::ZERO,
                Decimal::ZERO,
                Decimal::ZERO,
            )
            .unwrap(),
            PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE),
        )
    }
}
