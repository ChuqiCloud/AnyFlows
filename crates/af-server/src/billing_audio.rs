use std::{future::Future, pin::Pin, sync::Arc};

use af_billing::{
    BillingPrechargePort, BillingRequestLifecycle, BillingRequestPlan, BillingSettlementPort,
    BillingUsageDimensions, RefundSignalPort, RequestPricingSnapshotSource, UsageRecordPort,
};
use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal, GroupId};
use af_http::{AudioService, AudioServiceFuture};
use af_protocol::{
    AudioDuration, AudioTranscriptionTokenUsage, AudioTranscriptionUsage,
    CanonicalAudioTranscriptionRequest, TokenCount, Usage, UsageDetails, UsageSemantics,
    UsageSource,
};
use af_relay::AudioTranscriptionResponse;

use crate::{
    audio_duration::probe_audio_duration,
    billing_chat::{
        complete_with_dimensions_retries, map_lifecycle_error, map_snapshot_error,
        new_reservation_id, precharge_with_retries,
    },
};

const AUDIO_TOKENS_PER_MINUTE: u64 = 1_000;
const NANOSECONDS_PER_MINUTE: u64 = 60 * 1_000_000_000;
const PRECHARGE_DURATION_TOLERANCE_NANOSECONDS: u64 = 1_000_000_000;

/// 已固定路由计划的一次 Audio 转录异步执行结果。
pub type PlannedAudioExecutionFuture = Pin<
    Box<
        dyn Future<Output = Result<crate::RoutedExecution<AudioTranscriptionResponse>, AfError>>
            + Send
            + 'static,
    >,
>;

/// 在计费前完成候选与目标分组装配的 Audio 请求级计划。
pub trait PlannedAudioExecution: Send {
    /// 返回本计划全部候选共同的实际计费分组。
    fn target_group_id(&self) -> GroupId;

    /// 在预扣成功后消费计划并执行全部候选故障转移。
    fn execute(self: Box<Self>) -> PlannedAudioExecutionFuture;
}

/// 为已认证 Audio 转录请求生成不可变路由计划的对象安全端口。
pub trait AudioRoutePlanner: Send + Sync {
    /// 固定候选并完成安全装配；本方法不得发送上游请求。
    fn plan<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalAudioTranscriptionRequest,
        request_id: &'a str,
    ) -> AudioRoutePlanFuture<'a>;
}

/// Audio 路由计划生成的异步返回类型。
pub type AudioRoutePlanFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn PlannedAudioExecution>, AfError>> + Send + 'a>>;

/// 已装配时长探测与计费依赖的 Audio 转录服务装饰器。
pub struct BillingAudioService {
    inner: Arc<dyn AudioRoutePlanner>,
    pricing_source: Arc<dyn RequestPricingSnapshotSource>,
    precharge: Option<Arc<dyn BillingPrechargePort>>,
    refund: Option<Arc<dyn RefundSignalPort>>,
    settlement: Option<Arc<dyn BillingSettlementPort>>,
    usage: Arc<dyn UsageRecordPort>,
    request_outcomes: Option<crate::RequestOutcomeRuntime>,
}

