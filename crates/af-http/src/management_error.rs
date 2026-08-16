use axum::response::{IntoResponse, Response};
use http::{
    HeaderValue, StatusCode,
    header::{CACHE_CONTROL, RETRY_AFTER},
};
use serde::Serialize;
use utoipa::ToSchema;

/// 管理 API 对外稳定的错误码和英文文案。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagementError {
    /// JSON 正文或字段结构无效。
    InvalidRequest,
    /// 用户名或密码不正确。
    InvalidCredentials,
    /// 用户名密码登录已被运营设置关闭。
    LoginDisabled,
    /// JWT 缺失、无效、过期或对应用户已失效。
    InvalidSession,
    /// 当前会话没有访问该管理端点的角色权限。
    Forbidden,
    /// 系统已经完成首次安装，或并发请求先完成了安装。
    SetupConflict,
    /// 站点设置版本已被其他管理员推进。
    SiteSettingsConflict,
    /// 用户不存在或已经被软删除。
    UserNotFound,
    /// 用户名或邮箱与当前有效用户冲突。
    UserConflict,
    /// 企业不存在、已关闭或当前用户不是活动成员。
    OrganizationNotFound,
    /// 企业邀请不存在或不属于路径企业。
    OrganizationInvitationNotFound,
    /// 企业邀请批次、邮箱或生命周期事实发生冲突。
    OrganizationInvitationConflict,
    /// 企业授权下没有可供新邀请占用的席位。
    OrganizationSeatLimitReached,
    /// 企业邀请已经超过受信过期时间。
    OrganizationInvitationExpired,
    /// 邀请令牌、邮箱或当前状态不允许接受；不细分具体事实。
    OrganizationInvitationRejected,
    /// 钱包事件键已经绑定不同业务事实。
    WalletEventConflict,
    /// 负向调账会使用户余额低于零。
    WalletInsufficientQuota,
    /// 正向调账会超过额度整数上界。
    WalletOverflow,
    /// 调账提交结果未知，调用方必须复用同一事件键确认。
    WalletOutcomeUnknown,
    /// 当前实例尚未配置可用的 Stripe 充值通道。
    TopupUnavailable,
    /// Stripe 确定拒绝服务端构造的 PaymentIntent 请求。
    TopupProviderRejected,
    /// 充值幂等键已经绑定不同订单事实。
    TopupOrderConflict,
    /// 充值订单提交结果未知，调用方必须复用同一幂等键确认。
    TopupOrderOutcomeUnknown,
    /// 兑换码批次不存在。
    RedemptionBatchNotFound,
    /// 兑换码批次版本或不可变事实发生冲突。
    RedemptionBatchConflict,
    /// 兑换码格式无效或不存在。
    RedemptionCodeInvalid,
    /// 兑换码所属批次已经整体停用。
    RedemptionBatchDisabled,
    /// 兑换码所属批次已经过期。
    RedemptionCodeExpired,
    /// 兑换码已经由其他用户消费。
    RedemptionCodeAlreadyUsed,
    /// 兑换操作提交结果未知，调用方可重试同一明文确认。
    RedemptionOutcomeUnknown,
    /// 订阅计划不存在。
    SubscriptionPlanNotFound,
    /// 订阅计划已经停用，不能新增绑定。
    SubscriptionPlanDisabled,
    /// 用户订阅不存在或不属于路径指定用户。
    SubscriptionNotFound,
    /// 当前订阅状态不允许执行请求的生命周期动作。
    SubscriptionTransitionInvalid,
    /// 订阅窗口仍绑定在途计费预留。
    SubscriptionInUse,
    /// 订阅计划版本、业务标识或绑定事实发生冲突。
    SubscriptionConflict,
    /// 订阅写入结果未知，服务端重试后仍无法确认。
    SubscriptionOutcomeUnknown,
    /// 当前实例尚未启用公开注册。
    RegistrationDisabled,
    /// 当前客户端已耗尽公开注册固定窗口尝试次数。
    RegistrationRateLimited { retry_after_seconds: u64 },
    /// 验证码无效或带邮箱身份无法创建；不得细分具体原因。
    RegistrationRejected,
    /// 邀请码不存在、已失效或对应邀请人当前不可用。
    RegistrationInvitationRejected,
    /// 密码重置令牌无效、过期、重放或目标已不可重置。
    PasswordResetRejected,
    /// 当前密码校验失败；不代表当前会话已经失效。
    PasswordChangeRejected,
    /// SMTP 设置尚未达到可投递状态。
    EmailNotConfigured,
    /// SMTP 连接、认证或投递失败，底层响应不会向外透传。
    EmailDeliveryFailed,
    /// 分组不存在或已经被软删除。
    GroupNotFound,
    /// 分组名与当前有效分组冲突。
    GroupConflict,
    /// 分组仍被有效用户或有效令牌引用。
    GroupInUse,
    /// 智能路由不存在或已经被软删除。
    RouteNotFound,
    /// 智能路由名称与当前有效路由冲突。
    RouteConflict,
    /// 智能路由候选引用的渠道或凭据无效。
    RouteInvalidReference,
    /// 凭据专属代理不存在或已经被软删除。
    CredentialProxyNotFound,
    /// 凭据专属代理名称与当前有效目录冲突。
    CredentialProxyConflict,
    /// 凭据专属代理仍被有效凭据引用。
    CredentialProxyReferenced,
    /// 模型商品元数据不存在或已经被软删除。
    ModelNotFound,
    /// Canonical 模型标识与当前有效元数据冲突。
    ModelConflict,
    /// 渠道存在，但当前没有可用于模型发现的有效凭据或网络路径。
    ModelSyncChannelUnavailable,
    /// 当前渠道类型或协议不支持受控模型枚举。
    ModelSyncUnsupportedChannel,
    /// 上游模型枚举超过渠道级截止时间。
    ModelSyncUpstreamTimeout,
    /// 上游以非成功状态拒绝模型枚举请求。
    ModelSyncUpstreamRejected,
    /// 上游模型枚举响应无法通过结构与容量校验。
    ModelSyncInvalidResponse,
    /// 上游返回的模型候选数量超过服务端固定上限。
    ModelSyncCandidateLimitExceeded,
    /// 同步预览不存在或不属于当前管理员。
    ModelSyncPreviewNotFound,
    /// 同步预览已经超过服务端固定有效期。
    ModelSyncPreviewExpired,
    /// 同步预览已经成功应用，不能再次提交。
    ModelSyncPreviewAlreadyApplied,
    /// 同步应用与并发元数据写入发生冲突。
    ModelSyncConflict,
    /// 正式模型价格与并发写入发生乐观版本冲突。
    ModelPriceConflict,
    /// 管理试算提交的计费表达式无法通过版本或沙箱校验。
    ModelPriceExpressionInvalid,
    /// 管理试算提交的用量或倍率不满足结构化边界。
    ModelPriceExpressionPreviewInvalid,
    /// 已校验表达式无法在本次用量和倍率下完成精确执行。
    ModelPriceExpressionEvaluationFailed,
    /// models.dev 公开参考价读取超过硬截止时间。
    ModelPriceSourceTimeout,
    /// models.dev 公开参考价当前不可用。
    ModelPriceSourceUnavailable,
    /// models.dev 响应正文超过固定容量上限。
    ModelPriceSourceResponseTooLarge,
    /// models.dev 响应无法通过结构或金额校验。
    ModelPriceSourceInvalidResponse,
    /// models.dev 或本地匹配候选超过固定容量上限。
    ModelPriceSourceCandidateLimitExceeded,
    /// 令牌不存在或已经被软删除。
    TokenNotFound,
    /// 当前用户的未软删除 API Key 已达到容量上限。
    TokenLimitReached,
    /// 渠道不存在或已经被软删除。
    ChannelNotFound,
    /// 凭据不存在、已经软删除或不属于指定渠道。
    CredentialNotFound,
    /// 调试追踪记录不存在。
    DebugTraceNotFound,
    /// 渠道测活服务未在当前实例注入。
    ProbeUnavailable,
    /// 请求的上游 OAuth provider 未在启动配置中启用。
    OauthProviderNotConfigured,
    /// 目标 OAuth 凭据已经绑定其他 provider。
    OauthCredentialProviderMismatch,
    /// 单实例待完成 OAuth 授权已达到容量上限。
    OauthAuthorizationCapacityExceeded,
    /// OAuth 授权会话不存在或不属于当前管理员。
    OauthAuthorizationNotFound,
    /// OAuth 授权会话已经过期。
    OauthAuthorizationExpired,
    /// 上游 provider 拒绝本次 OAuth 授权。
    OauthAuthorizationDenied,
    /// OAuth token 交换超过硬截止时间。
    OauthUpstreamTimeout,
    /// OAuth token endpoint 拒绝请求。
    OauthUpstreamRejected,
    /// OAuth token endpoint 返回无法接受的响应。
    OauthUpstreamInvalidResponse,
    /// OAuth 依赖或受控网络路径当前不可用。
    OauthUnavailable,
    /// 分享令牌未知、已失效、已撤销或不属于当前用户。
    PlaygroundShareNotFound,
    /// 当前用户的有效分享数量已经达到上限。
    PlaygroundShareLimitReached,
    /// 私有会话标识未知或不属于当前用户。
    PlaygroundConversationNotFound,
    /// 当前用户的私有会话数量已经达到上限。
    PlaygroundConversationLimitReached,
    /// 私有会话 revision 已被其他请求推进。
    PlaygroundConversationConflict,
    /// 管理认证依赖发生内部故障。
    Internal,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ManagementError)]
