//! 管理 API 文档使用的闭合枚举 schema。

use utoipa::{
    ToSchema,
    openapi::{
        RefOr,
        schema::{
            AdditionalProperties, ArrayBuilder, KnownFormat, ObjectBuilder, Schema, SchemaFormat,
            SchemaType, Type,
        },
    },
};

/// 生成允许任意属性的 JSON 对象 schema。
pub(crate) fn free_form_object_schema() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .additional_properties(Some(AdditionalProperties::FreeForm(true)))
        .build()
        .into()
}

/// 生成只允许写入、且允许任意属性的 JSON 对象 schema。
pub(crate) fn sensitive_free_form_object_schema() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .additional_properties(Some(AdditionalProperties::FreeForm(true)))
        .write_only(Some(true))
        .build()
        .into()
}

/// 生成可省略或为 null 的只写自由对象 schema，用于保留已有敏感配置。
pub(crate) fn optional_sensitive_free_form_object_schema() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::from_iter([Type::Object, Type::Null]))
        .additional_properties(Some(AdditionalProperties::FreeForm(true)))
        .write_only(Some(true))
        .build()
        .into()
}

/// 生成只允许写入的字符串映射 schema。
pub(crate) fn sensitive_string_map_schema() -> RefOr<Schema> {
    string_map_schema(
        "敏感 Header 覆盖输入；拒绝认证、Content-Type、Accept、受控请求 ID、Host 和 hop-by-hop 头，永不随响应返回。",
        true,
    )
}

/// 生成可省略或为 null 的只写字符串映射 schema，用于保留已有敏感 Header。
pub(crate) fn optional_sensitive_string_map_schema() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::from_iter([Type::Object, Type::Null]))
        .additional_properties(Some(Schema::Object(
            ObjectBuilder::new().schema_type(Type::String).build(),
        )))
        .description(Some(
            "敏感 Header 覆盖输入；拒绝认证和协议托管头，省略或 null 时保留已有值，显式对象时替换。",
        ))
        .write_only(Some(true))
        .build()
        .into()
}

/// 生成渠道响应中的 Canonical 模型集合 schema。
pub(crate) fn channel_models_schema() -> RefOr<Schema> {
    string_array_schema(
        1,
        255,
        None,
        512,
        true,
        false,
        "按 Canonical 原文排序的渠道模型集合。",
    )
}

/// 生成渠道写入正文中的 Canonical 模型集合 schema。
pub(crate) fn channel_write_models_schema() -> RefOr<Schema> {
    string_array_schema(
        1,
        255,
        None,
        512,
        true,
        false,
        "渠道支持的 Canonical 模型集合；单项最多 255 个 UTF-8 字节，笛卡尔积最多 4096 项。",
    )
}

/// 生成渠道响应中的分组 ID 集合 schema。
pub(crate) fn channel_group_ids_schema() -> RefOr<Schema> {
    id_array_schema("按 ID 升序返回的渠道分组集合。")
}

/// 生成渠道写入正文中的分组 ID 集合 schema。
pub(crate) fn channel_write_group_ids_schema() -> RefOr<Schema> {
    id_array_schema("渠道关联的有效分组 ID 集合，与 models 的笛卡尔积最多 4096 项。")
}

/// 生成渠道响应中的模型映射 schema。
pub(crate) fn model_mapping_schema() -> RefOr<Schema> {
    bounded_string_map_schema("Canonical 请求模型到上游模型的映射。")
}

/// 生成渠道写入正文中的模型映射 schema。
pub(crate) fn model_mapping_write_schema() -> RefOr<Schema> {
    bounded_string_map_schema("Canonical 请求模型到上游模型的映射，序列化后最多 64 KiB。")
}

/// 生成可省略或为 null 的渠道模型映射写入 schema。
pub(crate) fn optional_model_mapping_write_schema() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::from_iter([Type::Object, Type::Null]))
        .additional_properties(Some(Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(1))
                .max_length(Some(256))
                .build(),
        )))
        .description(Some(
            "Canonical 请求模型到上游模型的映射；省略或 null 时保留原值。",
        ))
        .build()
        .into()
}

