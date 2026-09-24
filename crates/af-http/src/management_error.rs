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
    /// 用户名密码正确，但需要输入 TOTP 或备份码。
    TwoFactorRequired,
    /// TOTP 或备份码校验失败。
    TwoFactorInvalid,
    /// 用户名密码登录已被运营设置关闭。
    LoginDisabled,
    /// Turnstile token 无效、过期或缺失。
    TurnstileRejected,
    /// Turnstile 上游验证不可用，服务端失败关闭认证入口。
    TurnstileUnavailable,
    /// JWT 缺失、无效、过期或对应用户已失效。
    InvalidSession,
    /// 当前会话没有访问该管理端点的角色权限。
    Forbidden,
    /// 系统已经完成首次安装，或并发请求先完成了安装。
    SetupConflict,
    /// 站点设置版本已被其他管理员推进。
    SiteSettingsConflict,
    /// 公告写入请求字段无效。
    AnnouncementInvalidRequest,
    /// 公告不存在或已不可见。
    AnnouncementNotFound,
    /// 公告版本或生命周期状态发生冲突。
    AnnouncementConflict,
    /// 用户不存在或已经被软删除。
    UserNotFound,
    /// 用户名或邮箱与当前有效用户冲突。
    UserConflict,
    /// 企业不存在、已关闭或当前用户不是活动成员。
    OrganizationNotFound,
    /// 企业授信请求字段或状态值无效。
    OrganizationCreditInvalidRequest,
    /// 企业授信策略不存在或不属于路径企业。
    OrganizationCreditNotFound,
    /// 企业授信有效区间与既有策略冲突。
    OrganizationCreditIntervalConflict,
    /// 企业授信版本 CAS 冲突。
    OrganizationCreditConflict,
    /// 企业授信状态不能继续推进。
    OrganizationCreditTransitionInvalid,
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
    /// 企业审批事实不存在或不属于路径企业。
    OrganizationApprovalNotFound,
    /// 企业审批模板、申请或决定发生版本/幂等冲突。
    OrganizationApprovalConflict,
    /// 企业审批申请已经超过服务端过期时间。
    OrganizationApprovalExpired,
    /// 企业开通申请不存在或当前用户不可见。
    OrganizationProvisioningNotFound,
    /// 企业开通申请或审批版本发生冲突。
    OrganizationProvisioningConflict,
    /// 企业开通申请请求字段无效。
    OrganizationProvisioningInvalidRequest,
    /// 企业开通申请平台审批权限不足。
    OrganizationProvisioningForbidden,
    /// 企业开通申请服务暂不可用。
    OrganizationProvisioningUnavailable,
    /// 企业认证案件不存在或当前用户不可见。
    OrganizationVerificationNotFound,
    /// 企业认证案件版本或状态发生冲突。
    OrganizationVerificationConflict,
    /// 企业认证请求字段无效。
    OrganizationVerificationInvalidRequest,
    /// 当前用户没有企业认证操作权限。
    OrganizationVerificationForbidden,
    AccountVerificationSelfReview,
    /// 企业认证服务暂不可用。
    OrganizationVerificationUnavailable,
    /// 企业成员或团队 CAS 版本冲突。
    OrganizationLifecycleConflict,
    /// 企业成员或团队生命周期状态不允许当前转换。
    OrganizationLifecycleTransitionInvalid,
    /// 当前操作会移除企业最后一个 Owner。
    OrganizationLastOwner,
    /// 团队仍有活动成员归属。
    OrganizationTeamInUse,
    /// 企业授权不允许当前生命周期写入。
    OrganizationEntitlementRestricted,
    /// 企业授权投影暂时不可读取。
    OrganizationEntitlementUnavailable,
    /// 企业合同价记录不存在或不属于路径企业。
    OrganizationContractPriceNotFound,
    /// 企业合同价有效区间与同模型协议事实冲突。
    OrganizationContractPriceIntervalConflict,
    /// 企业合同价版本 CAS 冲突。
    OrganizationContractPriceConflict,
    /// 企业合同价已经关闭或不能再次关闭。
    OrganizationContractPriceTransitionInvalid,
    /// 企业套餐不存在或不可见。
    OrganizationPlanNotFound,
    /// 企业套餐版本或不可变价格快照发生冲突。
    OrganizationPlanConflict,
    /// 企业授权订单不存在或不属于路径企业。
    OrganizationPlanOrderNotFound,
    /// 企业授权订单幂等事实或状态冲突。
    OrganizationPlanOrderConflict,
    /// 企业授权订单写入结果未知，客户端必须复用同一幂等键确认。
    OrganizationPlanOrderOutcomeUnknown,
    /// 企业授权订单服务暂不可用。
    OrganizationPlanOrderUnavailable,
    OrganizationSsoProviderNotFound,
    OrganizationSsoProviderConflict,
    OrganizationSsoProviderTransitionInvalid,
    /// 当前会话、ticket 或二次验证不满足显式身份绑定前提。
    OrganizationSsoIdentityBindingRejected,
    /// 显式身份绑定已存在同一 Provider 或成员事实。
    OrganizationSsoIdentityBindingConflict,
    /// Owner break-glass 的当前密码或二次因子未通过校验。
    OrganizationSsoRecoveryRejected,
    /// Owner break-glass 的策略版本已被其他操作推进。
    OrganizationSsoRecoveryConflict,
    /// Owner break-glass 恢复事实暂时不可写入。
    OrganizationSsoRecoveryUnavailable,
    OrganizationSsoDomainNotFound,
    OrganizationSsoDomainConflict,
    OrganizationSsoDomainTransitionInvalid,
    OrganizationSsoDomainVerificationFailed,
    /// 企业团队容量已用尽。
    OrganizationTeamLimitReached,
    /// 钱包事件键已经绑定不同业务事实。
    WalletEventConflict,
    /// 负向调账会使用户余额低于零。
    WalletInsufficientQuota,
    /// 正向调账会超过额度整数上界。
    WalletOverflow,
    /// 调账提交结果未知，调用方必须复用同一事件键确认。
    WalletOutcomeUnknown,
    RefundNotFound,
    RefundConflict,
    RefundUnavailable,
    RefundAutoSubmitFailed,
    RefundOutcomeUnknown,
    /// 预算策略版本或预留状态与请求不一致。
    BudgetConflict,
    /// 预算写入结果未知，调用方必须按原请求重试或查询。
    BudgetOutcomeUnknown,
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
    RegistrationRateLimited {
        retry_after_seconds: u64,
    },
    /// 验证码无效或带邮箱身份无法创建；不得细分具体原因。
    RegistrationRejected,
    /// 邀请码不存在、已失效或对应邀请人当前不可用。
    RegistrationInvitationRejected,
    /// 密码重置令牌无效、过期、重放或目标已不可重置。
    PasswordResetRejected,
    /// 当前密码校验失败；不代表当前会话已经失效。
    PasswordChangeRejected,
    /// 当前用户已经启用 TOTP。
    TwoFactorAlreadyEnabled,
    /// 当前用户尚未启用 TOTP。
    TwoFactorNotEnabled,
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
    /// 企业 API Key 写操作提交结果未知，调用方必须查询确认。
    TokenOutcomeUnknown,
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
    /// 用户登录 OAuth Provider 设置已被其他管理员更新。
    OauthLoginSettingsConflict,
    /// 自定义 OAuth2 Provider 不存在。
    CustomOAuth2ProviderNotFound,
    /// 自定义 OAuth2 Provider 版本已被其他管理员推进。
    CustomOAuth2ProviderConflict,
    /// 模型厂商目录条目不存在。
    ModelProviderNotFound,
    /// 模型厂商目录版本已被其他管理员推进。
    ModelProviderConflict,
    /// 实名认证设置版本已被其他管理员推进。
    AccountVerificationSettingsConflict,
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
            Self::TwoFactorRequired => (
                StatusCode::UNAUTHORIZED,
                ManagementErrorBody {
                    code: "two_factor_required",
                    message: "Two-factor authentication required",
                },
            ),
            Self::TwoFactorInvalid => (
                StatusCode::UNAUTHORIZED,
                ManagementErrorBody {
                    code: "two_factor_invalid",
                    message: "Invalid two-factor authentication code",
                },
            ),
            Self::LoginDisabled => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "password_login_disabled",
                    message: "Password login disabled",
                },
            ),
            Self::TurnstileRejected => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "turnstile_rejected",
                    message: "Turnstile verification rejected",
                },
            ),
            Self::TurnstileUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "turnstile_unavailable",
                    message: "Turnstile verification unavailable",
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
            Self::AnnouncementInvalidRequest => (
                StatusCode::BAD_REQUEST,
                ManagementErrorBody {
                    code: "announcement_invalid_request",
                    message: "Announcement request is invalid",
                },
            ),
            Self::AnnouncementNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "announcement_not_found",
                    message: "Announcement not found",
                },
            ),
            Self::AnnouncementConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "announcement_conflict",
                    message: "Announcement version or state changed",
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
            Self::OrganizationCreditInvalidRequest => (
                StatusCode::BAD_REQUEST,
                ManagementErrorBody {
                    code: "organization_credit_invalid_request",
                    message: "Organization credit request is invalid",
                },
            ),
            Self::OrganizationCreditNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_credit_not_found",
                    message: "Organization credit term not found",
                },
            ),
            Self::OrganizationCreditIntervalConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_credit_interval_conflict",
                    message: "Organization credit term interval conflict",
                },
            ),
            Self::OrganizationCreditConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_credit_conflict",
                    message: "Organization credit term version conflict",
                },
            ),
            Self::OrganizationCreditTransitionInvalid => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_credit_transition_invalid",
                    message: "Organization credit term transition invalid",
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
            Self::OrganizationApprovalNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_approval_not_found",
                    message: "Organization approval not found",
                },
            ),
            Self::OrganizationApprovalConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_approval_conflict",
                    message: "Organization approval conflict",
                },
            ),
            Self::OrganizationApprovalExpired => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_approval_expired",
                    message: "Organization approval request expired",
                },
            ),
            Self::OrganizationProvisioningNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_provisioning_not_found",
                    message: "Organization provisioning request not found",
                },
            ),
            Self::OrganizationProvisioningConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_provisioning_conflict",
                    message: "Organization provisioning request conflict",
                },
            ),
            Self::OrganizationProvisioningInvalidRequest => (
                StatusCode::BAD_REQUEST,
                ManagementErrorBody {
                    code: "organization_provisioning_invalid_request",
                    message: "Organization provisioning request is invalid",
                },
            ),
            Self::OrganizationProvisioningForbidden => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "organization_provisioning_forbidden",
                    message: "Organization provisioning access is forbidden",
                },
            ),
            Self::OrganizationProvisioningUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "organization_provisioning_unavailable",
                    message: "Organization provisioning is temporarily unavailable",
                },
            ),
            Self::OrganizationVerificationNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_verification_not_found",
                    message: "Organization verification case not found",
                },
            ),
            Self::OrganizationVerificationConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_verification_conflict",
                    message: "Organization verification case conflict",
                },
            ),
            Self::OrganizationVerificationInvalidRequest => (
                StatusCode::BAD_REQUEST,
                ManagementErrorBody {
                    code: "organization_verification_invalid_request",
                    message: "Organization verification request is invalid",
                },
            ),
            Self::OrganizationVerificationForbidden => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "organization_verification_forbidden",
                    message: "Organization verification access is forbidden",
                },
            ),
            Self::AccountVerificationSelfReview => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "account_verification_self_review",
                    message: "Administrators cannot review their own verification",
                },
            ),
            Self::OrganizationVerificationUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "organization_verification_unavailable",
                    message: "Organization verification service is temporarily unavailable",
                },
            ),
            Self::OrganizationLifecycleConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_lifecycle_conflict",
                    message: "Organization lifecycle conflict",
                },
            ),
            Self::OrganizationLifecycleTransitionInvalid => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_lifecycle_transition_invalid",
                    message: "Organization lifecycle transition invalid",
                },
            ),
            Self::OrganizationLastOwner => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_last_owner",
                    message: "Organization must retain an active owner",
                },
            ),
            Self::OrganizationTeamInUse => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_team_in_use",
                    message: "Organization team still has active members",
                },
            ),
            Self::OrganizationEntitlementRestricted => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_entitlement_restricted",
                    message: "Organization entitlement restricts this operation",
                },
            ),
            Self::OrganizationEntitlementUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "organization_entitlement_unavailable",
                    message: "Organization entitlement service unavailable",
                },
            ),
            Self::OrganizationContractPriceNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_contract_price_not_found",
                    message: "Organization contract price not found",
                },
            ),
            Self::OrganizationContractPriceIntervalConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_contract_price_interval_conflict",
                    message: "Organization contract price interval conflict",
                },
            ),
            Self::OrganizationContractPriceConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_contract_price_conflict",
                    message: "Organization contract price version conflict",
                },
            ),
            Self::OrganizationContractPriceTransitionInvalid => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_contract_price_transition_invalid",
                    message: "Organization contract price transition invalid",
                },
            ),
            Self::OrganizationPlanNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_plan_not_found",
                    message: "Organization plan not found",
                },
            ),
            Self::OrganizationPlanConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_plan_conflict",
                    message: "Organization plan conflict",
                },
            ),
            Self::OrganizationPlanOrderNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_plan_order_not_found",
                    message: "Organization plan order not found",
                },
            ),
            Self::OrganizationPlanOrderConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_plan_order_conflict",
                    message: "Organization plan order conflict",
                },
            ),
            Self::OrganizationPlanOrderOutcomeUnknown => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "organization_plan_order_outcome_unknown",
                    message: "Organization plan order outcome unknown",
                },
            ),
            Self::OrganizationPlanOrderUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "organization_plan_order_unavailable",
                    message: "Organization plan order service unavailable",
                },
            ),
            Self::OrganizationSsoProviderNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_sso_provider_not_found",
                    message: "Organization SSO provider not found",
                },
            ),
            Self::OrganizationSsoProviderConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_sso_provider_conflict",
                    message: "Organization SSO provider conflict",
                },
            ),
            Self::OrganizationSsoProviderTransitionInvalid => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_sso_provider_transition_invalid",
                    message: "Organization SSO provider transition invalid",
                },
            ),
            Self::OrganizationSsoIdentityBindingRejected => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ManagementErrorBody {
                    code: "organization_sso_identity_binding_rejected",
                    message: "Organization SSO identity binding rejected",
                },
            ),
            Self::OrganizationSsoIdentityBindingConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_sso_identity_binding_conflict",
                    message: "Organization SSO identity binding conflict",
                },
            ),
            Self::OrganizationSsoRecoveryRejected => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ManagementErrorBody {
                    code: "organization_sso_recovery_rejected",
                    message: "Organization SSO recovery rejected",
                },
            ),
            Self::OrganizationSsoRecoveryConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_sso_recovery_conflict",
                    message: "Organization SSO recovery conflict",
                },
            ),
            Self::OrganizationSsoRecoveryUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "organization_sso_recovery_unavailable",
                    message: "Organization SSO recovery unavailable",
                },
            ),
            Self::OrganizationSsoDomainNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "organization_sso_domain_not_found",
                    message: "Organization SSO domain not found",
                },
            ),
            Self::OrganizationSsoDomainConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_sso_domain_conflict",
                    message: "Organization SSO domain conflict",
                },
            ),
            Self::OrganizationSsoDomainTransitionInvalid => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_sso_domain_transition_invalid",
                    message: "Organization SSO domain transition invalid",
                },
            ),
            Self::OrganizationSsoDomainVerificationFailed => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ManagementErrorBody {
                    code: "organization_sso_domain_verification_failed",
                    message: "Organization SSO domain verification failed",
                },
            ),
            Self::OrganizationTeamLimitReached => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "organization_team_limit_reached",
                    message: "Organization team limit reached",
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
            Self::RefundNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "refund_not_found",
                    message: "Refund request not found",
                },
            ),
            Self::RefundConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "refund_conflict",
                    message: "Refund request state conflict",
                },
            ),
            Self::RefundUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "refund_unavailable",
                    message: "Refund service unavailable",
                },
            ),
            Self::RefundAutoSubmitFailed => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "refund_auto_submit_failed",
                    message: "Refund provider rejected the automatic submission",
                },
            ),
            Self::RefundOutcomeUnknown => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "refund_outcome_unknown",
                    message: "Refund submission outcome unknown",
                },
            ),
            Self::BudgetConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "budget_conflict",
                    message: "Budget state conflict",
                },
            ),
            Self::BudgetOutcomeUnknown => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "budget_outcome_unknown",
                    message: "Budget write outcome unknown",
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
            Self::TwoFactorAlreadyEnabled => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "two_factor_already_enabled",
                    message: "Two-factor authentication is already enabled",
                },
            ),
            Self::TwoFactorNotEnabled => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "two_factor_not_enabled",
                    message: "Two-factor authentication is not enabled",
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
            Self::TokenOutcomeUnknown => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "token_outcome_unknown",
                    message: "Token write outcome unknown",
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
            Self::OauthLoginSettingsConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "oauth_login_settings_conflict",
                    message: "OAuth login settings changed",
                },
            ),
            Self::CustomOAuth2ProviderNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "custom_oauth2_provider_not_found",
                    message: "Custom OAuth2 provider not found",
                },
            ),
            Self::CustomOAuth2ProviderConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "custom_oauth2_provider_conflict",
                    message: "Custom OAuth2 provider changed",
                },
            ),
            Self::ModelProviderNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "model_provider_not_found",
                    message: "Model provider not found",
                },
            ),
            Self::ModelProviderConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "model_provider_conflict",
                    message: "Model provider changed",
                },
            ),
            Self::AccountVerificationSettingsConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "account_verification_settings_conflict",
                    message: "Account verification settings changed",
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
