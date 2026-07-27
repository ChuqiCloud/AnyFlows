// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

export type ClientOptions = {
    baseUrl: `${string}://${string}` | (string & {});
};
export type LoginRequest = {
    username: string;
    password: string;
};
export type SessionUser = {
    id: number;
    role: 'user' | 'admin';
};
export type LoginResponse = {
    access_token: string;
    token_type: 'Bearer';
    expires_in: number;
    expires_at: number;
    user: SessionUser;
};
export type SessionResponse = {
    user: SessionUser;
    expires_at: number;
};
export type AdminUserStatus = 'enabled' | 'disabled';
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
export type AdminUserListResponse = {
    users: Array<AdminUser>;
    next_cursor: number | null;
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
    rpm_limit: number | null;
    fallback_group_id: number | null;
    flags: {
        [key: string]: unknown;
    };
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
export type AdminGroupListResponse = {
    groups: Array<AdminGroup>;
    next_cursor: number | null;
};
export type ManagementError = {
    code: 'invalid_request' | 'invalid_credentials' | 'invalid_session' | 'forbidden' | 'user_not_found' | 'group_not_found' | 'token_not_found' | 'channel_not_found' | 'credential_not_found' | 'user_conflict' | 'group_conflict' | 'group_in_use' | 'internal_error';
    message: string;
};
export type AdminRoutingStatus = 'enabled' | 'disabled' | 'auto_disabled';
export type AdminRoutingWriteStatus = 'enabled' | 'disabled';
export type AdminChannelType = 'openai' | 'anthropic' | 'gemini' | 'bedrock' | 'vertex' | 'custom';
export type AdminChannelProtocol = 'openai_chat' | 'openai_responses' | 'anthropic' | 'gemini';
export type AdminChannelWriteRequest = {
    name: string;
    type: 'openai';
    protocol: 'openai_chat';
    base_url: string | null;
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
        [key: string]: unknown;
    };
    tag: string | null;
};
export type AdminChannel = {
    id: number;
    name: string;
    type: AdminChannelType;
    protocol: AdminChannelProtocol;
    base_url: string | null;
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
        [key: string]: unknown;
    };
    balance: number | null;
    used_quota: number;
    tag: string | null;
    created_at: number;
    updated_at: number;
};
export type AdminChannelListResponse = {
    channels: Array<AdminChannel>;
    next_cursor: number | null;
};
export type AdminCredentialKind = 'api_key' | 'oauth' | 'setup_token' | 'bedrock' | 'service_account' | 'upstream';
export type AdminCredentialMultiKeyMode = 'random' | 'round_robin';
export type AdminCredentialQuotaDimension = 'global' | 'spark';
export type AdminCredentialWriteFields = {
    kind: 'api_key' | 'oauth';
    status: AdminRoutingWriteStatus;
    multi_key_mode: AdminCredentialMultiKeyMode | null;
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
export type AdminCredentialCreateRequest = AdminCredentialWriteFields;
export type AdminCredentialUpdateRequest = AdminCredentialWriteFields;
export type AdminCredential = {
    id: number;
    channel_id: number;
    kind: AdminCredentialKind;
    status: AdminRoutingStatus;
    multi_key_mode: AdminCredentialMultiKeyMode | null;
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
export type AdminCredentialListResponse = {
    credentials: Array<AdminCredential>;
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
export type IssuedAdminToken = {
    api_key: string;
    token: AdminToken;
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
export type AdminChannelWriteRequestWritable = {
    name: string;
    type: 'openai';
    protocol: 'openai_chat';
    base_url: string | null;
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
        [key: string]: unknown;
    };
    header_override: {
        [key: string]: string;
    };
    settings: {
        [key: string]: unknown;
    };
    tag: string | null;
};
export type AdminCredentialCreateRequestWritable = AdminCredentialWriteFields & {
    secret: string;
};
export type AdminCredentialUpdateRequestWritable = AdminCredentialWriteFields & {
    secret: string | null;
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
    body: AdminChannelWriteRequestWritable;
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
    body: AdminChannelWriteRequestWritable;
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
