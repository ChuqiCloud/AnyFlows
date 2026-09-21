use std::{future::Future, pin::Pin, sync::Arc};

use af_billing::{
    BillingPrechargePort, BillingRequestLifecycle, BillingRequestPlan, BillingSettlementPort,
    BillingUsageDimensions, MAX_BILLING_DURATION_SECONDS, RefundSignalPort,
    RequestPricingSnapshotSource, UsageRecordPort,
};
use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal, GroupId};
use af_http::{SpeechService, SpeechServiceFuture};
use af_protocol::{
    AudioDuration, CanonicalAudioSpeechRequest, TokenCount, Usage, UsageDetails, UsageSemantics,
    UsageSource,
};
use af_relay::{SpeechResponse, estimate_openai_text_tokens};

use crate::{
    audio_duration::probe_speech_duration,
    billing_chat::{
        complete_with_dimensions_retries, map_lifecycle_error, map_snapshot_error,
        new_reservation_id, precharge_with_retries,
    },
};

// 当前 `$0.00025/秒` 与 `$12/百万音频 token` 的价格事实等价于每分钟 1250 token。
const SPEECH_AUDIO_TOKENS_PER_MINUTE: u64 = 1_250;
const NANOSECONDS_PER_MINUTE: u64 = 60 * 1_000_000_000;

/// 已固定路由计划的一次 Speech 异步执行结果。
pub type PlannedSpeechExecutionFuture = Pin<
    Box<
        dyn Future<Output = Result<crate::RoutedExecution<SpeechResponse>, AfError>>
            + Send
            + 'static,
    >,
>;

/// 在计费前完成候选与目标分组装配的 Speech 请求级计划。
pub trait PlannedSpeechExecution: Send {
    /// 返回本计划全部候选共同的实际计费分组。
    fn target_group_id(&self) -> GroupId;

    /// 在预扣成功后消费计划并执行全部候选故障转移。
    fn execute(self: Box<Self>) -> PlannedSpeechExecutionFuture;
}

/// 为已认证 Speech 请求生成不可变路由计划的对象安全端口。
pub trait SpeechRoutePlanner: Send + Sync {
    /// 固定候选并完成安全装配；本方法不得发送上游请求。
    fn plan<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalAudioSpeechRequest,
        request_id: &'a str,
    ) -> SpeechRoutePlanFuture<'a>;
}

/// Speech 路由计划生成的异步返回类型。
pub type SpeechRoutePlanFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn PlannedSpeechExecution>, AfError>> + Send + 'a>>;

/// 已装配真实时长计费依赖的 Speech 服务装饰器。
pub struct BillingSpeechService {
    inner: Arc<dyn SpeechRoutePlanner>,
    pricing_source: Arc<dyn RequestPricingSnapshotSource>,
    precharge: Option<Arc<dyn BillingPrechargePort>>,
    refund: Option<Arc<dyn RefundSignalPort>>,
    settlement: Option<Arc<dyn BillingSettlementPort>>,
    usage: Arc<dyn UsageRecordPort>,
    request_outcomes: Option<crate::RequestOutcomeRuntime>,
}

impl BillingSpeechService {
    /// 使用请求级定价快照来源与既有计费端口创建装饰器。
    #[must_use]
    pub fn new(
        inner: Arc<dyn SpeechRoutePlanner>,
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
        request: CanonicalAudioSpeechRequest,
        request_id: &str,
    ) -> Result<SpeechResponse, AfError> {
        let outcome = crate::RequestOutcomeContext::for_principal(
            request_id,
            af_domain::Protocol::OpenAiSpeech,
            af_domain::Operation::Audio,
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
        request: CanonicalAudioSpeechRequest,
        request_id: &str,
    ) -> Result<crate::RoutedExecution<SpeechResponse>, AfError> {
        let upper_bound = speech_usage_upper_bound(&request)?;
        let model = request.model().to_owned();
        let billing_request = request.clone();
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
                af_domain::Protocol::OpenAiSpeech,
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
        let (body, output_format) = response.into_parts();
        let duration = match probe_speech_duration(&body, output_format).await {
            Ok(duration) => duration,
            Err(error) => {
                // 上游已经成功产生成本但时长不可信，保留冻结上界且不伪造审计时长。
                let _ = complete_with_dimensions_retries(
                    &mut lifecycle,
                    upper_bound,
                    BillingUsageDimensions::empty(),
                )
                .await
                .map_err(map_lifecycle_error)?;
                return Err(error);
            }
        };
        let actual = speech_usage(&billing_request, duration, UsageSource::Estimated)?;
        if !usage_within_upper_bound(&actual, &upper_bound)? {
            let dimensions = BillingUsageDimensions::with_audio_duration(duration);
            let _ = complete_with_dimensions_retries(&mut lifecycle, upper_bound, dimensions)
                .await
                .map_err(map_lifecycle_error)?;
            return Err(AfError::Internal);
        }
        let dimensions = BillingUsageDimensions::with_audio_duration(duration);
        let _ = complete_with_dimensions_retries(&mut lifecycle, actual, dimensions)
            .await
            .map_err(map_lifecycle_error)?;
        Ok(crate::RoutedExecution::new(
            SpeechResponse::new(body, output_format),
            channel_id,
        ))
    }
}

impl SpeechService for BillingSpeechService {
    fn synthesize<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalAudioSpeechRequest,
        request_id: &'a str,
    ) -> SpeechServiceFuture<'a> {
        Box::pin(self.call(principal, user_concurrency, request, request_id))
    }
}