/// 生成渠道读写共用的闭合 Canonical 参数覆盖 schema。
pub(crate) fn channel_parameter_overrides_schema() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .property(
            "temperature",
            ObjectBuilder::new()
                .schema_type(Type::Number)
                .minimum(Some(0.0))
                .maximum(Some(2.0)),
        )
        .property(
            "top_p",
            ObjectBuilder::new()
                .schema_type(Type::Number)
                .minimum(Some(0.0))
                .maximum(Some(1.0)),
        )
        .property(
            "max_output_tokens",
            ObjectBuilder::new()
                .schema_type(Type::Integer)
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int64)))
                .minimum(Some(1))
                .maximum(Some(1_000_000)),
        )
        .property(
            "stop_sequences",
            ArrayBuilder::new()
                .items(
                    ObjectBuilder::new()
                        .schema_type(Type::String)
                        .min_length(Some(1))
                        .max_length(Some(1_024)),
                )
                .min_items(Some(1))
                .max_items(Some(4)),
        )
        .additional_properties(Some(AdditionalProperties::FreeForm(false)))
        .description(Some(
            "闭合的 Canonical 采样覆盖；max_output_tokens 仅作为上限，不会提高客户端值。",
        ))
        .build()
        .into()
}

/// 生成令牌响应中的模型白名单 schema。
pub(crate) fn token_model_limits_schema() -> RefOr<Schema> {
    string_array_schema(
        1,
        256,
        Some(1),
        512,
        false,
        true,
        "Canonical 模型名白名单；null 表示不限制，保留持久化顺序和重复项。",
    )
}

/// 生成令牌写入正文中的模型白名单 schema。
pub(crate) fn token_model_limits_write_schema() -> RefOr<Schema> {
    string_array_schema(
        1,
        256,
        Some(1),
        512,
        false,
        true,
        "Canonical 模型名白名单；null 表示不限制，顺序和重复项会保留。",
    )
}

/// 生成令牌响应中的 IP/CIDR 白名单 schema。
pub(crate) fn token_allow_ips_schema() -> RefOr<Schema> {
    string_array_schema(
        1,
        64,
        Some(1),
        64,
        false,
        true,
        "IP/CIDR 白名单；null 表示关闭过滤，保留持久化顺序和重复项。",
    )
}

/// 生成令牌写入正文中的 IP/CIDR 白名单 schema。
pub(crate) fn token_allow_ips_write_schema() -> RefOr<Schema> {
    string_array_schema(
        1,
        64,
        Some(1),
        64,
        false,
        true,
        "IP/CIDR 白名单；null 表示关闭过滤，顺序和重复项会保留。",
    )
}

fn id_array_schema(description: &str) -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(
            ObjectBuilder::new()
                .schema_type(Type::Integer)
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int64)))
                .minimum(Some(1))
                .build(),
        )
        .max_items(Some(64))
        .unique_items(true)
        .description(Some(description))
        .build()
        .into()
}

#[allow(
    clippy::too_many_arguments,
    reason = "集中保持数组边界与旧 OpenAPI 契约一致"
)]
fn string_array_schema(
    item_min_length: usize,
    item_max_length: usize,
    min_items: Option<usize>,
    max_items: usize,
    unique_items: bool,
    nullable: bool,
    description: &str,
) -> RefOr<Schema> {
    let schema_type = if nullable {
        SchemaType::from_iter([Type::Array, Type::Null])
    } else {
        SchemaType::from(Type::Array)
    };
    ArrayBuilder::new()
        .schema_type(schema_type)
        .items(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(item_min_length))
                .max_length(Some(item_max_length))
                .build(),
        )
        .min_items(min_items)
        .max_items(Some(max_items))
        .unique_items(unique_items)
        .description(Some(description))
        .build()
        .into()
}

fn bounded_string_map_schema(description: &str) -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .additional_properties(Some(Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(1))
                .max_length(Some(256))
                .build(),
        )))
        .description(Some(description))
        .build()
        .into()
}

