// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import type { Client, ClientMeta, Options as Options2, RequestResult, TDataShape } from './client';
import { client } from './client.gen';
import type { CreateAdminChannelData, CreateAdminChannelErrors, CreateAdminChannelResponses, CreateAdminCredentialData, CreateAdminCredentialErrors, CreateAdminCredentialResponses, CreateAdminGroupData, CreateAdminGroupErrors, CreateAdminGroupResponses, CreateAdminTokenData, CreateAdminTokenErrors, CreateAdminTokenResponses, CreateAdminUserData, CreateAdminUserErrors, CreateAdminUserResponses, CreatePlaygroundShareData, CreatePlaygroundShareErrors, CreatePlaygroundShareResponses, CreateUserTokenData, CreateUserTokenErrors, CreateUserTokenResponses, DeleteAdminChannelData, DeleteAdminChannelErrors, DeleteAdminChannelResponses, DeleteAdminCredentialData, DeleteAdminCredentialErrors, DeleteAdminCredentialResponses, DeleteAdminGroupData, DeleteAdminGroupErrors, DeleteAdminGroupResponses, DeleteAdminTokenData, DeleteAdminTokenErrors, DeleteAdminTokenResponses, DeleteAdminUserData, DeleteAdminUserErrors, DeleteAdminUserResponses, DeletePlaygroundConversationData, DeletePlaygroundConversationErrors, DeletePlaygroundConversationResponses, DeleteUserTokenData, DeleteUserTokenErrors, DeleteUserTokenResponses, GetAdminChannelData, GetAdminChannelErrors, GetAdminChannelResponses, GetAdminCredentialData, GetAdminCredentialErrors, GetAdminCredentialResponses, GetAdminDashboardData, GetAdminDashboardErrors, GetAdminDashboardResponses, GetAdminGroupData, GetAdminGroupErrors, GetAdminGroupResponses, GetAdminRegistrationPolicyData, GetAdminRegistrationPolicyErrors, GetAdminRegistrationPolicyResponses, GetAdminTokenData, GetAdminTokenErrors, GetAdminTokenResponses, GetAdminUserData, GetAdminUserErrors, GetAdminUserResponses, GetInitialSetupStatusData, GetInitialSetupStatusErrors, GetInitialSetupStatusResponses, GetManagementSessionData, GetManagementSessionErrors, GetManagementSessionResponses, GetPlaygroundConversationData, GetPlaygroundConversationErrors, GetPlaygroundConversationResponses, GetPlaygroundShareData, GetPlaygroundShareErrors, GetPlaygroundShareResponses, GetRegistrationStatusData, GetRegistrationStatusErrors, GetRegistrationStatusResponses, GetUserTokenData, GetUserTokenErrors, GetUserTokenResponses, InitializeAdminSetupData, InitializeAdminSetupErrors, InitializeAdminSetupResponses, ListAdminChannelsData, ListAdminChannelsErrors, ListAdminChannelsResponses, ListAdminCredentialsData, ListAdminCredentialsErrors, ListAdminCredentialsResponses, ListAdminGroupsData, ListAdminGroupsErrors, ListAdminGroupsResponses, ListAdminTokensData, ListAdminTokensErrors, ListAdminTokensResponses, ListAdminUsageLogsData, ListAdminUsageLogsErrors, ListAdminUsageLogsResponses, ListAdminUsersData, ListAdminUsersErrors, ListAdminUsersResponses, ListModelsData, ListModelsErrors, ListModelsResponses, ListPlaygroundConversationsData, ListPlaygroundConversationsErrors, ListPlaygroundConversationsResponses, ListUserTokensData, ListUserTokensErrors, ListUserTokensResponses, LoginManagementSessionData, LoginManagementSessionErrors, LoginManagementSessionResponses, ProbeAdminChannelData, ProbeAdminChannelErrors, ProbeAdminChannelResponses, RegisterUserData, RegisterUserErrors, RegisterUserResponses, RevokePlaygroundShareData, RevokePlaygroundShareErrors, RevokePlaygroundShareResponses, SavePlaygroundConversationData, SavePlaygroundConversationErrors, SavePlaygroundConversationResponses, UpdateAdminChannelData, UpdateAdminChannelErrors, UpdateAdminChannelResponses, UpdateAdminCredentialData, UpdateAdminCredentialErrors, UpdateAdminCredentialResponses, UpdateAdminGroupData, UpdateAdminGroupErrors, UpdateAdminGroupResponses, UpdateAdminRegistrationPolicyData, UpdateAdminRegistrationPolicyErrors, UpdateAdminRegistrationPolicyResponses, UpdateAdminTokenData, UpdateAdminTokenErrors, UpdateAdminTokenResponses, UpdateAdminUserData, UpdateAdminUserErrors, UpdateAdminUserResponses, UpdateUserTokenData, UpdateUserTokenErrors, UpdateUserTokenResponses } from './types.gen';
export type Options<TData extends TDataShape = TDataShape, ThrowOnError extends boolean = boolean, TResponse = unknown> = Options2<TData, ThrowOnError, TResponse> & {
    client?: Client;
    meta?: keyof ClientMeta extends never ? Record<string, unknown> : ClientMeta;
};
export const loginManagementSession = <ThrowOnError extends boolean = true>(options: Options<LoginManagementSessionData, ThrowOnError>): RequestResult<LoginManagementSessionResponses, LoginManagementSessionErrors, ThrowOnError> => (options.client ?? client).post<LoginManagementSessionResponses, LoginManagementSessionErrors, ThrowOnError>({
    url: '/api/auth/login',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getManagementSession = <ThrowOnError extends boolean = true>(options?: Options<GetManagementSessionData, ThrowOnError>): RequestResult<GetManagementSessionResponses, GetManagementSessionErrors, ThrowOnError> => (options?.client ?? client).get<GetManagementSessionResponses, GetManagementSessionErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/auth/session',
    ...options
});
export const getInitialSetupStatus = <ThrowOnError extends boolean = true>(options?: Options<GetInitialSetupStatusData, ThrowOnError>): RequestResult<GetInitialSetupStatusResponses, GetInitialSetupStatusErrors, ThrowOnError> => (options?.client ?? client).get<GetInitialSetupStatusResponses, GetInitialSetupStatusErrors, ThrowOnError>({ url: '/api/setup/status', ...options });
export const initializeAdminSetup = <ThrowOnError extends boolean = true>(options: Options<InitializeAdminSetupData, ThrowOnError>): RequestResult<InitializeAdminSetupResponses, InitializeAdminSetupErrors, ThrowOnError> => (options.client ?? client).post<InitializeAdminSetupResponses, InitializeAdminSetupErrors, ThrowOnError>({
    url: '/api/setup',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getRegistrationStatus = <ThrowOnError extends boolean = true>(options?: Options<GetRegistrationStatusData, ThrowOnError>): RequestResult<GetRegistrationStatusResponses, GetRegistrationStatusErrors, ThrowOnError> => (options?.client ?? client).get<GetRegistrationStatusResponses, GetRegistrationStatusErrors, ThrowOnError>({ url: '/api/registration/status', ...options });
export const registerUser = <ThrowOnError extends boolean = true>(options: Options<RegisterUserData, ThrowOnError>): RequestResult<RegisterUserResponses, RegisterUserErrors, ThrowOnError> => (options.client ?? client).post<RegisterUserResponses, RegisterUserErrors, ThrowOnError>({
    url: '/api/registration',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminRegistrationPolicy = <ThrowOnError extends boolean = true>(options?: Options<GetAdminRegistrationPolicyData, ThrowOnError>): RequestResult<GetAdminRegistrationPolicyResponses, GetAdminRegistrationPolicyErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminRegistrationPolicyResponses, GetAdminRegistrationPolicyErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/registration',
    ...options
});
export const updateAdminRegistrationPolicy = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminRegistrationPolicyData, ThrowOnError>): RequestResult<UpdateAdminRegistrationPolicyResponses, UpdateAdminRegistrationPolicyErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminRegistrationPolicyResponses, UpdateAdminRegistrationPolicyErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/registration',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminDashboard = <ThrowOnError extends boolean = true>(options?: Options<GetAdminDashboardData, ThrowOnError>): RequestResult<GetAdminDashboardResponses, GetAdminDashboardErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminDashboardResponses, GetAdminDashboardErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/dashboard',
    ...options
});
export const listAdminUsers = <ThrowOnError extends boolean = true>(options?: Options<ListAdminUsersData, ThrowOnError>): RequestResult<ListAdminUsersResponses, ListAdminUsersErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminUsersResponses, ListAdminUsersErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/users',
    ...options
});
export const createAdminUser = <ThrowOnError extends boolean = true>(options: Options<CreateAdminUserData, ThrowOnError>): RequestResult<CreateAdminUserResponses, CreateAdminUserErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminUserResponses, CreateAdminUserErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/users',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminUser = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminUserData, ThrowOnError>): RequestResult<DeleteAdminUserResponses, DeleteAdminUserErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminUserResponses, DeleteAdminUserErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/users/{id}',
    ...options
});
export const getAdminUser = <ThrowOnError extends boolean = true>(options: Options<GetAdminUserData, ThrowOnError>): RequestResult<GetAdminUserResponses, GetAdminUserErrors, ThrowOnError> => (options.client ?? client).get<GetAdminUserResponses, GetAdminUserErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/users/{id}',
    ...options
});
export const updateAdminUser = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminUserData, ThrowOnError>): RequestResult<UpdateAdminUserResponses, UpdateAdminUserErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminUserResponses, UpdateAdminUserErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/users/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminGroups = <ThrowOnError extends boolean = true>(options?: Options<ListAdminGroupsData, ThrowOnError>): RequestResult<ListAdminGroupsResponses, ListAdminGroupsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminGroupsResponses, ListAdminGroupsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/groups',
    ...options
});
export const createAdminGroup = <ThrowOnError extends boolean = true>(options: Options<CreateAdminGroupData, ThrowOnError>): RequestResult<CreateAdminGroupResponses, CreateAdminGroupErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminGroupResponses, CreateAdminGroupErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/groups',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminGroup = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminGroupData, ThrowOnError>): RequestResult<DeleteAdminGroupResponses, DeleteAdminGroupErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminGroupResponses, DeleteAdminGroupErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/groups/{id}',
    ...options
});
export const getAdminGroup = <ThrowOnError extends boolean = true>(options: Options<GetAdminGroupData, ThrowOnError>): RequestResult<GetAdminGroupResponses, GetAdminGroupErrors, ThrowOnError> => (options.client ?? client).get<GetAdminGroupResponses, GetAdminGroupErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/groups/{id}',
    ...options
});
export const updateAdminGroup = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminGroupData, ThrowOnError>): RequestResult<UpdateAdminGroupResponses, UpdateAdminGroupErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminGroupResponses, UpdateAdminGroupErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/groups/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listModels = <ThrowOnError extends boolean = true>(options?: Options<ListModelsData, ThrowOnError>): RequestResult<ListModelsResponses, ListModelsErrors, ThrowOnError> => (options?.client ?? client).get<ListModelsResponses, ListModelsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/models',
    ...options
});
export const createPlaygroundShare = <ThrowOnError extends boolean = true>(options: Options<CreatePlaygroundShareData, ThrowOnError>): RequestResult<CreatePlaygroundShareResponses, CreatePlaygroundShareErrors, ThrowOnError> => (options.client ?? client).post<CreatePlaygroundShareResponses, CreatePlaygroundShareErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/playground/shares',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const revokePlaygroundShare = <ThrowOnError extends boolean = true>(options: Options<RevokePlaygroundShareData, ThrowOnError>): RequestResult<RevokePlaygroundShareResponses, RevokePlaygroundShareErrors, ThrowOnError> => (options.client ?? client).delete<RevokePlaygroundShareResponses, RevokePlaygroundShareErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/playground/shares/{token}',
    ...options
});
export const getPlaygroundShare = <ThrowOnError extends boolean = true>(options: Options<GetPlaygroundShareData, ThrowOnError>): RequestResult<GetPlaygroundShareResponses, GetPlaygroundShareErrors, ThrowOnError> => (options.client ?? client).get<GetPlaygroundShareResponses, GetPlaygroundShareErrors, ThrowOnError>({ url: '/api/playground/shares/{token}', ...options });
export const listPlaygroundConversations = <ThrowOnError extends boolean = true>(options?: Options<ListPlaygroundConversationsData, ThrowOnError>): RequestResult<ListPlaygroundConversationsResponses, ListPlaygroundConversationsErrors, ThrowOnError> => (options?.client ?? client).get<ListPlaygroundConversationsResponses, ListPlaygroundConversationsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/playground/conversations',
    ...options
});
export const deletePlaygroundConversation = <ThrowOnError extends boolean = true>(options: Options<DeletePlaygroundConversationData, ThrowOnError>): RequestResult<DeletePlaygroundConversationResponses, DeletePlaygroundConversationErrors, ThrowOnError> => (options.client ?? client).delete<DeletePlaygroundConversationResponses, DeletePlaygroundConversationErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/playground/conversations/{conversation_id}',
    ...options
});
export const getPlaygroundConversation = <ThrowOnError extends boolean = true>(options: Options<GetPlaygroundConversationData, ThrowOnError>): RequestResult<GetPlaygroundConversationResponses, GetPlaygroundConversationErrors, ThrowOnError> => (options.client ?? client).get<GetPlaygroundConversationResponses, GetPlaygroundConversationErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/playground/conversations/{conversation_id}',
    ...options
});
export const savePlaygroundConversation = <ThrowOnError extends boolean = true>(options: Options<SavePlaygroundConversationData, ThrowOnError>): RequestResult<SavePlaygroundConversationResponses, SavePlaygroundConversationErrors, ThrowOnError> => (options.client ?? client).put<SavePlaygroundConversationResponses, SavePlaygroundConversationErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/playground/conversations/{conversation_id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listUserTokens = <ThrowOnError extends boolean = true>(options?: Options<ListUserTokensData, ThrowOnError>): RequestResult<ListUserTokensResponses, ListUserTokensErrors, ThrowOnError> => (options?.client ?? client).get<ListUserTokensResponses, ListUserTokensErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/tokens',
    ...options
});
export const createUserToken = <ThrowOnError extends boolean = true>(options: Options<CreateUserTokenData, ThrowOnError>): RequestResult<CreateUserTokenResponses, CreateUserTokenErrors, ThrowOnError> => (options.client ?? client).post<CreateUserTokenResponses, CreateUserTokenErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/tokens',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteUserToken = <ThrowOnError extends boolean = true>(options: Options<DeleteUserTokenData, ThrowOnError>): RequestResult<DeleteUserTokenResponses, DeleteUserTokenErrors, ThrowOnError> => (options.client ?? client).delete<DeleteUserTokenResponses, DeleteUserTokenErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/tokens/{id}',
    ...options
});
export const getUserToken = <ThrowOnError extends boolean = true>(options: Options<GetUserTokenData, ThrowOnError>): RequestResult<GetUserTokenResponses, GetUserTokenErrors, ThrowOnError> => (options.client ?? client).get<GetUserTokenResponses, GetUserTokenErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/tokens/{id}',
    ...options
});
export const updateUserToken = <ThrowOnError extends boolean = true>(options: Options<UpdateUserTokenData, ThrowOnError>): RequestResult<UpdateUserTokenResponses, UpdateUserTokenErrors, ThrowOnError> => (options.client ?? client).put<UpdateUserTokenResponses, UpdateUserTokenErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/tokens/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminTokens = <ThrowOnError extends boolean = true>(options?: Options<ListAdminTokensData, ThrowOnError>): RequestResult<ListAdminTokensResponses, ListAdminTokensErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminTokensResponses, ListAdminTokensErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/tokens',
    ...options
});
export const createAdminToken = <ThrowOnError extends boolean = true>(options: Options<CreateAdminTokenData, ThrowOnError>): RequestResult<CreateAdminTokenResponses, CreateAdminTokenErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminTokenResponses, CreateAdminTokenErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/tokens',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminToken = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminTokenData, ThrowOnError>): RequestResult<DeleteAdminTokenResponses, DeleteAdminTokenErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminTokenResponses, DeleteAdminTokenErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/tokens/{id}',
    ...options
});
export const getAdminToken = <ThrowOnError extends boolean = true>(options: Options<GetAdminTokenData, ThrowOnError>): RequestResult<GetAdminTokenResponses, GetAdminTokenErrors, ThrowOnError> => (options.client ?? client).get<GetAdminTokenResponses, GetAdminTokenErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/tokens/{id}',
    ...options
});
export const updateAdminToken = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminTokenData, ThrowOnError>): RequestResult<UpdateAdminTokenResponses, UpdateAdminTokenErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminTokenResponses, UpdateAdminTokenErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/tokens/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminUsageLogs = <ThrowOnError extends boolean = true>(options?: Options<ListAdminUsageLogsData, ThrowOnError>): RequestResult<ListAdminUsageLogsResponses, ListAdminUsageLogsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminUsageLogsResponses, ListAdminUsageLogsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/usage-logs',
    ...options
});
export const listAdminChannels = <ThrowOnError extends boolean = true>(options?: Options<ListAdminChannelsData, ThrowOnError>): RequestResult<ListAdminChannelsResponses, ListAdminChannelsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminChannelsResponses, ListAdminChannelsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels',
    ...options
});
export const createAdminChannel = <ThrowOnError extends boolean = true>(options: Options<CreateAdminChannelData, ThrowOnError>): RequestResult<CreateAdminChannelResponses, CreateAdminChannelErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminChannelResponses, CreateAdminChannelErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminChannel = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminChannelData, ThrowOnError>): RequestResult<DeleteAdminChannelResponses, DeleteAdminChannelErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminChannelResponses, DeleteAdminChannelErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{id}',
    ...options
});
export const getAdminChannel = <ThrowOnError extends boolean = true>(options: Options<GetAdminChannelData, ThrowOnError>): RequestResult<GetAdminChannelResponses, GetAdminChannelErrors, ThrowOnError> => (options.client ?? client).get<GetAdminChannelResponses, GetAdminChannelErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{id}',
    ...options
});
export const updateAdminChannel = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminChannelData, ThrowOnError>): RequestResult<UpdateAdminChannelResponses, UpdateAdminChannelErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminChannelResponses, UpdateAdminChannelErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const probeAdminChannel = <ThrowOnError extends boolean = true>(options: Options<ProbeAdminChannelData, ThrowOnError>): RequestResult<ProbeAdminChannelResponses, ProbeAdminChannelErrors, ThrowOnError> => (options.client ?? client).post<ProbeAdminChannelResponses, ProbeAdminChannelErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{id}/probe',
    ...options
});
export const listAdminCredentials = <ThrowOnError extends boolean = true>(options: Options<ListAdminCredentialsData, ThrowOnError>): RequestResult<ListAdminCredentialsResponses, ListAdminCredentialsErrors, ThrowOnError> => (options.client ?? client).get<ListAdminCredentialsResponses, ListAdminCredentialsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{channel_id}/credentials',
    ...options
});
export const createAdminCredential = <ThrowOnError extends boolean = true>(options: Options<CreateAdminCredentialData, ThrowOnError>): RequestResult<CreateAdminCredentialResponses, CreateAdminCredentialErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminCredentialResponses, CreateAdminCredentialErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{channel_id}/credentials',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminCredential = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminCredentialData, ThrowOnError>): RequestResult<DeleteAdminCredentialResponses, DeleteAdminCredentialErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminCredentialResponses, DeleteAdminCredentialErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}',
    ...options
});
export const getAdminCredential = <ThrowOnError extends boolean = true>(options: Options<GetAdminCredentialData, ThrowOnError>): RequestResult<GetAdminCredentialResponses, GetAdminCredentialErrors, ThrowOnError> => (options.client ?? client).get<GetAdminCredentialResponses, GetAdminCredentialErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}',
    ...options
});
export const updateAdminCredential = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminCredentialData, ThrowOnError>): RequestResult<UpdateAdminCredentialResponses, UpdateAdminCredentialErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminCredentialResponses, UpdateAdminCredentialErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