impl BillingAudioService {
    /// 使用请求级定价快照来源与既有计费端口创建装饰器。
    #[must_use]
    pub fn new(
        inner: Arc<dyn AudioRoutePlanner>,
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
        request: CanonicalAudioTranscriptionRequest,
        request_id: &str,
    ) -> Result<AudioTranscriptionResponse, AfError> {
        let outcome = crate::RequestOutcomeContext::for_principal(
            request_id,
            af_domain::Protocol::OpenAiAudio,
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
        request: CanonicalAudioTranscriptionRequest,
        request_id: &str,
    ) -> Result<crate::RoutedExecution<AudioTranscriptionResponse>, AfError> {
        let billing_request = request.clone();
        let model = request.model().to_owned();
        let planned = self
            .inner
            .plan(principal, user_concurrency, request, request_id)
            .await?;
        // 路由计划已经持有用户并发槽位，避免大文件时长探测绕过请求级并发限制。
        let local_duration = probe_audio_duration(billing_request.file()).await?;
        let upper_bound = audio_usage_upper_bound(&billing_request, local_duration)?;
        let local_estimate = audio_usage_estimate(&billing_request, local_duration)?;
        let snapshot = self
            .pricing_source
            .capture_for_request(
                &model,
                principal.group_id(),
                planned.target_group_id(),
                principal
                    .organization_principal()
                    .map(|organization| organization.organization_id()),
                af_domain::Protocol::OpenAiAudio,
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
        let normalized_usage = match upstream_usage {
            Some(AudioTranscriptionUsage::Tokens(usage)) => {
                normalize_token_usage(usage).map(|usage| (usage, local_duration))
            }
            Some(AudioTranscriptionUsage::Duration(duration)) => {
                normalize_duration_usage(duration, UsageSource::Upstream)
                    .map(|usage| (usage, duration))
            }
            None => Ok((local_estimate, local_duration)),
        };
        let (settlement_usage, audit_duration) = match normalized_usage {
            Ok(usage) => usage,
            Err(error) => {
                // 上游已成功后不得因异常 usage 退款；先按冻结上界结算并保留本地真实时长。
                let dimensions = BillingUsageDimensions::with_audio_duration(local_duration);
                let _ = complete_with_dimensions_retries(&mut lifecycle, upper_bound, dimensions)
                    .await
                    .map_err(map_lifecycle_error)?;
                return Err(error);
            }
        };
        let dimensions = BillingUsageDimensions::with_audio_duration(audit_duration);
        if !usage_within_upper_bound(&settlement_usage, &upper_bound)? {
            // 上游越过已冻结上界时仍结算保守上界，避免退款路径把已产生的成本退回。
            let _ = complete_with_dimensions_retries(&mut lifecycle, upper_bound, dimensions)
                .await
                .map_err(map_lifecycle_error)?;
            return Err(AfError::Internal);
        }
        let _ = complete_with_dimensions_retries(&mut lifecycle, settlement_usage, dimensions)
            .await
            .map_err(map_lifecycle_error)?;
        Ok(crate::RoutedExecution::new(
            AudioTranscriptionResponse::new(body, upstream_usage),
            channel_id,
        ))
    }
}

impl AudioService for BillingAudioService {
    fn transcribe<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalAudioTranscriptionRequest,
        request_id: &'a str,
    ) -> AudioServiceFuture<'a> {
        Box::pin(self.call(principal, user_concurrency, request, request_id))
    }
}

