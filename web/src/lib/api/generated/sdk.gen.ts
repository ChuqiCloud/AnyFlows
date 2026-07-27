// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import type { Client, ClientMeta, Options as Options2, RequestResult, TDataShape } from './client';
import { client } from './client.gen';
import type { CreateAdminGroupData, CreateAdminGroupErrors, CreateAdminGroupResponses, CreateAdminTokenData, CreateAdminTokenErrors, CreateAdminTokenResponses, CreateAdminUserData, CreateAdminUserErrors, CreateAdminUserResponses, DeleteAdminGroupData, DeleteAdminGroupErrors, DeleteAdminGroupResponses, DeleteAdminTokenData, DeleteAdminTokenErrors, DeleteAdminTokenResponses, DeleteAdminUserData, DeleteAdminUserErrors, DeleteAdminUserResponses, GetAdminChannelData, GetAdminChannelErrors, GetAdminChannelResponses, GetAdminCredentialData, GetAdminCredentialErrors, GetAdminCredentialResponses, GetAdminGroupData, GetAdminGroupErrors, GetAdminGroupResponses, GetAdminTokenData, GetAdminTokenErrors, GetAdminTokenResponses, GetAdminUserData, GetAdminUserErrors, GetAdminUserResponses, GetManagementSessionData, GetManagementSessionErrors, GetManagementSessionResponses, ListAdminChannelsData, ListAdminChannelsErrors, ListAdminChannelsResponses, ListAdminCredentialsData, ListAdminCredentialsErrors, ListAdminCredentialsResponses, ListAdminGroupsData, ListAdminGroupsErrors, ListAdminGroupsResponses, ListAdminTokensData, ListAdminTokensErrors, ListAdminTokensResponses, ListAdminUsersData, ListAdminUsersErrors, ListAdminUsersResponses, LoginManagementSessionData, LoginManagementSessionErrors, LoginManagementSessionResponses, UpdateAdminGroupData, UpdateAdminGroupErrors, UpdateAdminGroupResponses, UpdateAdminTokenData, UpdateAdminTokenErrors, UpdateAdminTokenResponses, UpdateAdminUserData, UpdateAdminUserErrors, UpdateAdminUserResponses } from './types.gen';
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
export const listAdminChannels = <ThrowOnError extends boolean = true>(options?: Options<ListAdminChannelsData, ThrowOnError>): RequestResult<ListAdminChannelsResponses, ListAdminChannelsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminChannelsResponses, ListAdminChannelsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels',
    ...options
});
export const getAdminChannel = <ThrowOnError extends boolean = true>(options: Options<GetAdminChannelData, ThrowOnError>): RequestResult<GetAdminChannelResponses, GetAdminChannelErrors, ThrowOnError> => (options.client ?? client).get<GetAdminChannelResponses, GetAdminChannelErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{id}',
    ...options
});
export const listAdminCredentials = <ThrowOnError extends boolean = true>(options: Options<ListAdminCredentialsData, ThrowOnError>): RequestResult<ListAdminCredentialsResponses, ListAdminCredentialsErrors, ThrowOnError> => (options.client ?? client).get<ListAdminCredentialsResponses, ListAdminCredentialsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{channel_id}/credentials',
    ...options
});
export const getAdminCredential = <ThrowOnError extends boolean = true>(options: Options<GetAdminCredentialData, ThrowOnError>): RequestResult<GetAdminCredentialResponses, GetAdminCredentialErrors, ThrowOnError> => (options.client ?? client).get<GetAdminCredentialResponses, GetAdminCredentialErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}',
    ...options
});
