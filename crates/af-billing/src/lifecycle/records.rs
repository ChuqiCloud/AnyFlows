use std::fmt;

use af_domain::{
    BillingReservationId, GatewayPrincipal, Operation, OrganizationServiceAccountRuntimeIdentity,
    Protocol, Quota,
};
use af_protocol::{
    AudioDuration, ReasoningEffort, TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource,
    VideoDuration, VideoResolution,
};

use crate::BillingMode;

/// 调用日志允许保存的请求标识最大字节数。
pub const MAX_USAGE_REQUEST_ID_BYTES: usize = 128;
/// 调用日志允许保存的客户端模型名最大字节数。
pub const MAX_USAGE_MODEL_BYTES: usize = 255;

/// 固定容量 UTF-8 文本，保持计费事实可复制且不在请求热路径追加堆分配。
#[derive(Clone, Copy, Eq, PartialEq)]
struct FixedUsageText<const N: usize> {
    bytes: [u8; N],
    len: u16,
}

impl<const N: usize> FixedUsageText<N> {
    fn new(value: &str) -> Result<Self, BillingUsageObservationError> {
        if value.is_empty() || value.len() > N || value.len() > usize::from(u16::MAX) {
            return Err(BillingUsageObservationError::InvalidText);
        }
        let mut bytes = [0; N];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Ok(Self {
            bytes,
            len: value.len() as u16,
        })
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("固定调用日志文本必须来自已验证 UTF-8 字符串")
    }
}

/// 一次模型调用中可进入用户核账日志的受控请求上下文。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BillingUsageContext {
    request_id: FixedUsageText<MAX_USAGE_REQUEST_ID_BYTES>,
    model: FixedUsageText<MAX_USAGE_MODEL_BYTES>,
    protocol: Protocol,
    operation: Operation,
    stream: bool,
    reasoning_effort: Option<ReasoningEffort>,
    reasoning_budget_tokens: Option<TokenCount>,
}

impl BillingUsageContext {
    /// 校验并固化不含正文、Header、IP 或上游身份的调用上下文。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: &str,
        model: &str,
        protocol: Protocol,
        operation: Operation,
        stream: bool,
        reasoning_effort: Option<ReasoningEffort>,
        reasoning_budget_tokens: Option<TokenCount>,
    ) -> Result<Self, BillingUsageObservationError> {
        Ok(Self {
            request_id: FixedUsageText::new(request_id)?,
            model: FixedUsageText::new(model)?,
            protocol,
            operation,
            stream,
            reasoning_effort,
            reasoning_budget_tokens,
        })
    }

    /// 返回由网关生成或验证的请求标识。
    #[must_use]
    pub fn request_id(&self) -> &str {
        self.request_id.as_str()
    }

    /// 返回映射前的客户端模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        self.model.as_str()
    }

    /// 返回客户端入口协议。
    #[must_use]
    pub const fn protocol(self) -> Protocol {
        self.protocol
    }

    /// 返回规范化操作类型。
    #[must_use]
    pub const fn operation(self) -> Operation {
        self.operation
    }

    /// 返回客户端是否请求流式响应。
    #[must_use]
    pub const fn stream(self) -> bool {
        self.stream
    }

    /// 返回客户端声明的规范化思考等级。
    #[must_use]
    pub const fn reasoning_effort(self) -> Option<ReasoningEffort> {
        self.reasoning_effort
    }

    /// 返回客户端声明的思考 token 预算。
    #[must_use]
    pub const fn reasoning_budget_tokens(self) -> Option<TokenCount> {
        self.reasoning_budget_tokens
    }
}

impl fmt::Debug for BillingUsageContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingUsageContext")
            .field("protocol", &self.protocol)
            .field("operation", &self.operation)
            .field("stream", &self.stream)
            .field("reasoning_effort", &self.reasoning_effort)
            .field(
                "has_reasoning_budget",
                &self.reasoning_budget_tokens.is_some(),
            )
            .finish_non_exhaustive()
    }
}

/// 成功调用可持久化的延迟观测，单位统一为毫秒。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BillingUsageTiming {
    first_token_ms: Option<i64>,
    duration_ms: i64,
}

impl BillingUsageTiming {
    /// 将单调时钟耗时转换为有界毫秒；首字耗时不得超过总耗时。
    pub fn new(
        first_token: Option<std::time::Duration>,
        duration: std::time::Duration,
    ) -> Result<Self, BillingUsageObservationError> {
        let duration_ms = i64::try_from(duration.as_millis())
            .map_err(|_| BillingUsageObservationError::InvalidTiming)?;
        let first_token_ms = first_token
            .map(|value| i64::try_from(value.as_millis()))
            .transpose()
            .map_err(|_| BillingUsageObservationError::InvalidTiming)?;
        if first_token_ms.is_some_and(|value| value > duration_ms) {
            return Err(BillingUsageObservationError::InvalidTiming);
        }
        Ok(Self {
            first_token_ms,
            duration_ms,
        })
    }

