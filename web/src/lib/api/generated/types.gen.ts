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
    client_simulation_profile?: null | ClientSimulationProfile;
    client_simulation_body_profile?: null | ClientSimulationBodyProfile;
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
    client_simulation_profile: null | ClientSimulationProfile;
    client_simulation_risk_accepted: boolean;
    client_simulation_body_profile: null | ClientSimulationBodyProfile;
    client_simulation_body_risk_accepted: boolean;
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
    client_simulation_profile: null | ClientSimulationProfile;
    client_simulation_risk_accepted: boolean;
    client_simulation_body_profile: null | ClientSimulationBodyProfile;
    client_simulation_body_risk_accepted: boolean;
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
    blocks_spark_shadow: boolean;
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
export type AdminCustomOAuth2Provider = {
    provider_key: string;
    display_name: string;
    client_id: string;
    authorization_endpoint_configured: boolean;
    token_endpoint_configured: boolean;
    userinfo_endpoint_configured: boolean;
    scope_configured: boolean;
    subject_field_configured: boolean;
    enabled: boolean;
    secret_configured: boolean;
    version: number;
};
export type AdminCustomOAuth2ProviderList = {
    providers: Array<AdminCustomOAuth2Provider>;
};
export type AdminCustomOAuth2ProviderRequest = {
    expected_version: number;
    display_name: string;
    client_id: string;
    authorization_endpoint: string;
    token_endpoint: string;
    userinfo_endpoint: string;
    scope: string;
    subject_field: string;
    enabled: boolean;
    clear_client_secret: boolean;
};
export type AdminDashboardChannelFlow = {
    protocol: AdminChannelProtocol;
    channel_id: number;
    channel_name: string;
    request_count: number;
};
export type AdminDashboardFailure = {
    kind: AdminDashboardFailureKind;
    request_count: number;
};
export type AdminDashboardFailureKind = 'invalid_request' | 'model_not_allowed' | 'insufficient_quota' | 'quota_limited' | 'concurrency_limited' | 'outcome_unknown' | 'upstream_rate_limited' | 'upstream_overloaded' | 'upstream_authentication' | 'upstream_quota' | 'upstream_model' | 'upstream_protocol' | 'upstream_server' | 'upstream_network' | 'internal';
export type AdminDashboardFlowPath = {
    user_id: number;
    group_id: number;
    group_name: string;
    channel_id: number;
    channel_name: string;
    model: string;
    request_count: number;
    quota_consumed: number;
};
export type AdminDashboardHourlyPoint = {
    period_start: number;
    period_end: number;
    request_count: number;
    quota_consumed: number;
};
export type AdminDashboardPerformance = {
    first_token_sample_count: number;
    average_first_token_ms?: number | null;
    slow_first_token_count: number;
    slow_first_token_threshold_ms: number;
    duration_sample_count: number;
    average_duration_ms?: number | null;
    slow_request_count: number;
    slow_request_threshold_ms: number;
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
    outcome_request_count: number;
    successful_request_count: number;
    failed_request_count: number;
    other_success_count: number;
    failures: Array<AdminDashboardFailure>;
    channel_flows: Array<AdminDashboardChannelFlow>;
    flow_request_count: number;
    flow_quota_consumed: number;
    flow_paths: Array<AdminDashboardFlowPath>;
    hourly: Array<AdminDashboardHourlyPoint>;
    performance: AdminDashboardPerformance;
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
    client_simulation_profile?: null | ClientSimulationProfile;
    client_simulation_result?: null | ClientSimulationResult;
    client_simulation_body_profile?: null | ClientSimulationBodyProfile;
    client_simulation_body_result?: null | ClientSimulationBodyPatchResult;
    request_method?: string | null;
    request_url?: string | null;
    response_status?: number | null;
    response_streamed: boolean;
};
export type AdminDebugTraceAttemptSnapshot = {
    candidate_index: number;
    request?: unknown;
    response?: unknown;
};
export type AdminDebugTraceDetailResponse = {
    trace: AdminDebugTrace;
    downstream_request?: null | AdminDebugTraceDownstreamRequest;
    attempts: Array<AdminDebugTraceAttempt>;
};
export type AdminDebugTraceDownstreamRequest = {
    method: string;
    path: string;
};
export type AdminDebugTraceListResponse = {
    traces: Array<AdminDebugTrace>;
    next_cursor: number | null;
};
export type AdminDebugTraceSettings = {
    enabled: boolean;
    sample_per_million: number;
    retention_hours: number;
    capture_headers: boolean;
    capture_bodies: boolean;
    max_body_bytes: number;
    version: number;
};
export type AdminDebugTraceSettingsRequest = {
    enabled: boolean;
    sample_per_million: number;
    retention_hours: number;
    capture_headers: boolean;
    capture_bodies: boolean;
    max_body_bytes: number;
};
export type AdminDebugTraceSnapshotRequest = {
    scope: AdminDebugTraceSnapshotScope;
};
export type AdminDebugTraceSnapshotScope = 'headers' | 'bodies';
export type AdminDebugTraceSnapshotsResponse = {
    scope: string;
    downstream?: unknown;
    attempts: Array<AdminDebugTraceAttemptSnapshot>;
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
export type AdminOAuthLoginProviderSettings = {
    enabled: boolean;
    client_id?: string | null;
    issuer_url?: string | null;
    client_secret_configured: boolean;
    callback_url?: string | null;
    version: number;
};
export type AdminOAuthLoginProviderSettingsRequest = {
    expected_version: number;
    enabled: boolean;
    client_id?: string | null;
    issuer_url?: string | null;
    client_secret?: string | null;
    clear_client_secret?: boolean;
};
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
    epay_refund_enabled: boolean;
    refund_auto_submit_enabled: boolean;
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
    epay_refund_enabled: boolean;
    refund_auto_submit_enabled: boolean;
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
export type AdminRefundApprovalStatus = 'pending' | 'approved' | 'rejected';
export type AdminRefundDecisionRequest = {
    reason: string | null;
};
export type AdminRefundListResponse = {
    entries: Array<AdminRefundRequest>;
    next_cursor: number | null;
};
export type AdminRefundManualCompletionRequest = {
    completion_key: string;
    expected_version: number;
    result: string;
    reference: string;
};
export type AdminRefundOrderKind = 'topup' | 'subscription';
export type AdminRefundRequest = {
    request_id: string;
    user_id: number;
    order_kind: AdminRefundOrderKind;
    order_key: string;
    provider: string;
    currency: string;
    original_amount_minor: number;
    refund_amount_minor: number;
    provider_refund_id: string | null;
    status: AdminRefundStatus;
    approval_status: AdminRefundApprovalStatus;
    approval_actor_id: number | null;
    approval_reason: string | null;
    version: number;
    created_at: number;
    updated_at: number;
};
export type AdminRefundStatus = 'requested' | 'submitted' | 'succeeded' | 'failed' | 'canceled' | 'manually_succeeded' | 'manually_failed';
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
    balance_display: BalanceDisplaySettings;
    version: number;
};
export type AdminSiteSettingsRequest = {
    site_name: string;
    public_base_url?: string | null;
    brand: AdminBrandSettingsRequest;
    balance_display: BalanceDisplaySettings;
    expected_version: number;
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
    price_provider: string;
    price_currency: string;
    price_amount_minor: number;
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
    organization_id: number | null;
    organization_team_id: number | null;
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
export type Announcement = {
    id: number;
    version: number;
    status: string;
    title_zh: string;
    title_en: string;
    body_zh: string;
    body_en: string;
    visible_from?: number | null;
    visible_until?: number | null;
    created_by: number;
    published_at?: number | null;
    revoked_at?: number | null;
    created_at: number;
    updated_at: number;
};
export type AnnouncementListResponse = {
    entries: Array<Announcement>;
};
export type AnnouncementMutationRequest = {
    expected_version: number;
};
export type AnnouncementUpdateRequest = {
    title_zh: string;
    title_en: string;
    body_zh: string;
    body_en: string;
    visible_from?: number | null;
    visible_until?: number | null;
    expected_version: number;
};
export type AnnouncementWriteRequest = {
    title_zh: string;
    title_en: string;
    body_zh: string;
    body_en: string;
    visible_from?: number | null;
    visible_until?: number | null;
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
export type BalanceDisplayMode = 'quota' | 'custom_unit';
export type BalanceDisplaySettings = {
    mode: BalanceDisplayMode;
    unit_name: string;
    unit_symbol: string;
    quota_units_per_display_unit: string;
    symbol_position: BalanceDisplaySymbolPosition;
    fraction_digits: number;
};
export type BalanceDisplaySymbolPosition = 'prefix' | 'suffix';
export type ClientSimulationBodyPatchResult = 'applied' | 'rejected';
export type ClientSimulationBodyProfile = 'anthropic_cli_system_date_v1';
export type ClientSimulationProfile = 'anthropic_cli_headers_v1';
export type ClientSimulationResult = 'not_applied' | 'applied' | 'failed';
export type ContractPriceCloseRequest = {
    effective_until: number;
    expected_version: number;
};
export type ContractPriceCreateRequest = {
    model: string;
    protocol: ContractPriceProtocol;
    prices: ContractPriceValues;
    effective_from: number;
    effective_until: number | null;
    expected_version: number | null;
};
export type ContractPriceListResponse = {
    prices: Array<ContractPriceResponse>;
};
export type ContractPriceProtocol = 'openai_chat' | 'openai_responses' | 'openai_embeddings' | 'openai_images' | 'openai_audio' | 'openai_speech' | 'jina_rerank' | 'cohere_rerank' | 'xai_video' | 'anthropic' | 'gemini';
export type ContractPriceResponse = {
    id: number;
    organization_id: number;
    model: string;
    protocol: ContractPriceProtocol;
    prices: ContractPriceValues;
    effective_from: number;
    effective_until: number | null;
    version: number;
    created_by_user_id: number;
    created_at: number;
    updated_at: number;
};
export type ContractPriceValues = {
    input: string;
    output: string;
    cache_read: string;
    cache_creation_5m: string;
    cache_creation_1h: string;
};
export type CreditAllocationListResponse = {
    allocations: Array<CreditAllocationResponse>;
    next_cursor: number | null;
};
export type CreditAllocationResponse = {
    id: number;
    organization_id: number;
    repayment_id: number;
    invoice_id: number;
    currency: string;
    amount_minor: number;
    allocated_at: number;
    invoice_settled: boolean;
    created_at: number;
};
export type CreditInvoiceListResponse = {
    invoices: Array<CreditInvoiceResponse>;
    next_cursor: number | null;
};
export type CreditInvoiceResponse = {
    id: number;
    organization_id: number;
    currency: string;
    amount_minor: number;
    term_version: number;
    period: CreditTermPeriodResponse;
    issued_at: number;
    due_at: number;
    status: CreditInvoiceStatusResponse;
    status_changed_at: number;
    settled_at: number | null;
    voided_at: number | null;
    created_at: number;
    updated_at: number;
};
export type CreditInvoiceStatusResponse = 'due' | 'overdue' | 'settled' | 'voided';
export type CreditRepaymentListResponse = {
    repayments: Array<CreditRepaymentResponse>;
    next_cursor: number | null;
};
export type CreditRepaymentResponse = {
    id: number;
    organization_id: number;
    currency: string;
    amount_minor: number;
    source_kind: CreditRepaymentSourceKindResponse;
    received_at: number;
    created_at: number;
};
export type CreditRepaymentSourceKindResponse = 'bank_transfer' | 'other_confirmed';
export type CreditTermCreateRequest = {
    currency: string;
    credit_limit: number;
    period: CreditTermPeriodRequest;
    effective_from: number;
    effective_until: number | null;
    expected_version: number | null;
};
export type CreditTermListResponse = {
    terms: Array<CreditTermResponse>;
    next_cursor: number | null;
};
export type CreditTermPeriodRequest = {
    days: number;
    kind: 'net_days';
} | {
    kind: 'end_of_month';
};
export type CreditTermPeriodResponse = {
    days: number;
    kind: 'net_days';
} | {
    kind: 'end_of_month';
};
export type CreditTermResponse = {
    id: number;
    organization_id: number;
    currency: string;
    credit_limit: number;
    period: CreditTermPeriodResponse;
    status: CreditTermStatusResponse;
    effective_from: number;
    effective_until: number | null;
    version: number;
    created_by_user_id: number;
    created_at: number;
    updated_at: number;
};
export type CreditTermStatusRequest = {
    status: string;
    expected_version: number;
};
export type CreditTermStatusResponse = 'active' | 'paused' | 'closed';
export type IssuedAdminRedemptionBatch = {
    batch: AdminRedemptionBatch;
    codes: Array<string>;
};
export type IssuedAdminToken = {
    api_key: string;
    token: AdminToken;
};
export type IssuedOrganizationTokenResponse = {
    api_key: string;
    token: OrganizationTokenResponse;
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
    code: 'invalid_request' | 'invalid_credentials' | 'two_factor_required' | 'two_factor_invalid' | 'password_login_disabled' | 'turnstile_rejected' | 'turnstile_unavailable' | 'invalid_session' | 'forbidden' | 'setup_conflict' | 'site_settings_conflict' | 'announcement_invalid_request' | 'announcement_not_found' | 'announcement_conflict' | 'user_not_found' | 'group_not_found' | 'token_not_found' | 'token_limit_reached' | 'token_outcome_unknown' | 'channel_not_found' | 'credential_not_found' | 'probe_unavailable' | 'oauth_provider_not_configured' | 'oauth_login_settings_conflict' | 'custom_oauth2_provider_not_found' | 'custom_oauth2_provider_conflict' | 'oauth_credential_provider_mismatch' | 'oauth_authorization_capacity_exceeded' | 'oauth_authorization_not_found' | 'oauth_authorization_expired' | 'oauth_authorization_denied' | 'oauth_upstream_timeout' | 'oauth_upstream_rejected' | 'oauth_upstream_invalid_response' | 'oauth_unavailable' | 'playground_share_not_found' | 'playground_share_limit_reached' | 'playground_conversation_not_found' | 'playground_conversation_limit_reached' | 'playground_conversation_conflict' | 'user_conflict' | 'organization_not_found' | 'organization_credit_invalid_request' | 'organization_credit_not_found' | 'organization_credit_interval_conflict' | 'organization_credit_conflict' | 'organization_credit_transition_invalid' | 'organization_approval_not_found' | 'organization_approval_conflict' | 'organization_approval_expired' | 'organization_contract_price_not_found' | 'organization_contract_price_interval_conflict' | 'organization_contract_price_conflict' | 'organization_contract_price_transition_invalid' | 'organization_plan_not_found' | 'organization_plan_conflict' | 'organization_sso_provider_not_found' | 'organization_sso_provider_conflict' | 'organization_sso_provider_transition_invalid' | 'organization_sso_identity_binding_rejected' | 'organization_sso_identity_binding_conflict' | 'organization_sso_recovery_rejected' | 'organization_sso_recovery_conflict' | 'organization_sso_recovery_unavailable' | 'organization_sso_domain_not_found' | 'organization_sso_domain_conflict' | 'organization_sso_domain_transition_invalid' | 'organization_sso_domain_verification_failed' | 'wallet_event_conflict' | 'wallet_insufficient_quota' | 'wallet_overflow' | 'wallet_outcome_unknown' | 'refund_not_found' | 'refund_conflict' | 'refund_unavailable' | 'refund_auto_submit_failed' | 'refund_outcome_unknown' | 'topup_unavailable' | 'topup_provider_rejected' | 'topup_order_conflict' | 'topup_order_outcome_unknown' | 'redemption_batch_not_found' | 'redemption_batch_conflict' | 'redemption_code_invalid' | 'redemption_batch_disabled' | 'redemption_code_expired' | 'redemption_code_already_used' | 'redemption_outcome_unknown' | 'subscription_plan_not_found' | 'subscription_plan_disabled' | 'subscription_not_found' | 'subscription_transition_invalid' | 'subscription_in_use' | 'subscription_conflict' | 'subscription_outcome_unknown' | 'registration_disabled' | 'registration_rate_limited' | 'registration_rejected' | 'invitation_rejected' | 'password_reset_rejected' | 'password_change_rejected' | 'two_factor_already_enabled' | 'two_factor_not_enabled' | 'email_not_configured' | 'email_delivery_failed' | 'group_conflict' | 'group_in_use' | 'route_not_found' | 'route_conflict' | 'route_invalid_reference' | 'debug_trace_not_found' | 'model_not_found' | 'model_conflict' | 'model_sync_channel_unavailable' | 'model_sync_unsupported_channel' | 'model_sync_upstream_timeout' | 'model_sync_upstream_rejected' | 'model_sync_invalid_response' | 'model_sync_candidate_limit_exceeded' | 'model_sync_preview_not_found' | 'model_sync_preview_expired' | 'model_sync_preview_already_applied' | 'model_sync_conflict' | 'model_price_conflict' | 'model_price_expression_invalid' | 'model_price_expression_preview_invalid' | 'model_price_expression_evaluation_failed' | 'model_price_source_timeout' | 'model_price_source_unavailable' | 'model_price_source_response_too_large' | 'model_price_source_invalid_response' | 'model_price_source_candidate_limit_exceeded' | 'internal_error';
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
export type NotificationChannel = 'email';
export type NotificationDeliveryState = 'queued' | 'accepted' | 'failed' | 'canceled';
export type NotificationKind = 'balance_alert' | 'subscription_balance_alert' | 'subscription_purchase';
export type OAuthLoginExchangeRequest = {
    ticket: string;
};
export type OAuthLoginStartResponse = {
    authorization_url: string;
};
export type OrganizationApprovalCursorQuery = {
    after?: number | null;
    limit?: number | null;
};
export type OrganizationApprovalDecision = {
    id: number;
    decision_key: string;
    kind: string;
    decided_by_user_id?: number | null;
    reason?: string | null;
    decided_at: number;
    created_at: number;
};
export type OrganizationApprovalDecisionKind = 'approved' | 'rejected';
export type OrganizationApprovalDecisionRequest = {
    decision_key: string;
    kind: OrganizationApprovalDecisionKind;
    reason?: string | null;
    decided_at?: number | null;
};
export type OrganizationApprovalNotification = {
    id: number;
    organization_id: number;
    request_id: number;
    decision_id: number;
    decision: OrganizationApprovalNotificationDecision;
    template_version: string;
    occurred_at: number;
    read_at?: number | null;
};
export type OrganizationApprovalNotificationDecision = 'approved' | 'rejected' | 'timed_out';
export type OrganizationApprovalNotificationListResponse = {
    entries: Array<OrganizationApprovalNotification>;
    next_cursor?: string | null;
    unread_count: number;
};
export type OrganizationApprovalNotificationMarkReadRequest = {
    notification_ids: Array<number>;
};
export type OrganizationApprovalNotificationMarkReadResponse = {
    marked_count: number;
    unread_count: number;
};
export type OrganizationApprovalRequest = {
    id: number;
    request_key: string;
    organization_id: number;
    resource_type: string;
    resource_summary: string;
    template_id: number;
    template_version: number;
    requested_by_user_id: number;
    expires_at: number;
    created_at: number;
    status: string;
    decision?: null | OrganizationApprovalDecision;
};
export type OrganizationApprovalRequestCreateRequest = {
    request_key: string;
    resource_type: OrganizationApprovalResourceType;
    resource_summary: string;
    template_id: number;
    template_version: number;
    expires_at: number;
};
export type OrganizationApprovalRequestListResponse = {
    requests: Array<OrganizationApprovalRequest>;
    next_after?: number | null;
};
export type OrganizationApprovalResourceType = 'member' | 'budget' | 'service_account' | 'contract_price' | 'funds';
export type OrganizationApprovalTemplate = {
    id: number;
    organization_id: number;
    template_key: string;
    resource_type: string;
    version: number;
    definition_digest: string;
    created_by_user_id: number;
    created_at: number;
};
export type OrganizationApprovalTemplateCreateRequest = {
    template_key: string;
    resource_type: OrganizationApprovalResourceType;
    definition_digest: string;
    expected_version?: number | null;
};
export type OrganizationApprovalTemplateListResponse = {
    templates: Array<OrganizationApprovalTemplate>;
    next_after?: number | null;
};
export type OrganizationAuditLogListResponse = {
    logs: Array<OrganizationAuditLogResponse>;
    next_cursor: number | null;
};
export type OrganizationAuditLogResponse = {
    id: number;
    actor_user_id: number;
    actor_username: string;
    actor_role: string | null;
    permission_code: string;
    operation: string;
    target_type: string;
    target_identifier: string | null;
    outcome: OrganizationAuditOutcomeResponse;
    platform_proxy: boolean;
    platform_reason: string | null;
    request_id: string;
    client_fingerprint: string | null;
    change_summary: string | null;
    created_at: number;
};
export type OrganizationAuditOutcomeResponse = 'succeeded' | 'denied' | 'failed';
export type OrganizationBudgetAllocationResponse = {
    policy_id: number;
    window_id: number;
    policy_version: number;
    window_version: number;
    reserved_quota: number;
};
export type OrganizationBudgetOverviewResponse = {
    policies: Array<OrganizationBudgetPolicyResponse>;
    windows: Array<OrganizationBudgetWindowResponse>;
};
export type OrganizationBudgetPeriodRequest = {
    kind: 'utc_day';
} | {
    kind: 'utc_week';
} | {
    kind: 'utc_month';
} | {
    seconds: number;
    kind: 'fixed';
};
export type OrganizationBudgetPeriodResponse = {
    kind: 'utc_day';
} | {
    kind: 'utc_week';
} | {
    kind: 'utc_month';
} | {
    seconds: number;
    kind: 'fixed';
};
export type OrganizationBudgetPolicyCreateRequest = {
    scope: OrganizationBudgetScopeRequest;
    period: OrganizationBudgetPeriodRequest;
    quota_limit: number | null;
    effective_from: number;
};
export type OrganizationBudgetPolicyResponse = {
    id: number;
    organization_id: number;
    scope: OrganizationBudgetScopeResponse;
    period: OrganizationBudgetPeriodResponse;
    quota_limit: number | null;
    effective_from: number;
    effective_until: number | null;
    version: number;
};
export type OrganizationBudgetReservationResponse = {
    reservation_id: string;
    organization_id: number;
    membership_id: number;
    team_id: number | null;
    user_id: number;
    status: OrganizationBudgetReservationStatusResponse;
    reserved_quota: number;
    actual_quota: number | null;
    organization: null | OrganizationBudgetAllocationResponse;
    team: null | OrganizationBudgetAllocationResponse;
    member: null | OrganizationBudgetAllocationResponse;
};
export type OrganizationBudgetReservationStatusResponse = 'reserved' | 'settlement_pending' | 'settled' | 'released';
export type OrganizationBudgetScopeRequest = {
    kind: 'organization';
} | {
    team_id: number;
    kind: 'team';
} | {
    membership_id: number;
    kind: 'member';
} | {
    department_id: number;
    kind: 'department';
};
export type OrganizationBudgetScopeResponse = {
    kind: 'organization';
} | {
    team_id: number;
    kind: 'team';
} | {
    membership_id: number;
    kind: 'member';
} | {
    department_id: number;
    kind: 'department';
};
export type OrganizationBudgetWindowResponse = {
    id: number;
    policy_id: number;
    organization_id: number;
    scope: OrganizationBudgetScopeResponse;
    policy_version: number;
    limit_snapshot: number;
    used_quota: number;
    frozen_quota: number;
    remaining_quota: number;
    window_starts_at: number;
    window_ends_at: number;
    version: number;
};
export type OrganizationCustomRoleChangeRequest = {
    name: string;
    description?: string | null;
    permissions: Array<string>;
    expected_version: number;
};
export type OrganizationCustomRoleCreateRequest = {
    name: string;
    description?: string | null;
    permissions: Array<string>;
};
export type OrganizationCustomRoleDisableRequest = {
    expected_version: number;
};
export type OrganizationCustomRoleListResponse = {
    roles: Array<OrganizationCustomRoleResponse>;
    next_cursor?: number | null;
};
export type OrganizationCustomRoleResponse = {
    role_key: string;
    name: string;
    description?: string | null;
    status: string;
    permissions: Array<string>;
    version: number;
    created_by_user_id: number;
    created_at: number;
    updated_at: number;
};
export type OrganizationDepartmentCreateRequest = {
    name: string;
    parent_department_id: number | null;
    sort_order: number;
};
export type OrganizationDepartmentDisableRequest = {
    expected_version: number;
};
export type OrganizationDepartmentListResponse = {
    departments: Array<OrganizationDepartmentResponse>;
    next_cursor?: string | null;
};
export type OrganizationDepartmentMembershipAddRequest = {
    department_id: number;
    is_primary: boolean;
    expected_membership_version: number | null;
};
export type OrganizationDepartmentMembershipListResponse = {
    department_memberships: Array<OrganizationDepartmentMembershipResponse>;
};
export type OrganizationDepartmentMembershipPrimaryRequest = {
    expected_membership_version: number;
};
export type OrganizationDepartmentMembershipRemoveRequest = {
    expected_version: number;
};
export type OrganizationDepartmentMembershipResponse = {
    id: number;
    organization_id: number;
    membership_id: number;
    department_id: number;
    is_primary: boolean;
    status: string;
    version: number;
    created_at: number;
    updated_at: number;
    removed_at?: number | null;
};
export type OrganizationDepartmentMoveRequest = {
    parent_department_id: number | null;
    expected_version: number;
};
export type OrganizationDepartmentResponse = {
    id: number;
    organization_id: number;
    department_key: string;
    parent_department_id?: number | null;
    name: string;
    sort_order: number;
    depth: number;
    status: string;
    version: number;
    created_at: number;
    updated_at: number;
    disabled_at?: number | null;
};
export type OrganizationEntitlementProjection = {
    organization_id: number;
    status: OrganizationEntitlementStatus;
    source: OrganizationEntitlementSource;
    team_management_enabled: boolean;
    sso_enabled: boolean;
    custom_roles_enabled: boolean;
    seat_limit: number;
    team_limit: number;
    organization_key_limit: number;
    audit_retention_days: number;
    valid_from: number;
    valid_until: number;
    grace_until: number | null;
    version: number;
    updated_at: number;
    applied_order_id: string | null;
    plan_id: string | null;
    plan_version: number | null;
};
export type OrganizationEntitlementResponse = {
    status: string;
    sso_enabled: boolean;
    team_management_enabled: boolean;
    custom_roles_enabled: boolean;
    seat_limit: number;
    active_seats: number;
    remaining_seats: number;
    team_limit: number;
    active_teams: number;
    remaining_teams: number;
    organization_key_limit: number;
    audit_retention_days: number;
    valid_until: number;
    grace_until?: number | null;
};
export type OrganizationEntitlementSource = 'platform_provisioning' | 'organization_plan_order' | 'other';
export type OrganizationEntitlementStatus = 'active' | 'grace' | 'read_only' | 'suspended' | 'closed';
export type OrganizationInvitationAcceptRequest = {
    token: string;
};
export type OrganizationInvitationAcceptResponse = {
    organization_id: number;
    membership_id: number;
    user_id: number;
    role: string;
    team_id?: number | null;
    status: string;
    version: number;
};
export type OrganizationInvitationBatchRequest = {
    batch_key: string;
    items: Array<OrganizationInvitationCreateRequest>;
};
export type OrganizationInvitationBatchResponse = {
    replayed: boolean;
    invitations: Array<OrganizationInvitationResponse>;
};
export type OrganizationInvitationCreateRequest = {
    email: string;
    role: string;
    team_id?: number | null;
    expires_at?: number | null;
};
export type OrganizationInvitationIssuedResponse = {
    invitation: OrganizationInvitationResponse;
    token: string;
};
export type OrganizationInvitationListResponse = {
    invitations: Array<OrganizationInvitationResponse>;
};
export type OrganizationInvitationPreviewItemResponse = {
    index: number;
    ready: boolean;
    issues: Array<string>;
};
export type OrganizationInvitationPreviewRequest = {
    items: Array<OrganizationInvitationCreateRequest>;
};
export type OrganizationInvitationPreviewResponse = {
    ready: boolean;
    available_seats: number;
    items: Array<OrganizationInvitationPreviewItemResponse>;
};
export type OrganizationInvitationResponse = {
    invitation_key: string;
    organization_id: number;
    role: string;
    team_id?: number | null;
    status: string;
    invited_by_user_id: number;
    accepted_by_user_id?: number | null;
    batch_key?: string | null;
    version: number;
    created_at: number;
    expires_at: number;
    finalized_at?: number | null;
};
export type OrganizationListResponse = {
    organizations: Array<OrganizationSummaryResponse>;
};
export type OrganizationMemberCustomRoleRequest = {
    role_key: string | null;
    expected_version: number;
};
export type OrganizationMemberCustomRoleResponse = {
    membership_id: number;
    custom_role_key: string | null;
    version: number;
};
export type OrganizationMembershipChangeRequest = {
    role: string;
    status: string;
    team_id?: number | null;
    expected_version: number;
};
export type OrganizationMembershipListResponse = {
    members: Array<OrganizationMembershipResponse>;
    next_cursor?: number | null;
};
export type OrganizationMembershipResponse = {
    id: number;
    organization_id: number;
    user_id: number;
    role: string;
    team_id?: number | null;
    custom_role_key: string | null;
    status: string;
    joined_at: number;
    suspended_at?: number | null;
    removed_at?: number | null;
    invited_by_user_id?: number | null;
    version: number;
};
export type OrganizationOverviewResponse = {
    id: number;
    name: string;
    slug: string;
    role: string;
    membership_id: number;
    permissions: Array<string>;
    entitlement: OrganizationEntitlementResponse;
};
export type OrganizationOwnershipTransferRequest = {
    target_membership_id: number;
    current_owner_version: number;
    target_expected_version: number;
};
export type OrganizationOwnershipTransferResponse = {
    previous_owner: OrganizationMembershipResponse;
    current_owner: OrganizationMembershipResponse;
};
export type OrganizationPlan = {
    plan_id: string;
    name: string;
    description?: string | null;
    status: OrganizationPlanStatus;
    team_management_enabled: boolean;
    sso_enabled: boolean;
    custom_roles_enabled: boolean;
    seat_limit: number;
    team_limit: number;
    organization_key_limit: number;
    audit_retention_days: number;
    validity_days: number;
    price_provider: string;
    price_currency: string;
    price_amount_minor: number;
    version: number;
    created_by_user_id: number;
    disabled_at?: number | null;
    created_at: number;
    updated_at: number;
};
export type OrganizationPlanCatalogResponse = {
    plans: Array<OrganizationPlan>;
    purchase_notice: string;
};
export type OrganizationPlanCreateRequest = {
    name: string;
    description?: string | null;
    team_management_enabled: boolean;
    sso_enabled: boolean;
    custom_roles_enabled: boolean;
    seat_limit: number;
    team_limit: number;
    organization_key_limit: number;
    audit_retention_days: number;
    validity_days: number;
    price_provider: string;
    price_currency: string;
    price_amount_minor: number;
};
export type OrganizationPlanDisableRequest = {
    expected_version: number;
};
export type OrganizationPlanListQuerySchema = {
    before?: number | null;
    limit?: number | null;
};
export type OrganizationPlanListResponse = {
    plans: Array<OrganizationPlan>;
    next_cursor?: number | null;
};
export type OrganizationPlanOrder = {
    order_id: string;
    organization_id: number;
    payer_user_id: number;
    plan_id: string;
    plan_version: number;
    provider: string;
    currency: string;
    amount_minor: number;
    kind: OrganizationPlanOrderKind;
    effective_at?: number | null;
    status: OrganizationPlanOrderStatus;
    version: number;
    payment_method?: string | null;
    expires_at?: number | null;
    paid_at?: number | null;
    closed_at?: number | null;
    created_at: number;
    updated_at: number;
    replayed: boolean;
};
export type OrganizationPlanOrderCreateRequest = {
    plan_id: string;
    idempotency_key: string;
    kind?: null | OrganizationPlanOrderKind;
    effective_at?: number | null;
};
export type OrganizationPlanOrderKind = 'initial' | 'renewal' | 'upgrade' | 'downgrade';
export type OrganizationPlanOrderPaymentRequest = {
    payment_method: string;
};
export type OrganizationPlanOrderPaymentResponse = {
    order: OrganizationPlanOrder;
    payment: OrganizationPlanPayment;
};
export type OrganizationPlanOrderStatus = 'created' | 'pending' | 'paid' | 'failed' | 'canceled' | 'expired';
export type OrganizationPlanPayment = {
    payment_intent_id: string | null;
    client_secret: string | null;
    redirect_url: string | null;
};
export type OrganizationPlanStatus = 'active' | 'disabled';
export type OrganizationProvisioningCreateRequest = {
    idempotency_key: string;
    organization_name: string;
    organization_slug: string;
    business_reason: string;
};
export type OrganizationProvisioningDecisionKind = 'approve' | 'reject';
export type OrganizationProvisioningDecisionRequest = {
    expected_version: number;
    decision: OrganizationProvisioningDecisionKind;
    reason?: string | null;
    entitlement?: null | OrganizationProvisioningEntitlement;
};
export type OrganizationProvisioningEntitlement = {
    seat_limit: number;
    team_limit: number;
    organization_key_limit: number;
    team_management_enabled: boolean;
    sso_enabled: boolean;
    custom_roles_enabled: boolean;
    audit_retention_days: number;
    valid_from: number;
    valid_until: number;
    grace_until?: number | null;
};
export type OrganizationProvisioningListQuerySchema = {
    before?: number | null;
    limit?: number | null;
    status?: null | OrganizationProvisioningStatus;
};
export type OrganizationProvisioningRequest = {
    id: number;
    request_key: string;
    idempotency_key: string;
    applicant_user_id: number;
    organization_name: string;
    organization_slug: string;
    business_reason: string;
    status: OrganizationProvisioningStatus;
    version: number;
    reviewer_user_id?: number | null;
    decision_reason?: string | null;
    organization_id?: number | null;
    created_at: number;
    updated_at: number;
    decided_at?: number | null;
    replayed: boolean;
};
export type OrganizationProvisioningRequestListResponse = {
    requests: Array<OrganizationProvisioningRequest>;
    next_before?: number | null;
};
export type OrganizationProvisioningStatus = 'pending' | 'approved' | 'rejected';
export type OrganizationScimAuditLogListResponse = {
    logs: Array<OrganizationScimAuditLogResponse>;
    nextCursor: number | null;
};
export type OrganizationScimAuditLogResponse = {
    id: number;
    tokenKey?: string | null;
    eventType: string;
    result: string;
    reason?: string | null;
    resourceKind?: string | null;
    resourceDigest?: string | null;
    resourceVersion?: number | null;
    requestFingerprint: string;
    retryAfterSeconds?: number | null;
    createdAt: number;
};
export type OrganizationServiceAccountBudgetBindingRequest = {
    policy_id: number;
    expected_version?: number | null;
};
export type OrganizationServiceAccountBudgetBindingResponse = {
    service_account_key: string;
    organization_id: number;
    policy_id: number;
    policy_version: number;
    scope: string;
    team_id?: number | null;
    version: number;
};
export type OrganizationServiceAccountCreateRequest = {
    name: string;
};
export type OrganizationServiceAccountIssuedResponse = {
    service_account: OrganizationServiceAccountResponse;
    key_key: string;
    key_prefix: string;
    secret: string;
};
export type OrganizationServiceAccountListResponse = {
    service_accounts: Array<OrganizationServiceAccountResponse>;
    next_cursor?: string | null;
};
export type OrganizationServiceAccountResponse = {
    service_account_key: string;
    organization_id: number;
    name: string;
    status: string;
    version: number;
    created_at: number;
    updated_at: number;
    revoked_at?: number | null;
};
export type OrganizationServiceAccountRotateRequest = {
    expected_version: number;
};
export type OrganizationServiceAccountRuntimeErrorResponse = {
    code: string;
    message: string;
};
export type OrganizationServiceAccountStatusRequest = {
    status: string;
    expected_version: number;
};
export type OrganizationSsoDiscoveryErrorResponse = {
    code: string;
    message: string;
};
export type OrganizationSsoDiscoveryRequest = {
    identifier: string;
};
export type OrganizationSsoDiscoveryResponse = {
    available: boolean;
};
export type OrganizationSsoDomain = {
    id: number;
    organization_id: number;
    provider_key: string;
    domain: string;
    status: string;
    proof_expires_at?: number | null;
    verified_at?: number | null;
    recheck_at?: number | null;
    revoked_at?: number | null;
    version: number;
};
export type OrganizationSsoDomainCreateRequest = {
    provider_key: string;
    domain: string;
    ttl_seconds: number;
};
export type OrganizationSsoDomainCreated = {
    domain: OrganizationSsoDomain;
    txt_record_name: string;
    txt_record_value: string;
};
export type OrganizationSsoDomainList = {
    domains: Array<OrganizationSsoDomain>;
};
export type OrganizationSsoDomainVerifyRequest = {
    expected_version: number;
};
export type OrganizationSsoIdentityBindingRequest = {
    method: string;
    credential?: unknown;
};
export type OrganizationSsoIdentityBindingResponse = {
    identity_id: number;
    organization_id: number;
    membership_id: number;
    status: string;
    binding_method: string;
    version: number;
};
export type OrganizationSsoLoginCallbackErrorResponse = {
    code: string;
    message: string;
};
export type OrganizationSsoLoginCallbackSuccessResponse = {
    ticket: string;
    expires_at: number;
};
export type OrganizationSsoLoginStartRequest = {
    provider_key: string;
    request_digest: string;
};
export type OrganizationSsoLoginStartResponse = {
    protocol: string;
    provider_version: number;
    state_digest: string;
    request_digest: string;
    expires_at: number;
    authorization_url?: string | null;
    saml_authn_request?: null | OrganizationSsoSamlAuthnRequestResponse;
};
export type OrganizationSsoPolicyRequest = {
    expected_version: number;
    policy: 'optional' | 'required';
};
export type OrganizationSsoPolicyResponse = {
    policy: 'optional' | 'required';
    recovery_ready: boolean;
    version: number;
};
export type OrganizationSsoProvider = {
    id: number;
    organization_id: number;
    provider_key: string;
    protocol: string;
    status: string;
    display_name: string;
    metadata_projection?: unknown;
    secret_configured: boolean;
    version: number;
    created_at: number;
    updated_at: number;
};
export type OrganizationSsoProviderList = {
    providers: Array<OrganizationSsoProvider>;
};
export type OrganizationSsoProviderRequest = {
    expected_version: number;
    protocol: string;
    status: string;
    display_name: string;
    metadata_projection?: unknown;
    clear_client_secret: boolean;
};
export type OrganizationSsoPublicLoginStartErrorResponse = {
    code: string;
    message: string;
};
export type OrganizationSsoPublicLoginStartRequest = {
    identifier: string;
};
export type OrganizationSsoPublicLoginStartResponse = {
    expires_at: number;
    authorization_url: string;
    protocol: 'oidc';
} | {
    expires_at: number;
    action: string;
    saml_request: string;
    relay_state: string;
    protocol: 'saml';
};
export type OrganizationSsoRecoveryRequest = {
    expected_version: number;
};
export type OrganizationSsoRecoveryResponse = {
    policy: 'optional' | 'required';
    recovery_ready: boolean;
    version: number;
};
export type OrganizationSsoSamlAuthnRequestFieldResponse = {
    name: string;
    value: string;
};
export type OrganizationSsoSamlAuthnRequestResponse = {
    action: string;
    fields: Array<OrganizationSsoSamlAuthnRequestFieldResponse>;
};
export type OrganizationSsoSessionExchangeErrorResponse = {
    code: string;
    message: string;
};
export type OrganizationSsoSessionExchangeRequest = {
    organization_id: number;
};
export type OrganizationSummaryResponse = {
    id: number;
    name: string;
    slug: string;
    role: string;
};
export type OrganizationTeamChangeRequest = {
    name: string;
    description?: string | null;
    expected_version: number;
};
export type OrganizationTeamCreateRequest = {
    name: string;
    description?: string | null;
};
export type OrganizationTeamDisableRequest = {
    expected_version: number;
};
export type OrganizationTeamListResponse = {
    teams: Array<OrganizationTeamResponse>;
    next_cursor?: number | null;
};
export type OrganizationTeamResponse = {
    id: number;
    organization_id: number;
    name: string;
    description?: string | null;
    status: string;
    version: number;
    created_at: number;
    updated_at: number;
    deleted_at?: number | null;
};
export type OrganizationTokenListResponse = {
    tokens: Array<OrganizationTokenResponse>;
    next_cursor: number | null;
};
export type OrganizationTokenResponse = {
    id: number;
    owner_user_id: number;
    team_id: number | null;
    key_prefix: string;
    name: string;
    status: OrganizationTokenStatusDto;
    remain_quota: number;
    unlimited_quota: boolean;
    used_quota: number;
    expired_at: number | null;
    model_limits: Array<string> | null;
    allow_ips: Array<string> | null;
    rate_limit_5h: number | null;
    rate_limit_1d: number | null;
    rate_limit_7d: number | null;
    usage_5h: number;
    usage_1d: number;
    usage_7d: number;
    max_requests: number | null;
    used_requests: number;
    created_at: number;
    updated_at: number;
};
export type OrganizationTokenStatusDto = 'enabled' | 'disabled';
export type OrganizationTokenWriteRequest = {
    name: string;
    status: OrganizationTokenStatusDto;
    remain_quota: number;
    unlimited_quota: boolean;
    expired_at: number | null;
    model_limits: Array<string> | null;
    allow_ips: Array<string> | null;
    rate_limit_5h: number | null;
    rate_limit_1d: number | null;
    rate_limit_7d: number | null;
    max_requests: number | null;
};
export type OrganizationTopupOrderCreateRequest = {
    idempotency_key: string;
    provider: string;
    payment_method: string;
    amount_minor: number;
};
export type OrganizationTopupOrderResponse = {
    order_id: string;
    provider: string;
    payment_method: string;
    status: OrganizationTopupOrderStatusResponse;
    amount_minor: number;
    currency: string;
    quota_amount: number;
    version: number;
    created_at: number;
    replayed: boolean;
    payment?: null | OrganizationTopupPaymentSessionResponse;
};
export type OrganizationTopupOrderStatusResponse = 'created' | 'pending' | 'paid' | 'failed' | 'canceled' | 'expired';
export type OrganizationTopupPaymentSessionResponse = {
    payment_intent_id: string;
    readonly client_secret: string;
    kind: 'stripe';
} | {
    redirect_url: string;
    kind: 'redirect';
};
export type OrganizationUsageLogListResponse = {
    scope: string;
    logs: Array<AdminUsageLog>;
    summaries: Array<OrganizationUsageLogSummaryResponse>;
    next_cursor: number | null;
};
export type OrganizationUsageLogSummaryResponse = {
    model: string | null;
    protocol: null | UsageLogProtocol;
    operation: null | UsageLogOperation;
    request_count: number;
    input_tokens: number;
    output_tokens: number;
    reasoning_tokens: number;
    quota: number;
    duration_ms: number;
};
export type OrganizationWalletAdjustmentRequest = {
    event_id: string;
    quota_delta: number;
    reason: string;
};
export type OrganizationWalletAdjustmentResponse = {
    entry: OrganizationWalletEntryResponse;
    replayed: boolean;
};
export type OrganizationWalletEntryResponse = {
    id: number;
    event_id: string;
    actor_user_id: number | null;
    entry_type: OrganizationWalletEntryTypeResponse;
    quota_delta: number;
    balance_before: number;
    balance_after: number;
    source: string;
    reason: string | null;
    created_at: number;
};
export type OrganizationWalletEntryTypeResponse = 'opening_balance' | 'admin_adjustment' | 'topup' | 'usage_settlement';
export type OrganizationWalletListResponse = {
    entries: Array<OrganizationWalletEntryResponse>;
    next_cursor: number | null;
};
export type OrganizationWalletSummaryResponse = {
    organization_id: number;
    balance: number;
    used_quota: number;
    frozen_quota: number;
    version: number;
};
export type PasskeyAuthenticationOptionsRequest = {
    username: string;
};
export type PasskeyAuthenticationOptionsResponse = {
    options: unknown;
};
export type PasskeyAuthenticationVerifyRequest = {
    credential: unknown;
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
export type PlatformAuditLog = {
    id: number;
    operator_user_id: number;
    operator_username: string | null;
    permission_code: string;
    route: string;
    operation: string;
    resource: string;
    resource_id: string | null;
    outcome: PlatformAuditOutcome;
    before_value: {
        [key: string]: unknown;
    } | null;
    after_value: {
        [key: string]: unknown;
    } | null;
    audit_info: {
        [key: string]: unknown;
    } | null;
    request_id: string;
    created_at: number;
};
export type PlatformAuditLogListResponse = {
    logs: Array<PlatformAuditLog>;
    next_cursor: number | null;
};
export type PlatformAuditOutcome = 'succeeded' | 'denied' | 'failed';
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
    oauth_providers: Array<PublicOAuthLoginProvider>;
    turnstile_site_key?: string | null;
};
export type PublicBrandSettings = {
    logo_url?: string | null;
    tagline?: string | null;
    description?: string | null;
};
export type PublicOAuthLoginProvider = {
    id: string;
    display_name: string;
};
export type PublicSiteSettings = {
    site_name: string;
    public_base_url?: string | null;
    brand: PublicBrandSettings;
    balance_display: BalanceDisplaySettings;
    authentication: PublicAuthenticationCapabilities;
};
export type RefundReconciliationEntry = {
    request_id: string;
    user_id: number;
    organization_id: number | null;
    approval_actor_id: number;
    order_kind: AdminRefundOrderKind;
    order_key: string;
    provider: string;
    amount_delta_minor: number;
    currency: string;
    status: RefundReconciliationStatus;
    created_at: number;
};
export type RefundReconciliationListResponse = {
    entries: Array<RefundReconciliationEntry>;
    next_cursor: number | null;
};
export type RefundReconciliationStatus = 'succeeded';
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
export type ResourceTypeListResponse = {
    schemas: Array<string>;
    totalResults: number;
    Resources: Array<ResourceTypeResponse>;
    startIndex: number;
    itemsPerPage: number;
};
export type ResourceTypeResponse = {
    id: string;
    name: string;
    endpoint: string;
    schema: string;
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
export type ScimAuthenticationSchemeResponse = {
    type: string;
    name: string;
    description: string;
    specUri: string;
    documentationUri: string;
};
export type ScimBulkResponse = {
    supported: boolean;
    maxOperations: number;
    maxPayloadSize: number;
};
export type ScimErrorResponse = {
    schemas: Array<string>;
    detail: string;
    status: string;
};
export type ScimFilterResponse = {
    supported: boolean;
    maxResults: number;
};
export type ScimGroupMember = {
    value: string;
};
export type ScimGroupMeta = {
    version: string;
};
export type ScimGroupResponse = {
    schemas: Array<string>;
    id: string;
    displayName: string;
    members: Array<ScimGroupMember>;
    meta: ScimGroupMeta;
};
export type ScimGroupWriteRequest = {
    id: string;
    displayName: string;
    members?: Array<ScimGroupMember>;
    meta: ScimGroupMeta;
};
export type ScimSupportResponse = {
    supported: boolean;
};
export type ScimUserMeta = {
    version: string;
};
export type ScimUserResponse = {
    schemas: Array<string>;
    id: string;
    userName: string;
    displayName?: string | null;
    active: boolean;
    meta: ScimUserMeta;
};
export type ScimUserWriteRequest = {
    id: string;
    userName: string;
    displayName?: string | null;
    active: boolean;
    meta: ScimUserMeta;
};
export type ServiceProviderConfigResponse = {
    schemas: Array<string>;
    patch: ScimSupportResponse;
    bulk: ScimBulkResponse;
    filter: ScimFilterResponse;
    changePassword: ScimSupportResponse;
    sort: ScimSupportResponse;
    etag: ScimSupportResponse;
    authenticationSchemes: Array<ScimAuthenticationSchemeResponse>;
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
export type SubscriptionCatalogPlan = {
    plan_id: string;
    name: string;
    quota_amount: number;
    cycle: SubscriptionCycle;
    plan_version: number;
    price_provider: string;
    price_currency: string;
    price_amount_minor: number;
};
export type SubscriptionCatalogResponse = {
    plans: Array<SubscriptionCatalogPlan>;
};
export type SubscriptionCycle = 'daily' | 'weekly' | 'monthly' | 'yearly';
export type SubscriptionOrder = {
    order_id: string;
    plan_id: string;
    plan_version: number;
    price_provider: string;
    price_currency: string;
    price_amount_minor: number;
    quota_amount: number;
    status: SubscriptionOrderStatus;
    version: number;
    expires_at: number | null;
    paid_at: number | null;
    created_at: number;
    updated_at: number;
    replayed: boolean;
};
export type SubscriptionOrderCreateRequest = {
    idempotency_key: string;
    plan_id: string;
    plan_version: number;
    price_provider: string;
    price_currency: string;
    price_amount_minor: number;
};
export type SubscriptionOrderPaymentRequest = {
    payment_method: string;
};
export type SubscriptionOrderPaymentResponse = {
    order: SubscriptionOrder;
    payment: SubscriptionPaymentResponse;
};
export type SubscriptionOrderStatus = 'created' | 'pending' | 'paid' | 'failed' | 'canceled' | 'expired';
export type SubscriptionPaymentResponse = {
    payment_intent_id: string | null;
    client_secret: string | null;
    redirect_url: string | null;
};
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
export type UserNotification = {
    id: number;
    kind: NotificationKind;
    channel: NotificationChannel;
    template_version: string;
    occurred_at: number;
    delivery_state: NotificationDeliveryState;
    delivery_attempts: number;
    observed_quota?: number | null;
    threshold_quota?: number | null;
    subscription_id?: string | null;
    window_ends_at?: number | null;
    quota_amount?: number | null;
    quota_used?: number | null;
    threshold_percent?: number | null;
    read_at?: number | null;
};
export type UserNotificationListResponse = {
    entries: Array<UserNotification>;
    next_cursor?: string | null;
    unread_count: number;
};
export type UserNotificationMarkReadRequest = {
    notification_ids: Array<number>;
};
export type UserNotificationMarkReadResponse = {
    marked_count: number;
    unread_count: number;
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
export type UserPasskeyListResponse = {
    items: Array<UserPasskeyResponse>;
};
export type UserPasskeyRegistrationOptionsResponse = {
    options: unknown;
    challenge_digest: string;
};
export type UserPasskeyRegistrationVerifyRequest = {
    credential: unknown;
    display_name: string;
};
export type UserPasskeyRenameRequest = {
    display_name: string;
};
export type UserPasskeyResponse = {
    id: number;
    display_name: string;
    created_at: number;
    last_used_at: number | null;
    revoked_at: number | null;
};
export type UserPasskeyRevokeRequest = {
    current_password: string;
    totp_code?: string | null;
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
export type UserTwoFactorEnrollmentResponse = {
    enabled: boolean;
    secret: string;
    otpauth_uri: string;
    backup_codes: Array<string>;
};
export type UserTwoFactorPasswordRequest = {
    current_password: string;
};
export type UserTwoFactorStatusResponse = {
    enabled: boolean;
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
    client_simulation_profile: null | ClientSimulationProfile;
    client_simulation_risk_accepted: boolean;
    client_simulation_body_profile: null | ClientSimulationBodyProfile;
    client_simulation_body_risk_accepted: boolean;
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
    client_simulation_profile: null | ClientSimulationProfile;
    client_simulation_risk_accepted: boolean;
    client_simulation_body_profile: null | ClientSimulationBodyProfile;
    client_simulation_body_risk_accepted: boolean;
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
export type AdminCustomOAuth2ProviderRequestWritable = {
    expected_version: number;
    display_name: string;
    client_id: string;
    authorization_endpoint: string;
    token_endpoint: string;
    userinfo_endpoint: string;
    scope: string;
    subject_field: string;
    enabled: boolean;
    client_secret?: string | null;
    clear_client_secret: boolean;
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
    epay_refund_enabled: boolean;
    refund_auto_submit_enabled: boolean;
    epay_quota_per_cny: number;
};
export type LoginRequestWritable = {
    username: string;
    password: string;
    totp_code?: string | null;
    turnstile_token?: string | null;
};
export type OrganizationSsoDomainVerifyRequestWritable = {
    proof_token: string;
    expected_version: number;
};
export type OrganizationSsoIdentityBindingRequestWritable = {
    ticket: string;
    method: string;
    current_password?: string | null;
    totp_code?: string | null;
    credential?: unknown;
};
export type OrganizationSsoProviderRequestWritable = {
    expected_version: number;
    protocol: string;
    status: string;
    display_name: string;
    metadata_projection?: unknown;
    client_secret?: string | null;
    clear_client_secret: boolean;
};
export type OrganizationSsoRecoveryRequestWritable = {
    expected_version: number;
    current_password: string;
    totp_code?: string | null;
};
export type OrganizationSsoSessionExchangeRequestWritable = {
    organization_id: number;
    ticket: string;
};
export type OrganizationTopupOrderResponseWritable = {
    order_id: string;
    provider: string;
    payment_method: string;
    status: OrganizationTopupOrderStatusResponse;
    amount_minor: number;
    currency: string;
    quota_amount: number;
    version: number;
    created_at: number;
    replayed: boolean;
    payment?: null | OrganizationTopupPaymentSessionResponseWritable;
};
export type OrganizationTopupPaymentSessionResponseWritable = {
    payment_intent_id: string;
    kind: 'stripe';
} | {
    redirect_url: string;
    kind: 'redirect';
};
export type RegistrationEmailVerificationRequestWritable = {
    email: string;
    turnstile_token?: string | null;
};
export type RegistrationRequestWritable = {
    username: string;
    email?: string | null;
    password: string;
    verification_code?: string | null;
    invite_code?: string | null;
    turnstile_token?: string | null;
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
    body: LoginRequestWritable;
    path?: never;
    query?: never;
    url: '/api/auth/login';
};
export type LoginManagementSessionErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
    503: ManagementError;
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
export type ListPublicAnnouncementsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/announcements';
};
export type ListPublicAnnouncementsErrors = {
    500: ManagementError;
};
export type ListPublicAnnouncementsError = ListPublicAnnouncementsErrors[keyof ListPublicAnnouncementsErrors];
export type ListPublicAnnouncementsResponses = {
    200: AnnouncementListResponse;
};
export type ListPublicAnnouncementsResponse = ListPublicAnnouncementsResponses[keyof ListPublicAnnouncementsResponses];
export type ListAdminAnnouncementsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/announcements';
};
export type ListAdminAnnouncementsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminAnnouncementsError = ListAdminAnnouncementsErrors[keyof ListAdminAnnouncementsErrors];
export type ListAdminAnnouncementsResponses = {
    200: AnnouncementListResponse;
};
export type ListAdminAnnouncementsResponse = ListAdminAnnouncementsResponses[keyof ListAdminAnnouncementsResponses];
export type CreateAdminAnnouncementData = {
    body: AnnouncementWriteRequest;
    path?: never;
    query?: never;
    url: '/api/admin/announcements';
};
export type CreateAdminAnnouncementErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type CreateAdminAnnouncementError = CreateAdminAnnouncementErrors[keyof CreateAdminAnnouncementErrors];
export type CreateAdminAnnouncementResponses = {
    200: Announcement;
};
export type CreateAdminAnnouncementResponse = CreateAdminAnnouncementResponses[keyof CreateAdminAnnouncementResponses];
export type UpdateAdminAnnouncementData = {
    body: AnnouncementUpdateRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/announcements/{id}';
};
export type UpdateAdminAnnouncementErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminAnnouncementError = UpdateAdminAnnouncementErrors[keyof UpdateAdminAnnouncementErrors];
export type UpdateAdminAnnouncementResponses = {
    200: Announcement;
};
export type UpdateAdminAnnouncementResponse = UpdateAdminAnnouncementResponses[keyof UpdateAdminAnnouncementResponses];
export type PublishAdminAnnouncementData = {
    body: AnnouncementMutationRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/announcements/{id}/publish';
};
export type PublishAdminAnnouncementErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type PublishAdminAnnouncementError = PublishAdminAnnouncementErrors[keyof PublishAdminAnnouncementErrors];
export type PublishAdminAnnouncementResponses = {
    200: Announcement;
};
export type PublishAdminAnnouncementResponse = PublishAdminAnnouncementResponses[keyof PublishAdminAnnouncementResponses];
export type RevokeAdminAnnouncementData = {
    body: AnnouncementMutationRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/announcements/{id}/revoke';
};
export type RevokeAdminAnnouncementErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type RevokeAdminAnnouncementError = RevokeAdminAnnouncementErrors[keyof RevokeAdminAnnouncementErrors];
export type RevokeAdminAnnouncementResponses = {
    200: Announcement;
};
export type RevokeAdminAnnouncementResponse = RevokeAdminAnnouncementResponses[keyof RevokeAdminAnnouncementResponses];
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
    body: RegistrationEmailVerificationRequestWritable;
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
    503: ManagementError;
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
    503: ManagementError;
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
export type ListAccountRefundReconciliationsData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/account/refund-reconciliations';
};
export type ListAccountRefundReconciliationsErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type ListAccountRefundReconciliationsError = ListAccountRefundReconciliationsErrors[keyof ListAccountRefundReconciliationsErrors];
export type ListAccountRefundReconciliationsResponses = {
    200: RefundReconciliationListResponse;
};
export type ListAccountRefundReconciliationsResponse = ListAccountRefundReconciliationsResponses[keyof ListAccountRefundReconciliationsResponses];
export type ListOrganizationRefundReconciliationsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/refund-reconciliations';
};
export type ListOrganizationRefundReconciliationsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationRefundReconciliationsError = ListOrganizationRefundReconciliationsErrors[keyof ListOrganizationRefundReconciliationsErrors];
export type ListOrganizationRefundReconciliationsResponses = {
    200: RefundReconciliationListResponse;
};
export type ListOrganizationRefundReconciliationsResponse = ListOrganizationRefundReconciliationsResponses[keyof ListOrganizationRefundReconciliationsResponses];
export type ListAdminRefundReconciliationsData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/admin/refund-reconciliations';
};
export type ListAdminRefundReconciliationsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminRefundReconciliationsError = ListAdminRefundReconciliationsErrors[keyof ListAdminRefundReconciliationsErrors];
export type ListAdminRefundReconciliationsResponses = {
    200: RefundReconciliationListResponse;
};
export type ListAdminRefundReconciliationsResponse = ListAdminRefundReconciliationsResponses[keyof ListAdminRefundReconciliationsResponses];
export type ListAdminRefundsData = {
    body?: never;
    path?: never;
    query?: {
        after?: number;
        approval_status?: string;
        limit?: number;
    };
    url: '/api/admin/refunds';
};
export type ListAdminRefundsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminRefundsError = ListAdminRefundsErrors[keyof ListAdminRefundsErrors];
export type ListAdminRefundsResponses = {
    200: AdminRefundListResponse;
};
export type ListAdminRefundsResponse = ListAdminRefundsResponses[keyof ListAdminRefundsResponses];
export type ApproveAdminRefundData = {
    body: AdminRefundDecisionRequest;
    path: {
        request_id: string;
    };
    query?: never;
    url: '/api/admin/refunds/{request_id}/approve';
};
export type ApproveAdminRefundErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    502: ManagementError;
    503: ManagementError;
};
export type ApproveAdminRefundError = ApproveAdminRefundErrors[keyof ApproveAdminRefundErrors];
export type ApproveAdminRefundResponses = {
    200: AdminRefundListResponse;
};
export type ApproveAdminRefundResponse = ApproveAdminRefundResponses[keyof ApproveAdminRefundResponses];
export type RejectAdminRefundData = {
    body: AdminRefundDecisionRequest;
    path: {
        request_id: string;
    };
    query?: never;
    url: '/api/admin/refunds/{request_id}/reject';
};
export type RejectAdminRefundErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type RejectAdminRefundError = RejectAdminRefundErrors[keyof RejectAdminRefundErrors];
export type RejectAdminRefundResponses = {
    200: AdminRefundListResponse;
};
export type RejectAdminRefundResponse = RejectAdminRefundResponses[keyof RejectAdminRefundResponses];
export type SubmitAdminRefundData = {
    body?: never;
    path: {
        request_id: string;
    };
    query?: never;
    url: '/api/admin/refunds/{request_id}/submit';
};
export type SubmitAdminRefundErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type SubmitAdminRefundError = SubmitAdminRefundErrors[keyof SubmitAdminRefundErrors];
export type SubmitAdminRefundResponses = {
    200: AdminRefundListResponse;
};
export type SubmitAdminRefundResponse = SubmitAdminRefundResponses[keyof SubmitAdminRefundResponses];
export type ManualCompleteAdminRefundData = {
    body: AdminRefundManualCompletionRequest;
    path: {
        request_id: string;
    };
    query?: never;
    url: '/api/admin/refunds/{request_id}/manual-complete';
};
export type ManualCompleteAdminRefundErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ManualCompleteAdminRefundError = ManualCompleteAdminRefundErrors[keyof ManualCompleteAdminRefundErrors];
export type ManualCompleteAdminRefundResponses = {
    200: AdminRefundListResponse;
};
export type ManualCompleteAdminRefundResponse = ManualCompleteAdminRefundResponses[keyof ManualCompleteAdminRefundResponses];
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
export type ListCurrentSubscriptionCatalogData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/subscription-catalog';
};
export type ListCurrentSubscriptionCatalogErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type ListCurrentSubscriptionCatalogError = ListCurrentSubscriptionCatalogErrors[keyof ListCurrentSubscriptionCatalogErrors];
export type ListCurrentSubscriptionCatalogResponses = {
    200: SubscriptionCatalogResponse;
};
export type ListCurrentSubscriptionCatalogResponse = ListCurrentSubscriptionCatalogResponses[keyof ListCurrentSubscriptionCatalogResponses];
export type CreateCurrentSubscriptionOrderData = {
    body: SubscriptionOrderCreateRequest;
    path?: never;
    query?: never;
    url: '/api/account/subscription-orders';
};
export type CreateCurrentSubscriptionOrderErrors = {
    400: ManagementError;
    401: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type CreateCurrentSubscriptionOrderError = CreateCurrentSubscriptionOrderErrors[keyof CreateCurrentSubscriptionOrderErrors];
export type CreateCurrentSubscriptionOrderResponses = {
    200: SubscriptionOrder;
    201: SubscriptionOrder;
};
export type CreateCurrentSubscriptionOrderResponse = CreateCurrentSubscriptionOrderResponses[keyof CreateCurrentSubscriptionOrderResponses];
export type GetCurrentSubscriptionOrderData = {
    body?: never;
    path: {
        order_id: string;
    };
    query?: never;
    url: '/api/account/subscription-orders/{order_id}';
};
export type GetCurrentSubscriptionOrderErrors = {
    400: ManagementError;
    401: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetCurrentSubscriptionOrderError = GetCurrentSubscriptionOrderErrors[keyof GetCurrentSubscriptionOrderErrors];
export type GetCurrentSubscriptionOrderResponses = {
    200: SubscriptionOrder;
};
export type GetCurrentSubscriptionOrderResponse = GetCurrentSubscriptionOrderResponses[keyof GetCurrentSubscriptionOrderResponses];
export type SubmitCurrentSubscriptionOrderPaymentData = {
    body: SubscriptionOrderPaymentRequest;
    path: {
        order_id: string;
    };
    query?: never;
    url: '/api/account/subscription-orders/{order_id}/payment';
};
export type SubmitCurrentSubscriptionOrderPaymentErrors = {
    400: ManagementError;
    401: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type SubmitCurrentSubscriptionOrderPaymentError = SubmitCurrentSubscriptionOrderPaymentErrors[keyof SubmitCurrentSubscriptionOrderPaymentErrors];
export type SubmitCurrentSubscriptionOrderPaymentResponses = {
    200: SubscriptionOrderPaymentResponse;
};
export type SubmitCurrentSubscriptionOrderPaymentResponse = SubmitCurrentSubscriptionOrderPaymentResponses[keyof SubmitCurrentSubscriptionOrderPaymentResponses];
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
export type DisableUserTwoFactorData = {
    body: UserTwoFactorPasswordRequest;
    path?: never;
    query?: never;
    url: '/api/account/two-factor';
};
export type DisableUserTwoFactorErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type DisableUserTwoFactorError = DisableUserTwoFactorErrors[keyof DisableUserTwoFactorErrors];
export type DisableUserTwoFactorResponses = {
    204: void;
};
export type DisableUserTwoFactorResponse = DisableUserTwoFactorResponses[keyof DisableUserTwoFactorResponses];
export type GetUserTwoFactorData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/two-factor';
};
export type GetUserTwoFactorErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type GetUserTwoFactorError = GetUserTwoFactorErrors[keyof GetUserTwoFactorErrors];
export type GetUserTwoFactorResponses = {
    200: UserTwoFactorStatusResponse;
};
export type GetUserTwoFactorResponse = GetUserTwoFactorResponses[keyof GetUserTwoFactorResponses];
export type EnableUserTwoFactorData = {
    body: UserTwoFactorPasswordRequest;
    path?: never;
    query?: never;
    url: '/api/account/two-factor';
};
export type EnableUserTwoFactorErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type EnableUserTwoFactorError = EnableUserTwoFactorErrors[keyof EnableUserTwoFactorErrors];
export type EnableUserTwoFactorResponses = {
    200: UserTwoFactorEnrollmentResponse;
};
export type EnableUserTwoFactorResponse = EnableUserTwoFactorResponses[keyof EnableUserTwoFactorResponses];
export type ListUserNotificationsData = {
    body?: never;
    path?: never;
    query?: {
        before?: string;
        limit?: number;
    };
    url: '/api/account/notifications';
};
export type ListUserNotificationsErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type ListUserNotificationsError = ListUserNotificationsErrors[keyof ListUserNotificationsErrors];
export type ListUserNotificationsResponses = {
    200: UserNotificationListResponse;
};
export type ListUserNotificationsResponse = ListUserNotificationsResponses[keyof ListUserNotificationsResponses];
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
export type ListUserPasskeysData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/passkeys';
};
export type ListUserPasskeysErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type ListUserPasskeysError = ListUserPasskeysErrors[keyof ListUserPasskeysErrors];
export type ListUserPasskeysResponses = {
    200: UserPasskeyListResponse;
};
export type ListUserPasskeysResponse = ListUserPasskeysResponses[keyof ListUserPasskeysResponses];
export type StartUserPasskeyRegistrationData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/passkeys/registration/options';
};
export type StartUserPasskeyRegistrationErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type StartUserPasskeyRegistrationError = StartUserPasskeyRegistrationErrors[keyof StartUserPasskeyRegistrationErrors];
export type StartUserPasskeyRegistrationResponses = {
    200: UserPasskeyRegistrationOptionsResponse;
};
export type StartUserPasskeyRegistrationResponse = StartUserPasskeyRegistrationResponses[keyof StartUserPasskeyRegistrationResponses];
export type FinishUserPasskeyRegistrationData = {
    body: UserPasskeyRegistrationVerifyRequest;
    path?: never;
    query?: never;
    url: '/api/account/passkeys/registration/verify';
};
export type FinishUserPasskeyRegistrationErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type FinishUserPasskeyRegistrationError = FinishUserPasskeyRegistrationErrors[keyof FinishUserPasskeyRegistrationErrors];
export type FinishUserPasskeyRegistrationResponses = {
    200: UserPasskeyResponse;
};
export type FinishUserPasskeyRegistrationResponse = FinishUserPasskeyRegistrationResponses[keyof FinishUserPasskeyRegistrationResponses];
export type RevokeUserPasskeyData = {
    body: UserPasskeyRevokeRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/account/passkeys/{id}';
};
export type RevokeUserPasskeyErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type RevokeUserPasskeyError = RevokeUserPasskeyErrors[keyof RevokeUserPasskeyErrors];
export type RevokeUserPasskeyResponses = {
    204: void;
};
export type RevokeUserPasskeyResponse = RevokeUserPasskeyResponses[keyof RevokeUserPasskeyResponses];
export type RenameUserPasskeyData = {
    body: UserPasskeyRenameRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/account/passkeys/{id}';
};
export type RenameUserPasskeyErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type RenameUserPasskeyError = RenameUserPasskeyErrors[keyof RenameUserPasskeyErrors];
export type RenameUserPasskeyResponses = {
    200: UserPasskeyResponse;
};
export type RenameUserPasskeyResponse = RenameUserPasskeyResponses[keyof RenameUserPasskeyResponses];
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
export type MarkUserNotificationsReadData = {
    body: UserNotificationMarkReadRequest;
    path?: never;
    query?: never;
    url: '/api/account/notifications/read';
};
export type MarkUserNotificationsReadErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type MarkUserNotificationsReadError = MarkUserNotificationsReadErrors[keyof MarkUserNotificationsReadErrors];
export type MarkUserNotificationsReadResponses = {
    200: UserNotificationMarkReadResponse;
};
export type MarkUserNotificationsReadResponse = MarkUserNotificationsReadResponses[keyof MarkUserNotificationsReadResponses];
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
export type StartGitHubOAuthLoginData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/auth/oauth/github/start';
};
export type StartGitHubOAuthLoginErrors = {
    409: ManagementError;
    500: ManagementError;
};
export type StartGitHubOAuthLoginError = StartGitHubOAuthLoginErrors[keyof StartGitHubOAuthLoginErrors];
export type StartGitHubOAuthLoginResponses = {
    200: OAuthLoginStartResponse;
};
export type StartGitHubOAuthLoginResponse = StartGitHubOAuthLoginResponses[keyof StartGitHubOAuthLoginResponses];
export type StartDiscordOAuthLoginData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/auth/oauth/discord/start';
};
export type StartDiscordOAuthLoginErrors = {
    409: ManagementError;
    500: ManagementError;
};
export type StartDiscordOAuthLoginError = StartDiscordOAuthLoginErrors[keyof StartDiscordOAuthLoginErrors];
export type StartDiscordOAuthLoginResponses = {
    200: OAuthLoginStartResponse;
};
export type StartDiscordOAuthLoginResponse = StartDiscordOAuthLoginResponses[keyof StartDiscordOAuthLoginResponses];
export type CompleteGitHubOAuthLoginData = {
    body?: never;
    path?: never;
    query?: {
        state?: string;
        code?: string;
        error?: string;
    };
    url: '/api/auth/oauth/github/callback';
};
export type CompleteGitHubOAuthLoginErrors = {
    500: ManagementError;
};
export type CompleteGitHubOAuthLoginError = CompleteGitHubOAuthLoginErrors[keyof CompleteGitHubOAuthLoginErrors];
export type CompleteDiscordOAuthLoginData = {
    body?: never;
    path?: never;
    query?: {
        state?: string;
        code?: string;
        error?: string;
    };
    url: '/api/auth/oauth/discord/callback';
};
export type CompleteDiscordOAuthLoginErrors = {
    500: ManagementError;
};
export type CompleteDiscordOAuthLoginError = CompleteDiscordOAuthLoginErrors[keyof CompleteDiscordOAuthLoginErrors];
export type ExchangeOAuthLoginTicketData = {
    body: OAuthLoginExchangeRequest;
    path?: never;
    query?: never;
    url: '/api/auth/oauth/exchange';
};
export type ExchangeOAuthLoginTicketErrors = {
    400: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ExchangeOAuthLoginTicketError = ExchangeOAuthLoginTicketErrors[keyof ExchangeOAuthLoginTicketErrors];
export type ExchangeOAuthLoginTicketResponses = {
    200: LoginResponse;
};
export type ExchangeOAuthLoginTicketResponse = ExchangeOAuthLoginTicketResponses[keyof ExchangeOAuthLoginTicketResponses];
export type GetAdminGitHubOAuthLoginSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/github';
};
export type GetAdminGitHubOAuthLoginSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminGitHubOAuthLoginSettingsError = GetAdminGitHubOAuthLoginSettingsErrors[keyof GetAdminGitHubOAuthLoginSettingsErrors];
export type GetAdminGitHubOAuthLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type GetAdminGitHubOAuthLoginSettingsResponse = GetAdminGitHubOAuthLoginSettingsResponses[keyof GetAdminGitHubOAuthLoginSettingsResponses];
export type UpdateAdminGitHubOAuthLoginSettingsData = {
    body: AdminOAuthLoginProviderSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/github';
};
export type UpdateAdminGitHubOAuthLoginSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminGitHubOAuthLoginSettingsError = UpdateAdminGitHubOAuthLoginSettingsErrors[keyof UpdateAdminGitHubOAuthLoginSettingsErrors];
export type UpdateAdminGitHubOAuthLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type UpdateAdminGitHubOAuthLoginSettingsResponse = UpdateAdminGitHubOAuthLoginSettingsResponses[keyof UpdateAdminGitHubOAuthLoginSettingsResponses];
export type GetAdminDiscordOAuthLoginSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/discord';
};
export type GetAdminDiscordOAuthLoginSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminDiscordOAuthLoginSettingsError = GetAdminDiscordOAuthLoginSettingsErrors[keyof GetAdminDiscordOAuthLoginSettingsErrors];
export type GetAdminDiscordOAuthLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type GetAdminDiscordOAuthLoginSettingsResponse = GetAdminDiscordOAuthLoginSettingsResponses[keyof GetAdminDiscordOAuthLoginSettingsResponses];
export type UpdateAdminDiscordOAuthLoginSettingsData = {
    body: AdminOAuthLoginProviderSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/discord';
};
export type UpdateAdminDiscordOAuthLoginSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminDiscordOAuthLoginSettingsError = UpdateAdminDiscordOAuthLoginSettingsErrors[keyof UpdateAdminDiscordOAuthLoginSettingsErrors];
export type UpdateAdminDiscordOAuthLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type UpdateAdminDiscordOAuthLoginSettingsResponse = UpdateAdminDiscordOAuthLoginSettingsResponses[keyof UpdateAdminDiscordOAuthLoginSettingsResponses];
export type ListAccountOrganizationsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/organizations';
};
export type ListAccountOrganizationsErrors = {
    401: ManagementError;
    500: ManagementError;
};
export type ListAccountOrganizationsError = ListAccountOrganizationsErrors[keyof ListAccountOrganizationsErrors];
export type ListAccountOrganizationsResponses = {
    200: OrganizationListResponse;
};
export type ListAccountOrganizationsResponse = ListAccountOrganizationsResponses[keyof ListAccountOrganizationsResponses];
export type GetOrganizationOverviewData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}';
};
export type GetOrganizationOverviewErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationOverviewError = GetOrganizationOverviewErrors[keyof GetOrganizationOverviewErrors];
export type GetOrganizationOverviewResponses = {
    200: OrganizationOverviewResponse;
};
export type GetOrganizationOverviewResponse = GetOrganizationOverviewResponses[keyof GetOrganizationOverviewResponses];
export type ListOrganizationDepartmentsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        after?: string;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/departments';
};
export type ListOrganizationDepartmentsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationDepartmentsError = ListOrganizationDepartmentsErrors[keyof ListOrganizationDepartmentsErrors];
export type ListOrganizationDepartmentsResponses = {
    200: OrganizationDepartmentListResponse;
};
export type ListOrganizationDepartmentsResponse = ListOrganizationDepartmentsResponses[keyof ListOrganizationDepartmentsResponses];
export type CreateOrganizationDepartmentData = {
    body: OrganizationDepartmentCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/departments';
};
export type CreateOrganizationDepartmentErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationDepartmentError = CreateOrganizationDepartmentErrors[keyof CreateOrganizationDepartmentErrors];
export type CreateOrganizationDepartmentResponses = {
    200: OrganizationDepartmentResponse;
};
export type CreateOrganizationDepartmentResponse = CreateOrganizationDepartmentResponses[keyof CreateOrganizationDepartmentResponses];
export type GetOrganizationDepartmentData = {
    body?: never;
    path: {
        organization_id: number;
        department_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/departments/{department_id}';
};
export type GetOrganizationDepartmentErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationDepartmentError = GetOrganizationDepartmentErrors[keyof GetOrganizationDepartmentErrors];
export type GetOrganizationDepartmentResponses = {
    200: OrganizationDepartmentResponse;
};
export type GetOrganizationDepartmentResponse = GetOrganizationDepartmentResponses[keyof GetOrganizationDepartmentResponses];
export type MoveOrganizationDepartmentData = {
    body: OrganizationDepartmentMoveRequest;
    path: {
        organization_id: number;
        department_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/departments/{department_id}/move';
};
export type MoveOrganizationDepartmentErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type MoveOrganizationDepartmentError = MoveOrganizationDepartmentErrors[keyof MoveOrganizationDepartmentErrors];
export type MoveOrganizationDepartmentResponses = {
    200: OrganizationDepartmentResponse;
};
export type MoveOrganizationDepartmentResponse = MoveOrganizationDepartmentResponses[keyof MoveOrganizationDepartmentResponses];
export type DisableOrganizationDepartmentData = {
    body: OrganizationDepartmentDisableRequest;
    path: {
        organization_id: number;
        department_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/departments/{department_id}/disable';
};
export type DisableOrganizationDepartmentErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type DisableOrganizationDepartmentError = DisableOrganizationDepartmentErrors[keyof DisableOrganizationDepartmentErrors];
export type DisableOrganizationDepartmentResponses = {
    200: OrganizationDepartmentResponse;
};
export type DisableOrganizationDepartmentResponse = DisableOrganizationDepartmentResponses[keyof DisableOrganizationDepartmentResponses];
export type ListOrganizationInvitationsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/invitations';
};
export type ListOrganizationInvitationsErrors = {
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationInvitationsError = ListOrganizationInvitationsErrors[keyof ListOrganizationInvitationsErrors];
export type ListOrganizationInvitationsResponses = {
    200: OrganizationInvitationListResponse;
};
export type ListOrganizationInvitationsResponse = ListOrganizationInvitationsResponses[keyof ListOrganizationInvitationsResponses];
export type CreateOrganizationInvitationData = {
    body: OrganizationInvitationCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/invitations';
};
export type CreateOrganizationInvitationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationInvitationError = CreateOrganizationInvitationErrors[keyof CreateOrganizationInvitationErrors];
export type CreateOrganizationInvitationResponses = {
    200: OrganizationInvitationIssuedResponse;
};
export type CreateOrganizationInvitationResponse = CreateOrganizationInvitationResponses[keyof CreateOrganizationInvitationResponses];
export type PreviewOrganizationInvitationsData = {
    body: OrganizationInvitationPreviewRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/invitations/preview';
};
export type PreviewOrganizationInvitationsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type PreviewOrganizationInvitationsError = PreviewOrganizationInvitationsErrors[keyof PreviewOrganizationInvitationsErrors];
export type PreviewOrganizationInvitationsResponses = {
    200: OrganizationInvitationPreviewResponse;
};
export type PreviewOrganizationInvitationsResponse = PreviewOrganizationInvitationsResponses[keyof PreviewOrganizationInvitationsResponses];
export type CreateOrganizationInvitationBatchData = {
    body: OrganizationInvitationBatchRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/invitations/batches';
};
export type CreateOrganizationInvitationBatchErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationInvitationBatchError = CreateOrganizationInvitationBatchErrors[keyof CreateOrganizationInvitationBatchErrors];
export type CreateOrganizationInvitationBatchResponses = {
    200: OrganizationInvitationBatchResponse;
};
export type CreateOrganizationInvitationBatchResponse = CreateOrganizationInvitationBatchResponses[keyof CreateOrganizationInvitationBatchResponses];
export type RevokeOrganizationInvitationData = {
    body?: never;
    path: {
        organization_id: number;
        invitation_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/invitations/{invitation_key}/revoke';
};
export type RevokeOrganizationInvitationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type RevokeOrganizationInvitationError = RevokeOrganizationInvitationErrors[keyof RevokeOrganizationInvitationErrors];
export type RevokeOrganizationInvitationResponses = {
    200: OrganizationInvitationResponse;
};
export type RevokeOrganizationInvitationResponse = RevokeOrganizationInvitationResponses[keyof RevokeOrganizationInvitationResponses];
export type ResendOrganizationInvitationData = {
    body?: never;
    path: {
        organization_id: number;
        invitation_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/invitations/{invitation_key}/resend';
};
export type ResendOrganizationInvitationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ResendOrganizationInvitationError = ResendOrganizationInvitationErrors[keyof ResendOrganizationInvitationErrors];
export type ResendOrganizationInvitationResponses = {
    200: OrganizationInvitationIssuedResponse;
};
export type ResendOrganizationInvitationResponse = ResendOrganizationInvitationResponses[keyof ResendOrganizationInvitationResponses];
export type AcceptOrganizationInvitationData = {
    body: OrganizationInvitationAcceptRequest;
    path?: never;
    query?: never;
    url: '/api/account/organization-invitations/accept';
};
export type AcceptOrganizationInvitationErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type AcceptOrganizationInvitationError = AcceptOrganizationInvitationErrors[keyof AcceptOrganizationInvitationErrors];
export type AcceptOrganizationInvitationResponses = {
    200: OrganizationInvitationAcceptResponse;
};
export type AcceptOrganizationInvitationResponse = AcceptOrganizationInvitationResponses[keyof AcceptOrganizationInvitationResponses];
export type ListOrganizationMembersData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/members';
};
export type ListOrganizationMembersErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationMembersError = ListOrganizationMembersErrors[keyof ListOrganizationMembersErrors];
export type ListOrganizationMembersResponses = {
    200: OrganizationMembershipListResponse;
};
export type ListOrganizationMembersResponse = ListOrganizationMembersResponses[keyof ListOrganizationMembersResponses];
export type ChangeOrganizationMemberData = {
    body: OrganizationMembershipChangeRequest;
    path: {
        organization_id: number;
        membership_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/members/{membership_id}';
};
export type ChangeOrganizationMemberErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ChangeOrganizationMemberError = ChangeOrganizationMemberErrors[keyof ChangeOrganizationMemberErrors];
export type ChangeOrganizationMemberResponses = {
    200: OrganizationMembershipResponse;
};
export type ChangeOrganizationMemberResponse = ChangeOrganizationMemberResponses[keyof ChangeOrganizationMemberResponses];
export type ListOrganizationMemberDepartmentMembershipsData = {
    body?: never;
    path: {
        organization_id: number;
        membership_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/members/{membership_id}/department-memberships';
};
export type ListOrganizationMemberDepartmentMembershipsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationMemberDepartmentMembershipsError = ListOrganizationMemberDepartmentMembershipsErrors[keyof ListOrganizationMemberDepartmentMembershipsErrors];
export type ListOrganizationMemberDepartmentMembershipsResponses = {
    200: OrganizationDepartmentMembershipListResponse;
};
export type ListOrganizationMemberDepartmentMembershipsResponse = ListOrganizationMemberDepartmentMembershipsResponses[keyof ListOrganizationMemberDepartmentMembershipsResponses];
export type AddOrganizationMemberDepartmentMembershipData = {
    body: OrganizationDepartmentMembershipAddRequest;
    path: {
        organization_id: number;
        membership_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/members/{membership_id}/department-memberships';
};
export type AddOrganizationMemberDepartmentMembershipErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type AddOrganizationMemberDepartmentMembershipError = AddOrganizationMemberDepartmentMembershipErrors[keyof AddOrganizationMemberDepartmentMembershipErrors];
export type AddOrganizationMemberDepartmentMembershipResponses = {
    200: OrganizationDepartmentMembershipResponse;
};
export type AddOrganizationMemberDepartmentMembershipResponse = AddOrganizationMemberDepartmentMembershipResponses[keyof AddOrganizationMemberDepartmentMembershipResponses];
export type SetPrimaryOrganizationMemberDepartmentMembershipData = {
    body: OrganizationDepartmentMembershipPrimaryRequest;
    path: {
        organization_id: number;
        membership_id: number;
        department_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/members/{membership_id}/department-memberships/{department_id}/primary';
};
export type SetPrimaryOrganizationMemberDepartmentMembershipErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type SetPrimaryOrganizationMemberDepartmentMembershipError = SetPrimaryOrganizationMemberDepartmentMembershipErrors[keyof SetPrimaryOrganizationMemberDepartmentMembershipErrors];
export type SetPrimaryOrganizationMemberDepartmentMembershipResponses = {
    200: OrganizationDepartmentMembershipResponse;
};
export type SetPrimaryOrganizationMemberDepartmentMembershipResponse = SetPrimaryOrganizationMemberDepartmentMembershipResponses[keyof SetPrimaryOrganizationMemberDepartmentMembershipResponses];
export type RemoveOrganizationMemberDepartmentMembershipData = {
    body: OrganizationDepartmentMembershipRemoveRequest;
    path: {
        organization_id: number;
        membership_id: number;
        department_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/members/{membership_id}/department-memberships/{department_id}';
};
export type RemoveOrganizationMemberDepartmentMembershipErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type RemoveOrganizationMemberDepartmentMembershipError = RemoveOrganizationMemberDepartmentMembershipErrors[keyof RemoveOrganizationMemberDepartmentMembershipErrors];
export type RemoveOrganizationMemberDepartmentMembershipResponses = {
    200: OrganizationDepartmentMembershipResponse;
};
export type RemoveOrganizationMemberDepartmentMembershipResponse = RemoveOrganizationMemberDepartmentMembershipResponses[keyof RemoveOrganizationMemberDepartmentMembershipResponses];
export type TransferOrganizationOwnershipData = {
    body: OrganizationOwnershipTransferRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/ownership-transfer';
};
export type TransferOrganizationOwnershipErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type TransferOrganizationOwnershipError = TransferOrganizationOwnershipErrors[keyof TransferOrganizationOwnershipErrors];
export type TransferOrganizationOwnershipResponses = {
    200: OrganizationOwnershipTransferResponse;
};
export type TransferOrganizationOwnershipResponse = TransferOrganizationOwnershipResponses[keyof TransferOrganizationOwnershipResponses];
export type ListOrganizationTeamsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/teams';
};
export type ListOrganizationTeamsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationTeamsError = ListOrganizationTeamsErrors[keyof ListOrganizationTeamsErrors];
export type ListOrganizationTeamsResponses = {
    200: OrganizationTeamListResponse;
};
export type ListOrganizationTeamsResponse = ListOrganizationTeamsResponses[keyof ListOrganizationTeamsResponses];
export type CreateOrganizationTeamData = {
    body: OrganizationTeamCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/teams';
};
export type CreateOrganizationTeamErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationTeamError = CreateOrganizationTeamErrors[keyof CreateOrganizationTeamErrors];
export type CreateOrganizationTeamResponses = {
    200: OrganizationTeamResponse;
};
export type CreateOrganizationTeamResponse = CreateOrganizationTeamResponses[keyof CreateOrganizationTeamResponses];
export type ChangeOrganizationTeamData = {
    body: OrganizationTeamChangeRequest;
    path: {
        organization_id: number;
        team_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/teams/{team_id}';
};
export type ChangeOrganizationTeamErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ChangeOrganizationTeamError = ChangeOrganizationTeamErrors[keyof ChangeOrganizationTeamErrors];
export type ChangeOrganizationTeamResponses = {
    200: OrganizationTeamResponse;
};
export type ChangeOrganizationTeamResponse = ChangeOrganizationTeamResponses[keyof ChangeOrganizationTeamResponses];
export type DisableOrganizationTeamData = {
    body: OrganizationTeamDisableRequest;
    path: {
        organization_id: number;
        team_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/teams/{team_id}/disable';
};
export type DisableOrganizationTeamErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type DisableOrganizationTeamError = DisableOrganizationTeamErrors[keyof DisableOrganizationTeamErrors];
export type DisableOrganizationTeamResponses = {
    200: OrganizationTeamResponse;
};
export type DisableOrganizationTeamResponse = DisableOrganizationTeamResponses[keyof DisableOrganizationTeamResponses];
export type ListOrganizationCustomRolesData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/roles';
};
export type ListOrganizationCustomRolesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationCustomRolesError = ListOrganizationCustomRolesErrors[keyof ListOrganizationCustomRolesErrors];
export type ListOrganizationCustomRolesResponses = {
    200: OrganizationCustomRoleListResponse;
};
export type ListOrganizationCustomRolesResponse = ListOrganizationCustomRolesResponses[keyof ListOrganizationCustomRolesResponses];
export type CreateOrganizationCustomRoleData = {
    body: OrganizationCustomRoleCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/roles';
};
export type CreateOrganizationCustomRoleErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationCustomRoleError = CreateOrganizationCustomRoleErrors[keyof CreateOrganizationCustomRoleErrors];
export type CreateOrganizationCustomRoleResponses = {
    200: OrganizationCustomRoleResponse;
};
export type CreateOrganizationCustomRoleResponse = CreateOrganizationCustomRoleResponses[keyof CreateOrganizationCustomRoleResponses];
export type GetOrganizationCustomRoleData = {
    body?: never;
    path: {
        organization_id: number;
        role_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/roles/{role_key}';
};
export type GetOrganizationCustomRoleErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationCustomRoleError = GetOrganizationCustomRoleErrors[keyof GetOrganizationCustomRoleErrors];
export type GetOrganizationCustomRoleResponses = {
    200: OrganizationCustomRoleResponse;
};
export type GetOrganizationCustomRoleResponse = GetOrganizationCustomRoleResponses[keyof GetOrganizationCustomRoleResponses];
export type UpdateOrganizationCustomRoleData = {
    body: OrganizationCustomRoleChangeRequest;
    path: {
        organization_id: number;
        role_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/roles/{role_key}';
};
export type UpdateOrganizationCustomRoleErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateOrganizationCustomRoleError = UpdateOrganizationCustomRoleErrors[keyof UpdateOrganizationCustomRoleErrors];
export type UpdateOrganizationCustomRoleResponses = {
    200: OrganizationCustomRoleResponse;
};
export type UpdateOrganizationCustomRoleResponse = UpdateOrganizationCustomRoleResponses[keyof UpdateOrganizationCustomRoleResponses];
export type DisableOrganizationCustomRoleData = {
    body: OrganizationCustomRoleDisableRequest;
    path: {
        organization_id: number;
        role_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/roles/{role_key}/disable';
};
export type DisableOrganizationCustomRoleErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type DisableOrganizationCustomRoleError = DisableOrganizationCustomRoleErrors[keyof DisableOrganizationCustomRoleErrors];
export type DisableOrganizationCustomRoleResponses = {
    200: OrganizationCustomRoleResponse;
};
export type DisableOrganizationCustomRoleResponse = DisableOrganizationCustomRoleResponses[keyof DisableOrganizationCustomRoleResponses];
export type ListOrganizationServiceAccountsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        after?: string;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/service-accounts';
};
export type ListOrganizationServiceAccountsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationServiceAccountsError = ListOrganizationServiceAccountsErrors[keyof ListOrganizationServiceAccountsErrors];
export type ListOrganizationServiceAccountsResponses = {
    200: OrganizationServiceAccountListResponse;
};
export type ListOrganizationServiceAccountsResponse = ListOrganizationServiceAccountsResponses[keyof ListOrganizationServiceAccountsResponses];
export type CreateOrganizationServiceAccountData = {
    body: OrganizationServiceAccountCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/service-accounts';
};
export type CreateOrganizationServiceAccountErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationServiceAccountError = CreateOrganizationServiceAccountErrors[keyof CreateOrganizationServiceAccountErrors];
export type CreateOrganizationServiceAccountResponses = {
    200: OrganizationServiceAccountIssuedResponse;
};
export type CreateOrganizationServiceAccountResponse = CreateOrganizationServiceAccountResponses[keyof CreateOrganizationServiceAccountResponses];
export type GetOrganizationServiceAccountData = {
    body?: never;
    path: {
        organization_id: number;
        service_account_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/service-accounts/{service_account_key}';
};
export type GetOrganizationServiceAccountErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationServiceAccountError = GetOrganizationServiceAccountErrors[keyof GetOrganizationServiceAccountErrors];
export type GetOrganizationServiceAccountResponses = {
    200: OrganizationServiceAccountResponse;
};
export type GetOrganizationServiceAccountResponse = GetOrganizationServiceAccountResponses[keyof GetOrganizationServiceAccountResponses];
export type ChangeOrganizationServiceAccountStatusData = {
    body: OrganizationServiceAccountStatusRequest;
    path: {
        organization_id: number;
        service_account_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/service-accounts/{service_account_key}/status';
};
export type ChangeOrganizationServiceAccountStatusErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ChangeOrganizationServiceAccountStatusError = ChangeOrganizationServiceAccountStatusErrors[keyof ChangeOrganizationServiceAccountStatusErrors];
export type ChangeOrganizationServiceAccountStatusResponses = {
    200: OrganizationServiceAccountResponse;
};
export type ChangeOrganizationServiceAccountStatusResponse = ChangeOrganizationServiceAccountStatusResponses[keyof ChangeOrganizationServiceAccountStatusResponses];
export type RotateOrganizationServiceAccountKeyData = {
    body: OrganizationServiceAccountRotateRequest;
    path: {
        organization_id: number;
        service_account_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/service-accounts/{service_account_key}/rotate-key';
};
export type RotateOrganizationServiceAccountKeyErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type RotateOrganizationServiceAccountKeyError = RotateOrganizationServiceAccountKeyErrors[keyof RotateOrganizationServiceAccountKeyErrors];
export type RotateOrganizationServiceAccountKeyResponses = {
    200: OrganizationServiceAccountIssuedResponse;
};
export type RotateOrganizationServiceAccountKeyResponse = RotateOrganizationServiceAccountKeyResponses[keyof RotateOrganizationServiceAccountKeyResponses];
export type GetOrganizationServiceAccountBudgetBindingData = {
    body?: never;
    path: {
        organization_id: number;
        service_account_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/service-accounts/{service_account_key}/budget-binding';
};
export type GetOrganizationServiceAccountBudgetBindingErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationServiceAccountBudgetBindingError = GetOrganizationServiceAccountBudgetBindingErrors[keyof GetOrganizationServiceAccountBudgetBindingErrors];
export type GetOrganizationServiceAccountBudgetBindingResponses = {
    200: OrganizationServiceAccountBudgetBindingResponse;
};
export type GetOrganizationServiceAccountBudgetBindingResponse = GetOrganizationServiceAccountBudgetBindingResponses[keyof GetOrganizationServiceAccountBudgetBindingResponses];
export type SetOrganizationServiceAccountBudgetBindingData = {
    body: OrganizationServiceAccountBudgetBindingRequest;
    path: {
        organization_id: number;
        service_account_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/service-accounts/{service_account_key}/budget-binding';
};
export type SetOrganizationServiceAccountBudgetBindingErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type SetOrganizationServiceAccountBudgetBindingError = SetOrganizationServiceAccountBudgetBindingErrors[keyof SetOrganizationServiceAccountBudgetBindingErrors];
export type SetOrganizationServiceAccountBudgetBindingResponses = {
    200: OrganizationServiceAccountBudgetBindingResponse;
};
export type SetOrganizationServiceAccountBudgetBindingResponse = SetOrganizationServiceAccountBudgetBindingResponses[keyof SetOrganizationServiceAccountBudgetBindingResponses];
export type SetOrganizationMemberCustomRoleData = {
    body: OrganizationMemberCustomRoleRequest;
    path: {
        organization_id: number;
        membership_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/members/{membership_id}/custom-role';
};
export type SetOrganizationMemberCustomRoleErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type SetOrganizationMemberCustomRoleError = SetOrganizationMemberCustomRoleErrors[keyof SetOrganizationMemberCustomRoleErrors];
export type SetOrganizationMemberCustomRoleResponses = {
    200: OrganizationMemberCustomRoleResponse;
};
export type SetOrganizationMemberCustomRoleResponse = SetOrganizationMemberCustomRoleResponses[keyof SetOrganizationMemberCustomRoleResponses];
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
    409: ManagementError;
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
export type ListAdminCustomOAuth2ProvidersData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/custom';
};
export type ListAdminCustomOAuth2ProvidersErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminCustomOAuth2ProvidersError = ListAdminCustomOAuth2ProvidersErrors[keyof ListAdminCustomOAuth2ProvidersErrors];
export type ListAdminCustomOAuth2ProvidersResponses = {
    200: AdminCustomOAuth2ProviderList;
};
export type ListAdminCustomOAuth2ProvidersResponse = ListAdminCustomOAuth2ProvidersResponses[keyof ListAdminCustomOAuth2ProvidersResponses];
export type GetAdminCustomOAuth2ProviderData = {
    body?: never;
    path: {
        provider_key: string;
    };
    query?: never;
    url: '/api/admin/authentication-settings/oauth/custom/{provider_key}';
};
export type GetAdminCustomOAuth2ProviderErrors = {
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetAdminCustomOAuth2ProviderError = GetAdminCustomOAuth2ProviderErrors[keyof GetAdminCustomOAuth2ProviderErrors];
export type GetAdminCustomOAuth2ProviderResponses = {
    200: AdminCustomOAuth2Provider;
};
export type GetAdminCustomOAuth2ProviderResponse = GetAdminCustomOAuth2ProviderResponses[keyof GetAdminCustomOAuth2ProviderResponses];
export type UpdateAdminCustomOAuth2ProviderData = {
    body: AdminCustomOAuth2ProviderRequestWritable;
    path: {
        provider_key: string;
    };
    query?: never;
    url: '/api/admin/authentication-settings/oauth/custom/{provider_key}';
};
export type UpdateAdminCustomOAuth2ProviderErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminCustomOAuth2ProviderError = UpdateAdminCustomOAuth2ProviderErrors[keyof UpdateAdminCustomOAuth2ProviderErrors];
export type UpdateAdminCustomOAuth2ProviderResponses = {
    200: AdminCustomOAuth2Provider;
};
export type UpdateAdminCustomOAuth2ProviderResponse = UpdateAdminCustomOAuth2ProviderResponses[keyof UpdateAdminCustomOAuth2ProviderResponses];
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
export type ReadAdminDebugTraceSnapshotsData = {
    body: AdminDebugTraceSnapshotRequest;
    path: {
        id: number;
    };
    query?: never;
    url: '/api/admin/debug-traces/{id}/snapshots';
};
export type ReadAdminDebugTraceSnapshotsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ReadAdminDebugTraceSnapshotsError = ReadAdminDebugTraceSnapshotsErrors[keyof ReadAdminDebugTraceSnapshotsErrors];
export type ReadAdminDebugTraceSnapshotsResponses = {
    200: AdminDebugTraceSnapshotsResponse;
};
export type ReadAdminDebugTraceSnapshotsResponse = ReadAdminDebugTraceSnapshotsResponses[keyof ReadAdminDebugTraceSnapshotsResponses];
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
export type ReceiveRefundWebhookData = {
    body?: unknown;
    headers: {
        'stripe-signature': string;
    };
    path: {
        provider: string;
    };
    query?: never;
    url: '/api/refund/webhook/{provider}';
};
export type ReceiveRefundWebhookErrors = {
    400: unknown;
    405: unknown;
    409: unknown;
    500: unknown;
    503: unknown;
};
export type ReceiveRefundWebhookResponses = {
    200: unknown;
};
export type GetOrganizationWalletData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/wallet';
};
export type GetOrganizationWalletErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationWalletError = GetOrganizationWalletErrors[keyof GetOrganizationWalletErrors];
export type GetOrganizationWalletResponses = {
    200: OrganizationWalletSummaryResponse;
};
export type GetOrganizationWalletResponse = GetOrganizationWalletResponses[keyof GetOrganizationWalletResponses];
export type ListOrganizationWalletEntriesData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/wallet/entries';
};
export type ListOrganizationWalletEntriesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationWalletEntriesError = ListOrganizationWalletEntriesErrors[keyof ListOrganizationWalletEntriesErrors];
export type ListOrganizationWalletEntriesResponses = {
    200: OrganizationWalletListResponse;
};
export type ListOrganizationWalletEntriesResponse = ListOrganizationWalletEntriesResponses[keyof ListOrganizationWalletEntriesResponses];
export type AdjustOrganizationWalletData = {
    body: OrganizationWalletAdjustmentRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/wallet/adjustments';
};
export type AdjustOrganizationWalletErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type AdjustOrganizationWalletError = AdjustOrganizationWalletErrors[keyof AdjustOrganizationWalletErrors];
export type AdjustOrganizationWalletResponses = {
    200: OrganizationWalletAdjustmentResponse;
    201: OrganizationWalletAdjustmentResponse;
};
export type AdjustOrganizationWalletResponse = AdjustOrganizationWalletResponses[keyof AdjustOrganizationWalletResponses];
export type CreateOrganizationTopupOrderData = {
    body: OrganizationTopupOrderCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/topups';
};
export type CreateOrganizationTopupOrderErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    502: ManagementError;
    503: ManagementError;
};
export type CreateOrganizationTopupOrderError = CreateOrganizationTopupOrderErrors[keyof CreateOrganizationTopupOrderErrors];
export type CreateOrganizationTopupOrderResponses = {
    200: OrganizationTopupOrderResponse;
    201: OrganizationTopupOrderResponse;
};
export type CreateOrganizationTopupOrderResponse = CreateOrganizationTopupOrderResponses[keyof CreateOrganizationTopupOrderResponses];
export type GetOrganizationBudgetsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/budgets';
};
export type GetOrganizationBudgetsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationBudgetsError = GetOrganizationBudgetsErrors[keyof GetOrganizationBudgetsErrors];
export type GetOrganizationBudgetsResponses = {
    200: OrganizationBudgetOverviewResponse;
};
export type GetOrganizationBudgetsResponse = GetOrganizationBudgetsResponses[keyof GetOrganizationBudgetsResponses];
export type CreateOrganizationBudgetPolicyData = {
    body: OrganizationBudgetPolicyCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/budgets';
};
export type CreateOrganizationBudgetPolicyErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type CreateOrganizationBudgetPolicyError = CreateOrganizationBudgetPolicyErrors[keyof CreateOrganizationBudgetPolicyErrors];
export type CreateOrganizationBudgetPolicyResponses = {
    201: OrganizationBudgetPolicyResponse;
};
export type CreateOrganizationBudgetPolicyResponse = CreateOrganizationBudgetPolicyResponses[keyof CreateOrganizationBudgetPolicyResponses];
export type GetOrganizationBudgetReservationData = {
    body?: never;
    path: {
        organization_id: number;
        reservation_id: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/budgets/reservations/{reservation_id}';
};
export type GetOrganizationBudgetReservationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationBudgetReservationError = GetOrganizationBudgetReservationErrors[keyof GetOrganizationBudgetReservationErrors];
export type GetOrganizationBudgetReservationResponses = {
    200: OrganizationBudgetReservationResponse;
};
export type GetOrganizationBudgetReservationResponse = GetOrganizationBudgetReservationResponses[keyof GetOrganizationBudgetReservationResponses];
export type ListOrganizationContractPricesData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/contract-prices';
};
export type ListOrganizationContractPricesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationContractPricesError = ListOrganizationContractPricesErrors[keyof ListOrganizationContractPricesErrors];
export type ListOrganizationContractPricesResponses = {
    200: ContractPriceListResponse;
};
export type ListOrganizationContractPricesResponse = ListOrganizationContractPricesResponses[keyof ListOrganizationContractPricesResponses];
export type CreateOrganizationContractPriceData = {
    body: ContractPriceCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/contract-prices';
};
export type CreateOrganizationContractPriceErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationContractPriceError = CreateOrganizationContractPriceErrors[keyof CreateOrganizationContractPriceErrors];
export type CreateOrganizationContractPriceResponses = {
    201: ContractPriceResponse;
};
export type CreateOrganizationContractPriceResponse = CreateOrganizationContractPriceResponses[keyof CreateOrganizationContractPriceResponses];
export type GetOrganizationContractPriceData = {
    body?: never;
    path: {
        organization_id: number;
        price_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/contract-prices/{price_id}';
};
export type GetOrganizationContractPriceErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationContractPriceError = GetOrganizationContractPriceErrors[keyof GetOrganizationContractPriceErrors];
export type GetOrganizationContractPriceResponses = {
    200: ContractPriceResponse;
};
export type GetOrganizationContractPriceResponse = GetOrganizationContractPriceResponses[keyof GetOrganizationContractPriceResponses];
export type CloseOrganizationContractPriceData = {
    body: ContractPriceCloseRequest;
    path: {
        organization_id: number;
        price_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/contract-prices/{price_id}/close';
};
export type CloseOrganizationContractPriceErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CloseOrganizationContractPriceError = CloseOrganizationContractPriceErrors[keyof CloseOrganizationContractPriceErrors];
export type CloseOrganizationContractPriceResponses = {
    200: ContractPriceResponse;
};
export type CloseOrganizationContractPriceResponse = CloseOrganizationContractPriceResponses[keyof CloseOrganizationContractPriceResponses];
export type ListOrganizationCreditTermsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/credit-terms';
};
export type ListOrganizationCreditTermsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationCreditTermsError = ListOrganizationCreditTermsErrors[keyof ListOrganizationCreditTermsErrors];
export type ListOrganizationCreditTermsResponses = {
    200: CreditTermListResponse;
};
export type ListOrganizationCreditTermsResponse = ListOrganizationCreditTermsResponses[keyof ListOrganizationCreditTermsResponses];
export type CreateOrganizationCreditTermData = {
    body: CreditTermCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/credit-terms';
};
export type CreateOrganizationCreditTermErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationCreditTermError = CreateOrganizationCreditTermErrors[keyof CreateOrganizationCreditTermErrors];
export type CreateOrganizationCreditTermResponses = {
    201: CreditTermResponse;
};
export type CreateOrganizationCreditTermResponse = CreateOrganizationCreditTermResponses[keyof CreateOrganizationCreditTermResponses];
export type GetOrganizationCreditTermData = {
    body?: never;
    path: {
        organization_id: number;
        term_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/credit-terms/{term_id}';
};
export type GetOrganizationCreditTermErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationCreditTermError = GetOrganizationCreditTermErrors[keyof GetOrganizationCreditTermErrors];
export type GetOrganizationCreditTermResponses = {
    200: CreditTermResponse;
};
export type GetOrganizationCreditTermResponse = GetOrganizationCreditTermResponses[keyof GetOrganizationCreditTermResponses];
export type UpdateOrganizationCreditTermStatusData = {
    body: CreditTermStatusRequest;
    path: {
        organization_id: number;
        term_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/credit-terms/{term_id}/status';
};
export type UpdateOrganizationCreditTermStatusErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateOrganizationCreditTermStatusError = UpdateOrganizationCreditTermStatusErrors[keyof UpdateOrganizationCreditTermStatusErrors];
export type UpdateOrganizationCreditTermStatusResponses = {
    200: CreditTermResponse;
};
export type UpdateOrganizationCreditTermStatusResponse = UpdateOrganizationCreditTermStatusResponses[keyof UpdateOrganizationCreditTermStatusResponses];
export type ListOrganizationCreditInvoicesData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/credit-invoices';
};
export type ListOrganizationCreditInvoicesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationCreditInvoicesError = ListOrganizationCreditInvoicesErrors[keyof ListOrganizationCreditInvoicesErrors];
export type ListOrganizationCreditInvoicesResponses = {
    200: CreditInvoiceListResponse;
};
export type ListOrganizationCreditInvoicesResponse = ListOrganizationCreditInvoicesResponses[keyof ListOrganizationCreditInvoicesResponses];
export type GetOrganizationCreditInvoiceData = {
    body?: never;
    path: {
        organization_id: number;
        invoice_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/credit-invoices/{invoice_id}';
};
export type GetOrganizationCreditInvoiceErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationCreditInvoiceError = GetOrganizationCreditInvoiceErrors[keyof GetOrganizationCreditInvoiceErrors];
export type GetOrganizationCreditInvoiceResponses = {
    200: CreditInvoiceResponse;
};
export type GetOrganizationCreditInvoiceResponse = GetOrganizationCreditInvoiceResponses[keyof GetOrganizationCreditInvoiceResponses];
export type ListOrganizationCreditRepaymentsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/credit-repayments';
};
export type ListOrganizationCreditRepaymentsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationCreditRepaymentsError = ListOrganizationCreditRepaymentsErrors[keyof ListOrganizationCreditRepaymentsErrors];
export type ListOrganizationCreditRepaymentsResponses = {
    200: CreditRepaymentListResponse;
};
export type ListOrganizationCreditRepaymentsResponse = ListOrganizationCreditRepaymentsResponses[keyof ListOrganizationCreditRepaymentsResponses];
export type GetOrganizationCreditRepaymentData = {
    body?: never;
    path: {
        organization_id: number;
        repayment_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/credit-repayments/{repayment_id}';
};
export type GetOrganizationCreditRepaymentErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationCreditRepaymentError = GetOrganizationCreditRepaymentErrors[keyof GetOrganizationCreditRepaymentErrors];
export type GetOrganizationCreditRepaymentResponses = {
    200: CreditRepaymentResponse;
};
export type GetOrganizationCreditRepaymentResponse = GetOrganizationCreditRepaymentResponses[keyof GetOrganizationCreditRepaymentResponses];
export type ListOrganizationCreditAllocationsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/credit-allocations';
};
export type ListOrganizationCreditAllocationsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationCreditAllocationsError = ListOrganizationCreditAllocationsErrors[keyof ListOrganizationCreditAllocationsErrors];
export type ListOrganizationCreditAllocationsResponses = {
    200: CreditAllocationListResponse;
};
export type ListOrganizationCreditAllocationsResponse = ListOrganizationCreditAllocationsResponses[keyof ListOrganizationCreditAllocationsResponses];
export type GetOrganizationCreditAllocationData = {
    body?: never;
    path: {
        organization_id: number;
        allocation_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/credit-allocations/{allocation_id}';
};
export type GetOrganizationCreditAllocationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationCreditAllocationError = GetOrganizationCreditAllocationErrors[keyof GetOrganizationCreditAllocationErrors];
export type GetOrganizationCreditAllocationResponses = {
    200: CreditAllocationResponse;
};
export type GetOrganizationCreditAllocationResponse = GetOrganizationCreditAllocationResponses[keyof GetOrganizationCreditAllocationResponses];
export type CreateOrganizationProvisioningRequestData = {
    body: OrganizationProvisioningCreateRequest;
    path?: never;
    query?: never;
    url: '/api/account/organization-provisioning';
};
export type CreateOrganizationProvisioningRequestErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type CreateOrganizationProvisioningRequestError = CreateOrganizationProvisioningRequestErrors[keyof CreateOrganizationProvisioningRequestErrors];
export type CreateOrganizationProvisioningRequestResponses = {
    200: OrganizationProvisioningRequest;
    201: OrganizationProvisioningRequest;
};
export type CreateOrganizationProvisioningRequestResponse = CreateOrganizationProvisioningRequestResponses[keyof CreateOrganizationProvisioningRequestResponses];
export type GetActiveOrganizationProvisioningRequestData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/organization-provisioning/active';
};
export type GetActiveOrganizationProvisioningRequestErrors = {
    401: ManagementError;
    404: ManagementError;
    503: ManagementError;
};
export type GetActiveOrganizationProvisioningRequestError = GetActiveOrganizationProvisioningRequestErrors[keyof GetActiveOrganizationProvisioningRequestErrors];
export type GetActiveOrganizationProvisioningRequestResponses = {
    200: OrganizationProvisioningRequest;
};
export type GetActiveOrganizationProvisioningRequestResponse = GetActiveOrganizationProvisioningRequestResponses[keyof GetActiveOrganizationProvisioningRequestResponses];
export type GetOrganizationProvisioningRequestData = {
    body?: never;
    path: {
        request_key: string;
    };
    query?: never;
    url: '/api/account/organization-provisioning/{request_key}';
};
export type GetOrganizationProvisioningRequestErrors = {
    400: ManagementError;
    401: ManagementError;
    404: ManagementError;
    503: ManagementError;
};
export type GetOrganizationProvisioningRequestError = GetOrganizationProvisioningRequestErrors[keyof GetOrganizationProvisioningRequestErrors];
export type GetOrganizationProvisioningRequestResponses = {
    200: OrganizationProvisioningRequest;
};
export type GetOrganizationProvisioningRequestResponse = GetOrganizationProvisioningRequestResponses[keyof GetOrganizationProvisioningRequestResponses];
export type ListAdminOrganizationProvisioningRequestsData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
        status?: OrganizationProvisioningStatus;
    };
    url: '/api/admin/organization-provisioning';
};
export type ListAdminOrganizationProvisioningRequestsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    503: ManagementError;
};
export type ListAdminOrganizationProvisioningRequestsError = ListAdminOrganizationProvisioningRequestsErrors[keyof ListAdminOrganizationProvisioningRequestsErrors];
export type ListAdminOrganizationProvisioningRequestsResponses = {
    200: OrganizationProvisioningRequestListResponse;
};
export type ListAdminOrganizationProvisioningRequestsResponse = ListAdminOrganizationProvisioningRequestsResponses[keyof ListAdminOrganizationProvisioningRequestsResponses];
export type GetAdminOrganizationProvisioningRequestData = {
    body?: never;
    path: {
        request_key: string;
    };
    query?: never;
    url: '/api/admin/organization-provisioning/{request_key}';
};
export type GetAdminOrganizationProvisioningRequestErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    503: ManagementError;
};
export type GetAdminOrganizationProvisioningRequestError = GetAdminOrganizationProvisioningRequestErrors[keyof GetAdminOrganizationProvisioningRequestErrors];
export type GetAdminOrganizationProvisioningRequestResponses = {
    200: OrganizationProvisioningRequest;
};
export type GetAdminOrganizationProvisioningRequestResponse = GetAdminOrganizationProvisioningRequestResponses[keyof GetAdminOrganizationProvisioningRequestResponses];
export type DecideOrganizationProvisioningRequestData = {
    body: OrganizationProvisioningDecisionRequest;
    path: {
        request_key: string;
    };
    query?: never;
    url: '/api/admin/organization-provisioning/{request_key}/decision';
};
export type DecideOrganizationProvisioningRequestErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type DecideOrganizationProvisioningRequestError = DecideOrganizationProvisioningRequestErrors[keyof DecideOrganizationProvisioningRequestErrors];
export type DecideOrganizationProvisioningRequestResponses = {
    200: OrganizationProvisioningRequest;
    202: OrganizationProvisioningRequest;
};
export type DecideOrganizationProvisioningRequestResponse = DecideOrganizationProvisioningRequestResponses[keyof DecideOrganizationProvisioningRequestResponses];
export type ListAdminOrganizationPlansData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/admin/organization-plans';
};
export type ListAdminOrganizationPlansErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    503: ManagementError;
};
export type ListAdminOrganizationPlansError = ListAdminOrganizationPlansErrors[keyof ListAdminOrganizationPlansErrors];
export type ListAdminOrganizationPlansResponses = {
    200: OrganizationPlanListResponse;
};
export type ListAdminOrganizationPlansResponse = ListAdminOrganizationPlansResponses[keyof ListAdminOrganizationPlansResponses];
export type CreateAdminOrganizationPlanData = {
    body: OrganizationPlanCreateRequest;
    path?: never;
    query?: never;
    url: '/api/admin/organization-plans';
};
export type CreateAdminOrganizationPlanErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    503: ManagementError;
};
export type CreateAdminOrganizationPlanError = CreateAdminOrganizationPlanErrors[keyof CreateAdminOrganizationPlanErrors];
export type CreateAdminOrganizationPlanResponses = {
    201: OrganizationPlan;
};
export type CreateAdminOrganizationPlanResponse = CreateAdminOrganizationPlanResponses[keyof CreateAdminOrganizationPlanResponses];
export type DisableAdminOrganizationPlanData = {
    body: OrganizationPlanDisableRequest;
    path: {
        plan_id: string;
    };
    query?: never;
    url: '/api/admin/organization-plans/{plan_id}/disable';
};
export type DisableAdminOrganizationPlanErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type DisableAdminOrganizationPlanError = DisableAdminOrganizationPlanErrors[keyof DisableAdminOrganizationPlanErrors];
export type DisableAdminOrganizationPlanResponses = {
    200: OrganizationPlan;
};
export type DisableAdminOrganizationPlanResponse = DisableAdminOrganizationPlanResponses[keyof DisableAdminOrganizationPlanResponses];
export type GetOrganizationPlanCatalogData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/organization-plan-catalog';
};
export type GetOrganizationPlanCatalogErrors = {
    401: ManagementError;
    503: ManagementError;
};
export type GetOrganizationPlanCatalogError = GetOrganizationPlanCatalogErrors[keyof GetOrganizationPlanCatalogErrors];
export type GetOrganizationPlanCatalogResponses = {
    200: OrganizationPlanCatalogResponse;
};
export type GetOrganizationPlanCatalogResponse = GetOrganizationPlanCatalogResponses[keyof GetOrganizationPlanCatalogResponses];
export type CreateOrganizationPlanOrderData = {
    body: OrganizationPlanOrderCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/plan-orders';
};
export type CreateOrganizationPlanOrderErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type CreateOrganizationPlanOrderError = CreateOrganizationPlanOrderErrors[keyof CreateOrganizationPlanOrderErrors];
export type CreateOrganizationPlanOrderResponses = {
    200: OrganizationPlanOrder;
    201: OrganizationPlanOrder;
};
export type CreateOrganizationPlanOrderResponse = CreateOrganizationPlanOrderResponses[keyof CreateOrganizationPlanOrderResponses];
export type GetOrganizationPlanOrderData = {
    body?: never;
    path: {
        organization_id: number;
        order_id: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/plan-orders/{order_id}';
};
export type GetOrganizationPlanOrderErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    503: ManagementError;
};
export type GetOrganizationPlanOrderError = GetOrganizationPlanOrderErrors[keyof GetOrganizationPlanOrderErrors];
export type GetOrganizationPlanOrderResponses = {
    200: OrganizationPlanOrder;
};
export type GetOrganizationPlanOrderResponse = GetOrganizationPlanOrderResponses[keyof GetOrganizationPlanOrderResponses];
export type SubmitOrganizationPlanOrderPaymentData = {
    body: OrganizationPlanOrderPaymentRequest;
    path: {
        organization_id: number;
        order_id: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/plan-orders/{order_id}/payment';
};
export type SubmitOrganizationPlanOrderPaymentErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type SubmitOrganizationPlanOrderPaymentError = SubmitOrganizationPlanOrderPaymentErrors[keyof SubmitOrganizationPlanOrderPaymentErrors];
export type SubmitOrganizationPlanOrderPaymentResponses = {
    200: OrganizationPlanOrderPaymentResponse;
};
export type SubmitOrganizationPlanOrderPaymentResponse = SubmitOrganizationPlanOrderPaymentResponses[keyof SubmitOrganizationPlanOrderPaymentResponses];
export type GetOrganizationEntitlementData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/entitlement';
};
export type GetOrganizationEntitlementErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    503: ManagementError;
};
export type GetOrganizationEntitlementError = GetOrganizationEntitlementErrors[keyof GetOrganizationEntitlementErrors];
export type GetOrganizationEntitlementResponses = {
    200: OrganizationEntitlementProjection;
};
export type GetOrganizationEntitlementResponse = GetOrganizationEntitlementResponses[keyof GetOrganizationEntitlementResponses];
export type GetPlatformOrganizationEntitlementData = {
    body?: never;
    headers: {
        'x-anyflows-platform-reason': string;
    };
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/platform/organizations/{organization_id}/entitlement';
};
export type GetPlatformOrganizationEntitlementErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    503: ManagementError;
};
export type GetPlatformOrganizationEntitlementError = GetPlatformOrganizationEntitlementErrors[keyof GetPlatformOrganizationEntitlementErrors];
export type GetPlatformOrganizationEntitlementResponses = {
    200: OrganizationEntitlementProjection;
};
export type GetPlatformOrganizationEntitlementResponse = GetPlatformOrganizationEntitlementResponses[keyof GetPlatformOrganizationEntitlementResponses];
export type DiscoverOrganizationSsoData = {
    body: OrganizationSsoDiscoveryRequest;
    path?: never;
    query?: never;
    url: '/api/auth/organization-sso/discovery';
};
export type DiscoverOrganizationSsoErrors = {
    400: OrganizationSsoDiscoveryErrorResponse;
    503: OrganizationSsoDiscoveryErrorResponse;
};
export type DiscoverOrganizationSsoError = DiscoverOrganizationSsoErrors[keyof DiscoverOrganizationSsoErrors];
export type DiscoverOrganizationSsoResponses = {
    200: OrganizationSsoDiscoveryResponse;
};
export type DiscoverOrganizationSsoResponse = DiscoverOrganizationSsoResponses[keyof DiscoverOrganizationSsoResponses];
export type StartPublicOrganizationSsoLoginData = {
    body: OrganizationSsoPublicLoginStartRequest;
    path?: never;
    query?: never;
    url: '/api/auth/organization-sso/login/start';
};
export type StartPublicOrganizationSsoLoginErrors = {
    400: OrganizationSsoPublicLoginStartErrorResponse;
    422: OrganizationSsoPublicLoginStartErrorResponse;
    503: OrganizationSsoPublicLoginStartErrorResponse;
};
export type StartPublicOrganizationSsoLoginError = StartPublicOrganizationSsoLoginErrors[keyof StartPublicOrganizationSsoLoginErrors];
export type StartPublicOrganizationSsoLoginResponses = {
    200: OrganizationSsoPublicLoginStartResponse;
};
export type StartPublicOrganizationSsoLoginResponse = StartPublicOrganizationSsoLoginResponses[keyof StartPublicOrganizationSsoLoginResponses];
export type ListOrganizationSsoProvidersData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/providers';
};
export type ListOrganizationSsoProvidersErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationSsoProvidersError = ListOrganizationSsoProvidersErrors[keyof ListOrganizationSsoProvidersErrors];
export type ListOrganizationSsoProvidersResponses = {
    200: OrganizationSsoProviderList;
};
export type ListOrganizationSsoProvidersResponse = ListOrganizationSsoProvidersResponses[keyof ListOrganizationSsoProvidersResponses];
export type CreateOrganizationSsoProviderData = {
    body: OrganizationSsoProviderRequestWritable;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/providers';
};
export type CreateOrganizationSsoProviderErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationSsoProviderError = CreateOrganizationSsoProviderErrors[keyof CreateOrganizationSsoProviderErrors];
export type CreateOrganizationSsoProviderResponses = {
    200: OrganizationSsoProvider;
};
export type CreateOrganizationSsoProviderResponse = CreateOrganizationSsoProviderResponses[keyof CreateOrganizationSsoProviderResponses];
export type GetOrganizationSsoProviderData = {
    body?: never;
    path: {
        organization_id: number;
        provider_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/providers/{provider_key}';
};
export type GetOrganizationSsoProviderErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationSsoProviderError = GetOrganizationSsoProviderErrors[keyof GetOrganizationSsoProviderErrors];
export type GetOrganizationSsoProviderResponses = {
    200: OrganizationSsoProvider;
};
export type GetOrganizationSsoProviderResponse = GetOrganizationSsoProviderResponses[keyof GetOrganizationSsoProviderResponses];
export type UpdateOrganizationSsoProviderData = {
    body: OrganizationSsoProviderRequestWritable;
    path: {
        organization_id: number;
        provider_key: string;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/providers/{provider_key}';
};
export type UpdateOrganizationSsoProviderErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateOrganizationSsoProviderError = UpdateOrganizationSsoProviderErrors[keyof UpdateOrganizationSsoProviderErrors];
export type UpdateOrganizationSsoProviderResponses = {
    200: OrganizationSsoProvider;
};
export type UpdateOrganizationSsoProviderResponse = UpdateOrganizationSsoProviderResponses[keyof UpdateOrganizationSsoProviderResponses];
export type ListOrganizationSsoDomainsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/domains';
};
export type ListOrganizationSsoDomainsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationSsoDomainsError = ListOrganizationSsoDomainsErrors[keyof ListOrganizationSsoDomainsErrors];
export type ListOrganizationSsoDomainsResponses = {
    200: OrganizationSsoDomainList;
};
export type ListOrganizationSsoDomainsResponse = ListOrganizationSsoDomainsResponses[keyof ListOrganizationSsoDomainsResponses];
export type CreateOrganizationSsoDomainData = {
    body: OrganizationSsoDomainCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/domains';
};
export type CreateOrganizationSsoDomainErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationSsoDomainError = CreateOrganizationSsoDomainErrors[keyof CreateOrganizationSsoDomainErrors];
export type CreateOrganizationSsoDomainResponses = {
    200: OrganizationSsoDomainCreated;
};
export type CreateOrganizationSsoDomainResponse = CreateOrganizationSsoDomainResponses[keyof CreateOrganizationSsoDomainResponses];
export type VerifyOrganizationSsoDomainData = {
    body: OrganizationSsoDomainVerifyRequestWritable;
    path: {
        organization_id: number;
        domain_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/domains/{domain_id}/verify';
};
export type VerifyOrganizationSsoDomainErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    422: ManagementError;
    500: ManagementError;
};
export type VerifyOrganizationSsoDomainError = VerifyOrganizationSsoDomainErrors[keyof VerifyOrganizationSsoDomainErrors];
export type VerifyOrganizationSsoDomainResponses = {
    200: OrganizationSsoDomain;
};
export type VerifyOrganizationSsoDomainResponse = VerifyOrganizationSsoDomainResponses[keyof VerifyOrganizationSsoDomainResponses];
export type StartOrganizationSsoLoginData = {
    body: OrganizationSsoLoginStartRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/login/start';
};
export type StartOrganizationSsoLoginErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type StartOrganizationSsoLoginError = StartOrganizationSsoLoginErrors[keyof StartOrganizationSsoLoginErrors];
export type StartOrganizationSsoLoginResponses = {
    200: OrganizationSsoLoginStartResponse;
};
export type StartOrganizationSsoLoginResponse = StartOrganizationSsoLoginResponses[keyof StartOrganizationSsoLoginResponses];
export type BindOrganizationSsoIdentityData = {
    body: OrganizationSsoIdentityBindingRequestWritable;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/identities/bind';
};
export type BindOrganizationSsoIdentityErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    422: ManagementError;
    500: ManagementError;
};
export type BindOrganizationSsoIdentityError = BindOrganizationSsoIdentityErrors[keyof BindOrganizationSsoIdentityErrors];
export type BindOrganizationSsoIdentityResponses = {
    200: OrganizationSsoIdentityBindingResponse;
};
export type BindOrganizationSsoIdentityResponse = BindOrganizationSsoIdentityResponses[keyof BindOrganizationSsoIdentityResponses];
export type GetOrganizationSsoPolicyData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/policy';
};
export type GetOrganizationSsoPolicyErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type GetOrganizationSsoPolicyError = GetOrganizationSsoPolicyErrors[keyof GetOrganizationSsoPolicyErrors];
export type GetOrganizationSsoPolicyResponses = {
    200: OrganizationSsoPolicyResponse;
};
export type GetOrganizationSsoPolicyResponse = GetOrganizationSsoPolicyResponses[keyof GetOrganizationSsoPolicyResponses];
export type UpdateOrganizationSsoPolicyData = {
    body: OrganizationSsoPolicyRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/policy';
};
export type UpdateOrganizationSsoPolicyErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type UpdateOrganizationSsoPolicyError = UpdateOrganizationSsoPolicyErrors[keyof UpdateOrganizationSsoPolicyErrors];
export type UpdateOrganizationSsoPolicyResponses = {
    200: OrganizationSsoPolicyResponse;
};
export type UpdateOrganizationSsoPolicyResponse = UpdateOrganizationSsoPolicyResponses[keyof UpdateOrganizationSsoPolicyResponses];
export type EstablishOrganizationSsoRecoveryData = {
    body: OrganizationSsoRecoveryRequestWritable;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/recovery/establish';
};
export type EstablishOrganizationSsoRecoveryErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    422: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type EstablishOrganizationSsoRecoveryError = EstablishOrganizationSsoRecoveryErrors[keyof EstablishOrganizationSsoRecoveryErrors];
export type EstablishOrganizationSsoRecoveryResponses = {
    200: OrganizationSsoRecoveryResponse;
};
export type EstablishOrganizationSsoRecoveryResponse = EstablishOrganizationSsoRecoveryResponses[keyof EstablishOrganizationSsoRecoveryResponses];
export type SuspendOrganizationSsoRequiredData = {
    body: OrganizationSsoRecoveryRequestWritable;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/sso/recovery/suspend-required';
};
export type SuspendOrganizationSsoRequiredErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    422: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type SuspendOrganizationSsoRequiredError = SuspendOrganizationSsoRequiredErrors[keyof SuspendOrganizationSsoRequiredErrors];
export type SuspendOrganizationSsoRequiredResponses = {
    200: OrganizationSsoRecoveryResponse;
};
export type SuspendOrganizationSsoRequiredResponse = SuspendOrganizationSsoRequiredResponses[keyof SuspendOrganizationSsoRequiredResponses];
export type CompleteOrganizationSsoOidcLoginData = {
    body?: never;
    path?: never;
    query: {
        code: string;
        state: string;
    };
    url: '/api/auth/organization-sso/oidc/callback';
};
export type CompleteOrganizationSsoOidcLoginErrors = {
    400: OrganizationSsoLoginCallbackErrorResponse;
    422: OrganizationSsoLoginCallbackErrorResponse;
    503: OrganizationSsoLoginCallbackErrorResponse;
};
export type CompleteOrganizationSsoOidcLoginError = CompleteOrganizationSsoOidcLoginErrors[keyof CompleteOrganizationSsoOidcLoginErrors];
export type CompleteOrganizationSsoOidcLoginResponses = {
    200: OrganizationSsoLoginCallbackSuccessResponse;
};
export type CompleteOrganizationSsoOidcLoginResponse = CompleteOrganizationSsoOidcLoginResponses[keyof CompleteOrganizationSsoOidcLoginResponses];
export type CompleteOrganizationSsoSamlLoginData = {
    body: string;
    path?: never;
    query?: never;
    url: '/api/auth/organization-sso/saml/callback';
};
export type CompleteOrganizationSsoSamlLoginErrors = {
    400: OrganizationSsoLoginCallbackErrorResponse;
    422: OrganizationSsoLoginCallbackErrorResponse;
    503: OrganizationSsoLoginCallbackErrorResponse;
};
export type CompleteOrganizationSsoSamlLoginError = CompleteOrganizationSsoSamlLoginErrors[keyof CompleteOrganizationSsoSamlLoginErrors];
export type ExchangeOrganizationSsoSessionData = {
    body: OrganizationSsoSessionExchangeRequestWritable;
    path?: never;
    query?: never;
    url: '/api/auth/organization-sso/session/exchange';
};
export type ExchangeOrganizationSsoSessionErrors = {
    400: OrganizationSsoSessionExchangeErrorResponse;
    422: OrganizationSsoSessionExchangeErrorResponse;
    503: OrganizationSsoSessionExchangeErrorResponse;
};
export type ExchangeOrganizationSsoSessionError = ExchangeOrganizationSsoSessionErrors[keyof ExchangeOrganizationSsoSessionErrors];
export type ExchangeOrganizationSsoSessionResponses = {
    200: LoginResponse;
};
export type ExchangeOrganizationSsoSessionResponse = ExchangeOrganizationSsoSessionResponses[keyof ExchangeOrganizationSsoSessionResponses];
export type ListOrganizationTokensData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/tokens';
};
export type ListOrganizationTokensErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationTokensError = ListOrganizationTokensErrors[keyof ListOrganizationTokensErrors];
export type ListOrganizationTokensResponses = {
    200: OrganizationTokenListResponse;
};
export type ListOrganizationTokensResponse = ListOrganizationTokensResponses[keyof ListOrganizationTokensResponses];
export type CreateOrganizationTokenData = {
    body: OrganizationTokenWriteRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/tokens';
};
export type CreateOrganizationTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type CreateOrganizationTokenError = CreateOrganizationTokenErrors[keyof CreateOrganizationTokenErrors];
export type CreateOrganizationTokenResponses = {
    201: IssuedOrganizationTokenResponse;
};
export type CreateOrganizationTokenResponse = CreateOrganizationTokenResponses[keyof CreateOrganizationTokenResponses];
export type ListOrganizationTokenDirectoryData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/tokens/directory';
};
export type ListOrganizationTokenDirectoryErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationTokenDirectoryError = ListOrganizationTokenDirectoryErrors[keyof ListOrganizationTokenDirectoryErrors];
export type ListOrganizationTokenDirectoryResponses = {
    200: OrganizationTokenListResponse;
};
export type ListOrganizationTokenDirectoryResponse = ListOrganizationTokenDirectoryResponses[keyof ListOrganizationTokenDirectoryResponses];
export type DisableOwnOrganizationTokenData = {
    body?: never;
    path: {
        organization_id: number;
        token_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/tokens/{token_id}';
};
export type DisableOwnOrganizationTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type DisableOwnOrganizationTokenError = DisableOwnOrganizationTokenErrors[keyof DisableOwnOrganizationTokenErrors];
export type DisableOwnOrganizationTokenResponses = {
    200: OrganizationTokenResponse;
};
export type DisableOwnOrganizationTokenResponse = DisableOwnOrganizationTokenResponses[keyof DisableOwnOrganizationTokenResponses];
export type GetOrganizationTokenData = {
    body?: never;
    path: {
        organization_id: number;
        token_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/tokens/{token_id}';
};
export type GetOrganizationTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationTokenError = GetOrganizationTokenErrors[keyof GetOrganizationTokenErrors];
export type GetOrganizationTokenResponses = {
    200: OrganizationTokenResponse;
};
export type GetOrganizationTokenResponse = GetOrganizationTokenResponses[keyof GetOrganizationTokenResponses];
export type UpdateOrganizationTokenData = {
    body: OrganizationTokenWriteRequest;
    path: {
        organization_id: number;
        token_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/tokens/{token_id}';
};
export type UpdateOrganizationTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type UpdateOrganizationTokenError = UpdateOrganizationTokenErrors[keyof UpdateOrganizationTokenErrors];
export type UpdateOrganizationTokenResponses = {
    200: OrganizationTokenResponse;
};
export type UpdateOrganizationTokenResponse = UpdateOrganizationTokenResponses[keyof UpdateOrganizationTokenResponses];
export type DisableOrganizationTokenData = {
    body?: never;
    path: {
        organization_id: number;
        token_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/tokens/{token_id}/disable';
};
export type DisableOrganizationTokenErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
    503: ManagementError;
};
export type DisableOrganizationTokenError = DisableOrganizationTokenErrors[keyof DisableOrganizationTokenErrors];
export type DisableOrganizationTokenResponses = {
    200: OrganizationTokenResponse;
};
export type DisableOrganizationTokenResponse = DisableOrganizationTokenResponses[keyof DisableOrganizationTokenResponses];
export type ListOrganizationUsageLogsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        scope?: string;
        before?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/usage-logs';
};
export type ListOrganizationUsageLogsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationUsageLogsError = ListOrganizationUsageLogsErrors[keyof ListOrganizationUsageLogsErrors];
export type ListOrganizationUsageLogsResponses = {
    200: OrganizationUsageLogListResponse;
};
export type ListOrganizationUsageLogsResponse = ListOrganizationUsageLogsResponses[keyof ListOrganizationUsageLogsResponses];
export type ListOrganizationAuditLogsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
        actor_user_id?: number;
        operation?: string;
        target_type?: string;
        outcome?: string;
    };
    url: '/api/organizations/{organization_id}/audit-logs';
};
export type ListOrganizationAuditLogsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationAuditLogsError = ListOrganizationAuditLogsErrors[keyof ListOrganizationAuditLogsErrors];
export type ListOrganizationAuditLogsResponses = {
    200: OrganizationAuditLogListResponse;
};
export type ListOrganizationAuditLogsResponse = ListOrganizationAuditLogsResponses[keyof ListOrganizationAuditLogsResponses];
export type ListPlatformOrganizationUsageLogsData = {
    body?: never;
    headers: {
        'x-anyflows-platform-reason': string;
    };
    path: {
        organization_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/platform/organizations/{organization_id}/usage-logs';
};
export type ListPlatformOrganizationUsageLogsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListPlatformOrganizationUsageLogsError = ListPlatformOrganizationUsageLogsErrors[keyof ListPlatformOrganizationUsageLogsErrors];
export type ListPlatformOrganizationUsageLogsResponses = {
    200: OrganizationUsageLogListResponse;
};
export type ListPlatformOrganizationUsageLogsResponse = ListPlatformOrganizationUsageLogsResponses[keyof ListPlatformOrganizationUsageLogsResponses];
export type ListPlatformOrganizationAuditLogsData = {
    body?: never;
    headers: {
        'x-anyflows-platform-reason': string;
    };
    path: {
        organization_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
        actor_user_id?: number;
        operation?: string;
        target_type?: string;
        outcome?: string;
    };
    url: '/api/platform/organizations/{organization_id}/audit-logs';
};
export type ListPlatformOrganizationAuditLogsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListPlatformOrganizationAuditLogsError = ListPlatformOrganizationAuditLogsErrors[keyof ListPlatformOrganizationAuditLogsErrors];
export type ListPlatformOrganizationAuditLogsResponses = {
    200: OrganizationAuditLogListResponse;
};
export type ListPlatformOrganizationAuditLogsResponse = ListPlatformOrganizationAuditLogsResponses[keyof ListPlatformOrganizationAuditLogsResponses];
export type ListOrganizationApprovalTemplatesData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/approvals/templates';
};
export type ListOrganizationApprovalTemplatesErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationApprovalTemplatesError = ListOrganizationApprovalTemplatesErrors[keyof ListOrganizationApprovalTemplatesErrors];
export type ListOrganizationApprovalTemplatesResponses = {
    200: OrganizationApprovalTemplateListResponse;
};
export type ListOrganizationApprovalTemplatesResponse = ListOrganizationApprovalTemplatesResponses[keyof ListOrganizationApprovalTemplatesResponses];
export type CreateOrganizationApprovalTemplateData = {
    body: OrganizationApprovalTemplateCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/approvals/templates';
};
export type CreateOrganizationApprovalTemplateErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationApprovalTemplateError = CreateOrganizationApprovalTemplateErrors[keyof CreateOrganizationApprovalTemplateErrors];
export type CreateOrganizationApprovalTemplateResponses = {
    201: OrganizationApprovalTemplate;
};
export type CreateOrganizationApprovalTemplateResponse = CreateOrganizationApprovalTemplateResponses[keyof CreateOrganizationApprovalTemplateResponses];
export type ListOrganizationApprovalRequestsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        after?: number;
        limit?: number;
    };
    url: '/api/organizations/{organization_id}/approvals/requests';
};
export type ListOrganizationApprovalRequestsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationApprovalRequestsError = ListOrganizationApprovalRequestsErrors[keyof ListOrganizationApprovalRequestsErrors];
export type ListOrganizationApprovalRequestsResponses = {
    200: OrganizationApprovalRequestListResponse;
};
export type ListOrganizationApprovalRequestsResponse = ListOrganizationApprovalRequestsResponses[keyof ListOrganizationApprovalRequestsResponses];
export type CreateOrganizationApprovalRequestData = {
    body: OrganizationApprovalRequestCreateRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/approvals/requests';
};
export type CreateOrganizationApprovalRequestErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type CreateOrganizationApprovalRequestError = CreateOrganizationApprovalRequestErrors[keyof CreateOrganizationApprovalRequestErrors];
export type CreateOrganizationApprovalRequestResponses = {
    201: OrganizationApprovalRequest;
};
export type CreateOrganizationApprovalRequestResponse = CreateOrganizationApprovalRequestResponses[keyof CreateOrganizationApprovalRequestResponses];
export type GetOrganizationApprovalRequestData = {
    body?: never;
    path: {
        organization_id: number;
        request_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/approvals/requests/{request_id}';
};
export type GetOrganizationApprovalRequestErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type GetOrganizationApprovalRequestError = GetOrganizationApprovalRequestErrors[keyof GetOrganizationApprovalRequestErrors];
export type GetOrganizationApprovalRequestResponses = {
    200: OrganizationApprovalRequest;
};
export type GetOrganizationApprovalRequestResponse = GetOrganizationApprovalRequestResponses[keyof GetOrganizationApprovalRequestResponses];
export type DecideOrganizationApprovalRequestData = {
    body: OrganizationApprovalDecisionRequest;
    path: {
        organization_id: number;
        request_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/approvals/requests/{request_id}/decision';
};
export type DecideOrganizationApprovalRequestErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type DecideOrganizationApprovalRequestError = DecideOrganizationApprovalRequestErrors[keyof DecideOrganizationApprovalRequestErrors];
export type DecideOrganizationApprovalRequestResponses = {
    200: OrganizationApprovalRequest;
};
export type DecideOrganizationApprovalRequestResponse = DecideOrganizationApprovalRequestResponses[keyof DecideOrganizationApprovalRequestResponses];
export type ListOrganizationApprovalNotificationsData = {
    body?: never;
    path?: never;
    query?: {
        before?: string;
        limit?: number;
    };
    url: '/api/account/organization-approval-notifications';
};
export type ListOrganizationApprovalNotificationsErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type ListOrganizationApprovalNotificationsError = ListOrganizationApprovalNotificationsErrors[keyof ListOrganizationApprovalNotificationsErrors];
export type ListOrganizationApprovalNotificationsResponses = {
    200: OrganizationApprovalNotificationListResponse;
};
export type ListOrganizationApprovalNotificationsResponse = ListOrganizationApprovalNotificationsResponses[keyof ListOrganizationApprovalNotificationsResponses];
export type MarkOrganizationApprovalNotificationsReadData = {
    body: OrganizationApprovalNotificationMarkReadRequest;
    path?: never;
    query?: never;
    url: '/api/account/organization-approval-notifications/read';
};
export type MarkOrganizationApprovalNotificationsReadErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type MarkOrganizationApprovalNotificationsReadError = MarkOrganizationApprovalNotificationsReadErrors[keyof MarkOrganizationApprovalNotificationsReadErrors];
export type MarkOrganizationApprovalNotificationsReadResponses = {
    200: OrganizationApprovalNotificationMarkReadResponse;
};
export type MarkOrganizationApprovalNotificationsReadResponse = MarkOrganizationApprovalNotificationsReadResponses[keyof MarkOrganizationApprovalNotificationsReadResponses];
export type ListOrganizationScimAuditLogsData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: {
        before?: number;
        limit?: number;
        event_type?: string;
        result?: string;
        reason?: string;
        token_key?: string;
        resource_kind?: string;
    };
    url: '/api/organizations/{organization_id}/scim/audit-logs';
};
export type ListOrganizationScimAuditLogsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ListOrganizationScimAuditLogsError = ListOrganizationScimAuditLogsErrors[keyof ListOrganizationScimAuditLogsErrors];
export type ListOrganizationScimAuditLogsResponses = {
    200: OrganizationScimAuditLogListResponse;
};
export type ListOrganizationScimAuditLogsResponse = ListOrganizationScimAuditLogsResponses[keyof ListOrganizationScimAuditLogsResponses];
export type AuthenticateOrganizationServiceAccountRuntimeData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/api/organizations/{organization_id}/service-accounts/runtime/authenticate';
};
export type AuthenticateOrganizationServiceAccountRuntimeErrors = {
    401: OrganizationServiceAccountRuntimeErrorResponse;
    503: OrganizationServiceAccountRuntimeErrorResponse;
};
export type AuthenticateOrganizationServiceAccountRuntimeError = AuthenticateOrganizationServiceAccountRuntimeErrors[keyof AuthenticateOrganizationServiceAccountRuntimeErrors];
export type AuthenticateOrganizationServiceAccountRuntimeResponses = {
    204: void;
};
export type AuthenticateOrganizationServiceAccountRuntimeResponse = AuthenticateOrganizationServiceAccountRuntimeResponses[keyof AuthenticateOrganizationServiceAccountRuntimeResponses];
export type GetScimServiceProviderConfigData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/scim/v2/{organization_id}/ServiceProviderConfig';
};
export type GetScimServiceProviderConfigErrors = {
    401: ScimErrorResponse;
    503: ScimErrorResponse;
};
export type GetScimServiceProviderConfigError = GetScimServiceProviderConfigErrors[keyof GetScimServiceProviderConfigErrors];
export type GetScimServiceProviderConfigResponses = {
    200: ServiceProviderConfigResponse;
};
export type GetScimServiceProviderConfigResponse = GetScimServiceProviderConfigResponses[keyof GetScimServiceProviderConfigResponses];
export type ListScimResourceTypesData = {
    body?: never;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/scim/v2/{organization_id}/ResourceTypes';
};
export type ListScimResourceTypesErrors = {
    401: ScimErrorResponse;
    503: ScimErrorResponse;
};
export type ListScimResourceTypesError = ListScimResourceTypesErrors[keyof ListScimResourceTypesErrors];
export type ListScimResourceTypesResponses = {
    200: ResourceTypeListResponse;
};
export type ListScimResourceTypesResponse = ListScimResourceTypesResponses[keyof ListScimResourceTypesResponses];
export type CreateScimUserData = {
    body: ScimUserWriteRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/scim/v2/{organization_id}/Users';
};
export type CreateScimUserErrors = {
    400: ScimErrorResponse;
    401: ScimErrorResponse;
    403: ScimErrorResponse;
    409: ScimErrorResponse;
    422: ScimErrorResponse;
    503: ScimErrorResponse;
};
export type CreateScimUserError = CreateScimUserErrors[keyof CreateScimUserErrors];
export type CreateScimUserResponses = {
    201: ScimUserResponse;
};
export type CreateScimUserResponse = CreateScimUserResponses[keyof CreateScimUserResponses];
export type ReplaceScimUserData = {
    body: ScimUserWriteRequest;
    path: {
        organization_id: number;
        resource_id: string;
    };
    query?: never;
    url: '/scim/v2/{organization_id}/Users/{resource_id}';
};
export type ReplaceScimUserErrors = {
    400: ScimErrorResponse;
    401: ScimErrorResponse;
    403: ScimErrorResponse;
    404: ScimErrorResponse;
    409: ScimErrorResponse;
    422: ScimErrorResponse;
    503: ScimErrorResponse;
};
export type ReplaceScimUserError = ReplaceScimUserErrors[keyof ReplaceScimUserErrors];
export type ReplaceScimUserResponses = {
    200: ScimUserResponse;
};
export type ReplaceScimUserResponse = ReplaceScimUserResponses[keyof ReplaceScimUserResponses];
export type CreateScimGroupData = {
    body: ScimGroupWriteRequest;
    path: {
        organization_id: number;
    };
    query?: never;
    url: '/scim/v2/{organization_id}/Groups';
};
export type CreateScimGroupErrors = {
    400: ScimErrorResponse;
    401: ScimErrorResponse;
    403: ScimErrorResponse;
    404: ScimErrorResponse;
    409: ScimErrorResponse;
    422: ScimErrorResponse;
    503: ScimErrorResponse;
};
export type CreateScimGroupError = CreateScimGroupErrors[keyof CreateScimGroupErrors];
export type CreateScimGroupResponses = {
    201: ScimGroupResponse;
};
export type CreateScimGroupResponse = CreateScimGroupResponses[keyof CreateScimGroupResponses];
export type DeleteScimGroupData = {
    body?: never;
    headers: {
        'if-match': string;
    };
    path: {
        organization_id: number;
        resource_id: string;
    };
    query?: never;
    url: '/scim/v2/{organization_id}/Groups/{resource_id}';
};
export type DeleteScimGroupErrors = {
    400: ScimErrorResponse;
    401: ScimErrorResponse;
    403: ScimErrorResponse;
    404: ScimErrorResponse;
    409: ScimErrorResponse;
    503: ScimErrorResponse;
};
export type DeleteScimGroupError = DeleteScimGroupErrors[keyof DeleteScimGroupErrors];
export type DeleteScimGroupResponses = {
    204: void;
};
export type DeleteScimGroupResponse = DeleteScimGroupResponses[keyof DeleteScimGroupResponses];
export type ReplaceScimGroupData = {
    body: ScimGroupWriteRequest;
    path: {
        organization_id: number;
        resource_id: string;
    };
    query?: never;
    url: '/scim/v2/{organization_id}/Groups/{resource_id}';
};
export type ReplaceScimGroupErrors = {
    400: ScimErrorResponse;
    401: ScimErrorResponse;
    403: ScimErrorResponse;
    404: ScimErrorResponse;
    409: ScimErrorResponse;
    422: ScimErrorResponse;
    503: ScimErrorResponse;
};
export type ReplaceScimGroupError = ReplaceScimGroupErrors[keyof ReplaceScimGroupErrors];
export type ReplaceScimGroupResponses = {
    200: ScimGroupResponse;
};
export type ReplaceScimGroupResponse = ReplaceScimGroupResponses[keyof ReplaceScimGroupResponses];
export type StartPasskeyAuthenticationData = {
    body: PasskeyAuthenticationOptionsRequest;
    path?: never;
    query?: never;
    url: '/api/auth/passkey/options';
};
export type StartPasskeyAuthenticationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type StartPasskeyAuthenticationError = StartPasskeyAuthenticationErrors[keyof StartPasskeyAuthenticationErrors];
export type StartPasskeyAuthenticationResponses = {
    200: PasskeyAuthenticationOptionsResponse;
};
export type StartPasskeyAuthenticationResponse = StartPasskeyAuthenticationResponses[keyof StartPasskeyAuthenticationResponses];
export type FinishPasskeyAuthenticationData = {
    body: PasskeyAuthenticationVerifyRequest;
    path?: never;
    query?: never;
    url: '/api/auth/passkey/verify';
};
export type FinishPasskeyAuthenticationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type FinishPasskeyAuthenticationError = FinishPasskeyAuthenticationErrors[keyof FinishPasskeyAuthenticationErrors];
export type FinishPasskeyAuthenticationResponses = {
    200: LoginResponse;
};
export type FinishPasskeyAuthenticationResponse = FinishPasskeyAuthenticationResponses[keyof FinishPasskeyAuthenticationResponses];
export type ListAdminPlatformAuditLogsData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/admin/audit-logs';
};
export type ListAdminPlatformAuditLogsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminPlatformAuditLogsError = ListAdminPlatformAuditLogsErrors[keyof ListAdminPlatformAuditLogsErrors];
export type ListAdminPlatformAuditLogsResponses = {
    200: PlatformAuditLogListResponse;
};
export type ListAdminPlatformAuditLogsResponse = ListAdminPlatformAuditLogsResponses[keyof ListAdminPlatformAuditLogsResponses];
export type ListSelfPlatformAuditLogsData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        limit?: number;
    };
    url: '/api/account/audit-logs';
};
export type ListSelfPlatformAuditLogsErrors = {
    400: ManagementError;
    401: ManagementError;
    500: ManagementError;
};
export type ListSelfPlatformAuditLogsError = ListSelfPlatformAuditLogsErrors[keyof ListSelfPlatformAuditLogsErrors];
export type ListSelfPlatformAuditLogsResponses = {
    200: PlatformAuditLogListResponse;
};
export type ListSelfPlatformAuditLogsResponse = ListSelfPlatformAuditLogsResponses[keyof ListSelfPlatformAuditLogsResponses];
export type StartOidcLoginData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/auth/oauth/oidc/start';
};
export type StartOidcLoginErrors = {
    409: ManagementError;
    500: ManagementError;
};
export type StartOidcLoginError = StartOidcLoginErrors[keyof StartOidcLoginErrors];
export type StartOidcLoginResponses = {
    200: OAuthLoginStartResponse;
};
export type StartOidcLoginResponse = StartOidcLoginResponses[keyof StartOidcLoginResponses];
export type StartLinuxDoLoginData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/auth/oauth/linuxdo/start';
};
export type StartLinuxDoLoginErrors = {
    409: ManagementError;
    500: ManagementError;
};
export type StartLinuxDoLoginError = StartLinuxDoLoginErrors[keyof StartLinuxDoLoginErrors];
export type StartLinuxDoLoginResponses = {
    200: OAuthLoginStartResponse;
};
export type StartLinuxDoLoginResponse = StartLinuxDoLoginResponses[keyof StartLinuxDoLoginResponses];
export type StartWeChatOAuthLoginData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/auth/oauth/wechat/start';
};
export type StartWeChatOAuthLoginErrors = {
    409: ManagementError;
    500: ManagementError;
};
export type StartWeChatOAuthLoginError = StartWeChatOAuthLoginErrors[keyof StartWeChatOAuthLoginErrors];
export type StartWeChatOAuthLoginResponses = {
    200: OAuthLoginStartResponse;
};
export type StartWeChatOAuthLoginResponse = StartWeChatOAuthLoginResponses[keyof StartWeChatOAuthLoginResponses];
export type StartTelegramLoginData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/auth/oauth/telegram/start';
};
export type StartTelegramLoginErrors = {
    409: ManagementError;
    500: ManagementError;
};
export type StartTelegramLoginError = StartTelegramLoginErrors[keyof StartTelegramLoginErrors];
export type StartTelegramLoginResponses = {
    200: OAuthLoginStartResponse;
};
export type StartTelegramLoginResponse = StartTelegramLoginResponses[keyof StartTelegramLoginResponses];
export type StartGoogleLoginData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/auth/oauth/google/start';
};
export type StartGoogleLoginErrors = {
    409: ManagementError;
    500: ManagementError;
};
export type StartGoogleLoginError = StartGoogleLoginErrors[keyof StartGoogleLoginErrors];
export type StartGoogleLoginResponses = {
    200: OAuthLoginStartResponse;
};
export type StartGoogleLoginResponse = StartGoogleLoginResponses[keyof StartGoogleLoginResponses];
export type StartCustomOAuth2LoginData = {
    body?: never;
    path: {
        provider_key: string;
    };
    query?: never;
    url: '/api/auth/oauth/custom/{provider_key}/start';
};
export type StartCustomOAuth2LoginErrors = {
    400: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type StartCustomOAuth2LoginError = StartCustomOAuth2LoginErrors[keyof StartCustomOAuth2LoginErrors];
export type StartCustomOAuth2LoginResponses = {
    200: OAuthLoginStartResponse;
};
export type StartCustomOAuth2LoginResponse = StartCustomOAuth2LoginResponses[keyof StartCustomOAuth2LoginResponses];
export type CompleteOidcLoginData = {
    body?: never;
    path?: never;
    query?: {
        state?: string;
        code?: string;
        error?: string;
    };
    url: '/api/auth/oauth/oidc/callback';
};
export type CompleteOidcLoginErrors = {
    500: ManagementError;
};
export type CompleteOidcLoginError = CompleteOidcLoginErrors[keyof CompleteOidcLoginErrors];
export type CompleteLinuxDoLoginData = {
    body?: never;
    path?: never;
    query?: {
        state?: string;
        code?: string;
        error?: string;
    };
    url: '/api/auth/oauth/linuxdo/callback';
};
export type CompleteLinuxDoLoginErrors = {
    500: ManagementError;
};
export type CompleteLinuxDoLoginError = CompleteLinuxDoLoginErrors[keyof CompleteLinuxDoLoginErrors];
export type CompleteWeChatOAuthLoginData = {
    body?: never;
    path?: never;
    query?: {
        state?: string;
        code?: string;
    };
    url: '/api/auth/oauth/wechat/callback';
};
export type CompleteWeChatOAuthLoginErrors = {
    500: ManagementError;
};
export type CompleteWeChatOAuthLoginError = CompleteWeChatOAuthLoginErrors[keyof CompleteWeChatOAuthLoginErrors];
export type CompleteTelegramLoginData = {
    body?: never;
    path?: never;
    query?: {
        state?: string;
        code?: string;
        error?: string;
    };
    url: '/api/auth/oauth/telegram/callback';
};
export type CompleteTelegramLoginErrors = {
    500: ManagementError;
};
export type CompleteTelegramLoginError = CompleteTelegramLoginErrors[keyof CompleteTelegramLoginErrors];
export type CompleteGoogleLoginData = {
    body?: never;
    path?: never;
    query?: {
        state?: string;
        code?: string;
        error?: string;
    };
    url: '/api/auth/oauth/google/callback';
};
export type CompleteGoogleLoginErrors = {
    500: ManagementError;
};
export type CompleteGoogleLoginError = CompleteGoogleLoginErrors[keyof CompleteGoogleLoginErrors];
export type CompleteCustomOAuth2LoginData = {
    body?: never;
    path: {
        provider_key: string;
    };
    query?: {
        state?: string;
        code?: string;
        error?: string;
    };
    url: '/api/auth/oauth/custom/{provider_key}/callback';
};
export type CompleteCustomOAuth2LoginErrors = {
    400: ManagementError;
    500: ManagementError;
};
export type CompleteCustomOAuth2LoginError = CompleteCustomOAuth2LoginErrors[keyof CompleteCustomOAuth2LoginErrors];
export type GetAdminOidcLoginSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/oidc';
};
export type GetAdminOidcLoginSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminOidcLoginSettingsError = GetAdminOidcLoginSettingsErrors[keyof GetAdminOidcLoginSettingsErrors];
export type GetAdminOidcLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type GetAdminOidcLoginSettingsResponse = GetAdminOidcLoginSettingsResponses[keyof GetAdminOidcLoginSettingsResponses];
export type UpdateAdminOidcLoginSettingsData = {
    body: AdminOAuthLoginProviderSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/oidc';
};
export type UpdateAdminOidcLoginSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminOidcLoginSettingsError = UpdateAdminOidcLoginSettingsErrors[keyof UpdateAdminOidcLoginSettingsErrors];
export type UpdateAdminOidcLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type UpdateAdminOidcLoginSettingsResponse = UpdateAdminOidcLoginSettingsResponses[keyof UpdateAdminOidcLoginSettingsResponses];
export type GetAdminLinuxDoLoginSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/linuxdo';
};
export type GetAdminLinuxDoLoginSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminLinuxDoLoginSettingsError = GetAdminLinuxDoLoginSettingsErrors[keyof GetAdminLinuxDoLoginSettingsErrors];
export type GetAdminLinuxDoLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type GetAdminLinuxDoLoginSettingsResponse = GetAdminLinuxDoLoginSettingsResponses[keyof GetAdminLinuxDoLoginSettingsResponses];
export type UpdateAdminLinuxDoLoginSettingsData = {
    body: AdminOAuthLoginProviderSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/linuxdo';
};
export type UpdateAdminLinuxDoLoginSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminLinuxDoLoginSettingsError = UpdateAdminLinuxDoLoginSettingsErrors[keyof UpdateAdminLinuxDoLoginSettingsErrors];
export type UpdateAdminLinuxDoLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type UpdateAdminLinuxDoLoginSettingsResponse = UpdateAdminLinuxDoLoginSettingsResponses[keyof UpdateAdminLinuxDoLoginSettingsResponses];
export type GetAdminWeChatOAuthLoginSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/wechat';
};
export type GetAdminWeChatOAuthLoginSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminWeChatOAuthLoginSettingsError = GetAdminWeChatOAuthLoginSettingsErrors[keyof GetAdminWeChatOAuthLoginSettingsErrors];
export type GetAdminWeChatOAuthLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type GetAdminWeChatOAuthLoginSettingsResponse = GetAdminWeChatOAuthLoginSettingsResponses[keyof GetAdminWeChatOAuthLoginSettingsResponses];
export type UpdateAdminWeChatOAuthLoginSettingsData = {
    body: AdminOAuthLoginProviderSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/wechat';
};
export type UpdateAdminWeChatOAuthLoginSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminWeChatOAuthLoginSettingsError = UpdateAdminWeChatOAuthLoginSettingsErrors[keyof UpdateAdminWeChatOAuthLoginSettingsErrors];
export type UpdateAdminWeChatOAuthLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type UpdateAdminWeChatOAuthLoginSettingsResponse = UpdateAdminWeChatOAuthLoginSettingsResponses[keyof UpdateAdminWeChatOAuthLoginSettingsResponses];
export type GetAdminTelegramOAuthLoginSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/telegram';
};
export type GetAdminTelegramOAuthLoginSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminTelegramOAuthLoginSettingsError = GetAdminTelegramOAuthLoginSettingsErrors[keyof GetAdminTelegramOAuthLoginSettingsErrors];
export type GetAdminTelegramOAuthLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type GetAdminTelegramOAuthLoginSettingsResponse = GetAdminTelegramOAuthLoginSettingsResponses[keyof GetAdminTelegramOAuthLoginSettingsResponses];
export type UpdateAdminTelegramOAuthLoginSettingsData = {
    body: AdminOAuthLoginProviderSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/telegram';
};
export type UpdateAdminTelegramOAuthLoginSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminTelegramOAuthLoginSettingsError = UpdateAdminTelegramOAuthLoginSettingsErrors[keyof UpdateAdminTelegramOAuthLoginSettingsErrors];
export type UpdateAdminTelegramOAuthLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type UpdateAdminTelegramOAuthLoginSettingsResponse = UpdateAdminTelegramOAuthLoginSettingsResponses[keyof UpdateAdminTelegramOAuthLoginSettingsResponses];
export type GetAdminGoogleOAuthLoginSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/google';
};
export type GetAdminGoogleOAuthLoginSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminGoogleOAuthLoginSettingsError = GetAdminGoogleOAuthLoginSettingsErrors[keyof GetAdminGoogleOAuthLoginSettingsErrors];
export type GetAdminGoogleOAuthLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type GetAdminGoogleOAuthLoginSettingsResponse = GetAdminGoogleOAuthLoginSettingsResponses[keyof GetAdminGoogleOAuthLoginSettingsResponses];
export type UpdateAdminGoogleOAuthLoginSettingsData = {
    body: AdminOAuthLoginProviderSettingsRequest;
    path?: never;
    query?: never;
    url: '/api/admin/authentication-settings/oauth/google';
};
export type UpdateAdminGoogleOAuthLoginSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminGoogleOAuthLoginSettingsError = UpdateAdminGoogleOAuthLoginSettingsErrors[keyof UpdateAdminGoogleOAuthLoginSettingsErrors];
export type UpdateAdminGoogleOAuthLoginSettingsResponses = {
    200: AdminOAuthLoginProviderSettings;
};
export type UpdateAdminGoogleOAuthLoginSettingsResponse = UpdateAdminGoogleOAuthLoginSettingsResponses[keyof UpdateAdminGoogleOAuthLoginSettingsResponses];