fn string_map_schema(description: &str, write_only: bool) -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .additional_properties(Some(Schema::Object(
            ObjectBuilder::new().schema_type(Type::String).build(),
        )))
        .description(Some(description))
        .write_only(Some(write_only))
        .build()
        .into()
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminSessionRole, rename_all = "snake_case")]
pub(crate) enum AdminSessionRoleSchema {
    User,
    Admin,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminUserStatus, rename_all = "snake_case")]
pub(crate) enum AdminUserStatusSchema {
    Enabled,
    Disabled,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminTokenStatus, rename_all = "snake_case")]
pub(crate) enum AdminTokenStatusSchema {
    Enabled,
    Disabled,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = UserTokenStatus, rename_all = "snake_case")]
pub(crate) enum UserTokenStatusSchema {
    Enabled,
    Disabled,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminRoutingStatus, rename_all = "snake_case")]
pub(crate) enum AdminRoutingStatusSchema {
    Enabled,
    Disabled,
    AutoDisabled,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(
    as = AdminRoutingWriteStatus,
    rename_all = "snake_case",
    description = "管理写接口只允许手动启用或禁用；自动禁用由运行时维护。"
)]
pub(crate) enum AdminRoutingWriteStatusSchema {
    Enabled,
    Disabled,
}

#[allow(dead_code, reason = "仅用于生成智能路由 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminRouteMode, rename_all = "snake_case")]
pub(crate) enum AdminRouteModeSchema {
    Pattern,
    ExplicitGroup,
}

#[allow(dead_code, reason = "仅用于生成智能路由 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminRouteStrategy, rename_all = "snake_case")]
pub(crate) enum AdminRouteStrategySchema {
    Weighted,
    RoundRobin,
    StableFirst,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminChannelType, rename_all = "snake_case")]
pub(crate) enum AdminChannelTypeSchema {
    Openai,
    Anthropic,
    Gemini,
    Jina,
    Cohere,
    Xai,
    Bedrock,
    Vertex,
    Custom,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminChannelProtocol, rename_all = "snake_case")]
pub(crate) enum AdminChannelProtocolSchema {
    OpenaiChat,
    OpenaiResponses,
    OpenaiEmbeddings,
    OpenaiImages,
    OpenaiAudio,
    OpenaiSpeech,
    JinaRerank,
    CohereRerank,
    XaiVideo,
    Anthropic,
    Gemini,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminChannelWriteType, rename_all = "snake_case")]
pub(crate) enum AdminChannelWriteTypeSchema {
    Openai,
    Anthropic,
    Gemini,
    Jina,
    Cohere,
    Xai,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminChannelWriteProtocol, rename_all = "snake_case")]
pub(crate) enum AdminChannelWriteProtocolSchema {
    OpenaiChat,
    OpenaiResponses,
    OpenaiEmbeddings,
    OpenaiImages,
    OpenaiAudio,
    OpenaiSpeech,
    JinaRerank,
    CohereRerank,
    XaiVideo,
    Anthropic,
    Gemini,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ClientSimulationProfile, rename_all = "snake_case")]
pub(crate) enum ClientSimulationProfileSchema {
    AnthropicCliHeadersV1,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ClientSimulationBodyProfile, rename_all = "snake_case")]
pub(crate) enum ClientSimulationBodyProfileSchema {
    AnthropicCliSystemDateV1,
}

/// 调试追踪使用的闭合正文补丁结果。
#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ClientSimulationBodyPatchResult, rename_all = "snake_case")]
pub(crate) enum ClientSimulationBodyPatchResultSchema {
    Applied,
    Rejected,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ClientSimulationResult, rename_all = "snake_case")]
pub(crate) enum ClientSimulationResultSchema {
    NotApplied,
    Applied,
    Failed,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ResponsesCompactMode, rename_all = "snake_case")]
pub(crate) enum ResponsesCompactModeSchema {
    Auto,
    ForceOn,
    ForceOff,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ResponsesCompactProbeResult, rename_all = "snake_case")]
pub(crate) enum ResponsesCompactProbeResultSchema {
    Unknown,
    Supported,
    Unsupported,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminCredentialKind, rename_all = "snake_case")]
pub(crate) enum AdminCredentialKindSchema {
    ApiKey,
    Oauth,
    SetupToken,
    Bedrock,
    ServiceAccount,
    Upstream,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminCredentialWriteKind, rename_all = "snake_case")]