    /// 返回流式请求首个可交付事件耗时。
    #[must_use]
    pub const fn first_token_ms(self) -> Option<i64> {
        self.first_token_ms
    }

    /// 返回调用取得成功终态的总耗时。
    #[must_use]
    pub const fn duration_ms(self) -> i64 {
        self.duration_ms
    }
}

/// 调用日志观测值校验错误，不携带模型或请求标识。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BillingUsageObservationError {
    /// 请求标识或模型为空、过长。
    InvalidText,
    /// 延迟无法安全表示或首字耗时晚于总耗时。
    InvalidTiming,
}

/// 与最终计费事实一起持久化的可选调用观测。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BillingUsageObservation {
    context: Option<BillingUsageContext>,
    timing: Option<BillingUsageTiming>,
}

impl BillingUsageObservation {
    /// 返回不附带模型调用上下文的空观测。
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            context: None,
            timing: None,
        }
    }

    /// 构造一条完整的成功调用观测。
    #[must_use]
    pub const fn new(context: BillingUsageContext, timing: BillingUsageTiming) -> Self {
        Self {
            context: Some(context),
            timing: Some(timing),
        }
    }

    /// 返回可选的请求上下文；旧记录和非模型调用可能为空。
    #[must_use]
    pub const fn context(self) -> Option<BillingUsageContext> {
        self.context
    }

    /// 返回可选的调用耗时；旧记录和异步任务可能为空。
    #[must_use]
    pub const fn timing(self) -> Option<BillingUsageTiming> {
        self.timing
    }
}

/// 请求完成后随 token usage 一并持久化的可选多模态事实。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BillingUsageDimensions {
    audio_duration: Option<AudioDuration>,
    video_duration: Option<VideoDuration>,
    video_resolution: Option<VideoResolution>,
}

impl BillingUsageDimensions {
    /// 返回不携带多模态事实的默认维度集合。
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            audio_duration: None,
            video_duration: None,
            video_resolution: None,
        }
    }

    /// 构造只在存在真实或本地探测事实时携带音频时长的维度集合。
    #[must_use]
    pub const fn with_audio_duration(audio_duration: AudioDuration) -> Self {
        Self {
            audio_duration: Some(audio_duration),
            video_duration: None,
            video_resolution: None,
        }
    }

    /// 构造来自视频成功终态的真实时长和请求明确携带的分辨率。
    ///
    /// 分辨率缺失表示请求交由上游选择默认值，调用方不得在审计层猜测具体档位。
    #[must_use]
    pub const fn with_video(
        video_duration: VideoDuration,
        video_resolution: Option<VideoResolution>,
    ) -> Self {
        Self {
            audio_duration: None,
            video_duration: Some(video_duration),
            video_resolution,
        }
    }

    /// 返回可选的音频时长；缺失时不得由存储层伪造零值。
    #[must_use]
    pub const fn audio_duration(self) -> Option<AudioDuration> {
        self.audio_duration
    }

    /// 返回可选的视频真实时长；只有成功终态返回可信输出时才存在。
    #[must_use]
    pub const fn video_duration(self) -> Option<VideoDuration> {
        self.video_duration
    }

    /// 返回请求明确携带的视频分辨率；缺失时保持 `None`。
    #[must_use]
    pub const fn video_resolution(self) -> Option<VideoResolution> {
        self.video_resolution
    }
}

/// 一次请求完成后交给用量记录边界的最小计费事实。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BillingUsageRecord {
    event_id: BillingReservationId,
    principal: GatewayPrincipal,
    service_account_identity: Option<OrganizationServiceAccountRuntimeIdentity>,
    usage: Usage,
    dimensions: BillingUsageDimensions,
    observation: BillingUsageObservation,
    billing_mode: BillingMode,
    quota: Quota,
}

impl BillingUsageRecord {
    #[cfg(test)]
    pub(crate) const fn new(
        event_id: BillingReservationId,
        principal: GatewayPrincipal,
        usage: Usage,
        billing_mode: BillingMode,
        quota: Quota,
    ) -> Self {
        Self::new_with_dimensions(
            event_id,
            principal,
            usage,
            BillingUsageDimensions::empty(),
            billing_mode,
            quota,
        )
    }

    pub(crate) const fn new_with_dimensions(
        event_id: BillingReservationId,
        principal: GatewayPrincipal,
        usage: Usage,
        dimensions: BillingUsageDimensions,
        billing_mode: BillingMode,
        quota: Quota,
    ) -> Self {
        Self {
            event_id,
            principal,
            service_account_identity: None,
            usage,
            dimensions,
            observation: BillingUsageObservation::empty(),
            billing_mode,
            quota,
        }
    }

