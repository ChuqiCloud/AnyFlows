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
export type AdminUserListResponse = {
    users: Array<AdminUser>;
    next_cursor: number | null;
};
export type ManagementError = {
    code: 'invalid_request' | 'invalid_credentials' | 'invalid_session' | 'forbidden' | 'user_not_found' | 'internal_error';
    message: string;
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
