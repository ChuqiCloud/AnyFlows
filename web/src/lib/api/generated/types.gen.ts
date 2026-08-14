// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

export type ClientOptions = {
    baseUrl: `${string}://${string}` | (string & {});
};
export type AdminAuthenticationSettings = {
    password_login_enabled: boolean;
    registration_enabled: boolean;
    default_group_id: number;
    initial_quota: number;
    invitation_rebate_quota: number;
    email_required: boolean;
    rate_limit_attempts: number;
    rate_limit_window_seconds: number;
    version: number;
};
export type AdminAuthenticationSettingsRequest = {
    password_login_enabled: boolean;
    registration_enabled: boolean;
    default_group_id: number;
    initial_quota: number;
    invitation_rebate_quota: number;
    email_required: boolean;
    rate_limit_attempts: number;
    rate_limit_window_seconds: number;
};
export type AdminBalanceAlertSettings = {
    enabled: boolean;
    default_threshold: number;
    reminder_interval_seconds: number;
    subscription_alert_enabled: boolean;
    subscription_remaining_percent: number;
    version: number;
};
export type AdminBalanceAlertSettingsRequest = {
    enabled: boolean;
    default_threshold: number;
    reminder_interval_seconds: number;
    subscription_alert_enabled: boolean;
    subscription_remaining_percent: number;
};
export type AdminBrandSettingsRequest = {
    logo_url?: string | null;
    tagline?: string | null;
    description?: string | null;
};
export type AdminChannel = {
    id: number;
    type: AdminChannelType;
    name: string;
    protocol: AdminChannelProtocol;
    base_url: string | null;
    timeout_secs: number | null;
    status: AdminRoutingStatus;
    weight: number;
    priority: number;
    auto_ban: boolean;
    auto_ban_rules: AdminChannelAutoBanRules;
    pool_mode: boolean;
    responses_websocket_enabled: boolean;
    responses_compact_mode: ResponsesCompactMode;
    responses_compact_model_mapping: {
        [key: string]: string;
    };
    responses_compact_probe: AdminResponsesCompactProbe;
    models: Array<string>;
    group_ids: Array<number>;
    model_mapping: {
        [key: string]: string;
    };
    param_override: {
        temperature?: number;
        top_p?: number;
        max_output_tokens?: number;
        stop_sequences?: Array<string>;
    };
    balance: number | null;
    used_quota: number;
    tag: string | null;
    created_at: number;
    updated_at: number;
};
export type AdminChannelAutoBanRules = {
    status_codes: Array<number>;
    keywords: Array<string>;
};
export type AdminChannelCreateRequest = {
    name: string;
    type: 'openai' | 'anthropic' | 'gemini' | 'jina' | 'cohere' | 'xai';
    protocol: 'openai_chat' | 'openai_responses' | 'openai_embeddings' | 'openai_images' | 'openai_audio' | 'openai_speech' | 'jina_rerank' | 'cohere_rerank' | 'xai_video' | 'anthropic' | 'gemini';
    base_url: string | null;
    timeout_secs?: number | null;
    status: AdminRoutingWriteStatus;
    weight: number;
    priority: number;
    auto_ban: boolean;
    auto_ban_rules?: AdminChannelAutoBanRules;
    pool_mode?: boolean;
    responses_websocket_enabled?: boolean;
    responses_compact_mode?: ResponsesCompactMode;
    responses_compact_model_mapping?: {
        [key: string]: string;
    };
    models: Array<string>;
    group_ids: Array<number>;
    model_mapping: {
        [key: string]: string;
    };
    param_override: {
        temperature?: number;
        top_p?: number;
        max_output_tokens?: number;
        stop_sequences?: Array<string>;
    };
    tag: string | null;
};
export type AdminChannelListResponse = {
    channels: Array<AdminChannel>;
    next_cursor: number | null;
};
export type AdminChannelProbeResponse = {
    status: AdminChannelProbeStatus;
    latency_ms: number;
};
export type AdminChannelProbeStatus = 'healthy' | 'unhealthy' | 'timeout';
export type AdminChannelProtocol = 'openai_chat' | 'openai_responses' | 'openai_embeddings' | 'openai_images' | 'openai_audio' | 'openai_speech' | 'jina_rerank' | 'cohere_rerank' | 'xai_video' | 'anthropic' | 'gemini';
export type AdminChannelType = 'openai' | 'anthropic' | 'gemini' | 'jina' | 'cohere' | 'xai' | 'bedrock' | 'vertex' | 'custom';
export type AdminChannelUpdateRequest = {
    name: string;
    type: 'openai' | 'anthropic' | 'gemini' | 'jina' | 'cohere' | 'xai';
    protocol: 'openai_chat' | 'openai_responses' | 'openai_embeddings' | 'openai_images' | 'openai_audio' | 'openai_speech' | 'jina_rerank' | 'cohere_rerank' | 'xai_video' | 'anthropic' | 'gemini';
    base_url: string | null;
    timeout_secs?: number | null;
    status: AdminRoutingWriteStatus;
    weight: number;
    priority: number;
    auto_ban: boolean;
    auto_ban_rules?: null | AdminChannelAutoBanRules;
    pool_mode?: boolean | null;
    responses_websocket_enabled?: boolean | null;
    responses_compact_mode?: null | ResponsesCompactMode;
    responses_compact_model_mapping?: {
        [key: string]: string;
    } | null;
    models: Array<string>;
    group_ids: Array<number>;
    model_mapping: {
        [key: string]: string;
    };
    param_override: {
        temperature?: number;
        top_p?: number;
        max_output_tokens?: number;
        stop_sequences?: Array<string>;
    };
    tag: string | null;
};
export type AdminCredential = {
    id: number;
    channel_id: number;
    kind: AdminCredentialKind;
    status: AdminRoutingStatus;
    multi_key_mode: null | AdminCredentialMultiKeyMode;
    priority: number;
    weight: number;
    concurrency: number | null;
    load_factor_micros: number | null;
    rate_multiplier_micros: number | null;
    schedulable: boolean;
    rate_limited_at: number | null;
    rate_limit_reset_at: number | null;
    overload_until: number | null;
    temp_unschedulable_until: number | null;
    session_window_start: number | null;
    session_window_end: number | null;
    parent_id: number | null;
    quota_dimension: AdminCredentialQuotaDimension;
    proxy_id: number | null;
    oauth_provider: string | null;
    oauth_token_pending: boolean;
    oauth_account_key: string | null;
    oauth_project_id: string | null;
    oauth_revision: number;
    last_used_at: number | null;
    created_at: number;
    updated_at: number;
};
export type AdminCredentialCreateRequest = {
    kind: 'api_key' | 'oauth' | 'bedrock' | 'service_account';
    secret: null | AdminCredentialSecret;
    status: AdminRoutingWriteStatus;
    multi_key_mode: null | AdminCredentialMultiKeyMode;
    priority: number;
    weight: number;
    concurrency: number | null;
    load_factor_micros: number | null;
    rate_multiplier_micros: number | null;
    schedulable: boolean;
    parent_id: number | null;
    quota_dimension: AdminCredentialQuotaDimension;
    proxy_id: number | null;
    oauth_provider: string | null;
    oauth_account_key: string | null;
    oauth_project_id: string | null;
};
export type AdminCredentialKind = 'api_key' | 'oauth' | 'setup_token' | 'bedrock' | 'service_account' | 'upstream';
export type AdminCredentialListResponse = {
    credentials: Array<AdminCredential>;
    next_cursor: number | null;
};
export type AdminCredentialMultiKeyMode = 'random' | 'round_robin';
export type AdminCredentialProxy = {
    id: number;
    name: string;
    scheme: AdminCredentialProxyScheme;
    host: string;
    port: number;
    username?: string | null;
    password_configured: boolean;
    trust_proxy_dns: boolean;
    enabled: boolean;
    version: number;
    created_at: number;
    updated_at: number;
};
export type AdminCredentialProxyListResponse = {
    proxies: Array<AdminCredentialProxy>;
    next_cursor: number | null;
};
export type AdminCredentialProxyScheme = 'http' | 'https' | 'socks5' | 'socks5h';
export type AdminCredentialProxyWriteRequest = {
    name: string;
    scheme: AdminCredentialProxyScheme;
    host: string;
    port: number;
    username?: string | null;
    trust_proxy_dns: boolean;
    enabled: boolean;
};
export type AdminCredentialQuotaDimension = 'global' | 'spark';
export type AdminCredentialSecret = {
    kind: 'api_key';
} | {
    kind: 'oauth';
} | {
    kind: 'bedrock';
} | {
    kind: 'service_account';
};
export type AdminCredentialUpdateRequest = {
    kind: 'api_key' | 'oauth' | 'bedrock' | 'service_account';
    secret: null | AdminCredentialSecret;
    status: AdminRoutingWriteStatus;
    multi_key_mode: null | AdminCredentialMultiKeyMode;
    priority: number;
    weight: number;
    concurrency: number | null;
    load_factor_micros: number | null;
    rate_multiplier_micros: number | null;
    schedulable: boolean;
    parent_id: number | null;
    quota_dimension: AdminCredentialQuotaDimension;
    proxy_id: number | null;
    oauth_provider: string | null;
    oauth_account_key: string | null;
    oauth_project_id: string | null;
};
export type AdminCredentialWriteFields = {
    kind: 'api_key' | 'oauth' | 'bedrock' | 'service_account';
    status: AdminRoutingWriteStatus;
    multi_key_mode: null | AdminCredentialMultiKeyMode;
    priority: number;
    weight: number;
    concurrency: number | null;
    load_factor_micros: number | null;
    rate_multiplier_micros: number | null;
    schedulable: boolean;
    parent_id: number | null;
    quota_dimension: AdminCredentialQuotaDimension;
    proxy_id: number | null;
    oauth_provider: string | null;
    oauth_account_key: string | null;
    oauth_project_id: string | null;
};
export type AdminDashboardResponse = {
    period_start: number;
    period_end: number;
    request_count: number;
    quota_consumed: number;
    upstream_usage_count: number;
    estimated_usage_count: number;
    per_token_request_count: number;
    per_call_request_count: number;
    free_request_count: number;
    enabled_channel_count: number;
    disabled_channel_count: number;
    auto_disabled_channel_count: number;
};
export type AdminDebugTrace = {
    id: number;
    request_id: string;
    user_id: number;
    token_id: number;
    group_id: number;
    requested_model: string;
    downstream_protocol: string;
    upstream_protocol: string;
    operation: string;
    outcome: string;
    selected_channel_id?: number | null;
    selected_credential_id?: number | null;
    routing_elapsed_ms: number;
    attempt_count: number;
    created_at: number;
};
export type AdminDebugTraceAttempt = {
    candidate_index: number;
    channel_id: number;
    credential_id: number;
    outcome: string;
    failure_kind?: string | null;
    upstream_status?: number | null;
    retry_decision: boolean;
    elapsed_ms: number;
};
export type AdminDebugTraceDetailResponse = {
    trace: AdminDebugTrace;
    attempts: Array<AdminDebugTraceAttempt>;
};
export type AdminDebugTraceListResponse = {
    traces: Array<AdminDebugTrace>;
    next_cursor: number | null;
};
export type AdminDebugTraceSettings = {
    enabled: boolean;
    sample_per_million: number;
    retention_hours: number;
    version: number;
};
export type AdminDebugTraceSettingsRequest = {
    enabled: boolean;
    sample_per_million: number;
    retention_hours: number;
};
export type AdminEmailSettings = {
    enabled: boolean;
    host: string;
    port: number;
    tls_mode: AdminEmailTlsMode;
    username?: string | null;
    password_configured: boolean;
    from_address: string;
    from_name?: string | null;
    reply_to?: string | null;
    timeout_seconds: number;
    version: number;
    delivery_ready: boolean;
};
export type AdminEmailSettingsRequest = {
    enabled: boolean;
    host: string;
    port: number;
    tls_mode: AdminEmailTlsMode;
    username?: string | null;
    from_address: string;
    from_name?: string | null;
    reply_to?: string | null;
    timeout_seconds: number;
};
export type AdminEmailTestRequest = {
    recipient: string;
};
export type AdminEmailTlsMode = 'start_tls' | 'tls';
export type AdminGroup = {
    id: number;
    name: string;
    display_name: string;
    ratio_micros: number;
    peak_ratio_micros: number | null;
    peak_start: string | null;
    peak_end: string | null;
    is_exclusive: boolean;
    daily_limit: number | null;
    weekly_limit: number | null;
    monthly_limit: number | null;
    daily_window: AdminGroupWindow;
    weekly_window: AdminGroupWindow;
    monthly_window: AdminGroupWindow;
    rpm_limit: number | null;
    fallback_group_id: number | null;
    flags: {
        [key: string]: unknown;
    };
};
export type AdminGroupListResponse = {
    groups: Array<AdminGroup>;
    next_cursor: number | null;
};
export type AdminGroupWindow = {
    usage: number;
    started_at: number;
    resets_at: number;
};
export type AdminGroupWriteRequest = {
    name: string;
    display_name: string;
    ratio_micros: number;
    peak_ratio_micros?: number | null;
    peak_start?: string | null;
    peak_end?: string | null;
    is_exclusive: boolean;
    daily_limit?: number | null;
    weekly_limit?: number | null;
    monthly_limit?: number | null;
    rpm_limit?: number | null;
    fallback_group_id?: number | null;
    flags: {
        [key: string]: unknown;
    };
};
export type AdminMissingModel = {
    model: string;
    channel_count: number;
    channels: Array<AdminMissingModelChannel>;
};
export type AdminMissingModelChannel = {
    channel_id: number;
    channel_name: string;
};
export type AdminMissingModelImportItemRequest = {
    model: string;
    display_name: string;
    provider: string;
    description?: string | null;
    icon_url?: string | null;
    tags: Array<string>;
    context_window?: number | null;
    input_modalities: Array<AdminModelModality>;
    output_modalities: Array<AdminModelModality>;
    supports_reasoning: boolean;
    supports_tool_calls: boolean;
};
export type AdminMissingModelImportRequest = {
    items: Array<AdminMissingModelImportItemRequest>;
};
export type AdminMissingModelImportResponse = {
    models: Array<AdminModel>;
};
export type AdminMissingModelListResponse = {
    models: Array<AdminMissingModel>;
    next_cursor: string | null;
};
export type AdminModel = {
    id: number;
    model: string;
    display_name: string;
    provider: string;
    description: string | null;
    icon_url: string | null;
    tags: Array<string>;
    context_window: number | null;
    input_modalities: Array<AdminModelModality>;
    output_modalities: Array<AdminModelModality>;
    supports_reasoning: boolean;
    supports_tool_calls: boolean;
    visibility: AdminModelVisibility;
    lifecycle: AdminModelLifecycle;
    created_at: number;
    updated_at: number;
};
export type AdminModelCreateRequest = {
    model: string;
    display_name: string;
    provider: string;
    description?: string | null;
    icon_url?: string | null;
    tags: Array<string>;
    context_window?: number | null;
    input_modalities: Array<AdminModelModality>;
    output_modalities: Array<AdminModelModality>;
    supports_reasoning: boolean;
    supports_tool_calls: boolean;
    visibility: AdminModelVisibility;
    lifecycle: AdminModelLifecycle;
};
export type AdminModelLifecycle = 'draft' | 'active' | 'deprecated' | 'retired';
export type AdminModelListResponse = {
    models: Array<AdminModel>;
    next_cursor: number | null;
};
export type AdminModelModality = 'text' | 'image' | 'audio' | 'video';
export type AdminModelPrice = {
    model: string;
    billing_mode: AdminModelPriceBillingMode;
    prices: AdminModelPriceValues;
    billing_expression: string | null;
    version: number;
};
export type AdminModelPriceBatchRequest = {
    items: Array<AdminModelPriceWriteItem>;
};
export type AdminModelPriceBatchResponse = {
    prices: Array<AdminModelPrice>;
};
export type AdminModelPriceBillingMode = 'per_token' | 'free' | 'expression';
export type AdminModelPriceExpressionPreviewRequest = {
    billing_expression: string;
    usage: AdminModelPriceExpressionUsage;
    ratios: AdminModelPriceExpressionRatios;
};
export type AdminModelPriceExpressionPreviewResponse = {
    matched_tier: string;
    variables: AdminModelPriceExpressionVariables;
    base_usd: string;
    total_usd: string;
    quota: string;
};
export type AdminModelPriceExpressionRatios = {
    group_micros: number;
    group_model_micros: number;
    peak_micros: number;
};
export type AdminModelPriceExpressionUsage = {
    input_tokens: number;
    output_tokens: number;
    cache_read_tokens: number;
    cache_creation_5m_tokens: number;
    cache_creation_1h_tokens: number;
    semantics: AdminModelPriceExpressionUsageSemantics;
};
export type AdminModelPriceExpressionUsageSemantics = 'inclusive' | 'cache_separated';
export type AdminModelPriceExpressionVariables = {
    input_tokens: number;
    output_tokens: number;
    cache_read_tokens: number;
    cache_creation_5m_tokens: number;
    cache_creation_1h_tokens: number;
    context_length_tokens: number;
};
export type AdminModelPriceListResponse = {
    prices: Array<AdminModelPrice>;
    next_cursor: string | null;
};
export type AdminModelPriceSourceCandidate = {
    model: string;
    provider: string;
    provider_name: string;
    source_model: string;
    source_name: string;
    last_updated: string | null;
    context_window: number | null;
    source_deprecation_date: string | null;
    costs: AdminModelPriceSourceCosts;
    has_tiered_pricing: boolean;
    source_deprecated: boolean;
};
export type AdminModelPriceSourceCosts = {
    input: string | null;
    output: string | null;
    cache_read: string | null;
    cache_write: string | null;
    cache_creation_5m: string | null;
    cache_creation_1h: string | null;
};
export type AdminModelPriceSourcePreview = {
    source: string;
    fetched_at: number;
    revision: string | null;
    candidates: Array<AdminModelPriceSourceCandidate>;
};
export type AdminModelPriceValues = {
    input: string;
    output: string;
    cache_read: string;
    cache_creation_5m: string;
    cache_creation_1h: string;
};
export type AdminModelPriceWriteItem = {
    model: string;
    expected_version: number | null;
    context_window: number | null;
    billing_mode: AdminModelPriceBillingMode;
    prices: AdminModelPriceValues;
    billing_expression?: string | null;
};
export type AdminModelSyncApplyItemRequest = {
    item_id: number;
    display_name: string;
    provider: string;
    description?: string | null;
    icon_url?: string | null;
    tags: Array<string>;
    context_window?: number | null;
    input_modalities: Array<AdminModelModality>;
    output_modalities: Array<AdminModelModality>;
    supports_reasoning: boolean;
    supports_tool_calls: boolean;
};
export type AdminModelSyncApplyRequest = {
    items: Array<AdminModelSyncApplyItemRequest>;
};
export type AdminModelSyncApplyResponse = {
    models: Array<AdminModel>;
};
export type AdminModelSyncPreview = {
    preview_id: string;
    channel_id: number;
    type: AdminChannelType;
    protocol: AdminChannelProtocol;
    expires_at: number;
    items: Array<AdminModelSyncPreviewItem>;
};
export type AdminModelSyncPreviewItem = {
    item_id: number;
    canonical_model: string;
    upstream_model: string | null;
    relation: AdminModelSyncRelation;
    display_name_hint: string | null;
    description_hint: string | null;
    context_window_hint: number | null;
    input_token_limit_hint: number | null;
    output_token_limit_hint: number | null;
    supported_methods: Array<string>;
    applied_model_id: number | null;
};
export type AdminModelSyncPreviewRequest = {
    channel_id: number;
};
export type AdminModelSyncRelation = 'missing_metadata' | 'discovered_unconfigured' | 'existing' | 'not_reported';
export type AdminModelUpdateRequest = {
    display_name: string;
    provider: string;
    description?: string | null;
    icon_url?: string | null;
    tags: Array<string>;
    context_window?: number | null;
    input_modalities: Array<AdminModelModality>;
    output_modalities: Array<AdminModelModality>;
    supports_reasoning: boolean;
    supports_tool_calls: boolean;
    visibility: AdminModelVisibility;
    lifecycle: AdminModelLifecycle;
};
export type AdminModelVisibility = 'public' | 'authenticated' | 'hidden';
export type AdminNetworkSettings = {
    mode: AdminNetworkSettingsMode;
    proxy_host?: string | null;
    proxy_port?: number | null;
    username?: string | null;
    password_configured: boolean;
    trust_proxy_dns: boolean;
    version: number;
};
export type AdminNetworkSettingsMode = 'inherit' | 'direct' | 'http' | 'https' | 'socks5' | 'socks5h';
export type AdminNetworkSettingsRequest = {
    mode: AdminNetworkSettingsMode;
    proxy_host?: string | null;
    proxy_port?: number | null;
    username?: string | null;
    trust_proxy_dns: boolean;
};
export type AdminOAuthAuthorizationRequest = {
    provider: AdminOAuthProvider;
};
export type AdminOAuthAuthorizationResponse = {
    provider: AdminOAuthProvider;
    authorization_url: string;
    redirect_uri: string;
    expires_in_seconds: number;
    loopback_listener_ready: boolean;
    manual_callback_supported: boolean;
};
export type AdminOAuthCompletionResponse = {
    status: AdminOAuthCompletionStatus;
};
export type AdminOAuthCompletionStatus = 'connected';
export type AdminOAuthManualCallbackRequest = {
    provider: AdminOAuthProvider;
};
export type AdminOAuthProvider = 'claude_code' | 'codex' | 'gemini' | 'antigravity';
export type AdminOAuthProviderListResponse = {
    providers: Array<AdminOAuthProviderStatus>;
};
export type AdminOAuthProviderStatus = {
    provider: AdminOAuthProvider;
    redirect_uri: string;
    callback_port: number;
    callback_path: string;
    loopback_listener_ready: boolean;
    manual_callback_supported: boolean;
};
export type AdminPaymentSettings = {
    stripe_enabled: boolean;
    stripe_publishable_key?: string | null;
    stripe_secret_key_configured: boolean;
    stripe_webhook_secret_configured: boolean;
    stripe_signature_tolerance_seconds: number;
    epay_enabled: boolean;
    epay_gateway_url?: string | null;
    epay_merchant_id?: string | null;
    epay_merchant_key_configured: boolean;
    epay_alipay_enabled: boolean;
    epay_wxpay_enabled: boolean;
    epay_quota_per_cny: number;
    version: number;
};
export type AdminPaymentSettingsRequest = {
    expected_version: number;
    stripe_enabled: boolean;
    stripe_publishable_key?: string | null;
    clear_stripe_secret_key: boolean;
    clear_stripe_webhook_secret: boolean;
    stripe_signature_tolerance_seconds: number;
    epay_enabled: boolean;
    epay_gateway_url?: string | null;
    epay_merchant_id?: string | null;
    clear_epay_merchant_key: boolean;
    epay_alipay_enabled: boolean;
    epay_wxpay_enabled: boolean;
    epay_quota_per_cny: number;
};
export type AdminRedemptionAuditBatch = {
    batch_id: string;
    name: string;
    created_by_user_id: number;
    status: AdminRedemptionBatchStatus;
    quota_amount: number;
    issued_count: number;
    redeemed_count: number;
    remaining_count: number;
    expired_count: number;
    disabled_count: number;
    expires_at: number | null;
    disabled_at: number | null;
    last_redeemed_at: number | null;
    created_at: number;
    updated_at: number;
};
export type AdminRedemptionAuditListResponse = {
    batches: Array<AdminRedemptionAuditBatch>;
    summary: AdminRedemptionAuditSummary;
    next_cursor: number | null;
};
export type AdminRedemptionAuditStatus = 'active' | 'expired' | 'disabled' | 'redeemed';
export type AdminRedemptionAuditSummary = {
    issued_count: number;
    redeemed_count: number;
    remaining_count: number;
    expired_count: number;
    disabled_count: number;
};
export type AdminRedemptionBatch = {
    batch_id: string;
    name: string;
    created_by_user_id: number;
    status: AdminRedemptionBatchStatus;
    quota_amount: number;
    code_count: number;
    redeemed_count: number;
    version: number;
    expires_at: number | null;
    disabled_at: number | null;
    created_at: number;
    updated_at: number;
};
export type AdminRedemptionBatchCreateRequest = {
    name: string;
    quota_amount: number;
    code_count: number;
    expires_at: number | null;
};
export type AdminRedemptionBatchDisableRequest = {
    expected_version: number;
};
export type AdminRedemptionBatchDisableResponse = {
    batch_id: string;
    version: number;
    disabled_at: number;
};
export type AdminRedemptionBatchListResponse = {
    batches: Array<AdminRedemptionBatch>;
    next_cursor: number | null;
};
export type AdminRedemptionBatchStatus = 'active' | 'disabled';
export type AdminResponsesCompactProbe = {
    result: ResponsesCompactProbeResult;
    checked_at: number | null;
    http_status: number | null;
};
export type AdminRoute = {
    id: number;
    name: string;
    model_pattern: string;
    mode: AdminRouteMode;
    strategy: AdminRouteStrategy;
    model_mapping: {
        [key: string]: unknown;
    };
    enabled: boolean;
    channels: Array<AdminRouteChannelResponse>;
};
export type AdminRouteChannelResponse = {
    id: number;
    channel_id: number;
    credential_id: number;
    priority: number;
    weight: number;
    enabled: boolean;
    success_count: number;
    fail_count: number;
    total_latency_ms: number;
    cooldown_level: number;
    cooldown_until?: number | null;
    last_selected_at?: number | null;
    last_failure_at?: number | null;
};
export type AdminRouteChannelWriteRequest = {
    channel_id: number;
    credential_id: number;
    priority: number;
    weight: number;
    enabled: boolean;
};
export type AdminRouteListResponse = {
    routes: Array<AdminRoute>;
    next_cursor: number | null;
};
export type AdminRouteMode = 'pattern' | 'explicit_group';
export type AdminRouteStrategy = 'weighted' | 'round_robin' | 'stable_first';
export type AdminRouteWriteRequest = {
    name: string;
    model_pattern: string;
    mode: AdminRouteMode;
    strategy: AdminRouteStrategy;
    model_mapping: {
        [key: string]: unknown;
    };
    enabled: boolean;
    channels: Array<AdminRouteChannelWriteRequest>;
};
export type AdminRoutingStatus = 'enabled' | 'disabled' | 'auto_disabled';
export type AdminRoutingWriteStatus = 'enabled' | 'disabled';
export type AdminSiteSettings = {
    site_name: string;
    public_base_url?: string | null;
    brand: PublicBrandSettings;
    version: number;
};
export type AdminSiteSettingsRequest = {
    site_name: string;
    public_base_url?: string | null;
    brand: AdminBrandSettingsRequest;
};
export type AdminSubscriptionPlan = {
    plan_id: string;
    name: string;
    created_by_user_id: number;
    status: SubscriptionPlanStatus;
    quota_amount: number;
    cycle: SubscriptionCycle;
    version: number;
    disabled_at: number | null;
    created_at: number;
    updated_at: number;
};
export type AdminSubscriptionPlanCreateRequest = {
    name: string;
    quota_amount: number;
    cycle: SubscriptionCycle;
};
export type AdminSubscriptionPlanDisableRequest = {
    expected_version: number;
};
export type AdminSubscriptionPlanListResponse = {
    plans: Array<AdminSubscriptionPlan>;
    next_cursor: number | null;
};
export type AdminToken = {
    id: number;
    user_id: number;
    key_prefix: string;
    name: string;
    status: AdminTokenStatus;
    group_id: number | null;
    remain_quota: number;
    unlimited_quota: boolean;
    used_quota: number;
    expired_at: number | null;
    model_limits: Array<string> | null;
    allow_ips: Array<string> | null;
    cross_group_retry: boolean;
    rate_limit_5h: number | null;
    rate_limit_1d: number | null;
    rate_limit_7d: number | null;
    usage_5h: number;
    usage_1d: number;
    usage_7d: number;
    window_5h_start: number;
    window_1d_start: number;
    window_7d_start: number;
    max_requests: number | null;
    used_requests: number;
};
export type AdminTokenListResponse = {
    tokens: Array<AdminToken>;
    next_cursor: number | null;
};
export type AdminTokenStatus = 'enabled' | 'disabled';
export type AdminTokenWriteRequest = {
    user_id: number;
    name: string;
    status: AdminTokenStatus;
    group_id: number | null;
    remain_quota: number;
    unlimited_quota: boolean;
    expired_at: number | null;
    model_limits: Array<string> | null;
    allow_ips: Array<string> | null;
    cross_group_retry: boolean;
    rate_limit_5h: number | null;
    rate_limit_1d: number | null;
    rate_limit_7d: number | null;
    max_requests: number | null;
};
export type AdminUsageLog = {
    id: number;
    event_id: string;
    user_id: number;
    username: string;
    token_id: number;
    group_id: number;
    billing_mode: AdminUsageLogBillingMode;
    input_tokens: number;
    output_tokens: number;
    cache_read: number;
    cache_creation_5m: number;
    cache_creation_1h: number;
    reasoning_tokens: number;
    audio_input_tokens: number;
    audio_output_tokens: number;
    audio_duration_nanoseconds: number | null;
    video_duration_seconds: number | null;
    video_resolution: null | AdminUsageLogVideoResolution;
    request_id: string | null;
    model: string | null;
    protocol: null | UsageLogProtocol;
    operation: null | UsageLogOperation;
    is_stream: boolean | null;
    reasoning_effort: null | UsageLogReasoningEffort;
    reasoning_budget_tokens: number | null;
    first_token_ms: number | null;
    duration_ms: number | null;
    usage_source: AdminUsageLogSource;
    usage_semantics: AdminUsageLogSemantics;
    quota: number;
    created_at: number;
};
export type AdminUsageLogBillingMode = 'per_token' | 'per_call' | 'free';
export type AdminUsageLogListResponse = {
    logs: Array<AdminUsageLog>;
    next_cursor: number | null;
};
export type AdminUsageLogSemantics = 'inclusive' | 'cache_separated';
export type AdminUsageLogSource = 'upstream' | 'estimated';
export type AdminUsageLogVideoResolution = '480p' | '720p' | '1080p';
export type AdminUser = {
    id: number;
    username: string;
    email: string | null;
    role: 'user' | 'admin';
    status: AdminUserStatus;
    default_group_id: number;
    quota: number;
    used_quota: number;
    frozen_quota: number;
    request_count: number;
    rpm_limit: number | null;
    concurrency: number | null;
};
export type AdminUserCreateRequest = {
    username: string;
    email?: string | null;
    password?: string | null;
    role: 'user' | 'admin';
    status: AdminUserStatus;
    default_group_id: number;
    quota: number;
    rpm_limit?: number | null;
    concurrency?: number | null;
};
export type AdminUserListResponse = {
    users: Array<AdminUser>;
    next_cursor: number | null;
};
export type AdminUserStatus = 'enabled' | 'disabled';
export type AdminUserSubscriptionBindRequest = {
    plan_id: string;
};
export type AdminUserSubscriptionLifecycleAction = 'suspend' | 'resume' | 'cancel';
export type AdminUserSubscriptionLifecycleRequest = {
    action: AdminUserSubscriptionLifecycleAction;
    expected_version: number;
};
export type AdminUserSubscriptionLifecycleResponse = {
    subscription: UserSubscription;
    periods_elapsed: number;
};
export type AdminUserUpdateRequest = {
    username: string;
    email?: string | null;
    password?: string | null;
    role: 'user' | 'admin';
    status: AdminUserStatus;
    default_group_id: number;
    rpm_limit?: number | null;
    concurrency?: number | null;
};
export type AdminWalletAdjustmentRequest = {
    event_id: string;
    quota_delta: number;
    reason: string;
};
export type AdminWalletEntry = {
    id: number;
    event_id: string;
    user_id: number;
    actor_user_id: number | null;
    entry_type: AdminWalletEntryType;
    quota_delta: number;
    balance_before: number;
    balance_after: number;
    reason: string | null;
    created_at: number;
};
export type AdminWalletEntryType = 'opening_balance' | 'admin_adjustment' | 'topup' | 'redemption' | 'invite_rebate';
export type AdminWalletListResponse = {
    entries: Array<AdminWalletEntry>;
    next_cursor: number | null;
};
export type AudioSpeechBinary = Blob | File;
export type AudioSpeechCustomVoice = {
    id: string;
};
export type AudioSpeechError = {
    error: AudioSpeechErrorBody;
};
export type AudioSpeechErrorBody = {
    code: string;
    message: string;
    param: string | null;
    type: string;
};
export type AudioSpeechOutputFormat = 'mp3' | 'opus' | 'aac' | 'flac' | 'wav' | 'pcm';
export type AudioSpeechRequest = {
    model: string;
    input: string;
    voice: AudioSpeechVoice;
    instructions?: string | null;
    response_format?: null | AudioSpeechOutputFormat;
    speed?: number | null;
    stream_format?: null | AudioSpeechStreamFormat;
};
export type AudioSpeechStreamFormat = 'audio';
export type AudioSpeechVoice = string | AudioSpeechCustomVoice;
export type IssuedAdminRedemptionBatch = {
    batch: AdminRedemptionBatch;
    codes: Array<string>;
};
export type IssuedAdminToken = {
    api_key: string;
    token: AdminToken;
};
export type IssuedUserToken = {
    api_key: string;
    token: UserToken;
};
export type LoginRequest = {
    username: string;
    password: string;
};
export type LoginResponse = {
    access_token: string;
    token_type: 'Bearer';
    expires_in: number;
    expires_at: number;
    user: SessionUser;
};
export type ManagementError = {
    code: 'invalid_request' | 'invalid_credentials' | 'password_login_disabled' | 'invalid_session' | 'forbidden' | 'setup_conflict' | 'user_not_found' | 'group_not_found' | 'token_not_found' | 'token_limit_reached' | 'channel_not_found' | 'credential_not_found' | 'probe_unavailable' | 'oauth_provider_not_configured' | 'oauth_credential_provider_mismatch' | 'oauth_authorization_capacity_exceeded' | 'oauth_authorization_not_found' | 'oauth_authorization_expired' | 'oauth_authorization_denied' | 'oauth_upstream_timeout' | 'oauth_upstream_rejected' | 'oauth_upstream_invalid_response' | 'oauth_unavailable' | 'playground_share_not_found' | 'playground_share_limit_reached' | 'playground_conversation_not_found' | 'playground_conversation_limit_reached' | 'playground_conversation_conflict' | 'user_conflict' | 'wallet_event_conflict' | 'wallet_insufficient_quota' | 'wallet_overflow' | 'wallet_outcome_unknown' | 'topup_unavailable' | 'topup_provider_rejected' | 'topup_order_conflict' | 'topup_order_outcome_unknown' | 'redemption_batch_not_found' | 'redemption_batch_conflict' | 'redemption_code_invalid' | 'redemption_batch_disabled' | 'redemption_code_expired' | 'redemption_code_already_used' | 'redemption_outcome_unknown' | 'subscription_plan_not_found' | 'subscription_plan_disabled' | 'subscription_not_found' | 'subscription_transition_invalid' | 'subscription_in_use' | 'subscription_conflict' | 'subscription_outcome_unknown' | 'registration_disabled' | 'registration_rate_limited' | 'registration_rejected' | 'invitation_rejected' | 'password_reset_rejected' | 'password_change_rejected' | 'email_not_configured' | 'email_delivery_failed' | 'group_conflict' | 'group_in_use' | 'route_not_found' | 'route_conflict' | 'route_invalid_reference' | 'debug_trace_not_found' | 'model_not_found' | 'model_conflict' | 'model_sync_channel_unavailable' | 'model_sync_unsupported_channel' | 'model_sync_upstream_timeout' | 'model_sync_upstream_rejected' | 'model_sync_invalid_response' | 'model_sync_candidate_limit_exceeded' | 'model_sync_preview_not_found' | 'model_sync_preview_expired' | 'model_sync_preview_already_applied' | 'model_sync_conflict' | 'model_price_conflict' | 'model_price_expression_invalid' | 'model_price_expression_preview_invalid' | 'model_price_expression_evaluation_failed' | 'model_price_source_timeout' | 'model_price_source_unavailable' | 'model_price_source_response_too_large' | 'model_price_source_invalid_response' | 'model_price_source_candidate_limit_exceeded' | 'internal_error';
    message: string;
};
export type ModelCatalogBillingMode = 'per_token' | 'free';
export type ModelCatalogCapability = 'reasoning' | 'tool_calls' | 'responses_compact';
export type ModelCatalogItem = {
    model: string;
    display_name: string;
    provider: string;
    description: string | null;
    icon_url: string | null;
    tags: Array<string>;
    context_window: number | null;
    input_modalities: Array<AdminModelModality>;
    output_modalities: Array<AdminModelModality>;
    supports_reasoning: boolean;
    supports_tool_calls: boolean;
    supports_responses_compact: boolean;
    lifecycle: ModelCatalogLifecycle;
    runtime_status: ModelCatalogRuntimeStatus;
    billing_mode: ModelCatalogBillingMode;
    prices: null | ModelCatalogTokenPrices;
    ratios: ModelCatalogRatios;
    price_version: number;
    available_protocols: Array<ModelCatalogProtocol>;
};
export type ModelCatalogLifecycle = 'active' | 'deprecated';
export type ModelCatalogListResponse = {
    pricing_scope: ModelCatalogPricingScope;
    models: Array<ModelCatalogItem>;
    next_cursor: string | null;
};
export type ModelCatalogModality = 'text' | 'image' | 'audio' | 'video';
export type ModelCatalogPricingScope = 'public_base' | 'group';
export type ModelCatalogProtocol = 'openai_chat' | 'openai_responses' | 'openai_embeddings' | 'openai_images' | 'openai_audio' | 'openai_speech' | 'jina_rerank' | 'cohere_rerank' | 'xai_video' | 'anthropic' | 'gemini';
export type ModelCatalogProvider = {
    name: string;
    model_count: number;
};
export type ModelCatalogProviderListResponse = {
    pricing_scope: ModelCatalogPricingScope;
    providers: Array<ModelCatalogProvider>;
};
export type ModelCatalogRatios = {
    group_micros: string;
    group_model_micros: string;
    peak_micros: string;
};
export type ModelCatalogRuntimeStatus = 'not_evaluated' | 'available';
export type ModelCatalogTokenPrices = {
    input: string;
    output: string;
    cache_read: string;
    cache_creation_5m: string;
    cache_creation_1h: string;
};
export type PasswordResetConfirmRequest = {
    token: string;
    password: string;
};
export type PasswordResetRequest = {
    email: string;
};
export type PasswordResetRequestResponse = {
    accepted: boolean;
};
export type PlaygroundConversationListResponse = {
    conversations: Array<PlaygroundConversationSummary>;
};
export type PlaygroundConversationResponse = {
    conversation_id: string;
    title: string;
    sessions: Array<PlaygroundShareSession>;
    revision: number;
    created_at: number;
    updated_at: number;
};
export type PlaygroundConversationSaveRequest = {
    revision?: number | null;
    sessions: Array<PlaygroundShareSession>;
};
export type PlaygroundConversationSummary = {
    conversation_id: string;
    title: string;
    models: Array<string>;
    revision: number;
    created_at: number;
    updated_at: number;
};
export type PlaygroundShareCreateRequest = {
    ttl_days: 1 | 7 | 30;
    sessions: Array<PlaygroundShareSession>;
};
export type PlaygroundShareCreateResponse = {
    token: string;
    created_at: number;
    expires_at: number;
};
export type PlaygroundShareMessage = {
    role: PlaygroundShareMessageRole;
    content: string;
};
export type PlaygroundShareMessageRole = 'user' | 'assistant';
export type PlaygroundShareReadResponse = {
    sessions: Array<PlaygroundShareSession>;
    created_at: number;
    expires_at: number;
};
export type PlaygroundShareSession = {
    model: string;
    messages: Array<PlaygroundShareMessage>;
};
export type PublicAuthenticationCapabilities = {
    password_login_enabled: boolean;
    registration_enabled: boolean;
    registration_email_required: boolean;
};
export type PublicBrandSettings = {
    logo_url?: string | null;
    tagline?: string | null;
    description?: string | null;
};
export type PublicSiteSettings = {
    site_name: string;
    public_base_url?: string | null;
    brand: PublicBrandSettings;
    authentication: PublicAuthenticationCapabilities;
};
export type RegistrationEmailVerificationRequest = {
    email: string;
};
export type RegistrationEmailVerificationResponse = {
    expires_at: number;
    next_send_at: number;
};
export type RegistrationRequest = {
    username: string;
    email?: string | null;
    invite_code?: string | null;
};
export type RegistrationStatusResponse = {
    password_login_enabled: boolean;
    enabled: boolean;
    email_required: boolean;
};
export type RerankDocument = string | RerankTextDocument;
export type RerankError = {
    error: RerankErrorBody;
};
export type RerankErrorBody = {
    code: string;
    message: string;
    param?: string | null;
    type: string;
};
export type RerankRequest = {
    model: string;
    query: string;
    documents: Array<RerankDocument>;
    top_n?: number;
    return_documents?: boolean;
};
export type RerankResponse = {
    id?: string;
    model?: string;
    object: string;
    results: Array<RerankResult>;
    usage?: RerankUsage;
};
export type RerankResult = {
    index: number;
    relevance_score: number;
    document?: RerankDocument;
};
export type RerankTextDocument = {
    text: string;
};
export type RerankUsage = {
    prompt_tokens: number;
    total_tokens: number;
    completion_tokens?: number;
};
export type ResponsesCompactError = {
    error: ResponsesCompactErrorBody;
};
export type ResponsesCompactErrorBody = {
    code: string;
    message: string;
    param: string | null;
    type: string;
};
export type ResponsesCompactInput = string | Array<ResponsesCompactItem>;
export type ResponsesCompactInputUsageDetails = {
    cached_tokens: number;
    cache_write_tokens: number;
};
export type ResponsesCompactItem = unknown;
export type ResponsesCompactMode = 'auto' | 'force_on' | 'force_off';
export type ResponsesCompactOutputUsageDetails = {
    reasoning_tokens: number;
};
export type ResponsesCompactProbeResult = 'unknown' | 'supported' | 'unsupported';
export type ResponsesCompactRequest = {
    model: string;
    input: ResponsesCompactInput;
    instructions?: string | null;
};
export type ResponsesCompactResponse = {
    id: string;
    object: string;
    created_at: number;
    output: Array<ResponsesCompactItem>;
    usage: ResponsesCompactUsage;
};
export type ResponsesCompactUsage = {
    input_tokens: number;
    input_tokens_details: ResponsesCompactInputUsageDetails;
    output_tokens: number;
    output_tokens_details: ResponsesCompactOutputUsageDetails;
    total_tokens: number;
};
export type SessionResponse = {
    user: SessionUser;
    expires_at: number;
};
export type SessionUser = {
    id: number;
    role: 'user' | 'admin';
};
export type SetupRequest = {
    username: string;
};
export type SetupStatusResponse = {
    setup_required: boolean;
};
export type SubscriptionCycle = 'daily' | 'weekly' | 'monthly' | 'yearly';
export type SubscriptionPlanStatus = 'active' | 'disabled';
export type UsageLogOperation = 'chat' | 'responses' | 'responses_compact' | 'embedding' | 'image' | 'audio' | 'rerank' | 'video' | 'count_tokens';
export type UsageLogProtocol = 'open_ai_chat' | 'open_ai_responses' | 'open_ai_embeddings' | 'open_ai_images' | 'open_ai_audio' | 'open_ai_speech' | 'jina_rerank' | 'cohere_rerank' | 'xai_video' | 'anthropic' | 'gemini';
export type UsageLogReasoningEffort = 'none' | 'minimal' | 'low' | 'medium' | 'high' | 'extra_high' | 'max';
export type UserInvitationRebateResponse = {
    quota_amount: number;
    credited_at: number;
};
export type UserInvitationSummaryResponse = {
    invite_code: string;
    invited_count: number;
    credited_count: number;
    current_rebate_quota: number;
    historical_rebate_quota: number;
    recent_rebates: Array<UserInvitationRebateResponse>;
};
export type UserNotificationPreferencesRequest = {
    email_product_updates: boolean;
    email_usage_alerts: boolean;
    balance_alert_threshold: number | null;
};
export type UserNotificationPreferencesResponse = {
    email_product_updates: boolean;
    email_usage_alerts: boolean;
    balance_alert_enabled: boolean;
    balance_alert_threshold: number | null;
    effective_balance_alert_threshold: number;
    subscription_alert_enabled: boolean;
    subscription_remaining_percent: number;
};
export type UserPasswordChangeRequest = {
    current_password: string;
    new_password: string;
};
export type UserProfileResponse = {
    id: number;
    username: string;
    email: string | null;
    role: 'user' | 'admin';
    notifications: UserNotificationPreferencesResponse;
};
export type UserProfileUpdateRequest = {
    username: string;
};
export type UserRedemptionRequest = {
    code: string;
};
export type UserRedemptionResult = {
    quota_amount: number;
    balance_after: number;
    redeemed_at: number;
    replayed: boolean;
};
export type UserSubscription = {
    subscription_id: string;
    user_id: number;
    plan_id: string;
    plan_name: string;
    plan_version: number;
    status: UserSubscriptionStatus;
    quota_amount: number;
    quota_used: number;
    cycle: SubscriptionCycle;
    window_started_at: number;
    window_ends_at: number;
    version: number;
    bound_at: number;
    status_changed_at: number;
    updated_at: number;
};
export type UserSubscriptionListResponse = {
    subscriptions: Array<UserSubscription>;
    next_cursor: number | null;
};
export type UserSubscriptionStatus = 'active' | 'suspended' | 'canceled' | 'expired';
export type UserToken = {
    id: number;
    key_prefix: string;
    name: string;
    status: UserTokenStatus;
    remain_quota: number;
    unlimited_quota: boolean;
    used_quota: number;
    expired_at: number | null;
    model_limits: Array<string> | null;
    allow_ips: Array<string> | null;
    created_at: number;
    updated_at: number;
};
export type UserTokenListResponse = {
    tokens: Array<UserToken>;
    next_cursor: number | null;
    capacity: number;
};
export type UserTokenStatus = 'enabled' | 'disabled';
export type UserTokenWriteRequest = {
    name: string;
    status: UserTokenStatus;
    remain_quota: number;
    unlimited_quota: boolean;
    expired_at: number | null;
    model_limits: Array<string> | null;
    allow_ips: Array<string> | null;
};
export type UserTopupConfiguration = {
    methods: Array<UserTopupMethod>;
};
export type UserTopupMethod = {
    provider: string;
    payment_method: string;
    currency: string;
    min_amount_minor: number;
    max_amount_minor: number;
    publishable_key?: string | null;
};
export type UserTopupOrder = {
    order_id: string;
    provider: string;
    payment_method: string;
    status: UserTopupOrderStatus;
    amount_minor: number;
    currency: string;
    quota_amount: number;
    version: number;
    created_at: number;
    replayed: boolean;
    payment?: null | UserTopupPaymentSession;
};
export type UserTopupOrderCreateRequest = {
    idempotency_key: string;
    provider: string;
    payment_method: string;
    amount_minor: number;
};
export type UserTopupOrderStatus = 'created' | 'pending' | 'paid' | 'failed' | 'canceled' | 'expired';
export type UserTopupPaymentSession = {
    payment_intent_id: string;
    readonly client_secret: string;
    kind: 'stripe';
} | {
    redirect_url: string;
    kind: 'redirect';
};
export type UserUsageLog = {
    id: number;
    token_id: number;
    request_id: string | null;
    model: string | null;
    protocol: null | UsageLogProtocol;
    operation: null | UsageLogOperation;
    is_stream: boolean | null;
    reasoning_effort: null | UsageLogReasoningEffort;
    reasoning_budget_tokens: number | null;
    first_token_ms: number | null;
    duration_ms: number | null;
    billing_mode: AdminUsageLogBillingMode;
    input_tokens: number;
    output_tokens: number;
    cache_read: number;
    cache_creation_5m: number;
    cache_creation_1h: number;
    reasoning_tokens: number;
    quota: number;
    created_at: number;
};
export type UserUsageLogListResponse = {
    logs: Array<UserUsageLog>;
    next_cursor: number | null;
};
export type UserWalletEntry = {
    id: number;
    entry_type: UserWalletEntryType;
    quota_delta: number;
    balance_before: number;
    balance_after: number;
    reason: string | null;
    created_at: number;
};
export type UserWalletEntryType = 'opening_balance' | 'admin_adjustment' | 'topup' | 'redemption' | 'invite_rebate';
export type UserWalletListResponse = {
    entries: Array<UserWalletEntry>;
    next_cursor: number | null;
};
export type UserWalletSummary = {
    balance: number;
    used_quota: number;
    frozen_quota: number;
};
export type VideoAspectRatio = '1:1' | '16:9' | '9:16' | '4:3' | '3:4' | '3:2' | '2:3';
export type VideoFailure = {
    code: VideoFailureCode;
    message: string;
};
export type VideoFailureCode = 'invalid_argument' | 'failed_precondition' | 'service_unavailable';
export type VideoGenerationRequest = {
    model: string;
    prompt: string;
    duration?: number | null;
    aspect_ratio?: null | VideoAspectRatio;
    resolution?: null | VideoResolution;
};
export type VideoOutput = {
    url: string;
    duration: number;
    respect_moderation: boolean;
};
export type VideoPollResponse = {
    status: VideoTaskStatus;
    video?: null | VideoOutput;
    model?: string | null;
    error?: null | VideoFailure;
};
export type VideoResolution = '480p' | '720p' | '1080p';
export type VideoSubmissionResponse = {
    request_id: string;
};
export type VideoTaskError = {
    error: VideoTaskErrorBody;
};
export type VideoTaskErrorBody = {
    code: string;
    message: string;
    param: string | null;
    type: string;
};
export type VideoTaskListItem = {
    id: string;
    model: string;
    status: VideoTaskStatus;
    progress_basis_points: number;
    created_at: number;
    updated_at: number;
};
export type VideoTaskListResponse = {
    data: Array<VideoTaskListItem>;
    next_cursor: string | null;
};
export type VideoTaskStatus = 'pending' | 'done' | 'expired' | 'failed';
export type AdminChannelCreateRequestWritable = {
    name: string;
    type: 'openai' | 'anthropic' | 'gemini' | 'jina' | 'cohere' | 'xai';
    protocol: 'openai_chat' | 'openai_responses' | 'openai_embeddings' | 'openai_images' | 'openai_audio' | 'openai_speech' | 'jina_rerank' | 'cohere_rerank' | 'xai_video' | 'anthropic' | 'gemini';
    base_url: string | null;
    timeout_secs?: number | null;
    status: AdminRoutingWriteStatus;
    weight: number;
    priority: number;
    auto_ban: boolean;
    auto_ban_rules?: AdminChannelAutoBanRules;
    pool_mode?: boolean;
    responses_websocket_enabled?: boolean;
    responses_compact_mode?: ResponsesCompactMode;
    responses_compact_model_mapping?: {
        [key: string]: string;
    };
    models: Array<string>;
    group_ids: Array<number>;
    model_mapping: {
        [key: string]: string;
    };
    param_override: {
        temperature?: number;
        top_p?: number;
        max_output_tokens?: number;
        stop_sequences?: Array<string>;
    };
    header_override: {
        [key: string]: string;
    };
    settings: {
        [key: string]: unknown;
    };
    tag: string | null;
};
export type AdminChannelUpdateRequestWritable = {
    name: string;
    type: 'openai' | 'anthropic' | 'gemini' | 'jina' | 'cohere' | 'xai';
    protocol: 'openai_chat' | 'openai_responses' | 'openai_embeddings' | 'openai_images' | 'openai_audio' | 'openai_speech' | 'jina_rerank' | 'cohere_rerank' | 'xai_video' | 'anthropic' | 'gemini';
    base_url: string | null;
    timeout_secs?: number | null;
    status: AdminRoutingWriteStatus;
    weight: number;
    priority: number;
    auto_ban: boolean;
    auto_ban_rules?: null | AdminChannelAutoBanRules;
    pool_mode?: boolean | null;
    responses_websocket_enabled?: boolean | null;
    responses_compact_mode?: null | ResponsesCompactMode;
    responses_compact_model_mapping?: {
        [key: string]: string;
    } | null;
    models: Array<string>;
    group_ids: Array<number>;
    model_mapping: {
        [key: string]: string;
    };
    param_override: {
        temperature?: number;
        top_p?: number;
        max_output_tokens?: number;
        stop_sequences?: Array<string>;
    };
    header_override?: {
        [key: string]: string;
    } | null;
    settings?: {
        [key: string]: unknown;
    } | null;
    tag: string | null;
};
export type AdminCredentialCreateRequestWritable = {
    kind: 'api_key' | 'oauth' | 'bedrock' | 'service_account';
    secret: null | AdminCredentialSecretWritable;
    status: AdminRoutingWriteStatus;
    multi_key_mode: null | AdminCredentialMultiKeyMode;
    priority: number;
    weight: number;
    concurrency: number | null;
    load_factor_micros: number | null;
    rate_multiplier_micros: number | null;
    schedulable: boolean;
    parent_id: number | null;
    quota_dimension: AdminCredentialQuotaDimension;
    proxy_id: number | null;
    oauth_provider: string | null;
    oauth_account_key: string | null;
    oauth_project_id: string | null;
};
export type AdminCredentialProxyWriteRequestWritable = {
    name: string;
    scheme: AdminCredentialProxyScheme;
    host: string;
    port: number;
    username?: string | null;
    password?: string | null;
    trust_proxy_dns: boolean;
    enabled: boolean;
};
export type AdminCredentialSecretWritable = {
    api_key: string;
    kind: 'api_key';
} | {
    access_token: string;
    kind: 'oauth';
} | {
    access_key_id: string;
    secret_access_key: string;
    session_token: string | null;
    kind: 'bedrock';
} | {
    client_email: string;
    private_key_id: string | null;
    private_key: string;
    kind: 'service_account';
};
export type AdminCredentialUpdateRequestWritable = {
    kind: 'api_key' | 'oauth' | 'bedrock' | 'service_account';
    secret: null | AdminCredentialSecretWritable;
    status: AdminRoutingWriteStatus;
    multi_key_mode: null | AdminCredentialMultiKeyMode;
    priority: number;
    weight: number;
    concurrency: number | null;
    load_factor_micros: number | null;
    rate_multiplier_micros: number | null;
    schedulable: boolean;
    parent_id: number | null;
    quota_dimension: AdminCredentialQuotaDimension;
    proxy_id: number | null;
    oauth_provider: string | null;
    oauth_account_key: string | null;
    oauth_project_id: string | null;
};
export type AdminEmailSettingsRequestWritable = {
    enabled: boolean;
    host: string;
    port: number;
    tls_mode: AdminEmailTlsMode;
    username?: string | null;
    password?: string | null;
    from_address: string;
    from_name?: string | null;
    reply_to?: string | null;
    timeout_seconds: number;
};
export type AdminNetworkSettingsRequestWritable = {
    mode: AdminNetworkSettingsMode;
    proxy_host?: string | null;
    proxy_port?: number | null;
    username?: string | null;
    password?: string | null;
    trust_proxy_dns: boolean;
};
export type AdminOAuthManualCallbackRequestWritable = {
    provider: AdminOAuthProvider;
    callback_url: string;
};
export type AdminPaymentSettingsRequestWritable = {
    expected_version: number;
    stripe_enabled: boolean;
    stripe_publishable_key?: string | null;
    stripe_secret_key?: string | null;
    clear_stripe_secret_key: boolean;
    stripe_webhook_secret?: string | null;
    clear_stripe_webhook_secret: boolean;
    stripe_signature_tolerance_seconds: number;
    epay_enabled: boolean;
    epay_gateway_url?: string | null;
    epay_merchant_id?: string | null;
    epay_merchant_key?: string | null;
    clear_epay_merchant_key: boolean;
    epay_alipay_enabled: boolean;
    epay_wxpay_enabled: boolean;
    epay_quota_per_cny: number;
};
export type RegistrationRequestWritable = {
    username: string;
    email?: string | null;
    password: string;
    verification_code?: string | null;
    invite_code?: string | null;
};
export type ResponsesCompactInputWritable = string | Array<ResponsesCompactItemWritable>;
export type ResponsesCompactItemWritable = unknown;
export type SetupRequestWritable = {
    username: string;
    password: string;
};
export type UserTopupOrderWritable = {
    order_id: string;
    provider: string;
    payment_method: string;
    status: UserTopupOrderStatus;
    amount_minor: number;
    currency: string;
    quota_amount: number;
    version: number;
    created_at: number;
    replayed: boolean;
    payment?: null | UserTopupPaymentSessionWritable;
};
export type UserTopupPaymentSessionWritable = {
    payment_intent_id: string;
    kind: 'stripe';
} | {
    redirect_url: string;
    kind: 'redirect';
};
export type LoginManagementSessionData = {
    body: LoginRequest;
    path?: never;
    query?: never;
    url: '/api/auth/login';
};
export type LoginManagementSessionErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type LoginManagementSessionError = LoginManagementSessionErrors[keyof LoginManagementSessionErrors];
export type LoginManagementSessionResponses = {
    200: LoginResponse;
};
export type LoginManagementSessionResponse = LoginManagementSessionResponses[keyof LoginManagementSessionResponses];
export type GetManagementSessionData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/auth/session';
};
export type GetManagementSessionErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type GetManagementSessionError = GetManagementSessionErrors[keyof GetManagementSessionErrors];
export type GetManagementSessionResponses = {
    200: SessionResponse;
};
export type GetManagementSessionResponse = GetManagementSessionResponses[keyof GetManagementSessionResponses];
export type GetInitialSetupStatusData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/setup/status';
};
export type GetInitialSetupStatusErrors = {
    500: ManagementError;
};
export type GetInitialSetupStatusError = GetInitialSetupStatusErrors[keyof GetInitialSetupStatusErrors];
export type GetInitialSetupStatusResponses = {
    200: SetupStatusResponse;
};
export type GetInitialSetupStatusResponse = GetInitialSetupStatusResponses[keyof GetInitialSetupStatusResponses];
export type InitializeAdminSetupData = {
    body: SetupRequestWritable;
    path?: never;
    query?: never;
    url: '/api/setup';
};
export type InitializeAdminSetupErrors = {
    400: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type InitializeAdminSetupError = InitializeAdminSetupErrors[keyof InitializeAdminSetupErrors];
export type InitializeAdminSetupResponses = {
    200: LoginResponse;
};
export type InitializeAdminSetupResponse = InitializeAdminSetupResponses[keyof InitializeAdminSetupResponses];
export type GetRegistrationStatusData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/registration/status';
};
export type GetRegistrationStatusErrors = {
    500: ManagementError;
};
export type GetRegistrationStatusError = GetRegistrationStatusErrors[keyof GetRegistrationStatusErrors];
export type GetRegistrationStatusResponses = {
    200: RegistrationStatusResponse;
};
export type GetRegistrationStatusResponse = GetRegistrationStatusResponses[keyof GetRegistrationStatusResponses];
export type SendRegistrationEmailVerificationData = {
    body: RegistrationEmailVerificationRequest;
    path?: never;
    query?: never;
    url: '/api/registration/email-verification';
};
export type SendRegistrationEmailVerificationErrors = {
    400: ManagementError;
    403: ManagementError;
    409: ManagementError;
    429: ManagementError;
    500: ManagementError;
    502: ManagementError;
};
export type SendRegistrationEmailVerificationError = SendRegistrationEmailVerificationErrors[keyof SendRegistrationEmailVerificationErrors];
export type SendRegistrationEmailVerificationResponses = {
    202: RegistrationEmailVerificationResponse;
};
export type SendRegistrationEmailVerificationResponse = SendRegistrationEmailVerificationResponses[keyof SendRegistrationEmailVerificationResponses];
export type RegisterUserData = {
    body: RegistrationRequestWritable;
    path?: never;
    query?: never;
    url: '/api/registration';
};
export type RegisterUserErrors = {
    400: ManagementError;
    403: ManagementError;
    409: ManagementError;
    429: ManagementError;
    500: ManagementError;
};
export type RegisterUserError = RegisterUserErrors[keyof RegisterUserErrors];
export type RegisterUserResponses = {
    201: LoginResponse;
};
export type RegisterUserResponse = RegisterUserResponses[keyof RegisterUserResponses];
export type GetAdminAuthenticationSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings';
};
export type GetAdminAuthenticationSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminAuthenticationSettingsError = GetAdminAuthenticationSettingsErrors[keyof GetAdminAuthenticationSettingsErrors];
export type GetAdminAuthenticationSettingsResponses = {
    200: AdminAuthenticationSettings;
};
export type GetAdminAuthenticationSettingsResponse = GetAdminAuthenticationSettingsResponses[keyof GetAdminAuthenticationSettingsResponses];
export type UpdateAdminAuthenticationSettingsData = {
    body: AdminAuthenticationSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings';
};
export type UpdateAdminAuthenticationSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type UpdateAdminAuthenticationSettingsError = UpdateAdminAuthenticationSettingsErrors[keyof UpdateAdminAuthenticationSettingsErrors];
export type UpdateAdminAuthenticationSettingsResponses = {
    200: AdminAuthenticationSettings;
};
export type UpdateAdminAuthenticationSettingsResponse = UpdateAdminAuthenticationSettingsResponses[keyof UpdateAdminAuthenticationSettingsResponses];
export type RerankData = {
    body: RerankRequest;
    path?: never;
    query?: never;
    url: '/v1/rerank';
};
export type RerankErrors = {
    400: RerankError;
    401: RerankError;
    429: RerankError;
    500: RerankError;
    503: RerankError;
};
export type RerankError2 = RerankErrors[keyof RerankErrors];
export type RerankResponses = {
    200: RerankResponse;
};
export type RerankResponse2 = RerankResponses[keyof RerankResponses];
export type CompactResponseData = {
    body: ResponsesCompactRequest;
    path?: never;
    query?: never;
    url: '/v1/responses/compact';
};
export type CompactResponseErrors = {
    400: ResponsesCompactError;
    401: ResponsesCompactError;
    429: ResponsesCompactError;
    500: ResponsesCompactError;
    503: ResponsesCompactError;
};
export type CompactResponseError = CompactResponseErrors[keyof CompactResponseErrors];
export type CompactResponseResponses = {
    200: ResponsesCompactResponse;
};
export type CompactResponseResponse = CompactResponseResponses[keyof CompactResponseResponses];
export type SynthesizeSpeechData = {
    body: AudioSpeechRequest;
    path?: never;
    query?: never;
    url: '/v1/audio/speech';
};
export type SynthesizeSpeechErrors = {
    400: AudioSpeechError;
    401: AudioSpeechError;
    404: AudioSpeechError;
    429: AudioSpeechError;
    500: AudioSpeechError;
    503: AudioSpeechError;
};
export type SynthesizeSpeechError = SynthesizeSpeechErrors[keyof SynthesizeSpeechErrors];
export type SynthesizeSpeechResponses = {
    200: AudioSpeechBinary;
};
export type SynthesizeSpeechResponse = SynthesizeSpeechResponses[keyof SynthesizeSpeechResponses];
export type ListVideoTasksData = {
    body?: never;
    path?: never;
    query?: {
        before?: string;
        limit?: number;
    };
    url: '/v1/videos';
};
export type ListVideoTasksErrors = {
    400: VideoTaskError;
    401: VideoTaskError;
    500: VideoTaskError;
};
export type ListVideoTasksError = ListVideoTasksErrors[keyof ListVideoTasksErrors];
export type ListVideoTasksResponses = {
    200: VideoTaskListResponse;
};
export type ListVideoTasksResponse = ListVideoTasksResponses[keyof ListVideoTasksResponses];
export type SubmitVideoTaskData = {
    body: VideoGenerationRequest;
    headers: {
        'Idempotency-Key': string;
    };
    path?: never;
    query?: never;
    url: '/v1/videos/generations';
};
export type SubmitVideoTaskErrors = {
    400: VideoTaskError;
    401: VideoTaskError;
    404: VideoTaskError;
    409: VideoTaskError;
    429: VideoTaskError;
    500: VideoTaskError;
    503: VideoTaskError;
};
export type SubmitVideoTaskError = SubmitVideoTaskErrors[keyof SubmitVideoTaskErrors];
export type SubmitVideoTaskResponses = {
    200: VideoSubmissionResponse;
};
export type SubmitVideoTaskResponse = SubmitVideoTaskResponses[keyof SubmitVideoTaskResponses];
export type PollVideoTaskData = {
    body?: never;
    path: {
        task_id: string;
    };
    query?: never;
    url: '/v1/videos/{task_id}';
};
export type PollVideoTaskErrors = {
    401: VideoTaskError;
    404: VideoTaskError;
    500: VideoTaskError;
    503: VideoTaskError;
};
export type PollVideoTaskError = PollVideoTaskErrors[keyof PollVideoTaskErrors];
export type PollVideoTaskResponses = {
    200: VideoPollResponse;
};
export type PollVideoTaskResponse = PollVideoTaskResponses[keyof PollVideoTaskResponses];
export type ListAdminRedemptionAuditData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
        batch_id?: string;
        status?: AdminRedemptionAuditStatus;
        redeemed_after?: number;
        redeemed_before?: number;
    };
    url: '/api/admin/redemption-audit';
};
export type ListAdminRedemptionAuditErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminRedemptionAuditError = ListAdminRedemptionAuditErrors[keyof ListAdminRedemptionAuditErrors];
export type ListAdminRedemptionAuditResponses = {
    200: AdminRedemptionAuditListResponse;
};
export type ListAdminRedemptionAuditResponse = ListAdminRedemptionAuditResponses[keyof ListAdminRedemptionAuditResponses];
export type ListAdminRedemptionBatchesData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/admin/redemption-batches';
};
export type ListAdminRedemptionBatchesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminRedemptionBatchesError = ListAdminRedemptionBatchesErrors[keyof ListAdminRedemptionBatchesErrors];
export type ListAdminRedemptionBatchesResponses = {
    200: AdminRedemptionBatchListResponse;
};
export type ListAdminRedemptionBatchesResponse = ListAdminRedemptionBatchesResponses[keyof ListAdminRedemptionBatchesResponses];
export type CreateAdminRedemptionBatchData = {
    body: AdminRedemptionBatchCreateRequest;
    path?: never;
    query?: never;
    url: '/api/admin/redemption-batches';
};
export type CreateAdminRedemptionBatchErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type CreateAdminRedemptionBatchError = CreateAdminRedemptionBatchErrors[keyof CreateAdminRedemptionBatchErrors];
export type CreateAdminRedemptionBatchResponses = {
    201: IssuedAdminRedemptionBatch;
};
export type CreateAdminRedemptionBatchResponse = CreateAdminRedemptionBatchResponses[keyof CreateAdminRedemptionBatchResponses];
export type DisableAdminRedemptionBatchData = {
    body: AdminRedemptionBatchDisableRequest;
    path: {
        batch_id: string;
    };
    query?: never;
    url: '/api/admin/redemption-batches/{batch_id}/disable';
};
export type DisableAdminRedemptionBatchErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type DisableAdminRedemptionBatchError = DisableAdminRedemptionBatchErrors[keyof DisableAdminRedemptionBatchErrors];
export type DisableAdminRedemptionBatchResponses = {
    200: AdminRedemptionBatchDisableResponse;
};
export type DisableAdminRedemptionBatchResponse = DisableAdminRedemptionBatchResponses[keyof DisableAdminRedemptionBatchResponses];
export type RedeemUserRedemptionCodeData = {
    body: UserRedemptionRequest;
    path?: never;
    query?: never;
    url: '/api/account/wallet/redemptions';
};
export type RedeemUserRedemptionCodeErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    410: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type RedeemUserRedemptionCodeError = RedeemUserRedemptionCodeErrors[keyof RedeemUserRedemptionCodeErrors];
export type RedeemUserRedemptionCodeResponses = {
    200: UserRedemptionResult;
};
export type RedeemUserRedemptionCodeResponse = RedeemUserRedemptionCodeResponses[keyof RedeemUserRedemptionCodeResponses];
export type ListAdminSubscriptionPlansData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/admin/subscription-plans';
};
export type ListAdminSubscriptionPlansErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminSubscriptionPlansError = ListAdminSubscriptionPlansErrors[keyof ListAdminSubscriptionPlansErrors];
export type ListAdminSubscriptionPlansResponses = {
    200: AdminSubscriptionPlanListResponse;
};
export type ListAdminSubscriptionPlansResponse = ListAdminSubscriptionPlansResponses[keyof ListAdminSubscriptionPlansResponses];
export type CreateAdminSubscriptionPlanData = {
    body: AdminSubscriptionPlanCreateRequest;
    path?: never;
    query?: never;
    url: '/api/admin/subscription-plans';
};
export type CreateAdminSubscriptionPlanErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type CreateAdminSubscriptionPlanError = CreateAdminSubscriptionPlanErrors[keyof CreateAdminSubscriptionPlanErrors];
export type CreateAdminSubscriptionPlanResponses = {
    201: AdminSubscriptionPlan;
};
export type CreateAdminSubscriptionPlanResponse = CreateAdminSubscriptionPlanResponses[keyof CreateAdminSubscriptionPlanResponses];
export type DisableAdminSubscriptionPlanData = {
    body: AdminSubscriptionPlanDisableRequest;
    path: {
        plan_id: string;
    };
    query?: never;
    url: '/api/admin/subscription-plans/{plan_id}/disable';
};
export type DisableAdminSubscriptionPlanErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type DisableAdminSubscriptionPlanError = DisableAdminSubscriptionPlanErrors[keyof DisableAdminSubscriptionPlanErrors];
export type DisableAdminSubscriptionPlanResponses = {
    200: AdminSubscriptionPlan;
};
export type DisableAdminSubscriptionPlanResponse = DisableAdminSubscriptionPlanResponses[keyof DisableAdminSubscriptionPlanResponses];
export type ListAdminUserSubscriptionsData = {
    body?: never;
    path: {
        user_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/admin/users/{user_id}/subscriptions';
};
export type ListAdminUserSubscriptionsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminUserSubscriptionsError = ListAdminUserSubscriptionsErrors[keyof ListAdminUserSubscriptionsErrors];
export type ListAdminUserSubscriptionsResponses = {
    200: UserSubscriptionListResponse;
};
export type ListAdminUserSubscriptionsResponse = ListAdminUserSubscriptionsResponses[keyof ListAdminUserSubscriptionsResponses];
export type BindAdminUserSubscriptionData = {
    body: AdminUserSubscriptionBindRequest;
    path: {
        user_id: number;
    };
    query?: never;
    url: '/api/admin/users/{user_id}/subscriptions';
};
export type BindAdminUserSubscriptionErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type BindAdminUserSubscriptionError = BindAdminUserSubscriptionErrors[keyof BindAdminUserSubscriptionErrors];
export type BindAdminUserSubscriptionResponses = {
    201: UserSubscription;
};
export type BindAdminUserSubscriptionResponse = BindAdminUserSubscriptionResponses[keyof BindAdminUserSubscriptionResponses];
export type TransitionAdminUserSubscriptionLifecycleData = {
    body: AdminUserSubscriptionLifecycleRequest;
    path: {
        user_id: number;
        subscription_id: string;
    };
    query?: never;
    url: '/api/admin/users/{user_id}/subscriptions/{subscription_id}/lifecycle';
};
export type TransitionAdminUserSubscriptionLifecycleErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type TransitionAdminUserSubscriptionLifecycleError = TransitionAdminUserSubscriptionLifecycleErrors[keyof TransitionAdminUserSubscriptionLifecycleErrors];
export type TransitionAdminUserSubscriptionLifecycleResponses = {
    200: AdminUserSubscriptionLifecycleResponse;
};
export type TransitionAdminUserSubscriptionLifecycleResponse = TransitionAdminUserSubscriptionLifecycleResponses[keyof TransitionAdminUserSubscriptionLifecycleResponses];
export type ListCurrentUserSubscriptionsData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/account/subscriptions';
};
export type ListCurrentUserSubscriptionsErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type ListCurrentUserSubscriptionsError = ListCurrentUserSubscriptionsErrors[keyof ListCurrentUserSubscriptionsErrors];
export type ListCurrentUserSubscriptionsResponses = {
    200: UserSubscriptionListResponse;
};
export type ListCurrentUserSubscriptionsResponse = ListCurrentUserSubscriptionsResponses[keyof ListCurrentUserSubscriptionsResponses];
export type RequestPasswordResetData = {
    body: PasswordResetRequest;
    path?: never;
    query?: never;
    url: '/api/auth/password-reset/request';
};
export type RequestPasswordResetErrors = {
    400: ManagementError;
    409: ManagementError;
    500: ManagementError;
    502: ManagementError;
};
export type RequestPasswordResetError = RequestPasswordResetErrors[keyof RequestPasswordResetErrors];
export type RequestPasswordResetResponses = {
    202: PasswordResetRequestResponse;
};
export type RequestPasswordResetResponse = RequestPasswordResetResponses[keyof RequestPasswordResetResponses];
export type ConfirmPasswordResetData = {
    body: PasswordResetConfirmRequest;
    path?: never;
    query?: never;
    url: '/api/auth/password-reset/confirm';
};
export type ConfirmPasswordResetErrors = {
    400: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ConfirmPasswordResetError = ConfirmPasswordResetErrors[keyof ConfirmPasswordResetErrors];
export type ConfirmPasswordResetResponses = {
    204: void;
};
export type ConfirmPasswordResetResponse = ConfirmPasswordResetResponses[keyof ConfirmPasswordResetResponses];
export type GetUserProfileData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/profile';
};
export type GetUserProfileErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type GetUserProfileError = GetUserProfileErrors[keyof GetUserProfileErrors];
export type GetUserProfileResponses = {
    200: UserProfileResponse;
};
export type GetUserProfileResponse = GetUserProfileResponses[keyof GetUserProfileResponses];
export type UpdateUserProfileData = {
    body: UserProfileUpdateRequest;
    path?: never;
    query?: never;
    url: '/api/account/profile';
};
export type UpdateUserProfileErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateUserProfileError = UpdateUserProfileErrors[keyof UpdateUserProfileErrors];
export type UpdateUserProfileResponses = {
    200: UserProfileResponse;
};
export type UpdateUserProfileResponse = UpdateUserProfileResponses[keyof UpdateUserProfileResponses];
export type ChangeUserPasswordData = {
    body: UserPasswordChangeRequest;
    path?: never;
    query?: never;
    url: '/api/account/password';
};
export type ChangeUserPasswordErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ChangeUserPasswordError = ChangeUserPasswordErrors[keyof ChangeUserPasswordErrors];
export type ChangeUserPasswordResponses = {
    204: void;
};
export type ChangeUserPasswordResponse = ChangeUserPasswordResponses[keyof ChangeUserPasswordResponses];
export type UpdateUserNotificationPreferencesData = {
    body: UserNotificationPreferencesRequest;
    path?: never;
    query?: never;
    url: '/api/account/notifications';
};
export type UpdateUserNotificationPreferencesErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type UpdateUserNotificationPreferencesError = UpdateUserNotificationPreferencesErrors[keyof UpdateUserNotificationPreferencesErrors];
export type UpdateUserNotificationPreferencesResponses = {
    200: UserProfileResponse;
};
export type UpdateUserNotificationPreferencesResponse = UpdateUserNotificationPreferencesResponses[keyof UpdateUserNotificationPreferencesResponses];
export type GetUserWalletData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/wallet';
};
export type GetUserWalletErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type GetUserWalletError = GetUserWalletErrors[keyof GetUserWalletErrors];
export type GetUserWalletResponses = {
    200: UserWalletSummary;
};
export type GetUserWalletResponse = GetUserWalletResponses[keyof GetUserWalletResponses];
export type ListUserWalletEntriesData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/account/wallet/entries';
};
export type ListUserWalletEntriesErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type ListUserWalletEntriesError = ListUserWalletEntriesErrors[keyof ListUserWalletEntriesErrors];
export type ListUserWalletEntriesResponses = {
    200: UserWalletListResponse;
};
export type ListUserWalletEntriesResponse = ListUserWalletEntriesResponses[keyof ListUserWalletEntriesResponses];
export type GetUserTopupConfigurationData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/wallet/topups/config';
};
export type GetUserTopupConfigurationErrors = {
    401: ManagementError;
    503: ManagementError;
};
export type GetUserTopupConfigurationError = GetUserTopupConfigurationErrors[keyof GetUserTopupConfigurationErrors];
export type GetUserTopupConfigurationResponses = {
    200: UserTopupConfiguration;
};
export type GetUserTopupConfigurationResponse = GetUserTopupConfigurationResponses[keyof GetUserTopupConfigurationResponses];
export type CreateUserTopupOrderData = {
    body: UserTopupOrderCreateRequest;
    path?: never;
    query?: never;
    url: '/api/account/wallet/topups';
};
export type CreateUserTopupOrderErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    500: ManagementError;
    502: ManagementError;
    503: ManagementError;
};
export type CreateUserTopupOrderError = CreateUserTopupOrderErrors[keyof CreateUserTopupOrderErrors];
export type CreateUserTopupOrderResponses = {
    200: UserTopupOrder;
    201: UserTopupOrder;
};
export type CreateUserTopupOrderResponse = CreateUserTopupOrderResponses[keyof CreateUserTopupOrderResponses];
export type GetUserInvitationsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/invitations';
};
export type GetUserInvitationsErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type GetUserInvitationsError = GetUserInvitationsErrors[keyof GetUserInvitationsErrors];
export type GetUserInvitationsResponses = {
    200: UserInvitationSummaryResponse;
};
export type GetUserInvitationsResponse = GetUserInvitationsResponses[keyof GetUserInvitationsResponses];
export type GetAdminEmailSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/email-settings';
};
export type GetAdminEmailSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminEmailSettingsError = GetAdminEmailSettingsErrors[keyof GetAdminEmailSettingsErrors];
export type GetAdminEmailSettingsResponses = {
    200: AdminEmailSettings;
};
export type GetAdminEmailSettingsResponse = GetAdminEmailSettingsResponses[keyof GetAdminEmailSettingsResponses];
export type UpdateAdminEmailSettingsData = {
    body: AdminEmailSettingsRequestWritable;
    path?: never;
    query?: never;
    url: '/api/admin/email-settings';
};
export type UpdateAdminEmailSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type UpdateAdminEmailSettingsError = UpdateAdminEmailSettingsErrors[keyof UpdateAdminEmailSettingsErrors];
export type UpdateAdminEmailSettingsResponses = {
    200: AdminEmailSettings;
};
export type UpdateAdminEmailSettingsResponse = UpdateAdminEmailSettingsResponses[keyof UpdateAdminEmailSettingsResponses];
export type SendAdminEmailTestData = {
    body: AdminEmailTestRequest;
    path?: never;
    query?: never;
    url: '/api/admin/email-settings/test';
};
export type SendAdminEmailTestErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
    502: ManagementError;
};
export type SendAdminEmailTestError = SendAdminEmailTestErrors[keyof SendAdminEmailTestErrors];
export type SendAdminEmailTestResponses = {
    204: void;
};
export type SendAdminEmailTestResponse = SendAdminEmailTestResponses[keyof SendAdminEmailTestResponses];
export type GetAdminNetworkSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/network-settings';
};
export type GetAdminNetworkSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminNetworkSettingsError = GetAdminNetworkSettingsErrors[keyof GetAdminNetworkSettingsErrors];
export type GetAdminNetworkSettingsResponses = {
    200: AdminNetworkSettings;
};
export type GetAdminNetworkSettingsResponse = GetAdminNetworkSettingsResponses[keyof GetAdminNetworkSettingsResponses];
export type UpdateAdminNetworkSettingsData = {
    body: AdminNetworkSettingsRequestWritable;
    path?: never;
    query?: never;
    url: '/api/admin/network-settings';
};
export type UpdateAdminNetworkSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type UpdateAdminNetworkSettingsError = UpdateAdminNetworkSettingsErrors[keyof UpdateAdminNetworkSettingsErrors];
export type UpdateAdminNetworkSettingsResponses = {
    200: AdminNetworkSettings;
};
export type UpdateAdminNetworkSettingsResponse = UpdateAdminNetworkSettingsResponses[keyof UpdateAdminNetworkSettingsResponses];
export type GetAdminPaymentSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/payment-settings';
};
export type GetAdminPaymentSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminPaymentSettingsError = GetAdminPaymentSettingsErrors[keyof GetAdminPaymentSettingsErrors];
export type GetAdminPaymentSettingsResponses = {
    200: AdminPaymentSettings;
};
export type GetAdminPaymentSettingsResponse = GetAdminPaymentSettingsResponses[keyof GetAdminPaymentSettingsResponses];
export type UpdateAdminPaymentSettingsData = {
    body: AdminPaymentSettingsRequestWritable;
    path?: never;
    query?: never;
    url: '/api/admin/payment-settings';
};
export type UpdateAdminPaymentSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type UpdateAdminPaymentSettingsError = UpdateAdminPaymentSettingsErrors[keyof UpdateAdminPaymentSettingsErrors];
export type UpdateAdminPaymentSettingsResponses = {
    200: AdminPaymentSettings;
};
export type UpdateAdminPaymentSettingsResponse = UpdateAdminPaymentSettingsResponses[keyof UpdateAdminPaymentSettingsResponses];
export type ListAdminOAuthProvidersData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/oauth/providers';
};
export type ListAdminOAuthProvidersErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminOAuthProvidersError = ListAdminOAuthProvidersErrors[keyof ListAdminOAuthProvidersErrors];
export type ListAdminOAuthProvidersResponses = {
    200: AdminOAuthProviderListResponse;
};
export type ListAdminOAuthProvidersResponse = ListAdminOAuthProvidersResponses[keyof ListAdminOAuthProvidersResponses];
export type BeginAdminOAuthAuthorizationData = {
    body: AdminOAuthAuthorizationRequest;
    path: {
        channel_id: number;
        credential_id: number;
    };
    query?: never;
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}/oauth-authorizations';
};
export type BeginAdminOAuthAuthorizationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    429: ManagementError;
    500: ManagementError;
};
export type BeginAdminOAuthAuthorizationError = BeginAdminOAuthAuthorizationErrors[keyof BeginAdminOAuthAuthorizationErrors];
export type BeginAdminOAuthAuthorizationResponses = {
    201: AdminOAuthAuthorizationResponse;
};
export type BeginAdminOAuthAuthorizationResponse = BeginAdminOAuthAuthorizationResponses[keyof BeginAdminOAuthAuthorizationResponses];
export type CompleteAdminOAuthManualCallbackData = {
    body: AdminOAuthManualCallbackRequestWritable;
    path?: never;
    query?: never;
    url: '/api/admin/oauth/authorizations/manual-callback';
};
export type CompleteAdminOAuthManualCallbackErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    410: ManagementError;
    500: ManagementError;
    502: ManagementError;
    503: ManagementError;
    504: ManagementError;
};
export type CompleteAdminOAuthManualCallbackError = CompleteAdminOAuthManualCallbackErrors[keyof CompleteAdminOAuthManualCallbackErrors];
export type CompleteAdminOAuthManualCallbackResponses = {
    200: AdminOAuthCompletionResponse;
};
export type CompleteAdminOAuthManualCallbackResponse = CompleteAdminOAuthManualCallbackResponses[keyof CompleteAdminOAuthManualCallbackResponses];
export type GetAdminBalanceAlertSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/balance-alert-settings';
};
export type GetAdminBalanceAlertSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminBalanceAlertSettingsError = GetAdminBalanceAlertSettingsErrors[keyof GetAdminBalanceAlertSettingsErrors];
export type GetAdminBalanceAlertSettingsResponses = {
    200: AdminBalanceAlertSettings;
};
export type GetAdminBalanceAlertSettingsResponse = GetAdminBalanceAlertSettingsResponses[keyof GetAdminBalanceAlertSettingsResponses];
export type UpdateAdminBalanceAlertSettingsData = {
    body: AdminBalanceAlertSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/balance-alert-settings';
};
export type UpdateAdminBalanceAlertSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type UpdateAdminBalanceAlertSettingsError = UpdateAdminBalanceAlertSettingsErrors[keyof UpdateAdminBalanceAlertSettingsErrors];
export type UpdateAdminBalanceAlertSettingsResponses = {
    200: AdminBalanceAlertSettings;
};
export type UpdateAdminBalanceAlertSettingsResponse = UpdateAdminBalanceAlertSettingsResponses[keyof UpdateAdminBalanceAlertSettingsResponses];
export type GetPublicSiteSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/site';
};
export type GetPublicSiteSettingsErrors = {
    500: ManagementError;
};
export type GetPublicSiteSettingsError = GetPublicSiteSettingsErrors[keyof GetPublicSiteSettingsErrors];
export type GetPublicSiteSettingsResponses = {
    200: PublicSiteSettings;
};
export type GetPublicSiteSettingsResponse = GetPublicSiteSettingsResponses[keyof GetPublicSiteSettingsResponses];
export type GetAdminSiteSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/site-settings';
};
export type GetAdminSiteSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminSiteSettingsError = GetAdminSiteSettingsErrors[keyof GetAdminSiteSettingsErrors];
export type GetAdminSiteSettingsResponses = {
    200: AdminSiteSettings;
};
export type GetAdminSiteSettingsResponse = GetAdminSiteSettingsResponses[keyof GetAdminSiteSettingsResponses];
export type UpdateAdminSiteSettingsData = {
    body: AdminSiteSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/site-settings';
};
export type UpdateAdminSiteSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type UpdateAdminSiteSettingsError = UpdateAdminSiteSettingsErrors[keyof UpdateAdminSiteSettingsErrors];
export type UpdateAdminSiteSettingsResponses = {
    200: AdminSiteSettings;
};
export type UpdateAdminSiteSettingsResponse = UpdateAdminSiteSettingsResponses[keyof UpdateAdminSiteSettingsResponses];
export type GetAdminDashboardData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/dashboard';
};
export type GetAdminDashboardErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminDashboardError = GetAdminDashboardErrors[keyof GetAdminDashboardErrors];
export type GetAdminDashboardResponses = {
    200: AdminDashboardResponse;
};
export type GetAdminDashboardResponse = GetAdminDashboardResponses[keyof GetAdminDashboardResponses];
export type ListAdminUsersData = {
    body?: never;
    path?: never;
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/admin/users';
};
export type ListAdminUsersErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminUsersError = ListAdminUsersErrors[keyof ListAdminUsersErrors];
export type ListAdminUsersResponses = {
    200: AdminUserListResponse;
};
export type ListAdminUsersResponse = ListAdminUsersResponses[keyof ListAdminUsersResponses];
export type CreateAdminUserData = {
    body: AdminUserCreateRequest;
    path?: never;
    query?: never;
    url: '/api/admin/users';
};
export type CreateAdminUserErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateAdminUserError = CreateAdminUserErrors[keyof CreateAdminUserErrors];
export type CreateAdminUserResponses = {
    201: AdminUser;
};
export type CreateAdminUserResponse = CreateAdminUserResponses[keyof CreateAdminUserResponses];
export type DeleteAdminUserData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/users/{id}';
};
export type DeleteAdminUserErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type DeleteAdminUserError = DeleteAdminUserErrors[keyof DeleteAdminUserErrors];
export type DeleteAdminUserResponses = {
    204: void;
};
export type DeleteAdminUserResponse = DeleteAdminUserResponses[keyof DeleteAdminUserResponses];
export type GetAdminUserData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/users/{id}';
};
export type GetAdminUserErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetAdminUserError = GetAdminUserErrors[keyof GetAdminUserErrors];
export type GetAdminUserResponses = {
    200: AdminUser;
};
export type GetAdminUserResponse = GetAdminUserResponses[keyof GetAdminUserResponses];
export type UpdateAdminUserData = {
    body: AdminUserUpdateRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/users/{id}';
};
export type UpdateAdminUserErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminUserError = UpdateAdminUserErrors[keyof UpdateAdminUserErrors];
export type UpdateAdminUserResponses = {
    200: AdminUser;
};
export type UpdateAdminUserResponse = UpdateAdminUserResponses[keyof UpdateAdminUserResponses];
export type ListAdminWalletEntriesData = {
    body?: never;
    path: {
        id: number;
    };
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/admin/users/{id}/wallet/entries';
};
export type ListAdminWalletEntriesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListAdminWalletEntriesError = ListAdminWalletEntriesErrors[keyof ListAdminWalletEntriesErrors];
export type ListAdminWalletEntriesResponses = {
    200: AdminWalletListResponse;
};
export type ListAdminWalletEntriesResponse = ListAdminWalletEntriesResponses[keyof ListAdminWalletEntriesResponses];
export type AdjustAdminWalletData = {
    body: AdminWalletAdjustmentRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/users/{id}/wallet/adjustments';
};
export type AdjustAdminWalletErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type AdjustAdminWalletError = AdjustAdminWalletErrors[keyof AdjustAdminWalletErrors];
export type AdjustAdminWalletResponses = {
    200: AdminWalletEntry;
    201: AdminWalletEntry;
};
export type AdjustAdminWalletResponse = AdjustAdminWalletResponses[keyof AdjustAdminWalletResponses];
export type ListAdminGroupsData = {
    body?: never;
    path?: never;
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/admin/groups';
};
export type ListAdminGroupsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminGroupsError = ListAdminGroupsErrors[keyof ListAdminGroupsErrors];
export type ListAdminGroupsResponses = {
    200: AdminGroupListResponse;
};
export type ListAdminGroupsResponse = ListAdminGroupsResponses[keyof ListAdminGroupsResponses];
export type CreateAdminGroupData = {
    body: AdminGroupWriteRequest;
    path?: never;
    query?: never;
    url: '/api/admin/groups';
};
export type CreateAdminGroupErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateAdminGroupError = CreateAdminGroupErrors[keyof CreateAdminGroupErrors];
export type CreateAdminGroupResponses = {
    201: AdminGroup;
};
export type CreateAdminGroupResponse = CreateAdminGroupResponses[keyof CreateAdminGroupResponses];
export type DeleteAdminGroupData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/groups/{id}';
};
export type DeleteAdminGroupErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type DeleteAdminGroupError = DeleteAdminGroupErrors[keyof DeleteAdminGroupErrors];
export type DeleteAdminGroupResponses = {
    204: void;
};
export type DeleteAdminGroupResponse = DeleteAdminGroupResponses[keyof DeleteAdminGroupResponses];
export type GetAdminGroupData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/groups/{id}';
};
export type GetAdminGroupErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetAdminGroupError = GetAdminGroupErrors[keyof GetAdminGroupErrors];
export type GetAdminGroupResponses = {
    200: AdminGroup;
};
export type GetAdminGroupResponse = GetAdminGroupResponses[keyof GetAdminGroupResponses];
export type UpdateAdminGroupData = {
    body: AdminGroupWriteRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/groups/{id}';
};
export type UpdateAdminGroupErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminGroupError = UpdateAdminGroupErrors[keyof UpdateAdminGroupErrors];
export type UpdateAdminGroupResponses = {
    200: AdminGroup;
};
export type UpdateAdminGroupResponse = UpdateAdminGroupResponses[keyof UpdateAdminGroupResponses];
export type ListAdminRoutesData = {
    body?: never;
    path?: never;
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/admin/routes';
};
export type ListAdminRoutesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminRoutesError = ListAdminRoutesErrors[keyof ListAdminRoutesErrors];
export type ListAdminRoutesResponses = {
    200: AdminRouteListResponse;
};
export type ListAdminRoutesResponse = ListAdminRoutesResponses[keyof ListAdminRoutesResponses];
export type CreateAdminRouteData = {
    body: AdminRouteWriteRequest;
    path?: never;
    query?: never;
    url: '/api/admin/routes';
};
export type CreateAdminRouteErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateAdminRouteError = CreateAdminRouteErrors[keyof CreateAdminRouteErrors];
export type CreateAdminRouteResponses = {
    201: AdminRoute;
};
export type CreateAdminRouteResponse = CreateAdminRouteResponses[keyof CreateAdminRouteResponses];
export type DeleteAdminRouteData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/routes/{id}';
};
export type DeleteAdminRouteErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type DeleteAdminRouteError = DeleteAdminRouteErrors[keyof DeleteAdminRouteErrors];
export type DeleteAdminRouteResponses = {
    204: void;
};
export type DeleteAdminRouteResponse = DeleteAdminRouteResponses[keyof DeleteAdminRouteResponses];
export type GetAdminRouteData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/routes/{id}';
};
export type GetAdminRouteErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetAdminRouteError = GetAdminRouteErrors[keyof GetAdminRouteErrors];
export type GetAdminRouteResponses = {
    200: AdminRoute;
};
export type GetAdminRouteResponse = GetAdminRouteResponses[keyof GetAdminRouteResponses];
export type UpdateAdminRouteData = {
    body: AdminRouteWriteRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/routes/{id}';
};
export type UpdateAdminRouteErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminRouteError = UpdateAdminRouteErrors[keyof UpdateAdminRouteErrors];
export type UpdateAdminRouteResponses = {
    200: AdminRoute;
};
export type UpdateAdminRouteResponse = UpdateAdminRouteResponses[keyof UpdateAdminRouteResponses];
export type ListModelsData = {
    body?: never;
    path?: never;
    query?: {
        q?: string;
        billing_mode?: ModelCatalogBillingMode;
        provider?: Array<string>;
        input_modality?: Array<ModelCatalogModality>;
        output_modality?: Array<ModelCatalogModality>;
        capability?: Array<ModelCatalogCapability>;
        protocol?: Array<ModelCatalogProtocol>;
        after?: string;
        limit?: number;
    };
    url: '/api/models';
};
export type ListModelsErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type ListModelsError = ListModelsErrors[keyof ListModelsErrors];
export type ListModelsResponses = {
    200: ModelCatalogListResponse;
};
export type ListModelsResponse = ListModelsResponses[keyof ListModelsResponses];
export type ListModelProvidersData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/model-providers';
};
export type ListModelProvidersErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type ListModelProvidersError = ListModelProvidersErrors[keyof ListModelProvidersErrors];
export type ListModelProvidersResponses = {
    200: ModelCatalogProviderListResponse;
};
export type ListModelProvidersResponse = ListModelProvidersResponses[keyof ListModelProvidersResponses];
export type ListAdminModelsData = {
    body?: never;
    path?: never;
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/admin/models';
};
export type ListAdminModelsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminModelsError = ListAdminModelsErrors[keyof ListAdminModelsErrors];
export type ListAdminModelsResponses = {
    200: AdminModelListResponse;
};
export type ListAdminModelsResponse = ListAdminModelsResponses[keyof ListAdminModelsResponses];
export type CreateAdminModelData = {
    body: AdminModelCreateRequest;
    path?: never;
    query?: never;
    url: '/api/admin/models';
};
export type CreateAdminModelErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateAdminModelError = CreateAdminModelErrors[keyof CreateAdminModelErrors];
export type CreateAdminModelResponses = {
    201: AdminModel;
};
export type CreateAdminModelResponse = CreateAdminModelResponses[keyof CreateAdminModelResponses];
export type DeleteAdminModelData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/models/{id}';
};
export type DeleteAdminModelErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type DeleteAdminModelError = DeleteAdminModelErrors[keyof DeleteAdminModelErrors];
export type DeleteAdminModelResponses = {
    204: void;
};
export type DeleteAdminModelResponse = DeleteAdminModelResponses[keyof DeleteAdminModelResponses];
export type GetAdminModelData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/models/{id}';
};
export type GetAdminModelErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetAdminModelError = GetAdminModelErrors[keyof GetAdminModelErrors];
export type GetAdminModelResponses = {
    200: AdminModel;
};
export type GetAdminModelResponse = GetAdminModelResponses[keyof GetAdminModelResponses];
export type UpdateAdminModelData = {
    body: AdminModelUpdateRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/models/{id}';
};
export type UpdateAdminModelErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type UpdateAdminModelError = UpdateAdminModelErrors[keyof UpdateAdminModelErrors];
export type UpdateAdminModelResponses = {
    200: AdminModel;
};
export type UpdateAdminModelResponse = UpdateAdminModelResponses[keyof UpdateAdminModelResponses];
export type ListMissingAdminModelsData = {
    body?: never;
    path?: never;
    query?: {
        after?: string;
        limit?: number;
    };
    url: '/api/admin/models/missing';
};
export type ListMissingAdminModelsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListMissingAdminModelsError = ListMissingAdminModelsErrors[keyof ListMissingAdminModelsErrors];
export type ListMissingAdminModelsResponses = {
    200: AdminMissingModelListResponse;
};
export type ListMissingAdminModelsResponse = ListMissingAdminModelsResponses[keyof ListMissingAdminModelsResponses];
export type ImportMissingAdminModelsData = {
    body: AdminMissingModelImportRequest;
    path?: never;
    query?: never;
    url: '/api/admin/models/missing';
};
export type ImportMissingAdminModelsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ImportMissingAdminModelsError = ImportMissingAdminModelsErrors[keyof ImportMissingAdminModelsErrors];
export type ImportMissingAdminModelsResponses = {
    201: AdminMissingModelImportResponse;
};
export type ImportMissingAdminModelsResponse = ImportMissingAdminModelsResponses[keyof ImportMissingAdminModelsResponses];
export type CreateAdminModelSyncPreviewData = {
    body: AdminModelSyncPreviewRequest;
    path?: never;
    query?: never;
    url: '/api/admin/models/sync-previews';
};
export type CreateAdminModelSyncPreviewErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    422: ManagementError;
    500: ManagementError;
    502: ManagementError;
    504: ManagementError;
};
export type CreateAdminModelSyncPreviewError = CreateAdminModelSyncPreviewErrors[keyof CreateAdminModelSyncPreviewErrors];
export type CreateAdminModelSyncPreviewResponses = {
    201: AdminModelSyncPreview;
};
export type CreateAdminModelSyncPreviewResponse = CreateAdminModelSyncPreviewResponses[keyof CreateAdminModelSyncPreviewResponses];
export type ApplyAdminModelSyncPreviewData = {
    body: AdminModelSyncApplyRequest;
    path: {
        preview_id: string;
    };
    query?: never;
    url: '/api/admin/models/sync-previews/{preview_id}/apply';
};
export type ApplyAdminModelSyncPreviewErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    410: ManagementError;
    500: ManagementError;
};
export type ApplyAdminModelSyncPreviewError = ApplyAdminModelSyncPreviewErrors[keyof ApplyAdminModelSyncPreviewErrors];
export type ApplyAdminModelSyncPreviewResponses = {
    200: AdminModelSyncApplyResponse;
};
export type ApplyAdminModelSyncPreviewResponse = ApplyAdminModelSyncPreviewResponses[keyof ApplyAdminModelSyncPreviewResponses];
export type ListAdminModelPricesData = {
    body?: never;
    path?: never;
    query?: {
        after?: string;
        limit?: number;
    };
    url: '/api/admin/model-prices';
};
export type ListAdminModelPricesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminModelPricesError = ListAdminModelPricesErrors[keyof ListAdminModelPricesErrors];
export type ListAdminModelPricesResponses = {
    200: AdminModelPriceListResponse;
};
export type ListAdminModelPricesResponse = ListAdminModelPricesResponses[keyof ListAdminModelPricesResponses];
export type PreviewAdminModelPricesData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/model-prices/models-dev-preview';
};
export type PreviewAdminModelPricesErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
    502: ManagementError;
    504: ManagementError;
};
export type PreviewAdminModelPricesError = PreviewAdminModelPricesErrors[keyof PreviewAdminModelPricesErrors];
export type PreviewAdminModelPricesResponses = {
    200: AdminModelPriceSourcePreview;
};
export type PreviewAdminModelPricesResponse = PreviewAdminModelPricesResponses[keyof PreviewAdminModelPricesResponses];
export type PreviewAdminLiteLlmModelPricesData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/model-prices/litellm-preview';
};
export type PreviewAdminLiteLlmModelPricesErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
    502: ManagementError;
    504: ManagementError;
};
export type PreviewAdminLiteLlmModelPricesError = PreviewAdminLiteLlmModelPricesErrors[keyof PreviewAdminLiteLlmModelPricesErrors];
export type PreviewAdminLiteLlmModelPricesResponses = {
    200: AdminModelPriceSourcePreview;
};
export type PreviewAdminLiteLlmModelPricesResponse = PreviewAdminLiteLlmModelPricesResponses[keyof PreviewAdminLiteLlmModelPricesResponses];
export type PreviewAdminModelPriceExpressionData = {
    body: AdminModelPriceExpressionPreviewRequest;
    path?: never;
    query?: never;
    url: '/api/admin/model-prices/expression-preview';
};
export type PreviewAdminModelPriceExpressionErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    422: ManagementError;
    500: ManagementError;
};
export type PreviewAdminModelPriceExpressionError = PreviewAdminModelPriceExpressionErrors[keyof PreviewAdminModelPriceExpressionErrors];
export type PreviewAdminModelPriceExpressionResponses = {
    200: AdminModelPriceExpressionPreviewResponse;
};
export type PreviewAdminModelPriceExpressionResponse = PreviewAdminModelPriceExpressionResponses[keyof PreviewAdminModelPriceExpressionResponses];
export type ApplyAdminModelPricesData = {
    body: AdminModelPriceBatchRequest;
    path?: never;
    query?: never;
    url: '/api/admin/model-prices/batch';
};
export type ApplyAdminModelPricesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ApplyAdminModelPricesError = ApplyAdminModelPricesErrors[keyof ApplyAdminModelPricesErrors];
export type ApplyAdminModelPricesResponses = {
    200: AdminModelPriceBatchResponse;
};
export type ApplyAdminModelPricesResponse = ApplyAdminModelPricesResponses[keyof ApplyAdminModelPricesResponses];
export type CreatePlaygroundShareData = {
    body: PlaygroundShareCreateRequest;
    path?: never;
    query?: never;
    url: '/api/playground/shares';
};
export type CreatePlaygroundShareErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreatePlaygroundShareError = CreatePlaygroundShareErrors[keyof CreatePlaygroundShareErrors];
export type CreatePlaygroundShareResponses = {
    200: PlaygroundShareCreateResponse;
};
export type CreatePlaygroundShareResponse = CreatePlaygroundShareResponses[keyof CreatePlaygroundShareResponses];
export type RevokePlaygroundShareData = {
    body?: never;
    path: {
        token: string;
    };
    query?: never;
    url: '/api/playground/shares/{token}';
};
export type RevokePlaygroundShareErrors = {
    401: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type RevokePlaygroundShareError = RevokePlaygroundShareErrors[keyof RevokePlaygroundShareErrors];
export type RevokePlaygroundShareResponses = {
    204: void;
};
export type RevokePlaygroundShareResponse = RevokePlaygroundShareResponses[keyof RevokePlaygroundShareResponses];
export type GetPlaygroundShareData = {
    body?: never;
    path: {
        token: string;
    };
    query?: never;
    url: '/api/playground/shares/{token}';
};
export type GetPlaygroundShareErrors = {
    404: ManagementError;
    500: ManagementError;
};
export type GetPlaygroundShareError = GetPlaygroundShareErrors[keyof GetPlaygroundShareErrors];
export type GetPlaygroundShareResponses = {
    200: PlaygroundShareReadResponse;
};
export type GetPlaygroundShareResponse = GetPlaygroundShareResponses[keyof GetPlaygroundShareResponses];
export type ListPlaygroundConversationsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/playground/conversations';
};
export type ListPlaygroundConversationsErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type ListPlaygroundConversationsError = ListPlaygroundConversationsErrors[keyof ListPlaygroundConversationsErrors];
export type ListPlaygroundConversationsResponses = {
    200: PlaygroundConversationListResponse;
};
export type ListPlaygroundConversationsResponse = ListPlaygroundConversationsResponses[keyof ListPlaygroundConversationsResponses];
export type DeletePlaygroundConversationData = {
    body?: never;
    path: {
        conversation_id: string;
    };
    query?: never;
    url: '/api/playground/conversations/{conversation_id}';
};
export type DeletePlaygroundConversationErrors = {
    401: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type DeletePlaygroundConversationError = DeletePlaygroundConversationErrors[keyof DeletePlaygroundConversationErrors];
export type DeletePlaygroundConversationResponses = {
    204: void;
};
export type DeletePlaygroundConversationResponse = DeletePlaygroundConversationResponses[keyof DeletePlaygroundConversationResponses];
export type GetPlaygroundConversationData = {
    body?: never;
    path: {
        conversation_id: string;
    };
    query?: never;
    url: '/api/playground/conversations/{conversation_id}';
};
export type GetPlaygroundConversationErrors = {
    401: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetPlaygroundConversationError = GetPlaygroundConversationErrors[keyof GetPlaygroundConversationErrors];
export type GetPlaygroundConversationResponses = {
    200: PlaygroundConversationResponse;
};
export type GetPlaygroundConversationResponse = GetPlaygroundConversationResponses[keyof GetPlaygroundConversationResponses];
export type SavePlaygroundConversationData = {
    body: PlaygroundConversationSaveRequest;
    path: {
        conversation_id: string;
    };
    query?: never;
    url: '/api/playground/conversations/{conversation_id}';
};
export type SavePlaygroundConversationErrors = {
    400: ManagementError;
    401: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type SavePlaygroundConversationError = SavePlaygroundConversationErrors[keyof SavePlaygroundConversationErrors];
export type SavePlaygroundConversationResponses = {
    200: PlaygroundConversationResponse;
};
export type SavePlaygroundConversationResponse = SavePlaygroundConversationResponses[keyof SavePlaygroundConversationResponses];
export type ListUserTokensData = {
    body?: never;
    path?: never;
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/tokens';
};
export type ListUserTokensErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type ListUserTokensError = ListUserTokensErrors[keyof ListUserTokensErrors];
export type ListUserTokensResponses = {
    200: UserTokenListResponse;
};
export type ListUserTokensResponse = ListUserTokensResponses[keyof ListUserTokensResponses];
export type CreateUserTokenData = {
    body: UserTokenWriteRequest;
    path?: never;
    query?: never;
    url: '/api/tokens';
};
export type CreateUserTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateUserTokenError = CreateUserTokenErrors[keyof CreateUserTokenErrors];
export type CreateUserTokenResponses = {
    201: IssuedUserToken;
};
export type CreateUserTokenResponse = CreateUserTokenResponses[keyof CreateUserTokenResponses];
export type CreatePlaygroundTokenData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/playground/token';
};
export type CreatePlaygroundTokenErrors = {
    401: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreatePlaygroundTokenError = CreatePlaygroundTokenErrors[keyof CreatePlaygroundTokenErrors];
export type CreatePlaygroundTokenResponses = {
    201: IssuedUserToken;
};
export type CreatePlaygroundTokenResponse = CreatePlaygroundTokenResponses[keyof CreatePlaygroundTokenResponses];
export type DeleteUserTokenData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/tokens/{id}';
};
export type DeleteUserTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type DeleteUserTokenError = DeleteUserTokenErrors[keyof DeleteUserTokenErrors];
export type DeleteUserTokenResponses = {
    204: void;
};
export type DeleteUserTokenResponse = DeleteUserTokenResponses[keyof DeleteUserTokenResponses];
export type GetUserTokenData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/tokens/{id}';
};
export type GetUserTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetUserTokenError = GetUserTokenErrors[keyof GetUserTokenErrors];
export type GetUserTokenResponses = {
    200: UserToken;
};
export type GetUserTokenResponse = GetUserTokenResponses[keyof GetUserTokenResponses];
export type UpdateUserTokenData = {
    body: UserTokenWriteRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/tokens/{id}';
};
export type UpdateUserTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type UpdateUserTokenError = UpdateUserTokenErrors[keyof UpdateUserTokenErrors];
export type UpdateUserTokenResponses = {
    200: UserToken;
};
export type UpdateUserTokenResponse = UpdateUserTokenResponses[keyof UpdateUserTokenResponses];
export type ListAdminTokensData = {
    body?: never;
    path?: never;
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/admin/tokens';
};
export type ListAdminTokensErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminTokensError = ListAdminTokensErrors[keyof ListAdminTokensErrors];
export type ListAdminTokensResponses = {
    200: AdminTokenListResponse;
};
export type ListAdminTokensResponse = ListAdminTokensResponses[keyof ListAdminTokensResponses];
export type CreateAdminTokenData = {
    body: AdminTokenWriteRequest;
    path?: never;
    query?: never;
    url: '/api/admin/tokens';
};
export type CreateAdminTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateAdminTokenError = CreateAdminTokenErrors[keyof CreateAdminTokenErrors];
export type CreateAdminTokenResponses = {
    201: IssuedAdminToken;
};
export type CreateAdminTokenResponse = CreateAdminTokenResponses[keyof CreateAdminTokenResponses];
export type DeleteAdminTokenData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/tokens/{id}';
};
export type DeleteAdminTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type DeleteAdminTokenError = DeleteAdminTokenErrors[keyof DeleteAdminTokenErrors];
export type DeleteAdminTokenResponses = {
    204: void;
};
export type DeleteAdminTokenResponse = DeleteAdminTokenResponses[keyof DeleteAdminTokenResponses];
export type GetAdminTokenData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/tokens/{id}';
};
export type GetAdminTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetAdminTokenError = GetAdminTokenErrors[keyof GetAdminTokenErrors];
export type GetAdminTokenResponses = {
    200: AdminToken;
};
export type GetAdminTokenResponse = GetAdminTokenResponses[keyof GetAdminTokenResponses];
export type UpdateAdminTokenData = {
    body: AdminTokenWriteRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/tokens/{id}';
};
export type UpdateAdminTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type UpdateAdminTokenError = UpdateAdminTokenErrors[keyof UpdateAdminTokenErrors];
export type UpdateAdminTokenResponses = {
    200: AdminToken;
};
export type UpdateAdminTokenResponse = UpdateAdminTokenResponses[keyof UpdateAdminTokenResponses];
export type ListAdminUsageLogsData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/admin/usage-logs';
};
export type ListAdminUsageLogsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminUsageLogsError = ListAdminUsageLogsErrors[keyof ListAdminUsageLogsErrors];
export type ListAdminUsageLogsResponses = {
    200: AdminUsageLogListResponse;
};
export type ListAdminUsageLogsResponse = ListAdminUsageLogsResponses[keyof ListAdminUsageLogsResponses];
export type ListUserUsageLogsData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/account/usage-logs';
};
export type ListUserUsageLogsErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type ListUserUsageLogsError = ListUserUsageLogsErrors[keyof ListUserUsageLogsErrors];
export type ListUserUsageLogsResponses = {
    200: UserUsageLogListResponse;
};
export type ListUserUsageLogsResponse = ListUserUsageLogsResponses[keyof ListUserUsageLogsResponses];
export type ListAdminChannelsData = {
    body?: never;
    path?: never;
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/admin/channels';
};
export type ListAdminChannelsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminChannelsError = ListAdminChannelsErrors[keyof ListAdminChannelsErrors];
export type ListAdminChannelsResponses = {
    200: AdminChannelListResponse;
};
export type ListAdminChannelsResponse = ListAdminChannelsResponses[keyof ListAdminChannelsResponses];
export type CreateAdminChannelData = {
    body: AdminChannelCreateRequestWritable;
    path?: never;
    query?: never;
    url: '/api/admin/channels';
};
export type CreateAdminChannelErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type CreateAdminChannelError = CreateAdminChannelErrors[keyof CreateAdminChannelErrors];
export type CreateAdminChannelResponses = {
    201: AdminChannel;
};
export type CreateAdminChannelResponse = CreateAdminChannelResponses[keyof CreateAdminChannelResponses];
export type DeleteAdminChannelData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/channels/{id}';
};
export type DeleteAdminChannelErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type DeleteAdminChannelError = DeleteAdminChannelErrors[keyof DeleteAdminChannelErrors];
export type DeleteAdminChannelResponses = {
    204: void;
};
export type DeleteAdminChannelResponse = DeleteAdminChannelResponses[keyof DeleteAdminChannelResponses];
export type GetAdminChannelData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/channels/{id}';
};
export type GetAdminChannelErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetAdminChannelError = GetAdminChannelErrors[keyof GetAdminChannelErrors];
export type GetAdminChannelResponses = {
    200: AdminChannel;
};
export type GetAdminChannelResponse = GetAdminChannelResponses[keyof GetAdminChannelResponses];
export type UpdateAdminChannelData = {
    body: AdminChannelUpdateRequestWritable;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/channels/{id}';
};
export type UpdateAdminChannelErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type UpdateAdminChannelError = UpdateAdminChannelErrors[keyof UpdateAdminChannelErrors];
export type UpdateAdminChannelResponses = {
    200: AdminChannel;
};
export type UpdateAdminChannelResponse = UpdateAdminChannelResponses[keyof UpdateAdminChannelResponses];
export type ProbeAdminChannelData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/channels/{id}/probe';
};
export type ProbeAdminChannelErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    503: ManagementError;
};
export type ProbeAdminChannelError = ProbeAdminChannelErrors[keyof ProbeAdminChannelErrors];
export type ProbeAdminChannelResponses = {
    200: AdminChannelProbeResponse;
};
export type ProbeAdminChannelResponse = ProbeAdminChannelResponses[keyof ProbeAdminChannelResponses];
export type ListAdminCredentialsData = {
    body?: never;
    path: {
        channel_id: number;
    };
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/admin/channels/{channel_id}/credentials';
};
export type ListAdminCredentialsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListAdminCredentialsError = ListAdminCredentialsErrors[keyof ListAdminCredentialsErrors];
export type ListAdminCredentialsResponses = {
    200: AdminCredentialListResponse;
};
export type ListAdminCredentialsResponse = ListAdminCredentialsResponses[keyof ListAdminCredentialsResponses];
export type CreateAdminCredentialData = {
    body: AdminCredentialCreateRequestWritable;
    path: {
        channel_id: number;
    };
    query?: never;
    url: '/api/admin/channels/{channel_id}/credentials';
};
export type CreateAdminCredentialErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type CreateAdminCredentialError = CreateAdminCredentialErrors[keyof CreateAdminCredentialErrors];
export type CreateAdminCredentialResponses = {
    201: AdminCredential;
};
export type CreateAdminCredentialResponse = CreateAdminCredentialResponses[keyof CreateAdminCredentialResponses];
export type DeleteAdminCredentialData = {
    body?: never;
    path: {
        channel_id: number;
        credential_id: number;
    };
    query?: never;
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}';
};
export type DeleteAdminCredentialErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type DeleteAdminCredentialError = DeleteAdminCredentialErrors[keyof DeleteAdminCredentialErrors];
export type DeleteAdminCredentialResponses = {
    204: void;
};
export type DeleteAdminCredentialResponse = DeleteAdminCredentialResponses[keyof DeleteAdminCredentialResponses];
export type GetAdminCredentialData = {
    body?: never;
    path: {
        channel_id: number;
        credential_id: number;
    };
    query?: never;
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}';
};
export type GetAdminCredentialErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetAdminCredentialError = GetAdminCredentialErrors[keyof GetAdminCredentialErrors];
export type GetAdminCredentialResponses = {
    200: AdminCredential;
};
export type GetAdminCredentialResponse = GetAdminCredentialResponses[keyof GetAdminCredentialResponses];
export type UpdateAdminCredentialData = {
    body: AdminCredentialUpdateRequestWritable;
    path: {
        channel_id: number;
        credential_id: number;
    };
    query?: never;
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}';
};
export type UpdateAdminCredentialErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type UpdateAdminCredentialError = UpdateAdminCredentialErrors[keyof UpdateAdminCredentialErrors];
export type UpdateAdminCredentialResponses = {
    200: AdminCredential;
};
export type UpdateAdminCredentialResponse = UpdateAdminCredentialResponses[keyof UpdateAdminCredentialResponses];
export type ListAdminCredentialProxiesData = {
    body?: never;
    path?: never;
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/admin/proxies';
};
export type ListAdminCredentialProxiesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminCredentialProxiesError = ListAdminCredentialProxiesErrors[keyof ListAdminCredentialProxiesErrors];
export type ListAdminCredentialProxiesResponses = {
    200: AdminCredentialProxyListResponse;
};
export type ListAdminCredentialProxiesResponse = ListAdminCredentialProxiesResponses[keyof ListAdminCredentialProxiesResponses];
export type CreateAdminCredentialProxyData = {
    body: AdminCredentialProxyWriteRequestWritable;
    path?: never;
    query?: never;
    url: '/api/admin/proxies';
};
export type CreateAdminCredentialProxyErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateAdminCredentialProxyError = CreateAdminCredentialProxyErrors[keyof CreateAdminCredentialProxyErrors];
export type CreateAdminCredentialProxyResponses = {
    201: AdminCredentialProxy;
};
export type CreateAdminCredentialProxyResponse = CreateAdminCredentialProxyResponses[keyof CreateAdminCredentialProxyResponses];
export type DeleteAdminCredentialProxyData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/proxies/{id}';
};
export type DeleteAdminCredentialProxyErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type DeleteAdminCredentialProxyError = DeleteAdminCredentialProxyErrors[keyof DeleteAdminCredentialProxyErrors];
export type DeleteAdminCredentialProxyResponses = {
    204: void;
};
export type DeleteAdminCredentialProxyResponse = DeleteAdminCredentialProxyResponses[keyof DeleteAdminCredentialProxyResponses];
export type GetAdminCredentialProxyData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/proxies/{id}';
};
export type GetAdminCredentialProxyErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetAdminCredentialProxyError = GetAdminCredentialProxyErrors[keyof GetAdminCredentialProxyErrors];
export type GetAdminCredentialProxyResponses = {
    200: AdminCredentialProxy;
};
export type GetAdminCredentialProxyResponse = GetAdminCredentialProxyResponses[keyof GetAdminCredentialProxyResponses];
export type UpdateAdminCredentialProxyData = {
    body: AdminCredentialProxyWriteRequestWritable;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/proxies/{id}';
};
export type UpdateAdminCredentialProxyErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminCredentialProxyError = UpdateAdminCredentialProxyErrors[keyof UpdateAdminCredentialProxyErrors];
export type UpdateAdminCredentialProxyResponses = {
    200: AdminCredentialProxy;
};
export type UpdateAdminCredentialProxyResponse = UpdateAdminCredentialProxyResponses[keyof UpdateAdminCredentialProxyResponses];
export type GetAdminDebugTraceSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/debug-trace-settings';
};
export type GetAdminDebugTraceSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminDebugTraceSettingsError = GetAdminDebugTraceSettingsErrors[keyof GetAdminDebugTraceSettingsErrors];
export type GetAdminDebugTraceSettingsResponses = {
    200: AdminDebugTraceSettings;
};
export type GetAdminDebugTraceSettingsResponse = GetAdminDebugTraceSettingsResponses[keyof GetAdminDebugTraceSettingsResponses];
export type UpdateAdminDebugTraceSettingsData = {
    body: AdminDebugTraceSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/debug-trace-settings';
};
export type UpdateAdminDebugTraceSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type UpdateAdminDebugTraceSettingsError = UpdateAdminDebugTraceSettingsErrors[keyof UpdateAdminDebugTraceSettingsErrors];
export type UpdateAdminDebugTraceSettingsResponses = {
    200: AdminDebugTraceSettings;
};
export type UpdateAdminDebugTraceSettingsResponse = UpdateAdminDebugTraceSettingsResponses[keyof UpdateAdminDebugTraceSettingsResponses];
export type ListAdminDebugTracesData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
        outcome?: string;
        model?: string;
        request_id?: string;
    };
    url: '/api/admin/debug-traces';
};
export type ListAdminDebugTracesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminDebugTracesError = ListAdminDebugTracesErrors[keyof ListAdminDebugTracesErrors];
export type ListAdminDebugTracesResponses = {
    200: AdminDebugTraceListResponse;
};
export type ListAdminDebugTracesResponse = ListAdminDebugTracesResponses[keyof ListAdminDebugTracesResponses];
export type GetAdminDebugTraceData = {
    body?: never;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/debug-traces/{id}';
};
export type GetAdminDebugTraceErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetAdminDebugTraceError = GetAdminDebugTraceErrors[keyof GetAdminDebugTraceErrors];
export type GetAdminDebugTraceResponses = {
    200: AdminDebugTraceDetailResponse;
};
export type GetAdminDebugTraceResponse = GetAdminDebugTraceResponses[keyof GetAdminDebugTraceResponses];
export type ReceivePaymentWebhookData = {
    body?: unknown;
    headers: {
        'stripe-signature': string;
    };
    path: {
        provider: string;
    };
    query?: never;
    url: '/api/payment/webhook/{provider}';
};
export type ReceivePaymentWebhookErrors = {
    400: unknown;
    405: unknown;
    409: unknown;
    500: unknown;
    503: unknown;
};
export type ReceivePaymentWebhookResponses = {
    200: unknown;
};