async fn start_lifecycle(
    service: &BillingSpeechService,
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

fn speech_usage_upper_bound(request: &CanonicalAudioSpeechRequest) -> Result<Usage, AfError> {
    let duration = AudioDuration::from_nanoseconds(
        u64::try_from(MAX_BILLING_DURATION_SECONDS)
            .ok()
            .and_then(|seconds| seconds.checked_mul(1_000_000_000))
            .ok_or(AfError::Internal)?,
    )
    .map_err(|_| AfError::Internal)?;
    speech_usage_with_input_tokens(
        duration,
        input_byte_upper_bound(request)?,
        UsageSource::Estimated,
    )
}

fn speech_usage(
    request: &CanonicalAudioSpeechRequest,
    duration: AudioDuration,
    source: UsageSource,
) -> Result<Usage, AfError> {
    let input_tokens = speech_input_units(request)?;
    speech_usage_with_input_tokens(duration, input_tokens.get(), source)
}

fn speech_input_units(request: &CanonicalAudioSpeechRequest) -> Result<TokenCount, AfError> {
    if request.model() == "tts-1" || request.model().starts_with("tts-1-") {
        // 传统 tts-1 家族按字符计价；复用 input_tokens 字段保存该模型的官方计费单位。
        let characters = std::iter::once(request.input())
            .chain(request.options().instructions())
            .try_fold(0_usize, |total, text| {
                total.checked_add(text.chars().count())
            });
        return characters
            .and_then(|value| i64::try_from(value).ok())
            .and_then(|value| TokenCount::new(value).ok())
            .ok_or(AfError::Internal);
    }
    estimate_openai_text_tokens(
        request.model(),
        std::iter::once(request.input()).chain(request.options().instructions()),
    )
    .map_err(|_| AfError::Internal)
}

fn input_byte_upper_bound(request: &CanonicalAudioSpeechRequest) -> Result<i64, AfError> {
    request
        .input_bytes()
        .checked_add(request.options().instructions().map_or(0, str::len))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(AfError::Internal)
}

fn speech_usage_with_input_tokens(
    duration: AudioDuration,
    input_tokens: i64,
    source: UsageSource,
) -> Result<Usage, AfError> {
    let output_tokens = duration
        .as_nanoseconds()
        .checked_mul(SPEECH_AUDIO_TOKENS_PER_MINUTE)
        .map(|value| value.div_ceil(NANOSECONDS_PER_MINUTE))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(AfError::Internal)?;
    let output_tokens = TokenCount::new(output_tokens).map_err(|_| AfError::Internal)?;
    Usage::new(
        TokenCount::new(input_tokens).map_err(|_| AfError::Internal)?,
        output_tokens,
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            output_tokens,
        ),
        source,
        UsageSemantics::Inclusive,
    )
    .map_err(|_| AfError::Internal)
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

#[cfg(test)]
mod tests {
    use af_protocol::{
        AudioSpeechOptions, AudioSpeechOutputFormat, AudioSpeechVoice, AudioSpeechVoiceName,
    };

    use super::*;

    #[test]
    fn one_minute_uses_tokenized_input_and_1250_audio_tokens() {
        let request = CanonicalAudioSpeechRequest::new(
            "gpt-speech-test".to_owned(),
            "hello".to_owned(),
            AudioSpeechOptions::new(
                AudioSpeechVoice::Named(AudioSpeechVoiceName::new("coral".to_owned()).unwrap()),
                Some("calm".to_owned()),
                Some(AudioSpeechOutputFormat::Wav),
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap();
        let usage = speech_usage(
            &request,
            AudioDuration::from_nanoseconds(60_000_000_000).unwrap(),
            UsageSource::Estimated,
        )
        .unwrap();

        assert_eq!(usage.input_tokens().get(), 3);
        assert_eq!(usage.output_tokens().get(), 1_250);
        assert_eq!(usage.details().audio_output().get(), 1_250);
    }

    #[test]
    fn precharge_freezes_the_shared_twenty_four_hour_limit() {
        let request = CanonicalAudioSpeechRequest::new(
            "tts-test".to_owned(),
            "hello".to_owned(),
            AudioSpeechOptions::new(
                AudioSpeechVoice::Named(AudioSpeechVoiceName::new("alloy".to_owned()).unwrap()),
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap();
        let usage = speech_usage_upper_bound(&request).unwrap();
        assert_eq!(usage.input_tokens().get(), 5);
        assert_eq!(usage.output_tokens().get(), 1_800_000);
    }

    #[test]
    fn legacy_tts_family_uses_unicode_character_input_units() {
        let request = CanonicalAudioSpeechRequest::new(
            "tts-1-hd-1106".to_owned(),
            "中文".to_owned(),
            AudioSpeechOptions::new(
                AudioSpeechVoice::Named(AudioSpeechVoiceName::new("alloy".to_owned()).unwrap()),
                Some("calm".to_owned()),
                None,
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(speech_input_units(&request).unwrap().get(), 6);
        assert_eq!(input_byte_upper_bound(&request).unwrap(), 10);
    }
}