pub(crate) struct ManagementErrorBody {
    #[schema(
        value_type = crate::openapi::schema::ManagementErrorCodeSchema,
        inline
    )]
    code: &'static str,
    message: &'static str,
}

impl IntoResponse for ManagementError {
    fn into_response(self) -> Response {
        let retry_after_seconds = match self {
            Self::RegistrationRateLimited {
                retry_after_seconds,
            } => Some(retry_after_seconds),
            _ => None,
        };
        let (status, body) = match self {
            Self::InvalidRequest => (
                StatusCode::BAD_REQUEST,
                ManagementErrorBody {
                    code: "invalid_request",
                    message: "Invalid request",
                },
            ),
            Self::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                ManagementErrorBody {
                    code: "invalid_credentials",
                    message: "Invalid credentials",
                },
            ),
            Self::LoginDisabled => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "password_login_disabled",
                    message: "Password login disabled",
                },
            ),
            Self::InvalidSession => (
                StatusCode::UNAUTHORIZED,
                ManagementErrorBody {
                    code: "invalid_session",
                    message: "Invalid session",
                },
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "forbidden",
                    message: "Forbidden",
                },
            ),
            Self::SetupConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "setup_conflict",
                    message: "Setup already completed",
                },
            ),
            Self::SiteSettingsConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "site_settings_conflict",
                    message: "Site settings changed",
                },
            ),
            Self::UserNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "user_not_found",
                    message: "User not found",
                },
            ),
            Self::UserConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "user_conflict",
                    message: "User conflict",
                },
            ),
            Self::OrganizationNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_not_found",
                    message: "Organization not found",
                },
            ),
            Self::OrganizationInvitationNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_invitation_not_found",
                    message: "Organization invitation not found",
                },
            ),
            Self::OrganizationInvitationConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_invitation_conflict",
                    message: "Organization invitation conflict",
                },
            ),
            Self::OrganizationSeatLimitReached => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_seat_limit_reached",
                    message: "Organization seat limit reached",
                },
            ),
            Self::OrganizationInvitationExpired => (
                StatusCode::GONE,
                ManagementErrorBody {
                    code: "organization_invitation_expired",
                    message: "Organization invitation expired",
                },
            ),
            Self::OrganizationInvitationRejected => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_invitation_rejected",
                    message: "Organization invitation rejected",
                },
            ),
            Self::WalletEventConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "wallet_event_conflict",
                    message: "Wallet event conflict",
                },
            ),
            Self::WalletInsufficientQuota => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "wallet_insufficient_quota",
                    message: "Wallet balance insufficient",
                },
            ),
            Self::WalletOverflow => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "wallet_overflow",
                    message: "Wallet balance overflow",
                },
            ),
            Self::WalletOutcomeUnknown => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "wallet_outcome_unknown",
                    message: "Wallet outcome unknown",
                },
            ),
            Self::TopupUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "topup_unavailable",
                    message: "Top-up service unavailable",
                },
            ),
            Self::TopupProviderRejected => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "topup_provider_rejected",
                    message: "Top-up provider rejected request",
                },
            ),
            Self::TopupOrderConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "topup_order_conflict",
                    message: "Top-up order conflict",
                },
            ),
            Self::TopupOrderOutcomeUnknown => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "topup_order_outcome_unknown",
                    message: "Top-up order outcome unknown",
                },
            ),
            Self::RedemptionBatchNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "redemption_batch_not_found",
                    message: "Redemption batch not found",
                },
            ),
            Self::RedemptionBatchConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "redemption_batch_conflict",
                    message: "Redemption batch conflict",
                },
            ),
            Self::RedemptionCodeInvalid => (
                StatusCode::BAD_REQUEST,
                ManagementErrorBody {
                    code: "redemption_code_invalid",
                    message: "Redemption code invalid",
                },
            ),
            Self::RedemptionBatchDisabled => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "redemption_batch_disabled",
                    message: "Redemption batch disabled",
                },
            ),
            Self::RedemptionCodeExpired => (
                StatusCode::GONE,
                ManagementErrorBody {
                    code: "redemption_code_expired",
                    message: "Redemption code expired",
                },
            ),
            Self::RedemptionCodeAlreadyUsed => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "redemption_code_already_used",
                    message: "Redemption code already used",
                },
            ),
            Self::RedemptionOutcomeUnknown => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "redemption_outcome_unknown",
                    message: "Redemption outcome unknown",
                },
            ),
            Self::SubscriptionPlanNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "subscription_plan_not_found",
                    message: "Subscription plan not found",
                },
            ),
            Self::SubscriptionPlanDisabled => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "subscription_plan_disabled",
                    message: "Subscription plan disabled",
                },
            ),
            Self::SubscriptionNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "subscription_not_found",
                    message: "Subscription not found",
                },
            ),
            Self::SubscriptionTransitionInvalid => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "subscription_transition_invalid",
                    message: "Subscription transition invalid",
                },
            ),
            Self::SubscriptionInUse => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "subscription_in_use",
                    message: "Subscription in use",
                },
            ),
            Self::SubscriptionConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "subscription_conflict",
                    message: "Subscription conflict",
                },
            ),
            Self::SubscriptionOutcomeUnknown => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "subscription_outcome_unknown",
                    message: "Subscription outcome unknown",
                },
            ),
            Self::RegistrationDisabled => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "registration_disabled",
                    message: "Registration disabled",
                },
            ),
            Self::RegistrationRateLimited { .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                ManagementErrorBody {
                    code: "registration_rate_limited",
                    message: "Registration rate limited",
                },
            ),
            Self::RegistrationRejected => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "registration_rejected",
                    message: "Registration rejected",
                },
            ),
            Self::RegistrationInvitationRejected => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "invitation_rejected",
                    message: "Invitation code rejected",
                },
            ),
            Self::PasswordResetRejected => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "password_reset_rejected",
                    message: "Password reset link is invalid or expired",
                },
            ),
            Self::PasswordChangeRejected => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "password_change_rejected",
                    message: "Current password is incorrect",
                },
            ),
            Self::EmailNotConfigured => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "email_not_configured",
                    message: "Email delivery is not configured",
                },
            ),
            Self::EmailDeliveryFailed => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "email_delivery_failed",
                    message: "Email delivery failed",
                },
            ),
            Self::GroupNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "group_not_found",
                    message: "Group not found",
                },
            ),
            Self::GroupConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "group_conflict",
                    message: "Group conflict",
                },
            ),
            Self::GroupInUse => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "group_in_use",
                    message: "Group in use",
                },
            ),
            Self::RouteNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "route_not_found",
                    message: "Route not found",
                },
            ),
            Self::RouteConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "route_conflict",
                    message: "Route conflict",
                },
            ),
            Self::RouteInvalidReference => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "route_invalid_reference",
                    message: "Route channel reference is invalid",
                },
            ),
            Self::CredentialProxyNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "credential_proxy_not_found",
                    message: "Credential proxy not found",
                },
            ),
            Self::CredentialProxyConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "credential_proxy_conflict",
                    message: "Credential proxy conflict",
                },
            ),
            Self::CredentialProxyReferenced => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "credential_proxy_referenced",
                    message: "Credential proxy is still referenced",
                },
            ),
            Self::ModelNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "model_not_found",
                    message: "Model not found",
                },
            ),
            Self::ModelConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "model_conflict",
                    message: "Model conflict",
                },
            ),
            Self::ModelSyncChannelUnavailable => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "model_sync_channel_unavailable",
                    message: "Model sync channel unavailable",
                },
            ),
            Self::ModelSyncUnsupportedChannel => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ManagementErrorBody {
                    code: "model_sync_unsupported_channel",
                    message: "Model sync unsupported for channel",
                },
            ),
            Self::ModelSyncUpstreamTimeout => (
                StatusCode::GATEWAY_TIMEOUT,
                ManagementErrorBody {
                    code: "model_sync_upstream_timeout",
                    message: "Model sync upstream timeout",
                },
            ),
            Self::ModelSyncUpstreamRejected => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "model_sync_upstream_rejected",
                    message: "Model sync upstream rejected",
                },
            ),
            Self::ModelSyncInvalidResponse => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "model_sync_invalid_response",
                    message: "Model sync invalid upstream response",
                },
            ),
            Self::ModelSyncCandidateLimitExceeded => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "model_sync_candidate_limit_exceeded",
                    message: "Model sync candidate limit exceeded",
                },
            ),
            Self::ModelSyncPreviewNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "model_sync_preview_not_found",
                    message: "Model sync preview not found",
                },
            ),
            Self::ModelSyncPreviewExpired => (
                StatusCode::GONE,
                ManagementErrorBody {
                    code: "model_sync_preview_expired",
                    message: "Model sync preview expired",
                },
            ),
            Self::ModelSyncPreviewAlreadyApplied => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "model_sync_preview_already_applied",
                    message: "Model sync preview already applied",
                },
            ),
            Self::ModelSyncConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "model_sync_conflict",
                    message: "Model sync conflict",
                },
            ),
            Self::ModelPriceConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "model_price_conflict",
                    message: "Model price conflict",
                },
            ),
            Self::ModelPriceExpressionInvalid => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ManagementErrorBody {
                    code: "model_price_expression_invalid",
                    message: "Model price expression invalid",
                },
            ),
            Self::ModelPriceExpressionPreviewInvalid => (
                StatusCode::BAD_REQUEST,
                ManagementErrorBody {
                    code: "model_price_expression_preview_invalid",
                    message: "Model price expression preview input invalid",
                },
            ),
            Self::ModelPriceExpressionEvaluationFailed => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ManagementErrorBody {
                    code: "model_price_expression_evaluation_failed",
                    message: "Model price expression evaluation failed",
                },
            ),
            Self::ModelPriceSourceTimeout => (
                StatusCode::GATEWAY_TIMEOUT,
                ManagementErrorBody {
                    code: "model_price_source_timeout",
                    message: "Model price source timeout",
                },
            ),
            Self::ModelPriceSourceUnavailable => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "model_price_source_unavailable",
                    message: "Model price source unavailable",
                },
            ),
            Self::ModelPriceSourceResponseTooLarge => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "model_price_source_response_too_large",
                    message: "Model price source response too large",
                },
            ),
            Self::ModelPriceSourceInvalidResponse => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "model_price_source_invalid_response",
                    message: "Model price source invalid response",
                },
            ),
            Self::ModelPriceSourceCandidateLimitExceeded => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "model_price_source_candidate_limit_exceeded",
                    message: "Model price source candidate limit exceeded",
                },
            ),
            Self::TokenNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "token_not_found",
                    message: "Token not found",
                },
            ),
            Self::TokenLimitReached => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "token_limit_reached",
                    message: "Token limit reached",
                },
            ),
            Self::ChannelNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "channel_not_found",
                    message: "Channel not found",
                },
            ),
            Self::CredentialNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "credential_not_found",
                    message: "Credential not found",
                },
            ),
            Self::DebugTraceNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "debug_trace_not_found",
                    message: "Debug trace not found",
                },
            ),
            Self::ProbeUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "probe_unavailable",
                    message: "Channel probe unavailable",
                },
            ),
            Self::OauthProviderNotConfigured => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "oauth_provider_not_configured",
                    message: "OAuth provider not configured",
                },
            ),
            Self::OauthCredentialProviderMismatch => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "oauth_credential_provider_mismatch",
                    message: "OAuth credential provider mismatch",
                },
            ),
            Self::OauthAuthorizationCapacityExceeded => (
                StatusCode::TOO_MANY_REQUESTS,
                ManagementErrorBody {
                    code: "oauth_authorization_capacity_exceeded",
                    message: "OAuth authorization capacity exceeded",
                },
            ),
            Self::OauthAuthorizationNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "oauth_authorization_not_found",
                    message: "OAuth authorization not found",
                },
            ),
            Self::OauthAuthorizationExpired => (
                StatusCode::GONE,
                ManagementErrorBody {
                    code: "oauth_authorization_expired",
                    message: "OAuth authorization expired",
                },
            ),
            Self::OauthAuthorizationDenied => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "oauth_authorization_denied",
                    message: "OAuth authorization denied",
                },
            ),
            Self::OauthUpstreamTimeout => (
                StatusCode::GATEWAY_TIMEOUT,
                ManagementErrorBody {
                    code: "oauth_upstream_timeout",
                    message: "OAuth upstream timeout",
                },
            ),
            Self::OauthUpstreamRejected => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "oauth_upstream_rejected",
                    message: "OAuth upstream rejected",
                },
            ),
            Self::OauthUpstreamInvalidResponse => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "oauth_upstream_invalid_response",
                    message: "OAuth upstream invalid response",
                },
            ),
            Self::OauthUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "oauth_unavailable",
                    message: "OAuth service unavailable",
                },
            ),
            Self::PlaygroundShareNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "playground_share_not_found",
                    message: "Playground share not found",
                },
            ),
            Self::PlaygroundShareLimitReached => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "playground_share_limit_reached",
                    message: "Playground share limit reached",
                },
            ),
            Self::PlaygroundConversationNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "playground_conversation_not_found",
                    message: "Playground conversation not found",
                },
            ),
            Self::PlaygroundConversationLimitReached => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "playground_conversation_limit_reached",
                    message: "Playground conversation limit reached",
                },
            ),
            Self::PlaygroundConversationConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "playground_conversation_conflict",
                    message: "Playground conversation conflict",
                },
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ManagementErrorBody {
                    code: "internal_error",
                    message: "Internal server error",
                },
            ),
        };
        let mut response = (status, axum::Json(body)).into_response();
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if let Some(retry_after_seconds) = retry_after_seconds {
            response.headers_mut().insert(
                RETRY_AFTER,
                HeaderValue::from_str(&retry_after_seconds.to_string())
                    .expect("非负整数秒数必须始终是有效响应头"),
            );
        }
        response
    }
}