async fn start_lifecycle(
    service: &BillingAudioService,
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

fn audio_usage_upper_bound(
    request: &CanonicalAudioTranscriptionRequest,
    duration: AudioDuration,
) -> Result<Usage, AfError> {
    let audio_tokens = duration_tokens(duration, PRECHARGE_DURATION_TOLERANCE_NANOSECONDS)?;
    normalized_usage(
        request_text_upper_bound(request)?,
        audio_tokens,
        audio_tokens,
        UsageSource::Estimated,
    )
}

fn audio_usage_estimate(
    request: &CanonicalAudioTranscriptionRequest,
    duration: AudioDuration,
) -> Result<Usage, AfError> {
    let audio_tokens = duration_tokens(duration, 0)?;
    normalized_usage(
        request_text_upper_bound(request)?,
        audio_tokens,
        audio_tokens,
        UsageSource::Estimated,
    )
}

fn normalize_token_usage(usage: AudioTranscriptionTokenUsage) -> Result<Usage, AfError> {
    let audio_input = usage
        .input_details()
        .and_then(|details| details.audio_tokens())
        .unwrap_or(TokenCount::ZERO);
    Usage::new(
        usage.input_tokens(),
        usage.output_tokens(),
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            audio_input,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .map_err(|_| AfError::Internal)
}

fn normalize_duration_usage(
    duration: AudioDuration,
    source: UsageSource,
) -> Result<Usage, AfError> {
    if duration.as_nanoseconds() == 0 {
        return Err(AfError::Internal);
    }
    let audio_tokens = duration_tokens(duration, 0)?;
    normalized_usage(0, audio_tokens, TokenCount::ZERO, source)
}

fn normalized_usage(
    text_input_tokens: i64,
    audio_input_tokens: TokenCount,
    output_tokens: TokenCount,
    source: UsageSource,
) -> Result<Usage, AfError> {
    let input_tokens = audio_input_tokens
        .get()
        .checked_add(text_input_tokens)
        .and_then(|value| TokenCount::new(value).ok())
        .ok_or(AfError::Internal)?;
    Usage::new(
        input_tokens,
        output_tokens,
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            audio_input_tokens,
            TokenCount::ZERO,
        ),
        source,
        UsageSemantics::Inclusive,
    )
    .map_err(|_| AfError::Internal)
}

fn duration_tokens(
    duration: AudioDuration,
    tolerance_nanoseconds: u64,
) -> Result<TokenCount, AfError> {
    let nanoseconds = duration
        .as_nanoseconds()
        .checked_add(tolerance_nanoseconds)
        .ok_or(AfError::Internal)?;
    let tokens = nanoseconds
        .checked_mul(AUDIO_TOKENS_PER_MINUTE)
        .map(|value| value.div_ceil(NANOSECONDS_PER_MINUTE))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(AfError::Internal)?;
    TokenCount::new(tokens).map_err(|_| AfError::Internal)
}

fn request_text_upper_bound(request: &CanonicalAudioTranscriptionRequest) -> Result<i64, AfError> {
    let options = request.options();
    let mut bytes = options.prompt().map_or(0_usize, str::len);
    if let Some(hints) = options.language_hints() {
        if let Some(language) = hints.as_single() {
            bytes = bytes
                .checked_add(language.as_str().len())
                .ok_or(AfError::Internal)?;
        } else if let Some(languages) = hints.as_multiple() {
            for language in languages {
                bytes = bytes
                    .checked_add(language.as_str().len())
                    .ok_or(AfError::Internal)?;
            }
        }
    }
    for keyword in options.keywords() {
        bytes = bytes
            .checked_add(keyword.as_str().len())
            .ok_or(AfError::Internal)?;
    }
    i64::try_from(bytes).map_err(|_| AfError::Internal)
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
    use af_adapter::Bytes;
    use af_protocol::{AudioInputTokenDetails, openai_audio};

    use super::*;

    #[test]
    fn one_minute_precharge_uses_one_second_tolerance_and_text_bytes() {
        let request = request_with_prompt("中文");
        let duration = AudioDuration::from_nanoseconds(60_000_000_000).unwrap();
        let usage = audio_usage_upper_bound(&request, duration).unwrap();

        assert_eq!(usage.details().audio_input().get(), 1_017);
        assert_eq!(usage.input_tokens().get(), 1_023);
        assert_eq!(usage.output_tokens().get(), 1_017);
    }

    #[test]
    fn token_and_duration_usage_keep_distinct_facts() {
        let token_usage = AudioTranscriptionTokenUsage::new(
            TokenCount::new(1_200).unwrap(),
            TokenCount::new(240).unwrap(),
            Some(
                AudioInputTokenDetails::new(
                    Some(TokenCount::new(1_000).unwrap()),
                    Some(TokenCount::new(200).unwrap()),
                )
                .unwrap(),
            ),
        )
        .unwrap();
        let normalized = normalize_token_usage(token_usage).unwrap();
        assert_eq!(normalized.input_tokens().get(), 1_200);
        assert_eq!(normalized.output_tokens().get(), 240);
        assert_eq!(normalized.details().audio_input().get(), 1_000);

        let duration = AudioDuration::from_nanoseconds(30_000_000_000).unwrap();
        let normalized = normalize_duration_usage(duration, UsageSource::Upstream).unwrap();
        assert_eq!(normalized.input_tokens().get(), 500);
        assert_eq!(normalized.output_tokens(), TokenCount::ZERO);
        assert_eq!(normalized.details().audio_input().get(), 500);
    }

    fn request_with_prompt(prompt: &str) -> CanonicalAudioTranscriptionRequest {
        let form = openai_audio::OpenAiTranscriptionForm::new(vec![
            openai_audio::OpenAiTranscriptionFormPart::text(
                "model".to_owned(),
                "gpt-audio-test".to_owned(),
            )
            .unwrap(),
            openai_audio::OpenAiTranscriptionFormPart::text("prompt".to_owned(), prompt.to_owned())
                .unwrap(),
            openai_audio::OpenAiTranscriptionFormPart::file(
                "file".to_owned(),
                "sample.wav".to_owned(),
                Some("audio/wav".to_owned()),
                Bytes::from_static(b"RIFF\x04\x00\x00\x00WAVEdata"),
            )
            .unwrap(),
        ])
        .unwrap();
        openai_audio::parse_transcription_request(form).unwrap()
    }
}