pub(crate) enum AdminCredentialWriteKindSchema {
    ApiKey,
    Oauth,
    Bedrock,
    ServiceAccount,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminCredentialMultiKeyMode, rename_all = "snake_case")]
pub(crate) enum AdminCredentialMultiKeyModeSchema {
    Random,
    RoundRobin,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminCredentialQuotaDimension, rename_all = "snake_case")]
pub(crate) enum AdminCredentialQuotaDimensionSchema {
    /// 普通凭据使用的全局额度窗口。
    Global,
    /// 仅 `gpt-5.3-codex-spark` 使用的影子额度窗口。
    Spark,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminUsageLogBillingMode, rename_all = "snake_case")]
pub(crate) enum AdminUsageLogBillingModeSchema {
    PerToken,
    PerCall,
    Free,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminUsageLogVideoResolution)]
pub(crate) enum AdminUsageLogVideoResolutionSchema {
    #[schema(rename = "480p")]
    P480,
    #[schema(rename = "720p")]
    P720,
    #[schema(rename = "1080p")]
    P1080,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = UsageLogProtocol, rename_all = "snake_case")]
pub(crate) enum UsageLogProtocolSchema {
    OpenAiChat,
    OpenAiResponses,
    OpenAiEmbeddings,
    OpenAiImages,
    OpenAiAudio,
    OpenAiSpeech,
    JinaRerank,
    CohereRerank,
    XaiVideo,
    Anthropic,
    Gemini,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = UsageLogOperation, rename_all = "snake_case")]
pub(crate) enum UsageLogOperationSchema {
    Chat,
    Responses,
    ResponsesCompact,
    Embedding,
    Image,
    Audio,
    Rerank,
    Video,
    CountTokens,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = UsageLogReasoningEffort, rename_all = "snake_case")]
pub(crate) enum UsageLogReasoningEffortSchema {
    None,
    Minimal,
    Low,
    Medium,
    High,
    ExtraHigh,
    Max,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ModelCatalogBillingMode, rename_all = "snake_case")]
pub(crate) enum ModelCatalogBillingModeSchema {
    PerToken,
    Free,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ModelCatalogModality, rename_all = "snake_case")]
pub(crate) enum ModelCatalogModalitySchema {
    Text,
    Image,
    Audio,
    Video,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ModelCatalogCapability, rename_all = "snake_case")]
pub(crate) enum ModelCatalogCapabilitySchema {
    Reasoning,
    ToolCalls,
    ResponsesCompact,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ModelCatalogPricingScope, rename_all = "snake_case")]
pub(crate) enum ModelCatalogPricingScopeSchema {
    PublicBase,
    Group,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminUsageLogSource, rename_all = "snake_case")]
pub(crate) enum AdminUsageLogSourceSchema {
    Upstream,
    Estimated,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = AdminUsageLogSemantics, rename_all = "snake_case")]
pub(crate) enum AdminUsageLogSemanticsSchema {
    Inclusive,
    CacheSeparated,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = BearerTokenType)]
pub(crate) enum BearerTokenTypeSchema {
    Bearer,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = ManagementErrorCode, rename_all = "snake_case")]
