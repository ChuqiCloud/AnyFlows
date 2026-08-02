// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import type { Client, ClientMeta, Options as Options2, RequestResult, TDataShape } from './client';
import { client } from './client.gen';
import type { AdjustAdminWalletData, AdjustAdminWalletErrors, AdjustAdminWalletResponses, ApplyAdminModelPricesData, ApplyAdminModelPricesErrors, ApplyAdminModelPricesResponses, ApplyAdminModelSyncPreviewData, ApplyAdminModelSyncPreviewErrors, ApplyAdminModelSyncPreviewResponses, ChangeUserPasswordData, ChangeUserPasswordErrors, ChangeUserPasswordResponses, ConfirmPasswordResetData, ConfirmPasswordResetErrors, ConfirmPasswordResetResponses, CreateAdminChannelData, CreateAdminChannelErrors, CreateAdminChannelResponses, CreateAdminCredentialData, CreateAdminCredentialErrors, CreateAdminCredentialResponses, CreateAdminGroupData, CreateAdminGroupErrors, CreateAdminGroupResponses, CreateAdminModelData, CreateAdminModelErrors, CreateAdminModelResponses, CreateAdminModelSyncPreviewData, CreateAdminModelSyncPreviewErrors, CreateAdminModelSyncPreviewResponses, CreateAdminRedemptionBatchData, CreateAdminRedemptionBatchErrors, CreateAdminRedemptionBatchResponses, CreateAdminTokenData, CreateAdminTokenErrors, CreateAdminTokenResponses, CreateAdminUserData, CreateAdminUserErrors, CreateAdminUserResponses, CreatePlaygroundShareData, CreatePlaygroundShareErrors, CreatePlaygroundShareResponses, CreateUserTokenData, CreateUserTokenErrors, CreateUserTokenResponses, DeleteAdminChannelData, DeleteAdminChannelErrors, DeleteAdminChannelResponses, DeleteAdminCredentialData, DeleteAdminCredentialErrors, DeleteAdminCredentialResponses, DeleteAdminGroupData, DeleteAdminGroupErrors, DeleteAdminGroupResponses, DeleteAdminModelData, DeleteAdminModelErrors, DeleteAdminModelResponses, DeleteAdminTokenData, DeleteAdminTokenErrors, DeleteAdminTokenResponses, DeleteAdminUserData, DeleteAdminUserErrors, DeleteAdminUserResponses, DeletePlaygroundConversationData, DeletePlaygroundConversationErrors, DeletePlaygroundConversationResponses, DeleteUserTokenData, DeleteUserTokenErrors, DeleteUserTokenResponses, DisableAdminRedemptionBatchData, DisableAdminRedemptionBatchErrors, DisableAdminRedemptionBatchResponses, GetAdminAuthenticationSettingsData, GetAdminAuthenticationSettingsErrors, GetAdminAuthenticationSettingsResponses, GetAdminBalanceAlertSettingsData, GetAdminBalanceAlertSettingsErrors, GetAdminBalanceAlertSettingsResponses, GetAdminChannelData, GetAdminChannelErrors, GetAdminChannelResponses, GetAdminCredentialData, GetAdminCredentialErrors, GetAdminCredentialResponses, GetAdminDashboardData, GetAdminDashboardErrors, GetAdminDashboardResponses, GetAdminEmailSettingsData, GetAdminEmailSettingsErrors, GetAdminEmailSettingsResponses, GetAdminGroupData, GetAdminGroupErrors, GetAdminGroupResponses, GetAdminModelData, GetAdminModelErrors, GetAdminModelResponses, GetAdminNetworkSettingsData, GetAdminNetworkSettingsErrors, GetAdminNetworkSettingsResponses, GetAdminSiteSettingsData, GetAdminSiteSettingsErrors, GetAdminSiteSettingsResponses, GetAdminTokenData, GetAdminTokenErrors, GetAdminTokenResponses, GetAdminUserData, GetAdminUserErrors, GetAdminUserResponses, GetInitialSetupStatusData, GetInitialSetupStatusErrors, GetInitialSetupStatusResponses, GetManagementSessionData, GetManagementSessionErrors, GetManagementSessionResponses, GetPlaygroundConversationData, GetPlaygroundConversationErrors, GetPlaygroundConversationResponses, GetPlaygroundShareData, GetPlaygroundShareErrors, GetPlaygroundShareResponses, GetPublicSiteSettingsData, GetPublicSiteSettingsErrors, GetPublicSiteSettingsResponses, GetRegistrationStatusData, GetRegistrationStatusErrors, GetRegistrationStatusResponses, GetUserInvitationsData, GetUserInvitationsErrors, GetUserInvitationsResponses, GetUserProfileData, GetUserProfileErrors, GetUserProfileResponses, GetUserTokenData, GetUserTokenErrors, GetUserTokenResponses, GetUserWalletData, GetUserWalletErrors, GetUserWalletResponses, InitializeAdminSetupData, InitializeAdminSetupErrors, InitializeAdminSetupResponses, ListAdminChannelsData, ListAdminChannelsErrors, ListAdminChannelsResponses, ListAdminCredentialsData, ListAdminCredentialsErrors, ListAdminCredentialsResponses, ListAdminGroupsData, ListAdminGroupsErrors, ListAdminGroupsResponses, ListAdminModelPricesData, ListAdminModelPricesErrors, ListAdminModelPricesResponses, ListAdminModelsData, ListAdminModelsErrors, ListAdminModelsResponses, ListAdminRedemptionBatchesData, ListAdminRedemptionBatchesErrors, ListAdminRedemptionBatchesResponses, ListAdminTokensData, ListAdminTokensErrors, ListAdminTokensResponses, ListAdminUsageLogsData, ListAdminUsageLogsErrors, ListAdminUsageLogsResponses, ListAdminUsersData, ListAdminUsersErrors, ListAdminUsersResponses, ListAdminWalletEntriesData, ListAdminWalletEntriesErrors, ListAdminWalletEntriesResponses, ListMissingAdminModelsData, ListMissingAdminModelsErrors, ListMissingAdminModelsResponses, ListModelsData, ListModelsErrors, ListModelsResponses, ListPlaygroundConversationsData, ListPlaygroundConversationsErrors, ListPlaygroundConversationsResponses, ListUserTokensData, ListUserTokensErrors, ListUserTokensResponses, ListUserWalletEntriesData, ListUserWalletEntriesErrors, ListUserWalletEntriesResponses, LoginManagementSessionData, LoginManagementSessionErrors, LoginManagementSessionResponses, PreviewAdminModelPricesData, PreviewAdminModelPricesErrors, PreviewAdminModelPricesResponses, ProbeAdminChannelData, ProbeAdminChannelErrors, ProbeAdminChannelResponses, RedeemUserRedemptionCodeData, RedeemUserRedemptionCodeErrors, RedeemUserRedemptionCodeResponses, RegisterUserData, RegisterUserErrors, RegisterUserResponses, RequestPasswordResetData, RequestPasswordResetErrors, RequestPasswordResetResponses, RevokePlaygroundShareData, RevokePlaygroundShareErrors, RevokePlaygroundShareResponses, SavePlaygroundConversationData, SavePlaygroundConversationErrors, SavePlaygroundConversationResponses, SendAdminEmailTestData, SendAdminEmailTestErrors, SendAdminEmailTestResponses, SendRegistrationEmailVerificationData, SendRegistrationEmailVerificationErrors, SendRegistrationEmailVerificationResponses, UpdateAdminAuthenticationSettingsData, UpdateAdminAuthenticationSettingsErrors, UpdateAdminAuthenticationSettingsResponses, UpdateAdminBalanceAlertSettingsData, UpdateAdminBalanceAlertSettingsErrors, UpdateAdminBalanceAlertSettingsResponses, UpdateAdminChannelData, UpdateAdminChannelErrors, UpdateAdminChannelResponses, UpdateAdminCredentialData, UpdateAdminCredentialErrors, UpdateAdminCredentialResponses, UpdateAdminEmailSettingsData, UpdateAdminEmailSettingsErrors, UpdateAdminEmailSettingsResponses, UpdateAdminGroupData, UpdateAdminGroupErrors, UpdateAdminGroupResponses, UpdateAdminModelData, UpdateAdminModelErrors, UpdateAdminModelResponses, UpdateAdminNetworkSettingsData, UpdateAdminNetworkSettingsErrors, UpdateAdminNetworkSettingsResponses, UpdateAdminSiteSettingsData, UpdateAdminSiteSettingsErrors, UpdateAdminSiteSettingsResponses, UpdateAdminTokenData, UpdateAdminTokenErrors, UpdateAdminTokenResponses, UpdateAdminUserData, UpdateAdminUserErrors, UpdateAdminUserResponses, UpdateUserNotificationPreferencesData, UpdateUserNotificationPreferencesErrors, UpdateUserNotificationPreferencesResponses, UpdateUserProfileData, UpdateUserProfileErrors, UpdateUserProfileResponses, UpdateUserTokenData, UpdateUserTokenErrors, UpdateUserTokenResponses } from './types.gen';
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
export const sendRegistrationEmailVerification = <ThrowOnError extends boolean = true>(options: Options<SendRegistrationEmailVerificationData, ThrowOnError>): RequestResult<SendRegistrationEmailVerificationResponses, SendRegistrationEmailVerificationErrors, ThrowOnError> => (options.client ?? client).post<SendRegistrationEmailVerificationResponses, SendRegistrationEmailVerificationErrors, ThrowOnError>({
    url: '/api/registration/email-verification',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const registerUser = <ThrowOnError extends boolean = true>(options: Options<RegisterUserData, ThrowOnError>): RequestResult<RegisterUserResponses, RegisterUserErrors, ThrowOnError> => (options.client ?? client).post<RegisterUserResponses, RegisterUserErrors, ThrowOnError>({
    url: '/api/registration',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminAuthenticationSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminAuthenticationSettingsData, ThrowOnError>): RequestResult<GetAdminAuthenticationSettingsResponses, GetAdminAuthenticationSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminAuthenticationSettingsResponses, GetAdminAuthenticationSettingsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/authentication-settings',
    ...options
});
export const updateAdminAuthenticationSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminAuthenticationSettingsData, ThrowOnError>): RequestResult<UpdateAdminAuthenticationSettingsResponses, UpdateAdminAuthenticationSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminAuthenticationSettingsResponses, UpdateAdminAuthenticationSettingsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/authentication-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminRedemptionBatches = <ThrowOnError extends boolean = true>(options?: Options<ListAdminRedemptionBatchesData, ThrowOnError>): RequestResult<ListAdminRedemptionBatchesResponses, ListAdminRedemptionBatchesErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminRedemptionBatchesResponses, ListAdminRedemptionBatchesErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/redemption-batches',
    ...options
});
export const createAdminRedemptionBatch = <ThrowOnError extends boolean = true>(options: Options<CreateAdminRedemptionBatchData, ThrowOnError>): RequestResult<CreateAdminRedemptionBatchResponses, CreateAdminRedemptionBatchErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminRedemptionBatchResponses, CreateAdminRedemptionBatchErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/redemption-batches',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const disableAdminRedemptionBatch = <ThrowOnError extends boolean = true>(options: Options<DisableAdminRedemptionBatchData, ThrowOnError>): RequestResult<DisableAdminRedemptionBatchResponses, DisableAdminRedemptionBatchErrors, ThrowOnError> => (options.client ?? client).post<DisableAdminRedemptionBatchResponses, DisableAdminRedemptionBatchErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/redemption-batches/{batch_id}/disable',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const redeemUserRedemptionCode = <ThrowOnError extends boolean = true>(options: Options<RedeemUserRedemptionCodeData, ThrowOnError>): RequestResult<RedeemUserRedemptionCodeResponses, RedeemUserRedemptionCodeErrors, ThrowOnError> => (options.client ?? client).post<RedeemUserRedemptionCodeResponses, RedeemUserRedemptionCodeErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/account/wallet/redemptions',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const requestPasswordReset = <ThrowOnError extends boolean = true>(options: Options<RequestPasswordResetData, ThrowOnError>): RequestResult<RequestPasswordResetResponses, RequestPasswordResetErrors, ThrowOnError> => (options.client ?? client).post<RequestPasswordResetResponses, RequestPasswordResetErrors, ThrowOnError>({
    url: '/api/auth/password-reset/request',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const confirmPasswordReset = <ThrowOnError extends boolean = true>(options: Options<ConfirmPasswordResetData, ThrowOnError>): RequestResult<ConfirmPasswordResetResponses, ConfirmPasswordResetErrors, ThrowOnError> => (options.client ?? client).post<ConfirmPasswordResetResponses, ConfirmPasswordResetErrors, ThrowOnError>({
    url: '/api/auth/password-reset/confirm',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getUserProfile = <ThrowOnError extends boolean = true>(options?: Options<GetUserProfileData, ThrowOnError>): RequestResult<GetUserProfileResponses, GetUserProfileErrors, ThrowOnError> => (options?.client ?? client).get<GetUserProfileResponses, GetUserProfileErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/account/profile',
    ...options
});
export const updateUserProfile = <ThrowOnError extends boolean = true>(options: Options<UpdateUserProfileData, ThrowOnError>): RequestResult<UpdateUserProfileResponses, UpdateUserProfileErrors, ThrowOnError> => (options.client ?? client).put<UpdateUserProfileResponses, UpdateUserProfileErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/account/profile',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const changeUserPassword = <ThrowOnError extends boolean = true>(options: Options<ChangeUserPasswordData, ThrowOnError>): RequestResult<ChangeUserPasswordResponses, ChangeUserPasswordErrors, ThrowOnError> => (options.client ?? client).put<ChangeUserPasswordResponses, ChangeUserPasswordErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/account/password',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const updateUserNotificationPreferences = <ThrowOnError extends boolean = true>(options: Options<UpdateUserNotificationPreferencesData, ThrowOnError>): RequestResult<UpdateUserNotificationPreferencesResponses, UpdateUserNotificationPreferencesErrors, ThrowOnError> => (options.client ?? client).put<UpdateUserNotificationPreferencesResponses, UpdateUserNotificationPreferencesErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/account/notifications',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getUserWallet = <ThrowOnError extends boolean = true>(options?: Options<GetUserWalletData, ThrowOnError>): RequestResult<GetUserWalletResponses, GetUserWalletErrors, ThrowOnError> => (options?.client ?? client).get<GetUserWalletResponses, GetUserWalletErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/account/wallet',
    ...options
});
export const listUserWalletEntries = <ThrowOnError extends boolean = true>(options?: Options<ListUserWalletEntriesData, ThrowOnError>): RequestResult<ListUserWalletEntriesResponses, ListUserWalletEntriesErrors, ThrowOnError> => (options?.client ?? client).get<ListUserWalletEntriesResponses, ListUserWalletEntriesErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/account/wallet/entries',
    ...options
});
export const getUserInvitations = <ThrowOnError extends boolean = true>(options?: Options<GetUserInvitationsData, ThrowOnError>): RequestResult<GetUserInvitationsResponses, GetUserInvitationsErrors, ThrowOnError> => (options?.client ?? client).get<GetUserInvitationsResponses, GetUserInvitationsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/account/invitations',
    ...options
});
export const getAdminEmailSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminEmailSettingsData, ThrowOnError>): RequestResult<GetAdminEmailSettingsResponses, GetAdminEmailSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminEmailSettingsResponses, GetAdminEmailSettingsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/email-settings',
    ...options
});
export const updateAdminEmailSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminEmailSettingsData, ThrowOnError>): RequestResult<UpdateAdminEmailSettingsResponses, UpdateAdminEmailSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminEmailSettingsResponses, UpdateAdminEmailSettingsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/email-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const sendAdminEmailTest = <ThrowOnError extends boolean = true>(options: Options<SendAdminEmailTestData, ThrowOnError>): RequestResult<SendAdminEmailTestResponses, SendAdminEmailTestErrors, ThrowOnError> => (options.client ?? client).post<SendAdminEmailTestResponses, SendAdminEmailTestErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/email-settings/test',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminNetworkSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminNetworkSettingsData, ThrowOnError>): RequestResult<GetAdminNetworkSettingsResponses, GetAdminNetworkSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminNetworkSettingsResponses, GetAdminNetworkSettingsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/network-settings',
    ...options
});
export const updateAdminNetworkSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminNetworkSettingsData, ThrowOnError>): RequestResult<UpdateAdminNetworkSettingsResponses, UpdateAdminNetworkSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminNetworkSettingsResponses, UpdateAdminNetworkSettingsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/network-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminBalanceAlertSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminBalanceAlertSettingsData, ThrowOnError>): RequestResult<GetAdminBalanceAlertSettingsResponses, GetAdminBalanceAlertSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminBalanceAlertSettingsResponses, GetAdminBalanceAlertSettingsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/balance-alert-settings',
    ...options
});
export const updateAdminBalanceAlertSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminBalanceAlertSettingsData, ThrowOnError>): RequestResult<UpdateAdminBalanceAlertSettingsResponses, UpdateAdminBalanceAlertSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminBalanceAlertSettingsResponses, UpdateAdminBalanceAlertSettingsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/balance-alert-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getPublicSiteSettings = <ThrowOnError extends boolean = true>(options?: Options<GetPublicSiteSettingsData, ThrowOnError>): RequestResult<GetPublicSiteSettingsResponses, GetPublicSiteSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetPublicSiteSettingsResponses, GetPublicSiteSettingsErrors, ThrowOnError>({ url: '/api/site', ...options });
export const getAdminSiteSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminSiteSettingsData, ThrowOnError>): RequestResult<GetAdminSiteSettingsResponses, GetAdminSiteSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminSiteSettingsResponses, GetAdminSiteSettingsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/site-settings',
    ...options
});
export const updateAdminSiteSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminSiteSettingsData, ThrowOnError>): RequestResult<UpdateAdminSiteSettingsResponses, UpdateAdminSiteSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminSiteSettingsResponses, UpdateAdminSiteSettingsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/site-settings',
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
export const listAdminWalletEntries = <ThrowOnError extends boolean = true>(options: Options<ListAdminWalletEntriesData, ThrowOnError>): RequestResult<ListAdminWalletEntriesResponses, ListAdminWalletEntriesErrors, ThrowOnError> => (options.client ?? client).get<ListAdminWalletEntriesResponses, ListAdminWalletEntriesErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/users/{id}/wallet/entries',
    ...options
});
export const adjustAdminWallet = <ThrowOnError extends boolean = true>(options: Options<AdjustAdminWalletData, ThrowOnError>): RequestResult<AdjustAdminWalletResponses, AdjustAdminWalletErrors, ThrowOnError> => (options.client ?? client).post<AdjustAdminWalletResponses, AdjustAdminWalletErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/users/{id}/wallet/adjustments',
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
export const listAdminModels = <ThrowOnError extends boolean = true>(options?: Options<ListAdminModelsData, ThrowOnError>): RequestResult<ListAdminModelsResponses, ListAdminModelsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminModelsResponses, ListAdminModelsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/models',
    ...options
});
export const createAdminModel = <ThrowOnError extends boolean = true>(options: Options<CreateAdminModelData, ThrowOnError>): RequestResult<CreateAdminModelResponses, CreateAdminModelErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminModelResponses, CreateAdminModelErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/models',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminModel = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminModelData, ThrowOnError>): RequestResult<DeleteAdminModelResponses, DeleteAdminModelErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminModelResponses, DeleteAdminModelErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/models/{id}',
    ...options
});
export const getAdminModel = <ThrowOnError extends boolean = true>(options: Options<GetAdminModelData, ThrowOnError>): RequestResult<GetAdminModelResponses, GetAdminModelErrors, ThrowOnError> => (options.client ?? client).get<GetAdminModelResponses, GetAdminModelErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/models/{id}',
    ...options
});
export const updateAdminModel = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminModelData, ThrowOnError>): RequestResult<UpdateAdminModelResponses, UpdateAdminModelErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminModelResponses, UpdateAdminModelErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/models/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listMissingAdminModels = <ThrowOnError extends boolean = true>(options?: Options<ListMissingAdminModelsData, ThrowOnError>): RequestResult<ListMissingAdminModelsResponses, ListMissingAdminModelsErrors, ThrowOnError> => (options?.client ?? client).get<ListMissingAdminModelsResponses, ListMissingAdminModelsErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/models/missing',
    ...options
});
export const createAdminModelSyncPreview = <ThrowOnError extends boolean = true>(options: Options<CreateAdminModelSyncPreviewData, ThrowOnError>): RequestResult<CreateAdminModelSyncPreviewResponses, CreateAdminModelSyncPreviewErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminModelSyncPreviewResponses, CreateAdminModelSyncPreviewErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/models/sync-previews',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const applyAdminModelSyncPreview = <ThrowOnError extends boolean = true>(options: Options<ApplyAdminModelSyncPreviewData, ThrowOnError>): RequestResult<ApplyAdminModelSyncPreviewResponses, ApplyAdminModelSyncPreviewErrors, ThrowOnError> => (options.client ?? client).post<ApplyAdminModelSyncPreviewResponses, ApplyAdminModelSyncPreviewErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/models/sync-previews/{preview_id}/apply',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminModelPrices = <ThrowOnError extends boolean = true>(options?: Options<ListAdminModelPricesData, ThrowOnError>): RequestResult<ListAdminModelPricesResponses, ListAdminModelPricesErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminModelPricesResponses, ListAdminModelPricesErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/model-prices',
    ...options
});
export const previewAdminModelPrices = <ThrowOnError extends boolean = true>(options?: Options<PreviewAdminModelPricesData, ThrowOnError>): RequestResult<PreviewAdminModelPricesResponses, PreviewAdminModelPricesErrors, ThrowOnError> => (options?.client ?? client).post<PreviewAdminModelPricesResponses, PreviewAdminModelPricesErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/model-prices/models-dev-preview',
    ...options
});
export const applyAdminModelPrices = <ThrowOnError extends boolean = true>(options: Options<ApplyAdminModelPricesData, ThrowOnError>): RequestResult<ApplyAdminModelPricesResponses, ApplyAdminModelPricesErrors, ThrowOnError> => (options.client ?? client).post<ApplyAdminModelPricesResponses, ApplyAdminModelPricesErrors, ThrowOnError>({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/api/admin/model-prices/batch',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
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
