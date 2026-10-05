// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

export type ClientOptions = {
    baseUrl: `${string}://${string}` | (string & {});
};
export type AccountVerificationDecisionRequest = {
    expected_version: number;
    status: number;
    reason?: string | null;
};
export type AccountVerificationDetailResponse = {
    case: AccountVerificationResponse;
    materials: Array<AccountVerificationMaterialResponse>;
};
export type AccountVerificationEligibilityResponse = {
    enterprise_verified: boolean;
    can_apply_for_organization: boolean;
    providers: Array<string>;
    individual_providers: Array<string>;
    enterprise_providers: Array<string>;
    individual_reason_required: boolean;
    enterprise_reason_required: boolean;
};
export type AccountVerificationListResponse = {
    cases: Array<AccountVerificationResponse>;
    next_cursor?: number | null;
};
export type AccountVerificationMaterialResponse = {
    id: number;
    case_id: number;
    kind: string;
    file_name: string;
    content_type: string;
    size_bytes: number;
};
export type AccountVerificationMultipartRequest = {
    metadata: string;
    file_0?: Blob | File | null;
};
export type AccountVerificationResponse = {
    id: number;
    user_id: number;
    kind: string;
    provider: string;
    provider_action_url?: string | null;
    provider_expires_at?: number | null;
    server_time: number;
    provider_status?: string | null;
    document_country: string;
    document_type: string;
    document_number_masked?: string | null;
    subject_name: string;
    summary: string;
    status: number;
    version: number;
    review_reason?: string | null;
    reviewer_user_id?: number | null;
    created_at: number;
    updated_at: number;
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
    provider?: string | null;
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
    provider?: string | null;
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
    provider?: string | null;
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
export type AdminCredentialImportFile = {
    name: string;
    content: string;
};
export type AdminCredentialImportItem = {
    file: string;
    index: number;
    action: string;
    credential_id?: number | null;
    message: string;
};
export type AdminCredentialImportRequest = {
    files: Array<AdminCredentialImportFile>;
};
export type AdminCredentialImportResponse = {
    total: number;
    created: number;
    skipped: number;
    failed: number;
    items: Array<AdminCredentialImportItem>;
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
export type AdminFailedCallLog = {
    id: number;
    request_id: string;
    model: string;
    protocol: UsageLogProtocol;
    operation: UsageLogOperation;
    error_kind: string;
    error_code: string;
    error_message: string;
    user_id: number | null;
    username: string | null;
    token_id: number | null;
    group_id: number | null;
    organization_id: number | null;
    organization_team_id: number | null;
    channel_id: number | null;
    duration_ms: number;
    created_at: number;
};
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
    epay_qr_enabled: boolean;
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
    epay_qr_enabled: boolean;
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
export type AdminSiteNavigationRequest = {
    navigation: SiteNavigation;
    expected_version: number;
};
export type AdminSiteSettings = {
    site_name: string;
    public_base_url?: string | null;
    brand: PublicBrandSettings;
    navigation: SiteNavigation;
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
    failed_logs: Array<AdminFailedCallLog>;
    next_cursor: number | null;
    failed_next_cursor: number | null;
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
export type AdminVerificationSettings = {
    source: string;
    manual_enabled: boolean;
    individual_manual_enabled: boolean;
    enterprise_manual_enabled: boolean;
    individual_reason_required: boolean;
    enterprise_reason_required: boolean;
    enabled: boolean;
    app_id?: string | null;
    private_key_configured: boolean;
    public_key_configured: boolean;
    gateway_url: string;
    biz_code: string;
    timeout_secs: number;
    version: number;
};
export type AdminVerificationSettingsRequest = {
    expected_version: number;
    manual_enabled: boolean;
    individual_manual_enabled?: boolean | null;
    enterprise_manual_enabled?: boolean | null;
    individual_reason_required?: boolean | null;
    enterprise_reason_required?: boolean | null;
    enabled: boolean;
    app_id?: string | null;
    gateway_url: string;
    biz_code: string;
    timeout_secs: number;
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
export type AnalyticsExportHealthState = 'disabled' | 'healthy' | 'backlog' | 'unavailable';
export type AnalyticsExportReplayRequest = {
    limit?: number | null;
};
export type AnalyticsExportReplayResponse = {
    replayed_count: number;
    backlog_count?: number | null;
};
export type AnalyticsExportStatusResponse = {
    enabled: boolean;
    state: AnalyticsExportHealthState;
    pending_count?: number | null;
    leased_count?: number | null;
    published_count?: number | null;
    backlog_count?: number | null;
};
export type Announcement = {
    id: number;
    version: number;
    status: string;
    audience: AnnouncementAudience;
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
export type AnnouncementAudience = 'public' | 'authenticated';
export type AnnouncementAudienceRequest = 'public' | 'authenticated';
export type AnnouncementListResponse = {
    entries: Array<Announcement>;
};
export type AnnouncementMutationRequest = {
    expected_version: number;
};
export type AnnouncementUpdateRequest = {
    audience?: null | AnnouncementAudienceRequest;
    title_zh: string;
    title_en: string;
    body_zh: string;
    body_en: string;
    visible_from?: number | null;
    visible_until?: number | null;
    expected_version: number;
};
export type AnnouncementWriteRequest = {
    audience?: null | AnnouncementAudienceRequest;
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
export type CredentialUsageSnapshot = {
    status: CredentialUsageStatus;
    windows: Array<CredentialUsageWindow>;
    credits_balance?: string | null;
    fetched_at?: number | null;
};
export type CredentialUsageStatus = 'available' | 'unsupported' | 'unavailable';
export type CredentialUsageWindow = {
    window_seconds: number;
    used_percent: number;
    reset_at?: number | null;
};
export type FrontendTemplateActivationRequest = {
    template_id?: string | null;
};
export type FrontendTemplateCatalog = {
    active_id?: string | null;
    templates: Array<FrontendTemplateSummary>;
};
export type FrontendTemplateImage = Blob | File;
export type FrontendTemplateSummary = {
    id: string;
    name: string;
    version: string;
    api_contract: string;
    builtin: boolean;
    description?: string | null;
    author?: string | null;
    preview_url?: string | null;
    valid: boolean;
    error?: string | null;
};
export type HttpExtensionDescriptor = {
    id: string;
    name: string;
    capabilities: Array<string>;
};
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
export type NotificationChannel = 'email' | 'in_app';
export type NotificationDeliveryState = 'queued' | 'accepted' | 'failed' | 'canceled' | 'available';
export type NotificationKind = 'balance_alert' | 'subscription_balance_alert' | 'subscription_purchase' | 'product_update';
export type OAuthLoginExchangeRequest = {
    ticket: string;
};
export type OAuthLoginStartResponse = {
    authorization_url: string;
};
export type OpenAiModel = {
    id: string;
    object: string;
    created: number;
    owned_by: string;
};
export type OpenAiModelList = {
    object: string;
    data: Array<OpenAiModel>;
};
export type OpenAiModelListError = {
    error: OpenAiModelListErrorBody;
};
export type OpenAiModelListErrorBody = {
    code: string;
    message: string;
    param: string | null;
    type: string;
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
    navigation: SiteNavigation;
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
export type ServiceLevelPoint = {
    period_start: number;
    successful_request_count: number;
    failed_request_count: number;
    unknown_request_count: number;
};
export type ServiceLevelReport = {
    period_start: number;
    period_end: number;
    total: number;
    unattributed_request_count: number;
    items: Array<ServiceLevelRow>;
};
export type ServiceLevelRow = {
    key: string;
    name: string;
    request_count: number;
    successful_request_count: number;
    failed_request_count: number;
    unknown_request_count: number;
    average_duration_ms?: number | null;
    hourly: Array<ServiceLevelPoint>;
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
export type SiteNavigation = {
    header_links: Array<SiteNavigationLink>;
    footer_groups: Array<SiteNavigationGroup>;
    sidebar_links: Array<SiteSidebarLink>;
};
export type SiteNavigationGroup = {
    title: string;
    title_en?: string | null;
    links: Array<SiteNavigationLink>;
};
export type SiteNavigationLink = {
    label: string;
    label_en?: string | null;
    url: string;
};
export type SiteSidebarLink = {
    label: string;
    label_en?: string | null;
    url: string;
    icon: string;
    kind?: string;
    level?: number;
    style?: string;
    audience: string;
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
export type UserEmailBindingConfirmRequest = {
    email: string;
};
export type UserEmailBindingVerificationRequest = {
    email: string;
};
export type UserEmailBindingVerificationResponse = {
    expires_at: number;
    next_send_at: number;
};
export type UserFailedCallLog = {
    id: number;
    request_id: string;
    model: string;
    protocol: UsageLogProtocol;
    operation: UsageLogOperation;
    error_code: string;
    error_message: string;
    duration_ms: number;
    created_at: number;
};
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
    announcement_id?: number | null;
    announcement_version?: number | null;
    announcement_title_zh?: string | null;
    announcement_title_en?: string | null;
    announcement_body_zh?: string | null;
    announcement_body_en?: string | null;
    announcement_status?: number | null;
    announcement_visible_until?: number | null;
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
    qr_enabled: boolean;
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
    failed_logs: Array<UserFailedCallLog>;
    next_cursor: number | null;
    failed_next_cursor: number | null;
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
    provider?: string | null;
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
    provider?: string | null;
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
    epay_qr_enabled: boolean;
    epay_refund_enabled: boolean;
    refund_auto_submit_enabled: boolean;
    epay_quota_per_cny: number;
};
export type AdminVerificationSettingsRequestWritable = {
    expected_version: number;
    manual_enabled: boolean;
    individual_manual_enabled?: boolean | null;
    enterprise_manual_enabled?: boolean | null;
    individual_reason_required?: boolean | null;
    enterprise_reason_required?: boolean | null;
    enabled: boolean;
    app_id?: string | null;
    private_key?: string | null;
    public_key?: string | null;
    gateway_url: string;
    biz_code: string;
    timeout_secs: number;
};
export type LoginRequestWritable = {
    username: string;
    password: string;
    totp_code?: string | null;
    turnstile_token?: string | null;
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
export type UserEmailBindingConfirmRequestWritable = {
    email: string;
    verification_code: string;
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
export type SendUserEmailBindingVerificationData = {
    body: UserEmailBindingVerificationRequest;
    path?: never;
    query?: never;
    url: '/api/account/profile/email-verification';
};
export type SendUserEmailBindingVerificationErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    429: ManagementError;
    500: ManagementError;
    502: ManagementError;
};
export type SendUserEmailBindingVerificationError = SendUserEmailBindingVerificationErrors[keyof SendUserEmailBindingVerificationErrors];
export type SendUserEmailBindingVerificationResponses = {
    200: UserEmailBindingVerificationResponse;
};
export type SendUserEmailBindingVerificationResponse = SendUserEmailBindingVerificationResponses[keyof SendUserEmailBindingVerificationResponses];
export type ConfirmUserEmailBindingData = {
    body: UserEmailBindingConfirmRequestWritable;
    path?: never;
    query?: never;
    url: '/api/account/profile/email';
};
export type ConfirmUserEmailBindingErrors = {
    400: ManagementError;
    401: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ConfirmUserEmailBindingError = ConfirmUserEmailBindingErrors[keyof ConfirmUserEmailBindingErrors];
export type ConfirmUserEmailBindingResponses = {
    200: UserProfileResponse;
};
export type ConfirmUserEmailBindingResponse = ConfirmUserEmailBindingResponses[keyof ConfirmUserEmailBindingResponses];
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
export type ListExtensionCatalogData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/extensions';
};
export type ListExtensionCatalogResponses = {
    200: Array<HttpExtensionDescriptor>;
};
export type ListExtensionCatalogResponse = ListExtensionCatalogResponses[keyof ListExtensionCatalogResponses];
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
export type UpdateAdminSiteNavigationData = {
    body: AdminSiteNavigationRequest;
    path?: never;
    query?: never;
    url: '/api/admin/site-settings/navigation';
};
export type UpdateAdminSiteNavigationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminSiteNavigationError = UpdateAdminSiteNavigationErrors[keyof UpdateAdminSiteNavigationErrors];
export type UpdateAdminSiteNavigationResponses = {
    200: AdminSiteSettings;
};
export type UpdateAdminSiteNavigationResponse = UpdateAdminSiteNavigationResponses[keyof UpdateAdminSiteNavigationResponses];
export type ListAdminFrontendTemplatesData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/frontend-templates';
};
export type ListAdminFrontendTemplatesErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ListAdminFrontendTemplatesError = ListAdminFrontendTemplatesErrors[keyof ListAdminFrontendTemplatesErrors];
export type ListAdminFrontendTemplatesResponses = {
    200: FrontendTemplateCatalog;
};
export type ListAdminFrontendTemplatesResponse = ListAdminFrontendTemplatesResponses[keyof ListAdminFrontendTemplatesResponses];
export type ScanAdminFrontendTemplatesData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/frontend-templates/scan';
};
export type ScanAdminFrontendTemplatesErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ScanAdminFrontendTemplatesError = ScanAdminFrontendTemplatesErrors[keyof ScanAdminFrontendTemplatesErrors];
export type ScanAdminFrontendTemplatesResponses = {
    200: FrontendTemplateCatalog;
};
export type ScanAdminFrontendTemplatesResponse = ScanAdminFrontendTemplatesResponses[keyof ScanAdminFrontendTemplatesResponses];
export type ActivateAdminFrontendTemplateData = {
    body: FrontendTemplateActivationRequest;
    path?: never;
    query?: never;
    url: '/api/admin/frontend-templates/active';
};
export type ActivateAdminFrontendTemplateErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type ActivateAdminFrontendTemplateError = ActivateAdminFrontendTemplateErrors[keyof ActivateAdminFrontendTemplateErrors];
export type ActivateAdminFrontendTemplateResponses = {
    200: FrontendTemplateCatalog;
};
export type ActivateAdminFrontendTemplateResponse = ActivateAdminFrontendTemplateResponses[keyof ActivateAdminFrontendTemplateResponses];
export type GetAdminFrontendTemplatePreviewData = {
    body?: never;
    path: {
        template_id: string;
    };
    query?: never;
    url: '/api/admin/frontend-templates/{template_id}/preview';
};
export type GetAdminFrontendTemplatePreviewErrors = {
    401: ManagementError;
    403: ManagementError;
    404: unknown;
    500: ManagementError;
};
export type GetAdminFrontendTemplatePreviewError = GetAdminFrontendTemplatePreviewErrors[keyof GetAdminFrontendTemplatePreviewErrors];
export type GetAdminFrontendTemplatePreviewResponses = {
    200: FrontendTemplateImage;
};
export type GetAdminFrontendTemplatePreviewResponse = GetAdminFrontendTemplatePreviewResponses[keyof GetAdminFrontendTemplatePreviewResponses];
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
export type GetAdminServiceLevelsData = {
    body?: never;
    path?: never;
    query?: {
        dimension?: string;
        search?: string;
        page?: number;
        page_size?: number;
        sort?: string;
    };
    url: '/api/admin/dashboard/service-levels';
};
export type GetAdminServiceLevelsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminServiceLevelsError = GetAdminServiceLevelsErrors[keyof GetAdminServiceLevelsErrors];
export type GetAdminServiceLevelsResponses = {
    200: ServiceLevelReport;
};
export type GetAdminServiceLevelsResponse = GetAdminServiceLevelsResponses[keyof GetAdminServiceLevelsResponses];
export type GetAdminAnalyticsExportStatusData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/analytics/export-status';
};
export type GetAdminAnalyticsExportStatusErrors = {
    401: ManagementError;
    403: ManagementError;
};
export type GetAdminAnalyticsExportStatusError = GetAdminAnalyticsExportStatusErrors[keyof GetAdminAnalyticsExportStatusErrors];
export type GetAdminAnalyticsExportStatusResponses = {
    200: AnalyticsExportStatusResponse;
};
export type GetAdminAnalyticsExportStatusResponse = GetAdminAnalyticsExportStatusResponses[keyof GetAdminAnalyticsExportStatusResponses];
export type ReplayAdminAnalyticsExportData = {
    body: AnalyticsExportReplayRequest;
    path?: never;
    query?: never;
    url: '/api/admin/analytics/export-replay';
};
export type ReplayAdminAnalyticsExportErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type ReplayAdminAnalyticsExportError = ReplayAdminAnalyticsExportErrors[keyof ReplayAdminAnalyticsExportErrors];
export type ReplayAdminAnalyticsExportResponses = {
    200: AnalyticsExportReplayResponse;
};
export type ReplayAdminAnalyticsExportResponse = ReplayAdminAnalyticsExportResponses[keyof ReplayAdminAnalyticsExportResponses];
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
export type ListGatewayModelsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/v1/models';
};
export type ListGatewayModelsErrors = {
    401: OpenAiModelListError;
    429: OpenAiModelListError;
    500: OpenAiModelListError;
};
export type ListGatewayModelsError = ListGatewayModelsErrors[keyof ListGatewayModelsErrors];
export type ListGatewayModelsResponses = {
    200: OpenAiModelList;
};
export type ListGatewayModelsResponse = ListGatewayModelsResponses[keyof ListGatewayModelsResponses];
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
        failed_before?: number;
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
        failed_before?: number;
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
export type ImportAdminCredentialsData = {
    body: AdminCredentialImportRequest;
    path: {
        channel_id: number;
    };
    query?: never;
    url: '/api/admin/channels/{channel_id}/credentials/import';
};
export type ImportAdminCredentialsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ImportAdminCredentialsError = ImportAdminCredentialsErrors[keyof ImportAdminCredentialsErrors];
export type ImportAdminCredentialsResponses = {
    200: AdminCredentialImportResponse;
};
export type ImportAdminCredentialsResponse = ImportAdminCredentialsResponses[keyof ImportAdminCredentialsResponses];
export type ExportAdminCredentialsData = {
    body?: never;
    path: {
        channel_id: number;
    };
    query?: never;
    url: '/api/admin/channels/{channel_id}/credentials/export';
};
export type ExportAdminCredentialsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    500: ManagementError;
};
export type ExportAdminCredentialsError = ExportAdminCredentialsErrors[keyof ExportAdminCredentialsErrors];
export type ExportAdminCredentialsResponses = {
    200: unknown;
};
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
export type GetAdminCredentialUsageData = {
    body?: never;
    path: {
        channel_id: number;
        credential_id: number;
    };
    query?: never;
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}/usage';
};
export type GetAdminCredentialUsageErrors = {
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
};
export type GetAdminCredentialUsageError = GetAdminCredentialUsageErrors[keyof GetAdminCredentialUsageErrors];
export type GetAdminCredentialUsageResponses = {
    200: CredentialUsageSnapshot;
};
export type GetAdminCredentialUsageResponse = GetAdminCredentialUsageResponses[keyof GetAdminCredentialUsageResponses];
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
export type ListAccountVerificationsData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        status?: number;
        limit?: number;
    };
    url: '/api/account/verifications';
};
export type ListAccountVerificationsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type ListAccountVerificationsError = ListAccountVerificationsErrors[keyof ListAccountVerificationsErrors];
export type ListAccountVerificationsResponses = {
    200: AccountVerificationListResponse;
};
export type ListAccountVerificationsResponse = ListAccountVerificationsResponses[keyof ListAccountVerificationsResponses];
export type SubmitAccountVerificationData = {
    body: AccountVerificationMultipartRequest;
    path?: never;
    query?: never;
    url: '/api/account/verifications';
};
export type SubmitAccountVerificationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type SubmitAccountVerificationError = SubmitAccountVerificationErrors[keyof SubmitAccountVerificationErrors];
export type SubmitAccountVerificationResponses = {
    200: AccountVerificationResponse;
};
export type SubmitAccountVerificationResponse = SubmitAccountVerificationResponses[keyof SubmitAccountVerificationResponses];
export type GetAccountVerificationData = {
    body?: never;
    path: {
        case_id: number;
    };
    query?: never;
    url: '/api/account/verifications/{case_id}';
};
export type GetAccountVerificationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type GetAccountVerificationError = GetAccountVerificationErrors[keyof GetAccountVerificationErrors];
export type GetAccountVerificationResponses = {
    200: AccountVerificationDetailResponse;
};
export type GetAccountVerificationResponse = GetAccountVerificationResponses[keyof GetAccountVerificationResponses];
export type SyncAccountVerificationProviderData = {
    body?: never;
    path: {
        case_id: number;
    };
    query?: never;
    url: '/api/account/verifications/{case_id}/provider-sync';
};
export type SyncAccountVerificationProviderErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type SyncAccountVerificationProviderError = SyncAccountVerificationProviderErrors[keyof SyncAccountVerificationProviderErrors];
export type SyncAccountVerificationProviderResponses = {
    200: AccountVerificationResponse;
};
export type SyncAccountVerificationProviderResponse = SyncAccountVerificationProviderResponses[keyof SyncAccountVerificationProviderResponses];
export type DownloadAccountVerificationMaterialData = {
    body?: never;
    path: {
        case_id: number;
        material_id: number;
    };
    query?: never;
    url: '/api/account/verifications/{case_id}/materials/{material_id}';
};
export type DownloadAccountVerificationMaterialErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type DownloadAccountVerificationMaterialError = DownloadAccountVerificationMaterialErrors[keyof DownloadAccountVerificationMaterialErrors];
export type DownloadAccountVerificationMaterialResponses = {
    200: Blob | File;
};
export type DownloadAccountVerificationMaterialResponse = DownloadAccountVerificationMaterialResponses[keyof DownloadAccountVerificationMaterialResponses];
export type ListAdminAccountVerificationsData = {
    body?: never;
    path?: never;
    query?: {
        before?: number;
        status?: number;
        limit?: number;
    };
    url: '/api/admin/account-verifications';
};
export type ListAdminAccountVerificationsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type ListAdminAccountVerificationsError = ListAdminAccountVerificationsErrors[keyof ListAdminAccountVerificationsErrors];
export type ListAdminAccountVerificationsResponses = {
    200: AccountVerificationListResponse;
};
export type ListAdminAccountVerificationsResponse = ListAdminAccountVerificationsResponses[keyof ListAdminAccountVerificationsResponses];
export type GetAdminAccountVerificationData = {
    body?: never;
    path: {
        case_id: number;
    };
    query?: never;
    url: '/api/admin/account-verifications/{case_id}';
};
export type GetAdminAccountVerificationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type GetAdminAccountVerificationError = GetAdminAccountVerificationErrors[keyof GetAdminAccountVerificationErrors];
export type GetAdminAccountVerificationResponses = {
    200: AccountVerificationDetailResponse;
};
export type GetAdminAccountVerificationResponse = GetAdminAccountVerificationResponses[keyof GetAdminAccountVerificationResponses];
export type DownloadAdminAccountVerificationMaterialData = {
    body?: never;
    path: {
        case_id: number;
        material_id: number;
    };
    query?: never;
    url: '/api/admin/account-verifications/{case_id}/materials/{material_id}';
};
export type DownloadAdminAccountVerificationMaterialErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type DownloadAdminAccountVerificationMaterialError = DownloadAdminAccountVerificationMaterialErrors[keyof DownloadAdminAccountVerificationMaterialErrors];
export type DownloadAdminAccountVerificationMaterialResponses = {
    200: Blob | File;
};
export type DownloadAdminAccountVerificationMaterialResponse = DownloadAdminAccountVerificationMaterialResponses[keyof DownloadAdminAccountVerificationMaterialResponses];
export type GetAccountVerificationEligibilityData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/account/verifications/eligibility';
};
export type GetAccountVerificationEligibilityErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type GetAccountVerificationEligibilityError = GetAccountVerificationEligibilityErrors[keyof GetAccountVerificationEligibilityErrors];
export type GetAccountVerificationEligibilityResponses = {
    200: AccountVerificationEligibilityResponse;
};
export type GetAccountVerificationEligibilityResponse = GetAccountVerificationEligibilityResponses[keyof GetAccountVerificationEligibilityResponses];
export type DecideAccountVerificationData = {
    body: AccountVerificationDecisionRequest;
    path: {
        case_id: number;
    };
    query?: never;
    url: '/api/admin/account-verifications/{case_id}/decision';
};
export type DecideAccountVerificationErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    404: ManagementError;
    409: ManagementError;
    503: ManagementError;
};
export type DecideAccountVerificationError = DecideAccountVerificationErrors[keyof DecideAccountVerificationErrors];
export type DecideAccountVerificationResponses = {
    200: AccountVerificationResponse;
};
export type DecideAccountVerificationResponse = DecideAccountVerificationResponses[keyof DecideAccountVerificationResponses];
export type GetAdminVerificationSettingsData = {
    body?: never;
    path?: never;
    query?: never;
    url: '/api/admin/account-verification-settings';
};
export type GetAdminVerificationSettingsErrors = {
    401: ManagementError;
    403: ManagementError;
    500: ManagementError;
};
export type GetAdminVerificationSettingsError = GetAdminVerificationSettingsErrors[keyof GetAdminVerificationSettingsErrors];
export type GetAdminVerificationSettingsResponses = {
    200: AdminVerificationSettings;
};
export type GetAdminVerificationSettingsResponse = GetAdminVerificationSettingsResponses[keyof GetAdminVerificationSettingsResponses];
export type UpdateAdminVerificationSettingsData = {
    body: AdminVerificationSettingsRequestWritable;
    path?: never;
    query?: never;
    url: '/api/admin/account-verification-settings';
};
export type UpdateAdminVerificationSettingsErrors = {
    400: ManagementError;
    401: ManagementError;
    403: ManagementError;
    409: ManagementError;
    500: ManagementError;
};
export type UpdateAdminVerificationSettingsError = UpdateAdminVerificationSettingsErrors[keyof UpdateAdminVerificationSettingsErrors];
export type UpdateAdminVerificationSettingsResponses = {
    200: AdminVerificationSettings;
};
export type UpdateAdminVerificationSettingsResponse = UpdateAdminVerificationSettingsResponses[keyof UpdateAdminVerificationSettingsResponses];
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