pub(crate) enum ManagementErrorCodeSchema {
    InvalidRequest,
    InvalidCredentials,
    TwoFactorRequired,
    TwoFactorInvalid,
    PasswordLoginDisabled,
    TurnstileRejected,
    TurnstileUnavailable,
    InvalidSession,
    Forbidden,
    SetupConflict,
    SiteSettingsConflict,
    AnnouncementInvalidRequest,
    AnnouncementNotFound,
    AnnouncementConflict,
    UserNotFound,
    GroupNotFound,
    TokenNotFound,
    TokenLimitReached,
    TokenOutcomeUnknown,
    ChannelNotFound,
    CredentialNotFound,
    ProbeUnavailable,
    OauthProviderNotConfigured,
    OauthLoginSettingsConflict,
    CustomOauth2ProviderNotFound,
    CustomOauth2ProviderConflict,
    OauthCredentialProviderMismatch,
    OauthAuthorizationCapacityExceeded,
    OauthAuthorizationNotFound,
    OauthAuthorizationExpired,
    OauthAuthorizationDenied,
    OauthUpstreamTimeout,
    OauthUpstreamRejected,
    OauthUpstreamInvalidResponse,
    OauthUnavailable,
    PlaygroundShareNotFound,
    PlaygroundShareLimitReached,
    PlaygroundConversationNotFound,
    PlaygroundConversationLimitReached,
    PlaygroundConversationConflict,
    UserConflict,
    OrganizationNotFound,
    OrganizationCreditInvalidRequest,
    OrganizationCreditNotFound,
    OrganizationCreditIntervalConflict,
    OrganizationCreditConflict,
    OrganizationCreditTransitionInvalid,
    OrganizationApprovalNotFound,
    OrganizationApprovalConflict,
    OrganizationApprovalExpired,
    OrganizationContractPriceNotFound,
    OrganizationContractPriceIntervalConflict,
    OrganizationContractPriceConflict,
    OrganizationContractPriceTransitionInvalid,
    OrganizationPlanNotFound,
    OrganizationPlanConflict,
    OrganizationSsoProviderNotFound,
    OrganizationSsoProviderConflict,
    OrganizationSsoProviderTransitionInvalid,
    OrganizationSsoIdentityBindingRejected,
    OrganizationSsoIdentityBindingConflict,
    OrganizationSsoRecoveryRejected,
    OrganizationSsoRecoveryConflict,
    OrganizationSsoRecoveryUnavailable,
    OrganizationSsoDomainNotFound,
    OrganizationSsoDomainConflict,
    OrganizationSsoDomainTransitionInvalid,
    OrganizationSsoDomainVerificationFailed,
    WalletEventConflict,
    WalletInsufficientQuota,
    WalletOverflow,
    WalletOutcomeUnknown,
    RefundNotFound,
    RefundConflict,
    RefundUnavailable,
    RefundAutoSubmitFailed,
    RefundOutcomeUnknown,
    TopupUnavailable,
    TopupProviderRejected,
    TopupOrderConflict,
    TopupOrderOutcomeUnknown,
    RedemptionBatchNotFound,
    RedemptionBatchConflict,
    RedemptionCodeInvalid,
    RedemptionBatchDisabled,
    RedemptionCodeExpired,
    RedemptionCodeAlreadyUsed,
    RedemptionOutcomeUnknown,
    SubscriptionPlanNotFound,
    SubscriptionPlanDisabled,
    SubscriptionNotFound,
    SubscriptionTransitionInvalid,
    SubscriptionInUse,
    SubscriptionConflict,
    SubscriptionOutcomeUnknown,
    RegistrationDisabled,
    RegistrationRateLimited,
    RegistrationRejected,
    InvitationRejected,
    PasswordResetRejected,
    PasswordChangeRejected,
    TwoFactorAlreadyEnabled,
    TwoFactorNotEnabled,
    EmailNotConfigured,
    EmailDeliveryFailed,
    GroupConflict,
    GroupInUse,
    RouteNotFound,
    RouteConflict,
    RouteInvalidReference,
    DebugTraceNotFound,
    ModelNotFound,
    ModelConflict,
    ModelSyncChannelUnavailable,
    ModelSyncUnsupportedChannel,
    ModelSyncUpstreamTimeout,
    ModelSyncUpstreamRejected,
    ModelSyncInvalidResponse,
    ModelSyncCandidateLimitExceeded,
    ModelSyncPreviewNotFound,
    ModelSyncPreviewExpired,
    ModelSyncPreviewAlreadyApplied,
    ModelSyncConflict,
    ModelPriceConflict,
    ModelPriceExpressionInvalid,
    ModelPriceExpressionPreviewInvalid,
    ModelPriceExpressionEvaluationFailed,
    ModelPriceSourceTimeout,
    ModelPriceSourceUnavailable,
    ModelPriceSourceResponseTooLarge,
    ModelPriceSourceInvalidResponse,
    ModelPriceSourceCandidateLimitExceeded,
    InternalError,
}
