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
    code: 'invalid_request' | 'invalid_credentials' | 'invalid_session' | 'forbidden' | 'user_not_found' | 'group_not_found' | 'token_not_found' | 'user_conflict' | 'group_conflict' | 'group_in_use' | 'internal_error';
    message: string;
};
export type AdminTokenStatus = 'enabled' | 'disabled';
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