    /// 为已经完成持久化结算的按次任务构造零 token 用量记录。
    ///
    /// 直接美元计费的任务不能伪装成 token 用量；真实视频时长和请求分辨率由
    /// `dimensions` 独立承载，事件标识必须与冻结和结算使用同一预留标识。
    #[must_use]
    pub fn for_per_call(
        event_id: BillingReservationId,
        principal: GatewayPrincipal,
        dimensions: BillingUsageDimensions,
        quota: Quota,
    ) -> Self {
        let usage = Usage::new(
            TokenCount::ZERO,
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
        .expect("全零按次用量必须始终满足规范化约束");
        Self::new_with_dimensions(
            event_id,
            principal,
            usage,
            dimensions,
            BillingMode::PerCall,
            quota,
        )
    }

    /// 返回用量存储重放必须复用的请求幂等标识。
    #[must_use]
    pub const fn event_id(self) -> BillingReservationId {
        self.event_id
    }

    /// 返回已认证的请求主体。
    #[must_use]
    pub const fn principal(self) -> GatewayPrincipal {
        self.principal
    }

    /// 返回可选的脱敏服务账号运行时身份；个人/API Key 调用保持为空。
    #[must_use]
    pub const fn service_account_identity(
        self,
    ) -> Option<OrganizationServiceAccountRuntimeIdentity> {
        self.service_account_identity
    }

    /// 绑定已完成企业与活动密钥校验的服务账号身份，供用量审计投影使用。
    ///
    /// 身份只包含企业、服务账号公开标识和密钥公开标识，不含密钥明文、摘要或自然人
    /// 身份；调用方应在生命周期完成前设置该值。
    #[must_use]
    pub const fn with_service_account_identity(
        mut self,
        identity: OrganizationServiceAccountRuntimeIdentity,
    ) -> Self {
        self.service_account_identity = Some(identity);
        self
    }

    pub(crate) const fn with_service_account_identity_opt(
        mut self,
        identity: Option<OrganizationServiceAccountRuntimeIdentity>,
    ) -> Self {
        self.service_account_identity = identity;
        self
    }

    /// 返回本次请求的最终规范化用量。
    #[must_use]
    pub const fn usage(self) -> Usage {
        self.usage
    }

    /// 返回随本次请求冻结的可选多模态审计维度。
    #[must_use]
    pub const fn dimensions(self) -> BillingUsageDimensions {
        self.dimensions
    }

    /// 附加与最终用量共同幂等持久化的调用观测。
    #[must_use]
    pub const fn with_observation(mut self, observation: BillingUsageObservation) -> Self {
        self.observation = observation;
        self
    }

    /// 返回可选的调用观测。
    #[must_use]
    pub const fn observation(self) -> BillingUsageObservation {
        self.observation
    }

    /// 返回快照解析得到的最终计费模式。
    #[must_use]
    pub const fn billing_mode(self) -> BillingMode {
        self.billing_mode
    }

    /// 返回最终结算额度；显式免费与零成本按量都可能为零。
    #[must_use]
    pub const fn quota(self) -> Quota {
        self.quota
    }
}

impl fmt::Debug for BillingUsageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingUsageRecord")
            .field("billing_mode", &self.billing_mode)
            .finish_non_exhaustive()
    }
}

/// 已结算并已被用量记录端口接收的请求结果。
#[must_use = "计费完成结果应交给后续响应或审计编排"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BillingCompletion {
    record: BillingUsageRecord,
}

impl BillingCompletion {
    pub(super) const fn new(record: BillingUsageRecord) -> Self {
        Self { record }
    }

    /// 返回本次请求的最终规范化用量。
    #[must_use]
    pub const fn usage(self) -> Usage {
        self.record.usage()
    }

    /// 返回本次请求已成功入队的多模态审计维度。
    #[must_use]
    pub const fn dimensions(self) -> BillingUsageDimensions {
        self.record.dimensions()
    }

    /// 返回已固化的脱敏服务账号运行时身份。
    #[must_use]
    pub const fn service_account_identity(
        self,
    ) -> Option<OrganizationServiceAccountRuntimeIdentity> {
        self.record.service_account_identity()
    }

    /// 返回已进入可靠记录队列的调用观测。
    #[must_use]
    pub const fn observation(self) -> BillingUsageObservation {
        self.record.observation()
    }

    /// 返回本次请求的最终计费模式。
    #[must_use]
    pub const fn billing_mode(self) -> BillingMode {
        self.record.billing_mode()
    }

    /// 返回本次请求的最终结算额度。
    #[must_use]
    pub const fn quota(self) -> Quota {
        self.record.quota()
    }

    /// 返回已经成功入队的用量记录。
    #[must_use]
    pub const fn usage_record(self) -> BillingUsageRecord {
        self.record
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_context_and_timing_reject_invalid_observations() {
        assert_eq!(
            BillingUsageContext::new(
                "",
                "gpt-test",
                Protocol::OpenAiChat,
                Operation::Chat,
                false,
                None,
                None,
            ),
            Err(BillingUsageObservationError::InvalidText)
        );
        assert_eq!(
            BillingUsageTiming::new(
                Some(std::time::Duration::from_millis(11)),
                std::time::Duration::from_millis(10),
            ),
            Err(BillingUsageObservationError::InvalidTiming)
        );
    }
}
