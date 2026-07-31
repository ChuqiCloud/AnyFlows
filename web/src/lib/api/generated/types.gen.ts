// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

export type ClientOptions = {
    baseUrl: `${string}://${string}` | (string & {});
};
export type AdminAuthenticationSettings = {
    password_login_enabled: boolean;
    registration_enabled: boolean;
    default_group_id: number;
    initial_quota: number;
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
    email_required: boolean;
    rate_limit_attempts: number;
    rate_limit_window_seconds: number;
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
export type AdminChannelCreateRequest = {
    name: string;
    type: 'openai' | 'anthropic' | 'gemini';
    protocol: 'openai_chat' | 'openai_responses' | 'anthropic' | 'gemini';
    base_url: string | null;
    timeout_secs?: number | null;
    status: AdminRoutingWriteStatus;
    weight: number;
    priority: number;
    auto_ban: boolean;
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
export type AdminChannelProtocol = 'openai_chat' | 'openai_responses' | 'anthropic' | 'gemini';
export type AdminChannelType = 'openai' | 'anthropic' | 'gemini' | 'bedrock' | 'vertex' | 'custom';
export type AdminChannelUpdateRequest = {
    name: string;
    type: 'openai' | 'anthropic' | 'gemini';
    protocol: 'openai_chat' | 'openai_responses' | 'anthropic' | 'gemini';
    base_url: string | null;
    timeout_secs?: number | null;
    status: AdminRoutingWriteStatus;
    weight: number;
    priority: number;
    auto_ban: boolean;
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
    oauth_account_key: string | null;
    oauth_project_id: string | null;
    last_used_at: number | null;
    created_at: number;
    updated_at: number;
};
export type AdminCredentialCreateRequest = {
    kind: 'api_key' | 'oauth';
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
export type AdminCredentialQuotaDimension = 'global' | 'spark';
export type AdminCredentialUpdateRequest = {
    kind: 'api_key' | 'oauth';
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
    kind: 'api_key' | 'oauth';
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
    free_request_count: number;
    enabled_channel_count: number;
    disabled_channel_count: number;
    auto_disabled_channel_count: number;
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
    usage_source: AdminUsageLogSource;
    usage_semantics: AdminUsageLogSemantics;
    quota: number;
    created_at: number;
};
export type AdminUsageLogBillingMode = 'per_token' | 'free';
export type AdminUsageLogListResponse = {
    logs: Array<AdminUsageLog>;
    next_cursor: number | null;
};
export type AdminUsageLogSemantics = 'inclusive' | 'cache_separated';
export type AdminUsageLogSource = 'upstream' | 'estimated';
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
export type AdminUserListResponse = {
    users: Array<AdminUser>;
    next_cursor: number | null;
};
export type AdminUserStatus = 'enabled' | 'disabled';
export type AdminUserWriteRequest = {
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
    code: 'invalid_request' | 'invalid_credentials' | 'password_login_disabled' | 'invalid_session' | 'forbidden' | 'setup_conflict' | 'user_not_found' | 'group_not_found' | 'token_not_found' | 'token_limit_reached' | 'channel_not_found' | 'credential_not_found' | 'probe_unavailable' | 'playground_share_not_found' | 'playground_share_limit_reached' | 'playground_conversation_not_found' | 'playground_conversation_limit_reached' | 'playground_conversation_conflict' | 'user_conflict' | 'registration_disabled' | 'registration_rate_limited' | 'registration_rejected' | 'password_reset_rejected' | 'password_change_rejected' | 'email_not_configured' | 'email_delivery_failed' | 'group_conflict' | 'group_in_use' | 'model_not_found' | 'model_conflict' | 'model_sync_channel_unavailable' | 'model_sync_unsupported_channel' | 'model_sync_upstream_timeout' | 'model_sync_upstream_rejected' | 'model_sync_invalid_response' | 'model_sync_candidate_limit_exceeded' | 'model_sync_preview_not_found' | 'model_sync_preview_expired' | 'model_sync_preview_already_applied' | 'model_sync_conflict' | 'internal_error';
    message: string;
};
export type ModelCatalogBillingMode = 'per_token' | 'free';
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
    lifecycle: ModelCatalogLifecycle;
    runtime_status: ModelCatalogRuntimeStatus;
    billing_mode: ModelCatalogBillingMode;
    prices: null | ModelCatalogTokenPrices;
    ratios: ModelCatalogRatios;
    price_version: number;
};
export type ModelCatalogLifecycle = 'active' | 'deprecated';
export type ModelCatalogListResponse = {
    pricing_scope: ModelCatalogPricingScope;
    models: Array<ModelCatalogItem>;
    next_cursor: string | null;
};
export type ModelCatalogPricingScope = 'public_base' | 'group';
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
};
export type RegistrationStatusResponse = {
    password_login_enabled: boolean;
    enabled: boolean;
    email_required: boolean;
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
export type UserNotificationPreferencesRequest = {
    email_product_updates: boolean;
    email_usage_alerts: boolean;
};
export type UserNotificationPreferencesResponse = {
    email_product_updates: boolean;
    email_usage_alerts: boolean;
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
export type AdminChannelCreateRequestWritable = {
    name: string;
    type: 'openai' | 'anthropic' | 'gemini';
    protocol: 'openai_chat' | 'openai_responses' | 'anthropic' | 'gemini';
    base_url: string | null;
    timeout_secs?: number | null;
    status: AdminRoutingWriteStatus;
    weight: number;
    priority: number;
    auto_ban: boolean;
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
    type: 'openai' | 'anthropic' | 'gemini';
    protocol: 'openai_chat' | 'openai_responses' | 'anthropic' | 'gemini';
    base_url: string | null;
    timeout_secs?: number | null;
    status: AdminRoutingWriteStatus;
    weight: number;
    priority: number;
    auto_ban: boolean;
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
    kind: 'api_key' | 'oauth';
    secret: string;
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
export type AdminCredentialUpdateRequestWritable = {
    kind: 'api_key' | 'oauth';
    secret: string | null;
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
export type RegistrationRequestWritable = {
    username: string;
    email?: string | null;
    password: string;
    verification_code?: string | null;
};
export type SetupRequestWritable = {
    username: string;
    password: string;
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
    body: AdminUserWriteRequest;
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
    body: AdminUserWriteRequest;
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
export type ListModelsData = {
    body?: never;
    path?: never;
    query?: {
        q?: string;
        billing_mode?: ModelCatalogBillingMode;
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
