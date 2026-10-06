// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import { type Client, type ClientMeta, formDataBodySerializer, type Options as Options2, type RequestResult, type TDataShape } from './client';
import { client } from './client.gen';
import type { ActivateAdminFrontendTemplateData, ActivateAdminFrontendTemplateErrors, ActivateAdminFrontendTemplateResponses, AdjustAdminWalletData, AdjustAdminWalletErrors, AdjustAdminWalletResponses, ApplyAdminModelPricesData, ApplyAdminModelPricesErrors, ApplyAdminModelPricesResponses, ApplyAdminModelSyncPreviewData, ApplyAdminModelSyncPreviewErrors, ApplyAdminModelSyncPreviewResponses, ApproveAdminRefundData, ApproveAdminRefundErrors, ApproveAdminRefundResponses, BeginAdminOAuthAuthorizationData, BeginAdminOAuthAuthorizationErrors, BeginAdminOAuthAuthorizationResponses, BindAdminUserSubscriptionData, BindAdminUserSubscriptionErrors, BindAdminUserSubscriptionResponses, ChangeUserPasswordData, ChangeUserPasswordErrors, ChangeUserPasswordResponses, CompactResponseData, CompactResponseErrors, CompactResponseResponses, CompleteAdminOAuthManualCallbackData, CompleteAdminOAuthManualCallbackErrors, CompleteAdminOAuthManualCallbackResponses, CompleteCustomOAuth2LoginData, CompleteCustomOAuth2LoginErrors, CompleteDiscordOAuthLoginData, CompleteDiscordOAuthLoginErrors, CompleteGitHubOAuthLoginData, CompleteGitHubOAuthLoginErrors, CompleteGoogleLoginData, CompleteGoogleLoginErrors, CompleteLinuxDoLoginData, CompleteLinuxDoLoginErrors, CompleteOidcLoginData, CompleteOidcLoginErrors, CompleteTelegramLoginData, CompleteTelegramLoginErrors, CompleteWeChatOAuthLoginData, CompleteWeChatOAuthLoginErrors, ConfirmPasswordResetData, ConfirmPasswordResetErrors, ConfirmPasswordResetResponses, ConfirmUserEmailBindingData, ConfirmUserEmailBindingErrors, ConfirmUserEmailBindingResponses, CreateAdminAnnouncementData, CreateAdminAnnouncementErrors, CreateAdminAnnouncementResponses, CreateAdminChannelData, CreateAdminChannelErrors, CreateAdminChannelResponses, CreateAdminCredentialData, CreateAdminCredentialErrors, CreateAdminCredentialProxyData, CreateAdminCredentialProxyErrors, CreateAdminCredentialProxyResponses, CreateAdminCredentialResponses, CreateAdminGroupData, CreateAdminGroupErrors, CreateAdminGroupResponses, CreateAdminModelData, CreateAdminModelErrors, CreateAdminModelResponses, CreateAdminModelSyncPreviewData, CreateAdminModelSyncPreviewErrors, CreateAdminModelSyncPreviewResponses, CreateAdminRedemptionBatchData, CreateAdminRedemptionBatchErrors, CreateAdminRedemptionBatchResponses, CreateAdminRouteData, CreateAdminRouteErrors, CreateAdminRouteResponses, CreateAdminSubscriptionPlanData, CreateAdminSubscriptionPlanErrors, CreateAdminSubscriptionPlanResponses, CreateAdminTokenData, CreateAdminTokenErrors, CreateAdminTokenResponses, CreateAdminUserData, CreateAdminUserErrors, CreateAdminUserResponses, CreateCurrentSubscriptionOrderData, CreateCurrentSubscriptionOrderErrors, CreateCurrentSubscriptionOrderResponses, CreatePlaygroundShareData, CreatePlaygroundShareErrors, CreatePlaygroundShareResponses, CreateUserTokenData, CreateUserTokenErrors, CreateUserTokenResponses, CreateUserTopupOrderData, CreateUserTopupOrderErrors, CreateUserTopupOrderResponses, DecideAccountVerificationData, DecideAccountVerificationErrors, DecideAccountVerificationResponses, DeleteAdminChannelData, DeleteAdminChannelErrors, DeleteAdminChannelResponses, DeleteAdminCredentialData, DeleteAdminCredentialErrors, DeleteAdminCredentialProxyData, DeleteAdminCredentialProxyErrors, DeleteAdminCredentialProxyResponses, DeleteAdminCredentialResponses, DeleteAdminGroupData, DeleteAdminGroupErrors, DeleteAdminGroupResponses, DeleteAdminModelData, DeleteAdminModelErrors, DeleteAdminModelResponses, DeleteAdminRouteData, DeleteAdminRouteErrors, DeleteAdminRouteResponses, DeleteAdminTokenData, DeleteAdminTokenErrors, DeleteAdminTokenResponses, DeleteAdminUserData, DeleteAdminUserErrors, DeleteAdminUserResponses, DeletePlaygroundConversationData, DeletePlaygroundConversationErrors, DeletePlaygroundConversationResponses, DeleteUserTokenData, DeleteUserTokenErrors, DeleteUserTokenResponses, DisableAdminRedemptionBatchData, DisableAdminRedemptionBatchErrors, DisableAdminRedemptionBatchResponses, DisableAdminSubscriptionPlanData, DisableAdminSubscriptionPlanErrors, DisableAdminSubscriptionPlanResponses, DisableUserTwoFactorData, DisableUserTwoFactorErrors, DisableUserTwoFactorResponses, DownloadAccountVerificationMaterialData, DownloadAccountVerificationMaterialErrors, DownloadAccountVerificationMaterialResponses, DownloadAdminAccountVerificationMaterialData, DownloadAdminAccountVerificationMaterialErrors, DownloadAdminAccountVerificationMaterialResponses, EnableUserTwoFactorData, EnableUserTwoFactorErrors, EnableUserTwoFactorResponses, ExchangeOAuthLoginTicketData, ExchangeOAuthLoginTicketErrors, ExchangeOAuthLoginTicketResponses, ExportAdminCredentialsData, ExportAdminCredentialsErrors, ExportAdminCredentialsResponses, FinishPasskeyAuthenticationData, FinishPasskeyAuthenticationErrors, FinishPasskeyAuthenticationResponses, FinishUserPasskeyRegistrationData, FinishUserPasskeyRegistrationErrors, FinishUserPasskeyRegistrationResponses, GetAccountVerificationData, GetAccountVerificationEligibilityData, GetAccountVerificationEligibilityErrors, GetAccountVerificationEligibilityResponses, GetAccountVerificationErrors, GetAccountVerificationResponses, GetAdminAccountVerificationData, GetAdminAccountVerificationErrors, GetAdminAccountVerificationResponses, GetAdminAnalyticsExportStatusData, GetAdminAnalyticsExportStatusErrors, GetAdminAnalyticsExportStatusResponses, GetAdminAuthenticationSettingsData, GetAdminAuthenticationSettingsErrors, GetAdminAuthenticationSettingsResponses, GetAdminBalanceAlertSettingsData, GetAdminBalanceAlertSettingsErrors, GetAdminBalanceAlertSettingsResponses, GetAdminChannelData, GetAdminChannelErrors, GetAdminChannelResponses, GetAdminCredentialData, GetAdminCredentialErrors, GetAdminCredentialProxyData, GetAdminCredentialProxyErrors, GetAdminCredentialProxyResponses, GetAdminCredentialResponses, GetAdminCredentialUsageData, GetAdminCredentialUsageErrors, GetAdminCredentialUsageResponses, GetAdminCustomOAuth2ProviderData, GetAdminCustomOAuth2ProviderErrors, GetAdminCustomOAuth2ProviderResponses, GetAdminDashboardData, GetAdminDashboardErrors, GetAdminDashboardResponses, GetAdminDebugTraceData, GetAdminDebugTraceErrors, GetAdminDebugTraceResponses, GetAdminDebugTraceSettingsData, GetAdminDebugTraceSettingsErrors, GetAdminDebugTraceSettingsResponses, GetAdminDiscordOAuthLoginSettingsData, GetAdminDiscordOAuthLoginSettingsErrors, GetAdminDiscordOAuthLoginSettingsResponses, GetAdminEmailSettingsData, GetAdminEmailSettingsErrors, GetAdminEmailSettingsResponses, GetAdminFrontendTemplatePreviewData, GetAdminFrontendTemplatePreviewErrors, GetAdminFrontendTemplatePreviewResponses, GetAdminGitHubOAuthLoginSettingsData, GetAdminGitHubOAuthLoginSettingsErrors, GetAdminGitHubOAuthLoginSettingsResponses, GetAdminGoogleOAuthLoginSettingsData, GetAdminGoogleOAuthLoginSettingsErrors, GetAdminGoogleOAuthLoginSettingsResponses, GetAdminGroupData, GetAdminGroupErrors, GetAdminGroupResponses, GetAdminLinuxDoLoginSettingsData, GetAdminLinuxDoLoginSettingsErrors, GetAdminLinuxDoLoginSettingsResponses, GetAdminModelData, GetAdminModelErrors, GetAdminModelResponses, GetAdminNetworkSettingsData, GetAdminNetworkSettingsErrors, GetAdminNetworkSettingsResponses, GetAdminOidcLoginSettingsData, GetAdminOidcLoginSettingsErrors, GetAdminOidcLoginSettingsResponses, GetAdminPaymentSettingsData, GetAdminPaymentSettingsErrors, GetAdminPaymentSettingsResponses, GetAdminRouteData, GetAdminRouteErrors, GetAdminRouteResponses, GetAdminServiceLevelsData, GetAdminServiceLevelsErrors, GetAdminServiceLevelsResponses, GetAdminSiteSettingsData, GetAdminSiteSettingsErrors, GetAdminSiteSettingsResponses, GetAdminTelegramOAuthLoginSettingsData, GetAdminTelegramOAuthLoginSettingsErrors, GetAdminTelegramOAuthLoginSettingsResponses, GetAdminTokenData, GetAdminTokenErrors, GetAdminTokenResponses, GetAdminUserData, GetAdminUserErrors, GetAdminUserResponses, GetAdminVerificationSettingsData, GetAdminVerificationSettingsErrors, GetAdminVerificationSettingsResponses, GetAdminWeChatOAuthLoginSettingsData, GetAdminWeChatOAuthLoginSettingsErrors, GetAdminWeChatOAuthLoginSettingsResponses, GetCurrentSubscriptionOrderData, GetCurrentSubscriptionOrderErrors, GetCurrentSubscriptionOrderResponses, GetInitialSetupStatusData, GetInitialSetupStatusErrors, GetInitialSetupStatusResponses, GetManagementSessionData, GetManagementSessionErrors, GetManagementSessionResponses, GetPlaygroundConversationData, GetPlaygroundConversationErrors, GetPlaygroundConversationResponses, GetPlaygroundShareData, GetPlaygroundShareErrors, GetPlaygroundShareResponses, GetPublicSiteSettingsData, GetPublicSiteSettingsErrors, GetPublicSiteSettingsResponses, GetRegistrationStatusData, GetRegistrationStatusErrors, GetRegistrationStatusResponses, GetUserInvitationsData, GetUserInvitationsErrors, GetUserInvitationsResponses, GetUserProfileData, GetUserProfileErrors, GetUserProfileResponses, GetUserTokenData, GetUserTokenErrors, GetUserTokenResponses, GetUserTopupConfigurationData, GetUserTopupConfigurationErrors, GetUserTopupConfigurationResponses, GetUserTwoFactorData, GetUserTwoFactorErrors, GetUserTwoFactorResponses, GetUserWalletData, GetUserWalletErrors, GetUserWalletResponses, ImportAdminCredentialsData, ImportAdminCredentialsErrors, ImportAdminCredentialsResponses, ImportMissingAdminModelsData, ImportMissingAdminModelsErrors, ImportMissingAdminModelsResponses, InitializeAdminSetupData, InitializeAdminSetupErrors, InitializeAdminSetupResponses, ListAccountRefundReconciliationsData, ListAccountRefundReconciliationsErrors, ListAccountRefundReconciliationsResponses, ListAccountVerificationsData, ListAccountVerificationsErrors, ListAccountVerificationsResponses, ListAdminAccountVerificationsData, ListAdminAccountVerificationsErrors, ListAdminAccountVerificationsResponses, ListAdminAnnouncementsData, ListAdminAnnouncementsErrors, ListAdminAnnouncementsResponses, ListAdminChannelsData, ListAdminChannelsErrors, ListAdminChannelsResponses, ListAdminCredentialProxiesData, ListAdminCredentialProxiesErrors, ListAdminCredentialProxiesResponses, ListAdminCredentialsData, ListAdminCredentialsErrors, ListAdminCredentialsResponses, ListAdminCustomOAuth2ProvidersData, ListAdminCustomOAuth2ProvidersErrors, ListAdminCustomOAuth2ProvidersResponses, ListAdminDebugTracesData, ListAdminDebugTracesErrors, ListAdminDebugTracesResponses, ListAdminFrontendTemplatesData, ListAdminFrontendTemplatesErrors, ListAdminFrontendTemplatesResponses, ListAdminGroupsData, ListAdminGroupsErrors, ListAdminGroupsResponses, ListAdminModelPricesData, ListAdminModelPricesErrors, ListAdminModelPricesResponses, ListAdminModelsData, ListAdminModelsErrors, ListAdminModelsResponses, ListAdminOAuthProvidersData, ListAdminOAuthProvidersErrors, ListAdminOAuthProvidersResponses, ListAdminPlatformAuditLogsData, ListAdminPlatformAuditLogsErrors, ListAdminPlatformAuditLogsResponses, ListAdminRedemptionAuditData, ListAdminRedemptionAuditErrors, ListAdminRedemptionAuditResponses, ListAdminRedemptionBatchesData, ListAdminRedemptionBatchesErrors, ListAdminRedemptionBatchesResponses, ListAdminRefundReconciliationsData, ListAdminRefundReconciliationsErrors, ListAdminRefundReconciliationsResponses, ListAdminRefundsData, ListAdminRefundsErrors, ListAdminRefundsResponses, ListAdminRoutesData, ListAdminRoutesErrors, ListAdminRoutesResponses, ListAdminSubscriptionPlansData, ListAdminSubscriptionPlansErrors, ListAdminSubscriptionPlansResponses, ListAdminTokensData, ListAdminTokensErrors, ListAdminTokensResponses, ListAdminUsageLogsData, ListAdminUsageLogsErrors, ListAdminUsageLogsResponses, ListAdminUsersData, ListAdminUsersErrors, ListAdminUsersResponses, ListAdminUserSubscriptionsData, ListAdminUserSubscriptionsErrors, ListAdminUserSubscriptionsResponses, ListAdminWalletEntriesData, ListAdminWalletEntriesErrors, ListAdminWalletEntriesResponses, ListCurrentSubscriptionCatalogData, ListCurrentSubscriptionCatalogErrors, ListCurrentSubscriptionCatalogResponses, ListCurrentUserSubscriptionsData, ListCurrentUserSubscriptionsErrors, ListCurrentUserSubscriptionsResponses, ListExtensionCatalogData, ListExtensionCatalogResponses, ListGatewayModelsData, ListGatewayModelsErrors, ListGatewayModelsResponses, ListMissingAdminModelsData, ListMissingAdminModelsErrors, ListMissingAdminModelsResponses, ListModelProvidersData, ListModelProvidersErrors, ListModelProvidersResponses, ListModelsData, ListModelsErrors, ListModelsResponses, ListOrganizationRefundReconciliationsData, ListOrganizationRefundReconciliationsErrors, ListOrganizationRefundReconciliationsResponses, ListPlaygroundConversationsData, ListPlaygroundConversationsErrors, ListPlaygroundConversationsResponses, ListPublicAnnouncementsData, ListPublicAnnouncementsErrors, ListPublicAnnouncementsResponses, ListSelfPlatformAuditLogsData, ListSelfPlatformAuditLogsErrors, ListSelfPlatformAuditLogsResponses, ListUserNotificationsData, ListUserNotificationsErrors, ListUserNotificationsResponses, ListUserPasskeysData, ListUserPasskeysErrors, ListUserPasskeysResponses, ListUserTokensData, ListUserTokensErrors, ListUserTokensResponses, ListUserUsageLogsData, ListUserUsageLogsErrors, ListUserUsageLogsResponses, ListUserWalletEntriesData, ListUserWalletEntriesErrors, ListUserWalletEntriesResponses, ListVideoTasksData, ListVideoTasksErrors, ListVideoTasksResponses, LoginManagementSessionData, LoginManagementSessionErrors, LoginManagementSessionResponses, ManualCompleteAdminRefundData, ManualCompleteAdminRefundErrors, ManualCompleteAdminRefundResponses, MarkUserNotificationsReadData, MarkUserNotificationsReadErrors, MarkUserNotificationsReadResponses, PollVideoTaskData, PollVideoTaskErrors, PollVideoTaskResponses, PreviewAdminLiteLlmModelPricesData, PreviewAdminLiteLlmModelPricesErrors, PreviewAdminLiteLlmModelPricesResponses, PreviewAdminModelPriceExpressionData, PreviewAdminModelPriceExpressionErrors, PreviewAdminModelPriceExpressionResponses, PreviewAdminModelPricesData, PreviewAdminModelPricesErrors, PreviewAdminModelPricesResponses, ProbeAdminChannelData, ProbeAdminChannelErrors, ProbeAdminChannelResponses, PublishAdminAnnouncementData, PublishAdminAnnouncementErrors, PublishAdminAnnouncementResponses, ReadAdminDebugTraceSnapshotsData, ReadAdminDebugTraceSnapshotsErrors, ReadAdminDebugTraceSnapshotsResponses, ReceivePaymentWebhookData, ReceivePaymentWebhookErrors, ReceivePaymentWebhookResponses, ReceiveRefundWebhookData, ReceiveRefundWebhookErrors, ReceiveRefundWebhookResponses, RedeemUserRedemptionCodeData, RedeemUserRedemptionCodeErrors, RedeemUserRedemptionCodeResponses, RegisterUserData, RegisterUserErrors, RegisterUserResponses, RejectAdminRefundData, RejectAdminRefundErrors, RejectAdminRefundResponses, RenameUserPasskeyData, RenameUserPasskeyErrors, RenameUserPasskeyResponses, ReplayAdminAnalyticsExportData, ReplayAdminAnalyticsExportErrors, ReplayAdminAnalyticsExportResponses, RequestPasswordResetData, RequestPasswordResetErrors, RequestPasswordResetResponses, RerankData, RerankErrors, RerankResponses, RevokeAdminAnnouncementData, RevokeAdminAnnouncementErrors, RevokeAdminAnnouncementResponses, RevokePlaygroundShareData, RevokePlaygroundShareErrors, RevokePlaygroundShareResponses, RevokeUserPasskeyData, RevokeUserPasskeyErrors, RevokeUserPasskeyResponses, SavePlaygroundConversationData, SavePlaygroundConversationErrors, SavePlaygroundConversationResponses, ScanAdminFrontendTemplatesData, ScanAdminFrontendTemplatesErrors, ScanAdminFrontendTemplatesResponses, SendAdminEmailTestData, SendAdminEmailTestErrors, SendAdminEmailTestResponses, SendRegistrationEmailVerificationData, SendRegistrationEmailVerificationErrors, SendRegistrationEmailVerificationResponses, SendUserEmailBindingVerificationData, SendUserEmailBindingVerificationErrors, SendUserEmailBindingVerificationResponses, StartCustomOAuth2LoginData, StartCustomOAuth2LoginErrors, StartCustomOAuth2LoginResponses, StartDiscordOAuthLoginData, StartDiscordOAuthLoginErrors, StartDiscordOAuthLoginResponses, StartGitHubOAuthLoginData, StartGitHubOAuthLoginErrors, StartGitHubOAuthLoginResponses, StartGoogleLoginData, StartGoogleLoginErrors, StartGoogleLoginResponses, StartLinuxDoLoginData, StartLinuxDoLoginErrors, StartLinuxDoLoginResponses, StartOidcLoginData, StartOidcLoginErrors, StartOidcLoginResponses, StartPasskeyAuthenticationData, StartPasskeyAuthenticationErrors, StartPasskeyAuthenticationResponses, StartTelegramLoginData, StartTelegramLoginErrors, StartTelegramLoginResponses, StartUserPasskeyRegistrationData, StartUserPasskeyRegistrationErrors, StartUserPasskeyRegistrationResponses, StartWeChatOAuthLoginData, StartWeChatOAuthLoginErrors, StartWeChatOAuthLoginResponses, SubmitAccountVerificationData, SubmitAccountVerificationErrors, SubmitAccountVerificationResponses, SubmitAdminRefundData, SubmitAdminRefundErrors, SubmitAdminRefundResponses, SubmitCurrentSubscriptionOrderPaymentData, SubmitCurrentSubscriptionOrderPaymentErrors, SubmitCurrentSubscriptionOrderPaymentResponses, SubmitVideoTaskData, SubmitVideoTaskErrors, SubmitVideoTaskResponses, SyncAccountVerificationProviderData, SyncAccountVerificationProviderErrors, SyncAccountVerificationProviderResponses, SynthesizeSpeechData, SynthesizeSpeechErrors, SynthesizeSpeechResponses, TransitionAdminUserSubscriptionLifecycleData, TransitionAdminUserSubscriptionLifecycleErrors, TransitionAdminUserSubscriptionLifecycleResponses, UpdateAdminAnnouncementData, UpdateAdminAnnouncementErrors, UpdateAdminAnnouncementResponses, UpdateAdminAuthenticationSettingsData, UpdateAdminAuthenticationSettingsErrors, UpdateAdminAuthenticationSettingsResponses, UpdateAdminBalanceAlertSettingsData, UpdateAdminBalanceAlertSettingsErrors, UpdateAdminBalanceAlertSettingsResponses, UpdateAdminChannelData, UpdateAdminChannelErrors, UpdateAdminChannelResponses, UpdateAdminCredentialData, UpdateAdminCredentialErrors, UpdateAdminCredentialProxyData, UpdateAdminCredentialProxyErrors, UpdateAdminCredentialProxyResponses, UpdateAdminCredentialResponses, UpdateAdminCustomOAuth2ProviderData, UpdateAdminCustomOAuth2ProviderErrors, UpdateAdminCustomOAuth2ProviderResponses, UpdateAdminDebugTraceSettingsData, UpdateAdminDebugTraceSettingsErrors, UpdateAdminDebugTraceSettingsResponses, UpdateAdminDiscordOAuthLoginSettingsData, UpdateAdminDiscordOAuthLoginSettingsErrors, UpdateAdminDiscordOAuthLoginSettingsResponses, UpdateAdminEmailSettingsData, UpdateAdminEmailSettingsErrors, UpdateAdminEmailSettingsResponses, UpdateAdminGitHubOAuthLoginSettingsData, UpdateAdminGitHubOAuthLoginSettingsErrors, UpdateAdminGitHubOAuthLoginSettingsResponses, UpdateAdminGoogleOAuthLoginSettingsData, UpdateAdminGoogleOAuthLoginSettingsErrors, UpdateAdminGoogleOAuthLoginSettingsResponses, UpdateAdminGroupData, UpdateAdminGroupErrors, UpdateAdminGroupResponses, UpdateAdminLinuxDoLoginSettingsData, UpdateAdminLinuxDoLoginSettingsErrors, UpdateAdminLinuxDoLoginSettingsResponses, UpdateAdminModelData, UpdateAdminModelErrors, UpdateAdminModelResponses, UpdateAdminNetworkSettingsData, UpdateAdminNetworkSettingsErrors, UpdateAdminNetworkSettingsResponses, UpdateAdminOidcLoginSettingsData, UpdateAdminOidcLoginSettingsErrors, UpdateAdminOidcLoginSettingsResponses, UpdateAdminPaymentSettingsData, UpdateAdminPaymentSettingsErrors, UpdateAdminPaymentSettingsResponses, UpdateAdminRouteData, UpdateAdminRouteErrors, UpdateAdminRouteResponses, UpdateAdminSiteNavigationData, UpdateAdminSiteNavigationErrors, UpdateAdminSiteNavigationResponses, UpdateAdminSiteSettingsData, UpdateAdminSiteSettingsErrors, UpdateAdminSiteSettingsResponses, UpdateAdminTelegramOAuthLoginSettingsData, UpdateAdminTelegramOAuthLoginSettingsErrors, UpdateAdminTelegramOAuthLoginSettingsResponses, UpdateAdminTokenData, UpdateAdminTokenErrors, UpdateAdminTokenResponses, UpdateAdminUserData, UpdateAdminUserErrors, UpdateAdminUserResponses, UpdateAdminVerificationSettingsData, UpdateAdminVerificationSettingsErrors, UpdateAdminVerificationSettingsResponses, UpdateAdminWeChatOAuthLoginSettingsData, UpdateAdminWeChatOAuthLoginSettingsErrors, UpdateAdminWeChatOAuthLoginSettingsResponses, UpdateUserNotificationPreferencesData, UpdateUserNotificationPreferencesErrors, UpdateUserNotificationPreferencesResponses, UpdateUserProfileData, UpdateUserProfileErrors, UpdateUserProfileResponses, UpdateUserTokenData, UpdateUserTokenErrors, UpdateUserTokenResponses } from './types.gen';
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
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/auth/session',
    ...options
});
export const listPublicAnnouncements = <ThrowOnError extends boolean = true>(options?: Options<ListPublicAnnouncementsData, ThrowOnError>): RequestResult<ListPublicAnnouncementsResponses, ListPublicAnnouncementsErrors, ThrowOnError> => (options?.client ?? client).get<ListPublicAnnouncementsResponses, ListPublicAnnouncementsErrors, ThrowOnError>({ url: '/api/announcements', ...options });
export const listAdminAnnouncements = <ThrowOnError extends boolean = true>(options?: Options<ListAdminAnnouncementsData, ThrowOnError>): RequestResult<ListAdminAnnouncementsResponses, ListAdminAnnouncementsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminAnnouncementsResponses, ListAdminAnnouncementsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/announcements',
    ...options
});
export const createAdminAnnouncement = <ThrowOnError extends boolean = true>(options: Options<CreateAdminAnnouncementData, ThrowOnError>): RequestResult<CreateAdminAnnouncementResponses, CreateAdminAnnouncementErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminAnnouncementResponses, CreateAdminAnnouncementErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/announcements',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const updateAdminAnnouncement = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminAnnouncementData, ThrowOnError>): RequestResult<UpdateAdminAnnouncementResponses, UpdateAdminAnnouncementErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminAnnouncementResponses, UpdateAdminAnnouncementErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/announcements/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const publishAdminAnnouncement = <ThrowOnError extends boolean = true>(options: Options<PublishAdminAnnouncementData, ThrowOnError>): RequestResult<PublishAdminAnnouncementResponses, PublishAdminAnnouncementErrors, ThrowOnError> => (options.client ?? client).post<PublishAdminAnnouncementResponses, PublishAdminAnnouncementErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/announcements/{id}/publish',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const revokeAdminAnnouncement = <ThrowOnError extends boolean = true>(options: Options<RevokeAdminAnnouncementData, ThrowOnError>): RequestResult<RevokeAdminAnnouncementResponses, RevokeAdminAnnouncementErrors, ThrowOnError> => (options.client ?? client).post<RevokeAdminAnnouncementResponses, RevokeAdminAnnouncementErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/announcements/{id}/revoke',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
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
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings',
    ...options
});
export const updateAdminAuthenticationSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminAuthenticationSettingsData, ThrowOnError>): RequestResult<UpdateAdminAuthenticationSettingsResponses, UpdateAdminAuthenticationSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminAuthenticationSettingsResponses, UpdateAdminAuthenticationSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const rerank = <ThrowOnError extends boolean = true>(options: Options<RerankData, ThrowOnError>): RequestResult<RerankResponses, RerankErrors, ThrowOnError> => (options.client ?? client).post<RerankResponses, RerankErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/v1/rerank',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const compactResponse = <ThrowOnError extends boolean = true>(options: Options<CompactResponseData, ThrowOnError>): RequestResult<CompactResponseResponses, CompactResponseErrors, ThrowOnError> => (options.client ?? client).post<CompactResponseResponses, CompactResponseErrors, ThrowOnError>({
    security: [{
            key: 'apiKeyAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/v1/responses/compact',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const synthesizeSpeech = <ThrowOnError extends boolean = true>(options: Options<SynthesizeSpeechData, ThrowOnError>): RequestResult<SynthesizeSpeechResponses, SynthesizeSpeechErrors, ThrowOnError> => (options.client ?? client).post<SynthesizeSpeechResponses, SynthesizeSpeechErrors, ThrowOnError>({
    security: [{
            key: 'apiKeyAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/v1/audio/speech',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listVideoTasks = <ThrowOnError extends boolean = true>(options?: Options<ListVideoTasksData, ThrowOnError>): RequestResult<ListVideoTasksResponses, ListVideoTasksErrors, ThrowOnError> => (options?.client ?? client).get<ListVideoTasksResponses, ListVideoTasksErrors, ThrowOnError>({
    security: [{
            key: 'apiKeyAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/v1/videos',
    ...options
});
export const submitVideoTask = <ThrowOnError extends boolean = true>(options: Options<SubmitVideoTaskData, ThrowOnError>): RequestResult<SubmitVideoTaskResponses, SubmitVideoTaskErrors, ThrowOnError> => (options.client ?? client).post<SubmitVideoTaskResponses, SubmitVideoTaskErrors, ThrowOnError>({
    security: [{
            key: 'apiKeyAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/v1/videos/generations',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const pollVideoTask = <ThrowOnError extends boolean = true>(options: Options<PollVideoTaskData, ThrowOnError>): RequestResult<PollVideoTaskResponses, PollVideoTaskErrors, ThrowOnError> => (options.client ?? client).get<PollVideoTaskResponses, PollVideoTaskErrors, ThrowOnError>({
    security: [{
            key: 'apiKeyAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/v1/videos/{task_id}',
    ...options
});
export const listAdminRedemptionAudit = <ThrowOnError extends boolean = true>(options?: Options<ListAdminRedemptionAuditData, ThrowOnError>): RequestResult<ListAdminRedemptionAuditResponses, ListAdminRedemptionAuditErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminRedemptionAuditResponses, ListAdminRedemptionAuditErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/redemption-audit',
    ...options
});
export const listAdminRedemptionBatches = <ThrowOnError extends boolean = true>(options?: Options<ListAdminRedemptionBatchesData, ThrowOnError>): RequestResult<ListAdminRedemptionBatchesResponses, ListAdminRedemptionBatchesErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminRedemptionBatchesResponses, ListAdminRedemptionBatchesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/redemption-batches',
    ...options
});
export const createAdminRedemptionBatch = <ThrowOnError extends boolean = true>(options: Options<CreateAdminRedemptionBatchData, ThrowOnError>): RequestResult<CreateAdminRedemptionBatchResponses, CreateAdminRedemptionBatchErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminRedemptionBatchResponses, CreateAdminRedemptionBatchErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/redemption-batches',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const disableAdminRedemptionBatch = <ThrowOnError extends boolean = true>(options: Options<DisableAdminRedemptionBatchData, ThrowOnError>): RequestResult<DisableAdminRedemptionBatchResponses, DisableAdminRedemptionBatchErrors, ThrowOnError> => (options.client ?? client).post<DisableAdminRedemptionBatchResponses, DisableAdminRedemptionBatchErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/redemption-batches/{batch_id}/disable',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const redeemUserRedemptionCode = <ThrowOnError extends boolean = true>(options: Options<RedeemUserRedemptionCodeData, ThrowOnError>): RequestResult<RedeemUserRedemptionCodeResponses, RedeemUserRedemptionCodeErrors, ThrowOnError> => (options.client ?? client).post<RedeemUserRedemptionCodeResponses, RedeemUserRedemptionCodeErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/wallet/redemptions',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAccountRefundReconciliations = <ThrowOnError extends boolean = true>(options?: Options<ListAccountRefundReconciliationsData, ThrowOnError>): RequestResult<ListAccountRefundReconciliationsResponses, ListAccountRefundReconciliationsErrors, ThrowOnError> => (options?.client ?? client).get<ListAccountRefundReconciliationsResponses, ListAccountRefundReconciliationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/refund-reconciliations',
    ...options
});
export const listOrganizationRefundReconciliations = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationRefundReconciliationsData, ThrowOnError>): RequestResult<ListOrganizationRefundReconciliationsResponses, ListOrganizationRefundReconciliationsErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationRefundReconciliationsResponses, ListOrganizationRefundReconciliationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/refund-reconciliations',
    ...options
});
export const listAdminRefundReconciliations = <ThrowOnError extends boolean = true>(options?: Options<ListAdminRefundReconciliationsData, ThrowOnError>): RequestResult<ListAdminRefundReconciliationsResponses, ListAdminRefundReconciliationsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminRefundReconciliationsResponses, ListAdminRefundReconciliationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/refund-reconciliations',
    ...options
});
export const listAdminRefunds = <ThrowOnError extends boolean = true>(options?: Options<ListAdminRefundsData, ThrowOnError>): RequestResult<ListAdminRefundsResponses, ListAdminRefundsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminRefundsResponses, ListAdminRefundsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/refunds',
    ...options
});
export const approveAdminRefund = <ThrowOnError extends boolean = true>(options: Options<ApproveAdminRefundData, ThrowOnError>): RequestResult<ApproveAdminRefundResponses, ApproveAdminRefundErrors, ThrowOnError> => (options.client ?? client).post<ApproveAdminRefundResponses, ApproveAdminRefundErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/refunds/{request_id}/approve',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const rejectAdminRefund = <ThrowOnError extends boolean = true>(options: Options<RejectAdminRefundData, ThrowOnError>): RequestResult<RejectAdminRefundResponses, RejectAdminRefundErrors, ThrowOnError> => (options.client ?? client).post<RejectAdminRefundResponses, RejectAdminRefundErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/refunds/{request_id}/reject',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const submitAdminRefund = <ThrowOnError extends boolean = true>(options: Options<SubmitAdminRefundData, ThrowOnError>): RequestResult<SubmitAdminRefundResponses, SubmitAdminRefundErrors, ThrowOnError> => (options.client ?? client).post<SubmitAdminRefundResponses, SubmitAdminRefundErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/refunds/{request_id}/submit',
    ...options
});
export const manualCompleteAdminRefund = <ThrowOnError extends boolean = true>(options: Options<ManualCompleteAdminRefundData, ThrowOnError>): RequestResult<ManualCompleteAdminRefundResponses, ManualCompleteAdminRefundErrors, ThrowOnError> => (options.client ?? client).post<ManualCompleteAdminRefundResponses, ManualCompleteAdminRefundErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/refunds/{request_id}/manual-complete',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminSubscriptionPlans = <ThrowOnError extends boolean = true>(options?: Options<ListAdminSubscriptionPlansData, ThrowOnError>): RequestResult<ListAdminSubscriptionPlansResponses, ListAdminSubscriptionPlansErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminSubscriptionPlansResponses, ListAdminSubscriptionPlansErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/subscription-plans',
    ...options
});
export const createAdminSubscriptionPlan = <ThrowOnError extends boolean = true>(options: Options<CreateAdminSubscriptionPlanData, ThrowOnError>): RequestResult<CreateAdminSubscriptionPlanResponses, CreateAdminSubscriptionPlanErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminSubscriptionPlanResponses, CreateAdminSubscriptionPlanErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/subscription-plans',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const disableAdminSubscriptionPlan = <ThrowOnError extends boolean = true>(options: Options<DisableAdminSubscriptionPlanData, ThrowOnError>): RequestResult<DisableAdminSubscriptionPlanResponses, DisableAdminSubscriptionPlanErrors, ThrowOnError> => (options.client ?? client).post<DisableAdminSubscriptionPlanResponses, DisableAdminSubscriptionPlanErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/subscription-plans/{plan_id}/disable',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminUserSubscriptions = <ThrowOnError extends boolean = true>(options: Options<ListAdminUserSubscriptionsData, ThrowOnError>): RequestResult<ListAdminUserSubscriptionsResponses, ListAdminUserSubscriptionsErrors, ThrowOnError> => (options.client ?? client).get<ListAdminUserSubscriptionsResponses, ListAdminUserSubscriptionsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/users/{user_id}/subscriptions',
    ...options
});
export const bindAdminUserSubscription = <ThrowOnError extends boolean = true>(options: Options<BindAdminUserSubscriptionData, ThrowOnError>): RequestResult<BindAdminUserSubscriptionResponses, BindAdminUserSubscriptionErrors, ThrowOnError> => (options.client ?? client).post<BindAdminUserSubscriptionResponses, BindAdminUserSubscriptionErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/users/{user_id}/subscriptions',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const transitionAdminUserSubscriptionLifecycle = <ThrowOnError extends boolean = true>(options: Options<TransitionAdminUserSubscriptionLifecycleData, ThrowOnError>): RequestResult<TransitionAdminUserSubscriptionLifecycleResponses, TransitionAdminUserSubscriptionLifecycleErrors, ThrowOnError> => (options.client ?? client).post<TransitionAdminUserSubscriptionLifecycleResponses, TransitionAdminUserSubscriptionLifecycleErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/users/{user_id}/subscriptions/{subscription_id}/lifecycle',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listCurrentUserSubscriptions = <ThrowOnError extends boolean = true>(options?: Options<ListCurrentUserSubscriptionsData, ThrowOnError>): RequestResult<ListCurrentUserSubscriptionsResponses, ListCurrentUserSubscriptionsErrors, ThrowOnError> => (options?.client ?? client).get<ListCurrentUserSubscriptionsResponses, ListCurrentUserSubscriptionsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/subscriptions',
    ...options
});
export const listCurrentSubscriptionCatalog = <ThrowOnError extends boolean = true>(options?: Options<ListCurrentSubscriptionCatalogData, ThrowOnError>): RequestResult<ListCurrentSubscriptionCatalogResponses, ListCurrentSubscriptionCatalogErrors, ThrowOnError> => (options?.client ?? client).get<ListCurrentSubscriptionCatalogResponses, ListCurrentSubscriptionCatalogErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/subscription-catalog',
    ...options
});
export const createCurrentSubscriptionOrder = <ThrowOnError extends boolean = true>(options: Options<CreateCurrentSubscriptionOrderData, ThrowOnError>): RequestResult<CreateCurrentSubscriptionOrderResponses, CreateCurrentSubscriptionOrderErrors, ThrowOnError> => (options.client ?? client).post<CreateCurrentSubscriptionOrderResponses, CreateCurrentSubscriptionOrderErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/subscription-orders',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getCurrentSubscriptionOrder = <ThrowOnError extends boolean = true>(options: Options<GetCurrentSubscriptionOrderData, ThrowOnError>): RequestResult<GetCurrentSubscriptionOrderResponses, GetCurrentSubscriptionOrderErrors, ThrowOnError> => (options.client ?? client).get<GetCurrentSubscriptionOrderResponses, GetCurrentSubscriptionOrderErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/subscription-orders/{order_id}',
    ...options
});
export const submitCurrentSubscriptionOrderPayment = <ThrowOnError extends boolean = true>(options: Options<SubmitCurrentSubscriptionOrderPaymentData, ThrowOnError>): RequestResult<SubmitCurrentSubscriptionOrderPaymentResponses, SubmitCurrentSubscriptionOrderPaymentErrors, ThrowOnError> => (options.client ?? client).post<SubmitCurrentSubscriptionOrderPaymentResponses, SubmitCurrentSubscriptionOrderPaymentErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/subscription-orders/{order_id}/payment',
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
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/profile',
    ...options
});
export const updateUserProfile = <ThrowOnError extends boolean = true>(options: Options<UpdateUserProfileData, ThrowOnError>): RequestResult<UpdateUserProfileResponses, UpdateUserProfileErrors, ThrowOnError> => (options.client ?? client).put<UpdateUserProfileResponses, UpdateUserProfileErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/profile',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const sendUserEmailBindingVerification = <ThrowOnError extends boolean = true>(options: Options<SendUserEmailBindingVerificationData, ThrowOnError>): RequestResult<SendUserEmailBindingVerificationResponses, SendUserEmailBindingVerificationErrors, ThrowOnError> => (options.client ?? client).post<SendUserEmailBindingVerificationResponses, SendUserEmailBindingVerificationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/profile/email-verification',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const confirmUserEmailBinding = <ThrowOnError extends boolean = true>(options: Options<ConfirmUserEmailBindingData, ThrowOnError>): RequestResult<ConfirmUserEmailBindingResponses, ConfirmUserEmailBindingErrors, ThrowOnError> => (options.client ?? client).put<ConfirmUserEmailBindingResponses, ConfirmUserEmailBindingErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/profile/email',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const changeUserPassword = <ThrowOnError extends boolean = true>(options: Options<ChangeUserPasswordData, ThrowOnError>): RequestResult<ChangeUserPasswordResponses, ChangeUserPasswordErrors, ThrowOnError> => (options.client ?? client).put<ChangeUserPasswordResponses, ChangeUserPasswordErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/password',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const disableUserTwoFactor = <ThrowOnError extends boolean = true>(options: Options<DisableUserTwoFactorData, ThrowOnError>): RequestResult<DisableUserTwoFactorResponses, DisableUserTwoFactorErrors, ThrowOnError> => (options.client ?? client).delete<DisableUserTwoFactorResponses, DisableUserTwoFactorErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/two-factor',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getUserTwoFactor = <ThrowOnError extends boolean = true>(options?: Options<GetUserTwoFactorData, ThrowOnError>): RequestResult<GetUserTwoFactorResponses, GetUserTwoFactorErrors, ThrowOnError> => (options?.client ?? client).get<GetUserTwoFactorResponses, GetUserTwoFactorErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/two-factor',
    ...options
});
export const enableUserTwoFactor = <ThrowOnError extends boolean = true>(options: Options<EnableUserTwoFactorData, ThrowOnError>): RequestResult<EnableUserTwoFactorResponses, EnableUserTwoFactorErrors, ThrowOnError> => (options.client ?? client).post<EnableUserTwoFactorResponses, EnableUserTwoFactorErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/two-factor',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listUserNotifications = <ThrowOnError extends boolean = true>(options?: Options<ListUserNotificationsData, ThrowOnError>): RequestResult<ListUserNotificationsResponses, ListUserNotificationsErrors, ThrowOnError> => (options?.client ?? client).get<ListUserNotificationsResponses, ListUserNotificationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/notifications',
    ...options
});
export const updateUserNotificationPreferences = <ThrowOnError extends boolean = true>(options: Options<UpdateUserNotificationPreferencesData, ThrowOnError>): RequestResult<UpdateUserNotificationPreferencesResponses, UpdateUserNotificationPreferencesErrors, ThrowOnError> => (options.client ?? client).put<UpdateUserNotificationPreferencesResponses, UpdateUserNotificationPreferencesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/notifications',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listUserPasskeys = <ThrowOnError extends boolean = true>(options?: Options<ListUserPasskeysData, ThrowOnError>): RequestResult<ListUserPasskeysResponses, ListUserPasskeysErrors, ThrowOnError> => (options?.client ?? client).get<ListUserPasskeysResponses, ListUserPasskeysErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/passkeys',
    ...options
});
export const startUserPasskeyRegistration = <ThrowOnError extends boolean = true>(options?: Options<StartUserPasskeyRegistrationData, ThrowOnError>): RequestResult<StartUserPasskeyRegistrationResponses, StartUserPasskeyRegistrationErrors, ThrowOnError> => (options?.client ?? client).post<StartUserPasskeyRegistrationResponses, StartUserPasskeyRegistrationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/passkeys/registration/options',
    ...options
});
export const finishUserPasskeyRegistration = <ThrowOnError extends boolean = true>(options: Options<FinishUserPasskeyRegistrationData, ThrowOnError>): RequestResult<FinishUserPasskeyRegistrationResponses, FinishUserPasskeyRegistrationErrors, ThrowOnError> => (options.client ?? client).post<FinishUserPasskeyRegistrationResponses, FinishUserPasskeyRegistrationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/passkeys/registration/verify',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const revokeUserPasskey = <ThrowOnError extends boolean = true>(options: Options<RevokeUserPasskeyData, ThrowOnError>): RequestResult<RevokeUserPasskeyResponses, RevokeUserPasskeyErrors, ThrowOnError> => (options.client ?? client).delete<RevokeUserPasskeyResponses, RevokeUserPasskeyErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/passkeys/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const renameUserPasskey = <ThrowOnError extends boolean = true>(options: Options<RenameUserPasskeyData, ThrowOnError>): RequestResult<RenameUserPasskeyResponses, RenameUserPasskeyErrors, ThrowOnError> => (options.client ?? client).patch<RenameUserPasskeyResponses, RenameUserPasskeyErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/passkeys/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getUserWallet = <ThrowOnError extends boolean = true>(options?: Options<GetUserWalletData, ThrowOnError>): RequestResult<GetUserWalletResponses, GetUserWalletErrors, ThrowOnError> => (options?.client ?? client).get<GetUserWalletResponses, GetUserWalletErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/wallet',
    ...options
});
export const listUserWalletEntries = <ThrowOnError extends boolean = true>(options?: Options<ListUserWalletEntriesData, ThrowOnError>): RequestResult<ListUserWalletEntriesResponses, ListUserWalletEntriesErrors, ThrowOnError> => (options?.client ?? client).get<ListUserWalletEntriesResponses, ListUserWalletEntriesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/wallet/entries',
    ...options
});
export const markUserNotificationsRead = <ThrowOnError extends boolean = true>(options: Options<MarkUserNotificationsReadData, ThrowOnError>): RequestResult<MarkUserNotificationsReadResponses, MarkUserNotificationsReadErrors, ThrowOnError> => (options.client ?? client).post<MarkUserNotificationsReadResponses, MarkUserNotificationsReadErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/notifications/read',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getUserTopupConfiguration = <ThrowOnError extends boolean = true>(options?: Options<GetUserTopupConfigurationData, ThrowOnError>): RequestResult<GetUserTopupConfigurationResponses, GetUserTopupConfigurationErrors, ThrowOnError> => (options?.client ?? client).get<GetUserTopupConfigurationResponses, GetUserTopupConfigurationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/wallet/topups/config',
    ...options
});
export const createUserTopupOrder = <ThrowOnError extends boolean = true>(options: Options<CreateUserTopupOrderData, ThrowOnError>): RequestResult<CreateUserTopupOrderResponses, CreateUserTopupOrderErrors, ThrowOnError> => (options.client ?? client).post<CreateUserTopupOrderResponses, CreateUserTopupOrderErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/wallet/topups',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getUserInvitations = <ThrowOnError extends boolean = true>(options?: Options<GetUserInvitationsData, ThrowOnError>): RequestResult<GetUserInvitationsResponses, GetUserInvitationsErrors, ThrowOnError> => (options?.client ?? client).get<GetUserInvitationsResponses, GetUserInvitationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/invitations',
    ...options
});
export const getAdminEmailSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminEmailSettingsData, ThrowOnError>): RequestResult<GetAdminEmailSettingsResponses, GetAdminEmailSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminEmailSettingsResponses, GetAdminEmailSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/email-settings',
    ...options
});
export const updateAdminEmailSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminEmailSettingsData, ThrowOnError>): RequestResult<UpdateAdminEmailSettingsResponses, UpdateAdminEmailSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminEmailSettingsResponses, UpdateAdminEmailSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/email-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const sendAdminEmailTest = <ThrowOnError extends boolean = true>(options: Options<SendAdminEmailTestData, ThrowOnError>): RequestResult<SendAdminEmailTestResponses, SendAdminEmailTestErrors, ThrowOnError> => (options.client ?? client).post<SendAdminEmailTestResponses, SendAdminEmailTestErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/email-settings/test',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listExtensionCatalog = <ThrowOnError extends boolean = true>(options?: Options<ListExtensionCatalogData, ThrowOnError>): RequestResult<ListExtensionCatalogResponses, unknown, ThrowOnError> => (options?.client ?? client).get<ListExtensionCatalogResponses, unknown, ThrowOnError>({ url: '/api/extensions', ...options });
export const getAdminNetworkSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminNetworkSettingsData, ThrowOnError>): RequestResult<GetAdminNetworkSettingsResponses, GetAdminNetworkSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminNetworkSettingsResponses, GetAdminNetworkSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/network-settings',
    ...options
});
export const updateAdminNetworkSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminNetworkSettingsData, ThrowOnError>): RequestResult<UpdateAdminNetworkSettingsResponses, UpdateAdminNetworkSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminNetworkSettingsResponses, UpdateAdminNetworkSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/network-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminPaymentSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminPaymentSettingsData, ThrowOnError>): RequestResult<GetAdminPaymentSettingsResponses, GetAdminPaymentSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminPaymentSettingsResponses, GetAdminPaymentSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/payment-settings',
    ...options
});
export const updateAdminPaymentSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminPaymentSettingsData, ThrowOnError>): RequestResult<UpdateAdminPaymentSettingsResponses, UpdateAdminPaymentSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminPaymentSettingsResponses, UpdateAdminPaymentSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/payment-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminOAuthProviders = <ThrowOnError extends boolean = true>(options?: Options<ListAdminOAuthProvidersData, ThrowOnError>): RequestResult<ListAdminOAuthProvidersResponses, ListAdminOAuthProvidersErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminOAuthProvidersResponses, ListAdminOAuthProvidersErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/oauth/providers',
    ...options
});
export const beginAdminOAuthAuthorization = <ThrowOnError extends boolean = true>(options: Options<BeginAdminOAuthAuthorizationData, ThrowOnError>): RequestResult<BeginAdminOAuthAuthorizationResponses, BeginAdminOAuthAuthorizationErrors, ThrowOnError> => (options.client ?? client).post<BeginAdminOAuthAuthorizationResponses, BeginAdminOAuthAuthorizationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}/oauth-authorizations',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const completeAdminOAuthManualCallback = <ThrowOnError extends boolean = true>(options: Options<CompleteAdminOAuthManualCallbackData, ThrowOnError>): RequestResult<CompleteAdminOAuthManualCallbackResponses, CompleteAdminOAuthManualCallbackErrors, ThrowOnError> => (options.client ?? client).post<CompleteAdminOAuthManualCallbackResponses, CompleteAdminOAuthManualCallbackErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/oauth/authorizations/manual-callback',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const startGitHubOAuthLogin = <ThrowOnError extends boolean = true>(options?: Options<StartGitHubOAuthLoginData, ThrowOnError>): RequestResult<StartGitHubOAuthLoginResponses, StartGitHubOAuthLoginErrors, ThrowOnError> => (options?.client ?? client).post<StartGitHubOAuthLoginResponses, StartGitHubOAuthLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/github/start', ...options });
export const startDiscordOAuthLogin = <ThrowOnError extends boolean = true>(options?: Options<StartDiscordOAuthLoginData, ThrowOnError>): RequestResult<StartDiscordOAuthLoginResponses, StartDiscordOAuthLoginErrors, ThrowOnError> => (options?.client ?? client).post<StartDiscordOAuthLoginResponses, StartDiscordOAuthLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/discord/start', ...options });
export const completeGitHubOAuthLogin = <ThrowOnError extends boolean = true>(options?: Options<CompleteGitHubOAuthLoginData, ThrowOnError>): RequestResult<unknown, CompleteGitHubOAuthLoginErrors, ThrowOnError> => (options?.client ?? client).get<unknown, CompleteGitHubOAuthLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/github/callback', ...options });
export const completeDiscordOAuthLogin = <ThrowOnError extends boolean = true>(options?: Options<CompleteDiscordOAuthLoginData, ThrowOnError>): RequestResult<unknown, CompleteDiscordOAuthLoginErrors, ThrowOnError> => (options?.client ?? client).get<unknown, CompleteDiscordOAuthLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/discord/callback', ...options });
export const exchangeOAuthLoginTicket = <ThrowOnError extends boolean = true>(options: Options<ExchangeOAuthLoginTicketData, ThrowOnError>): RequestResult<ExchangeOAuthLoginTicketResponses, ExchangeOAuthLoginTicketErrors, ThrowOnError> => (options.client ?? client).post<ExchangeOAuthLoginTicketResponses, ExchangeOAuthLoginTicketErrors, ThrowOnError>({
    url: '/api/auth/oauth/exchange',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminGitHubOAuthLoginSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminGitHubOAuthLoginSettingsData, ThrowOnError>): RequestResult<GetAdminGitHubOAuthLoginSettingsResponses, GetAdminGitHubOAuthLoginSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminGitHubOAuthLoginSettingsResponses, GetAdminGitHubOAuthLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/github',
    ...options
});
export const updateAdminGitHubOAuthLoginSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminGitHubOAuthLoginSettingsData, ThrowOnError>): RequestResult<UpdateAdminGitHubOAuthLoginSettingsResponses, UpdateAdminGitHubOAuthLoginSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminGitHubOAuthLoginSettingsResponses, UpdateAdminGitHubOAuthLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/github',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminDiscordOAuthLoginSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminDiscordOAuthLoginSettingsData, ThrowOnError>): RequestResult<GetAdminDiscordOAuthLoginSettingsResponses, GetAdminDiscordOAuthLoginSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminDiscordOAuthLoginSettingsResponses, GetAdminDiscordOAuthLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/discord',
    ...options
});
export const updateAdminDiscordOAuthLoginSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminDiscordOAuthLoginSettingsData, ThrowOnError>): RequestResult<UpdateAdminDiscordOAuthLoginSettingsResponses, UpdateAdminDiscordOAuthLoginSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminDiscordOAuthLoginSettingsResponses, UpdateAdminDiscordOAuthLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/discord',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminBalanceAlertSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminBalanceAlertSettingsData, ThrowOnError>): RequestResult<GetAdminBalanceAlertSettingsResponses, GetAdminBalanceAlertSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminBalanceAlertSettingsResponses, GetAdminBalanceAlertSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/balance-alert-settings',
    ...options
});
export const updateAdminBalanceAlertSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminBalanceAlertSettingsData, ThrowOnError>): RequestResult<UpdateAdminBalanceAlertSettingsResponses, UpdateAdminBalanceAlertSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminBalanceAlertSettingsResponses, UpdateAdminBalanceAlertSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/balance-alert-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getPublicSiteSettings = <ThrowOnError extends boolean = true>(options?: Options<GetPublicSiteSettingsData, ThrowOnError>): RequestResult<GetPublicSiteSettingsResponses, GetPublicSiteSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetPublicSiteSettingsResponses, GetPublicSiteSettingsErrors, ThrowOnError>({ url: '/api/site', ...options });
export const getAdminSiteSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminSiteSettingsData, ThrowOnError>): RequestResult<GetAdminSiteSettingsResponses, GetAdminSiteSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminSiteSettingsResponses, GetAdminSiteSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/site-settings',
    ...options
});
export const updateAdminSiteSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminSiteSettingsData, ThrowOnError>): RequestResult<UpdateAdminSiteSettingsResponses, UpdateAdminSiteSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminSiteSettingsResponses, UpdateAdminSiteSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/site-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const updateAdminSiteNavigation = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminSiteNavigationData, ThrowOnError>): RequestResult<UpdateAdminSiteNavigationResponses, UpdateAdminSiteNavigationErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminSiteNavigationResponses, UpdateAdminSiteNavigationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/site-settings/navigation',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminFrontendTemplates = <ThrowOnError extends boolean = true>(options?: Options<ListAdminFrontendTemplatesData, ThrowOnError>): RequestResult<ListAdminFrontendTemplatesResponses, ListAdminFrontendTemplatesErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminFrontendTemplatesResponses, ListAdminFrontendTemplatesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/frontend-templates',
    ...options
});
export const scanAdminFrontendTemplates = <ThrowOnError extends boolean = true>(options?: Options<ScanAdminFrontendTemplatesData, ThrowOnError>): RequestResult<ScanAdminFrontendTemplatesResponses, ScanAdminFrontendTemplatesErrors, ThrowOnError> => (options?.client ?? client).post<ScanAdminFrontendTemplatesResponses, ScanAdminFrontendTemplatesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/frontend-templates/scan',
    ...options
});
export const activateAdminFrontendTemplate = <ThrowOnError extends boolean = true>(options: Options<ActivateAdminFrontendTemplateData, ThrowOnError>): RequestResult<ActivateAdminFrontendTemplateResponses, ActivateAdminFrontendTemplateErrors, ThrowOnError> => (options.client ?? client).put<ActivateAdminFrontendTemplateResponses, ActivateAdminFrontendTemplateErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/frontend-templates/active',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminFrontendTemplatePreview = <ThrowOnError extends boolean = true>(options: Options<GetAdminFrontendTemplatePreviewData, ThrowOnError>): RequestResult<GetAdminFrontendTemplatePreviewResponses, GetAdminFrontendTemplatePreviewErrors, ThrowOnError> => (options.client ?? client).get<GetAdminFrontendTemplatePreviewResponses, GetAdminFrontendTemplatePreviewErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/frontend-templates/{template_id}/preview',
    ...options
});
export const getAdminDashboard = <ThrowOnError extends boolean = true>(options?: Options<GetAdminDashboardData, ThrowOnError>): RequestResult<GetAdminDashboardResponses, GetAdminDashboardErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminDashboardResponses, GetAdminDashboardErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/dashboard',
    ...options
});
export const getAdminServiceLevels = <ThrowOnError extends boolean = true>(options?: Options<GetAdminServiceLevelsData, ThrowOnError>): RequestResult<GetAdminServiceLevelsResponses, GetAdminServiceLevelsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminServiceLevelsResponses, GetAdminServiceLevelsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/dashboard/service-levels',
    ...options
});
export const getAdminAnalyticsExportStatus = <ThrowOnError extends boolean = true>(options?: Options<GetAdminAnalyticsExportStatusData, ThrowOnError>): RequestResult<GetAdminAnalyticsExportStatusResponses, GetAdminAnalyticsExportStatusErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminAnalyticsExportStatusResponses, GetAdminAnalyticsExportStatusErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/analytics/export-status',
    ...options
});
export const replayAdminAnalyticsExport = <ThrowOnError extends boolean = true>(options: Options<ReplayAdminAnalyticsExportData, ThrowOnError>): RequestResult<ReplayAdminAnalyticsExportResponses, ReplayAdminAnalyticsExportErrors, ThrowOnError> => (options.client ?? client).post<ReplayAdminAnalyticsExportResponses, ReplayAdminAnalyticsExportErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/analytics/export-replay',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminUsers = <ThrowOnError extends boolean = true>(options?: Options<ListAdminUsersData, ThrowOnError>): RequestResult<ListAdminUsersResponses, ListAdminUsersErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminUsersResponses, ListAdminUsersErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/users',
    ...options
});
export const createAdminUser = <ThrowOnError extends boolean = true>(options: Options<CreateAdminUserData, ThrowOnError>): RequestResult<CreateAdminUserResponses, CreateAdminUserErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminUserResponses, CreateAdminUserErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/users',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminUser = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminUserData, ThrowOnError>): RequestResult<DeleteAdminUserResponses, DeleteAdminUserErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminUserResponses, DeleteAdminUserErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/users/{id}',
    ...options
});
export const getAdminUser = <ThrowOnError extends boolean = true>(options: Options<GetAdminUserData, ThrowOnError>): RequestResult<GetAdminUserResponses, GetAdminUserErrors, ThrowOnError> => (options.client ?? client).get<GetAdminUserResponses, GetAdminUserErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/users/{id}',
    ...options
});
export const updateAdminUser = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminUserData, ThrowOnError>): RequestResult<UpdateAdminUserResponses, UpdateAdminUserErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminUserResponses, UpdateAdminUserErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/users/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminWalletEntries = <ThrowOnError extends boolean = true>(options: Options<ListAdminWalletEntriesData, ThrowOnError>): RequestResult<ListAdminWalletEntriesResponses, ListAdminWalletEntriesErrors, ThrowOnError> => (options.client ?? client).get<ListAdminWalletEntriesResponses, ListAdminWalletEntriesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/users/{id}/wallet/entries',
    ...options
});
export const adjustAdminWallet = <ThrowOnError extends boolean = true>(options: Options<AdjustAdminWalletData, ThrowOnError>): RequestResult<AdjustAdminWalletResponses, AdjustAdminWalletErrors, ThrowOnError> => (options.client ?? client).post<AdjustAdminWalletResponses, AdjustAdminWalletErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/users/{id}/wallet/adjustments',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminGroups = <ThrowOnError extends boolean = true>(options?: Options<ListAdminGroupsData, ThrowOnError>): RequestResult<ListAdminGroupsResponses, ListAdminGroupsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminGroupsResponses, ListAdminGroupsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/groups',
    ...options
});
export const createAdminGroup = <ThrowOnError extends boolean = true>(options: Options<CreateAdminGroupData, ThrowOnError>): RequestResult<CreateAdminGroupResponses, CreateAdminGroupErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminGroupResponses, CreateAdminGroupErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/groups',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminGroup = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminGroupData, ThrowOnError>): RequestResult<DeleteAdminGroupResponses, DeleteAdminGroupErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminGroupResponses, DeleteAdminGroupErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/groups/{id}',
    ...options
});
export const getAdminGroup = <ThrowOnError extends boolean = true>(options: Options<GetAdminGroupData, ThrowOnError>): RequestResult<GetAdminGroupResponses, GetAdminGroupErrors, ThrowOnError> => (options.client ?? client).get<GetAdminGroupResponses, GetAdminGroupErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/groups/{id}',
    ...options
});
export const updateAdminGroup = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminGroupData, ThrowOnError>): RequestResult<UpdateAdminGroupResponses, UpdateAdminGroupErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminGroupResponses, UpdateAdminGroupErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/groups/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminRoutes = <ThrowOnError extends boolean = true>(options?: Options<ListAdminRoutesData, ThrowOnError>): RequestResult<ListAdminRoutesResponses, ListAdminRoutesErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminRoutesResponses, ListAdminRoutesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/routes',
    ...options
});
export const createAdminRoute = <ThrowOnError extends boolean = true>(options: Options<CreateAdminRouteData, ThrowOnError>): RequestResult<CreateAdminRouteResponses, CreateAdminRouteErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminRouteResponses, CreateAdminRouteErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/routes',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminRoute = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminRouteData, ThrowOnError>): RequestResult<DeleteAdminRouteResponses, DeleteAdminRouteErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminRouteResponses, DeleteAdminRouteErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/routes/{id}',
    ...options
});
export const getAdminRoute = <ThrowOnError extends boolean = true>(options: Options<GetAdminRouteData, ThrowOnError>): RequestResult<GetAdminRouteResponses, GetAdminRouteErrors, ThrowOnError> => (options.client ?? client).get<GetAdminRouteResponses, GetAdminRouteErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/routes/{id}',
    ...options
});
export const updateAdminRoute = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminRouteData, ThrowOnError>): RequestResult<UpdateAdminRouteResponses, UpdateAdminRouteErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminRouteResponses, UpdateAdminRouteErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/routes/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listModels = <ThrowOnError extends boolean = true>(options?: Options<ListModelsData, ThrowOnError>): RequestResult<ListModelsResponses, ListModelsErrors, ThrowOnError> => (options?.client ?? client).get<ListModelsResponses, ListModelsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/models',
    ...options
});
export const listModelProviders = <ThrowOnError extends boolean = true>(options?: Options<ListModelProvidersData, ThrowOnError>): RequestResult<ListModelProvidersResponses, ListModelProvidersErrors, ThrowOnError> => (options?.client ?? client).get<ListModelProvidersResponses, ListModelProvidersErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/model-providers',
    ...options
});
export const listGatewayModels = <ThrowOnError extends boolean = true>(options?: Options<ListGatewayModelsData, ThrowOnError>): RequestResult<ListGatewayModelsResponses, ListGatewayModelsErrors, ThrowOnError> => (options?.client ?? client).get<ListGatewayModelsResponses, ListGatewayModelsErrors, ThrowOnError>({
    security: [{
            key: 'apiKeyAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/v1/models',
    ...options
});
export const listAdminModels = <ThrowOnError extends boolean = true>(options?: Options<ListAdminModelsData, ThrowOnError>): RequestResult<ListAdminModelsResponses, ListAdminModelsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminModelsResponses, ListAdminModelsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/models',
    ...options
});
export const createAdminModel = <ThrowOnError extends boolean = true>(options: Options<CreateAdminModelData, ThrowOnError>): RequestResult<CreateAdminModelResponses, CreateAdminModelErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminModelResponses, CreateAdminModelErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/models',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminModel = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminModelData, ThrowOnError>): RequestResult<DeleteAdminModelResponses, DeleteAdminModelErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminModelResponses, DeleteAdminModelErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/models/{id}',
    ...options
});
export const getAdminModel = <ThrowOnError extends boolean = true>(options: Options<GetAdminModelData, ThrowOnError>): RequestResult<GetAdminModelResponses, GetAdminModelErrors, ThrowOnError> => (options.client ?? client).get<GetAdminModelResponses, GetAdminModelErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/models/{id}',
    ...options
});
export const updateAdminModel = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminModelData, ThrowOnError>): RequestResult<UpdateAdminModelResponses, UpdateAdminModelErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminModelResponses, UpdateAdminModelErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/models/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listMissingAdminModels = <ThrowOnError extends boolean = true>(options?: Options<ListMissingAdminModelsData, ThrowOnError>): RequestResult<ListMissingAdminModelsResponses, ListMissingAdminModelsErrors, ThrowOnError> => (options?.client ?? client).get<ListMissingAdminModelsResponses, ListMissingAdminModelsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/models/missing',
    ...options
});
export const importMissingAdminModels = <ThrowOnError extends boolean = true>(options: Options<ImportMissingAdminModelsData, ThrowOnError>): RequestResult<ImportMissingAdminModelsResponses, ImportMissingAdminModelsErrors, ThrowOnError> => (options.client ?? client).post<ImportMissingAdminModelsResponses, ImportMissingAdminModelsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/models/missing',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const createAdminModelSyncPreview = <ThrowOnError extends boolean = true>(options: Options<CreateAdminModelSyncPreviewData, ThrowOnError>): RequestResult<CreateAdminModelSyncPreviewResponses, CreateAdminModelSyncPreviewErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminModelSyncPreviewResponses, CreateAdminModelSyncPreviewErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/models/sync-previews',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const applyAdminModelSyncPreview = <ThrowOnError extends boolean = true>(options: Options<ApplyAdminModelSyncPreviewData, ThrowOnError>): RequestResult<ApplyAdminModelSyncPreviewResponses, ApplyAdminModelSyncPreviewErrors, ThrowOnError> => (options.client ?? client).post<ApplyAdminModelSyncPreviewResponses, ApplyAdminModelSyncPreviewErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/models/sync-previews/{preview_id}/apply',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminModelPrices = <ThrowOnError extends boolean = true>(options?: Options<ListAdminModelPricesData, ThrowOnError>): RequestResult<ListAdminModelPricesResponses, ListAdminModelPricesErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminModelPricesResponses, ListAdminModelPricesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/model-prices',
    ...options
});
export const previewAdminModelPrices = <ThrowOnError extends boolean = true>(options?: Options<PreviewAdminModelPricesData, ThrowOnError>): RequestResult<PreviewAdminModelPricesResponses, PreviewAdminModelPricesErrors, ThrowOnError> => (options?.client ?? client).post<PreviewAdminModelPricesResponses, PreviewAdminModelPricesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/model-prices/models-dev-preview',
    ...options
});
export const previewAdminLiteLlmModelPrices = <ThrowOnError extends boolean = true>(options?: Options<PreviewAdminLiteLlmModelPricesData, ThrowOnError>): RequestResult<PreviewAdminLiteLlmModelPricesResponses, PreviewAdminLiteLlmModelPricesErrors, ThrowOnError> => (options?.client ?? client).post<PreviewAdminLiteLlmModelPricesResponses, PreviewAdminLiteLlmModelPricesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/model-prices/litellm-preview',
    ...options
});
export const previewAdminModelPriceExpression = <ThrowOnError extends boolean = true>(options: Options<PreviewAdminModelPriceExpressionData, ThrowOnError>): RequestResult<PreviewAdminModelPriceExpressionResponses, PreviewAdminModelPriceExpressionErrors, ThrowOnError> => (options.client ?? client).post<PreviewAdminModelPriceExpressionResponses, PreviewAdminModelPriceExpressionErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/model-prices/expression-preview',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const applyAdminModelPrices = <ThrowOnError extends boolean = true>(options: Options<ApplyAdminModelPricesData, ThrowOnError>): RequestResult<ApplyAdminModelPricesResponses, ApplyAdminModelPricesErrors, ThrowOnError> => (options.client ?? client).post<ApplyAdminModelPricesResponses, ApplyAdminModelPricesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/model-prices/batch',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const createPlaygroundShare = <ThrowOnError extends boolean = true>(options: Options<CreatePlaygroundShareData, ThrowOnError>): RequestResult<CreatePlaygroundShareResponses, CreatePlaygroundShareErrors, ThrowOnError> => (options.client ?? client).post<CreatePlaygroundShareResponses, CreatePlaygroundShareErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/playground/shares',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const revokePlaygroundShare = <ThrowOnError extends boolean = true>(options: Options<RevokePlaygroundShareData, ThrowOnError>): RequestResult<RevokePlaygroundShareResponses, RevokePlaygroundShareErrors, ThrowOnError> => (options.client ?? client).delete<RevokePlaygroundShareResponses, RevokePlaygroundShareErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/playground/shares/{token}',
    ...options
});
export const getPlaygroundShare = <ThrowOnError extends boolean = true>(options: Options<GetPlaygroundShareData, ThrowOnError>): RequestResult<GetPlaygroundShareResponses, GetPlaygroundShareErrors, ThrowOnError> => (options.client ?? client).get<GetPlaygroundShareResponses, GetPlaygroundShareErrors, ThrowOnError>({ url: '/api/playground/shares/{token}', ...options });
export const listPlaygroundConversations = <ThrowOnError extends boolean = true>(options?: Options<ListPlaygroundConversationsData, ThrowOnError>): RequestResult<ListPlaygroundConversationsResponses, ListPlaygroundConversationsErrors, ThrowOnError> => (options?.client ?? client).get<ListPlaygroundConversationsResponses, ListPlaygroundConversationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/playground/conversations',
    ...options
});
export const deletePlaygroundConversation = <ThrowOnError extends boolean = true>(options: Options<DeletePlaygroundConversationData, ThrowOnError>): RequestResult<DeletePlaygroundConversationResponses, DeletePlaygroundConversationErrors, ThrowOnError> => (options.client ?? client).delete<DeletePlaygroundConversationResponses, DeletePlaygroundConversationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/playground/conversations/{conversation_id}',
    ...options
});
export const getPlaygroundConversation = <ThrowOnError extends boolean = true>(options: Options<GetPlaygroundConversationData, ThrowOnError>): RequestResult<GetPlaygroundConversationResponses, GetPlaygroundConversationErrors, ThrowOnError> => (options.client ?? client).get<GetPlaygroundConversationResponses, GetPlaygroundConversationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/playground/conversations/{conversation_id}',
    ...options
});
export const savePlaygroundConversation = <ThrowOnError extends boolean = true>(options: Options<SavePlaygroundConversationData, ThrowOnError>): RequestResult<SavePlaygroundConversationResponses, SavePlaygroundConversationErrors, ThrowOnError> => (options.client ?? client).put<SavePlaygroundConversationResponses, SavePlaygroundConversationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/playground/conversations/{conversation_id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listUserTokens = <ThrowOnError extends boolean = true>(options?: Options<ListUserTokensData, ThrowOnError>): RequestResult<ListUserTokensResponses, ListUserTokensErrors, ThrowOnError> => (options?.client ?? client).get<ListUserTokensResponses, ListUserTokensErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/tokens',
    ...options
});
export const createUserToken = <ThrowOnError extends boolean = true>(options: Options<CreateUserTokenData, ThrowOnError>): RequestResult<CreateUserTokenResponses, CreateUserTokenErrors, ThrowOnError> => (options.client ?? client).post<CreateUserTokenResponses, CreateUserTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/tokens',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteUserToken = <ThrowOnError extends boolean = true>(options: Options<DeleteUserTokenData, ThrowOnError>): RequestResult<DeleteUserTokenResponses, DeleteUserTokenErrors, ThrowOnError> => (options.client ?? client).delete<DeleteUserTokenResponses, DeleteUserTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/tokens/{id}',
    ...options
});
export const getUserToken = <ThrowOnError extends boolean = true>(options: Options<GetUserTokenData, ThrowOnError>): RequestResult<GetUserTokenResponses, GetUserTokenErrors, ThrowOnError> => (options.client ?? client).get<GetUserTokenResponses, GetUserTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/tokens/{id}',
    ...options
});
export const updateUserToken = <ThrowOnError extends boolean = true>(options: Options<UpdateUserTokenData, ThrowOnError>): RequestResult<UpdateUserTokenResponses, UpdateUserTokenErrors, ThrowOnError> => (options.client ?? client).put<UpdateUserTokenResponses, UpdateUserTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/tokens/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminTokens = <ThrowOnError extends boolean = true>(options?: Options<ListAdminTokensData, ThrowOnError>): RequestResult<ListAdminTokensResponses, ListAdminTokensErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminTokensResponses, ListAdminTokensErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/tokens',
    ...options
});
export const createAdminToken = <ThrowOnError extends boolean = true>(options: Options<CreateAdminTokenData, ThrowOnError>): RequestResult<CreateAdminTokenResponses, CreateAdminTokenErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminTokenResponses, CreateAdminTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/tokens',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminToken = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminTokenData, ThrowOnError>): RequestResult<DeleteAdminTokenResponses, DeleteAdminTokenErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminTokenResponses, DeleteAdminTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/tokens/{id}',
    ...options
});
export const getAdminToken = <ThrowOnError extends boolean = true>(options: Options<GetAdminTokenData, ThrowOnError>): RequestResult<GetAdminTokenResponses, GetAdminTokenErrors, ThrowOnError> => (options.client ?? client).get<GetAdminTokenResponses, GetAdminTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/tokens/{id}',
    ...options
});
export const updateAdminToken = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminTokenData, ThrowOnError>): RequestResult<UpdateAdminTokenResponses, UpdateAdminTokenErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminTokenResponses, UpdateAdminTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/tokens/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminUsageLogs = <ThrowOnError extends boolean = true>(options?: Options<ListAdminUsageLogsData, ThrowOnError>): RequestResult<ListAdminUsageLogsResponses, ListAdminUsageLogsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminUsageLogsResponses, ListAdminUsageLogsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/usage-logs',
    ...options
});
export const listUserUsageLogs = <ThrowOnError extends boolean = true>(options?: Options<ListUserUsageLogsData, ThrowOnError>): RequestResult<ListUserUsageLogsResponses, ListUserUsageLogsErrors, ThrowOnError> => (options?.client ?? client).get<ListUserUsageLogsResponses, ListUserUsageLogsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/usage-logs',
    ...options
});
export const listAdminChannels = <ThrowOnError extends boolean = true>(options?: Options<ListAdminChannelsData, ThrowOnError>): RequestResult<ListAdminChannelsResponses, ListAdminChannelsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminChannelsResponses, ListAdminChannelsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels',
    ...options
});
export const createAdminChannel = <ThrowOnError extends boolean = true>(options: Options<CreateAdminChannelData, ThrowOnError>): RequestResult<CreateAdminChannelResponses, CreateAdminChannelErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminChannelResponses, CreateAdminChannelErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminChannel = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminChannelData, ThrowOnError>): RequestResult<DeleteAdminChannelResponses, DeleteAdminChannelErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminChannelResponses, DeleteAdminChannelErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{id}',
    ...options
});
export const getAdminChannel = <ThrowOnError extends boolean = true>(options: Options<GetAdminChannelData, ThrowOnError>): RequestResult<GetAdminChannelResponses, GetAdminChannelErrors, ThrowOnError> => (options.client ?? client).get<GetAdminChannelResponses, GetAdminChannelErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{id}',
    ...options
});
export const updateAdminChannel = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminChannelData, ThrowOnError>): RequestResult<UpdateAdminChannelResponses, UpdateAdminChannelErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminChannelResponses, UpdateAdminChannelErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const probeAdminChannel = <ThrowOnError extends boolean = true>(options: Options<ProbeAdminChannelData, ThrowOnError>): RequestResult<ProbeAdminChannelResponses, ProbeAdminChannelErrors, ThrowOnError> => (options.client ?? client).post<ProbeAdminChannelResponses, ProbeAdminChannelErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{id}/probe',
    ...options
});
export const listAdminCredentials = <ThrowOnError extends boolean = true>(options: Options<ListAdminCredentialsData, ThrowOnError>): RequestResult<ListAdminCredentialsResponses, ListAdminCredentialsErrors, ThrowOnError> => (options.client ?? client).get<ListAdminCredentialsResponses, ListAdminCredentialsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{channel_id}/credentials',
    ...options
});
export const createAdminCredential = <ThrowOnError extends boolean = true>(options: Options<CreateAdminCredentialData, ThrowOnError>): RequestResult<CreateAdminCredentialResponses, CreateAdminCredentialErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminCredentialResponses, CreateAdminCredentialErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{channel_id}/credentials',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const importAdminCredentials = <ThrowOnError extends boolean = true>(options: Options<ImportAdminCredentialsData, ThrowOnError>): RequestResult<ImportAdminCredentialsResponses, ImportAdminCredentialsErrors, ThrowOnError> => (options.client ?? client).post<ImportAdminCredentialsResponses, ImportAdminCredentialsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{channel_id}/credentials/import',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const exportAdminCredentials = <ThrowOnError extends boolean = true>(options: Options<ExportAdminCredentialsData, ThrowOnError>): RequestResult<ExportAdminCredentialsResponses, ExportAdminCredentialsErrors, ThrowOnError> => (options.client ?? client).get<ExportAdminCredentialsResponses, ExportAdminCredentialsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{channel_id}/credentials/export',
    ...options
});
export const deleteAdminCredential = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminCredentialData, ThrowOnError>): RequestResult<DeleteAdminCredentialResponses, DeleteAdminCredentialErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminCredentialResponses, DeleteAdminCredentialErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}',
    ...options
});
export const getAdminCredential = <ThrowOnError extends boolean = true>(options: Options<GetAdminCredentialData, ThrowOnError>): RequestResult<GetAdminCredentialResponses, GetAdminCredentialErrors, ThrowOnError> => (options.client ?? client).get<GetAdminCredentialResponses, GetAdminCredentialErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}',
    ...options
});
export const updateAdminCredential = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminCredentialData, ThrowOnError>): RequestResult<UpdateAdminCredentialResponses, UpdateAdminCredentialErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminCredentialResponses, UpdateAdminCredentialErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminCredentialUsage = <ThrowOnError extends boolean = true>(options: Options<GetAdminCredentialUsageData, ThrowOnError>): RequestResult<GetAdminCredentialUsageResponses, GetAdminCredentialUsageErrors, ThrowOnError> => (options.client ?? client).get<GetAdminCredentialUsageResponses, GetAdminCredentialUsageErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}/usage',
    ...options
});
export const listAdminCredentialProxies = <ThrowOnError extends boolean = true>(options?: Options<ListAdminCredentialProxiesData, ThrowOnError>): RequestResult<ListAdminCredentialProxiesResponses, ListAdminCredentialProxiesErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminCredentialProxiesResponses, ListAdminCredentialProxiesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/proxies',
    ...options
});
export const createAdminCredentialProxy = <ThrowOnError extends boolean = true>(options: Options<CreateAdminCredentialProxyData, ThrowOnError>): RequestResult<CreateAdminCredentialProxyResponses, CreateAdminCredentialProxyErrors, ThrowOnError> => (options.client ?? client).post<CreateAdminCredentialProxyResponses, CreateAdminCredentialProxyErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/proxies',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const deleteAdminCredentialProxy = <ThrowOnError extends boolean = true>(options: Options<DeleteAdminCredentialProxyData, ThrowOnError>): RequestResult<DeleteAdminCredentialProxyResponses, DeleteAdminCredentialProxyErrors, ThrowOnError> => (options.client ?? client).delete<DeleteAdminCredentialProxyResponses, DeleteAdminCredentialProxyErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/proxies/{id}',
    ...options
});
export const getAdminCredentialProxy = <ThrowOnError extends boolean = true>(options: Options<GetAdminCredentialProxyData, ThrowOnError>): RequestResult<GetAdminCredentialProxyResponses, GetAdminCredentialProxyErrors, ThrowOnError> => (options.client ?? client).get<GetAdminCredentialProxyResponses, GetAdminCredentialProxyErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/proxies/{id}',
    ...options
});
export const updateAdminCredentialProxy = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminCredentialProxyData, ThrowOnError>): RequestResult<UpdateAdminCredentialProxyResponses, UpdateAdminCredentialProxyErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminCredentialProxyResponses, UpdateAdminCredentialProxyErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/proxies/{id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminCustomOAuth2Providers = <ThrowOnError extends boolean = true>(options?: Options<ListAdminCustomOAuth2ProvidersData, ThrowOnError>): RequestResult<ListAdminCustomOAuth2ProvidersResponses, ListAdminCustomOAuth2ProvidersErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminCustomOAuth2ProvidersResponses, ListAdminCustomOAuth2ProvidersErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/custom',
    ...options
});
export const getAdminCustomOAuth2Provider = <ThrowOnError extends boolean = true>(options: Options<GetAdminCustomOAuth2ProviderData, ThrowOnError>): RequestResult<GetAdminCustomOAuth2ProviderResponses, GetAdminCustomOAuth2ProviderErrors, ThrowOnError> => (options.client ?? client).get<GetAdminCustomOAuth2ProviderResponses, GetAdminCustomOAuth2ProviderErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/custom/{provider_key}',
    ...options
});
export const updateAdminCustomOAuth2Provider = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminCustomOAuth2ProviderData, ThrowOnError>): RequestResult<UpdateAdminCustomOAuth2ProviderResponses, UpdateAdminCustomOAuth2ProviderErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminCustomOAuth2ProviderResponses, UpdateAdminCustomOAuth2ProviderErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/custom/{provider_key}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminDebugTraceSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminDebugTraceSettingsData, ThrowOnError>): RequestResult<GetAdminDebugTraceSettingsResponses, GetAdminDebugTraceSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminDebugTraceSettingsResponses, GetAdminDebugTraceSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/debug-trace-settings',
    ...options
});
export const updateAdminDebugTraceSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminDebugTraceSettingsData, ThrowOnError>): RequestResult<UpdateAdminDebugTraceSettingsResponses, UpdateAdminDebugTraceSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminDebugTraceSettingsResponses, UpdateAdminDebugTraceSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/debug-trace-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminDebugTraces = <ThrowOnError extends boolean = true>(options?: Options<ListAdminDebugTracesData, ThrowOnError>): RequestResult<ListAdminDebugTracesResponses, ListAdminDebugTracesErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminDebugTracesResponses, ListAdminDebugTracesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/debug-traces',
    ...options
});
export const getAdminDebugTrace = <ThrowOnError extends boolean = true>(options: Options<GetAdminDebugTraceData, ThrowOnError>): RequestResult<GetAdminDebugTraceResponses, GetAdminDebugTraceErrors, ThrowOnError> => (options.client ?? client).get<GetAdminDebugTraceResponses, GetAdminDebugTraceErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/debug-traces/{id}',
    ...options
});
export const readAdminDebugTraceSnapshots = <ThrowOnError extends boolean = true>(options: Options<ReadAdminDebugTraceSnapshotsData, ThrowOnError>): RequestResult<ReadAdminDebugTraceSnapshotsResponses, ReadAdminDebugTraceSnapshotsErrors, ThrowOnError> => (options.client ?? client).post<ReadAdminDebugTraceSnapshotsResponses, ReadAdminDebugTraceSnapshotsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/debug-traces/{id}/snapshots',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const receivePaymentWebhook = <ThrowOnError extends boolean = true>(options: Options<ReceivePaymentWebhookData, ThrowOnError>): RequestResult<ReceivePaymentWebhookResponses, ReceivePaymentWebhookErrors, ThrowOnError> => (options.client ?? client).post<ReceivePaymentWebhookResponses, ReceivePaymentWebhookErrors, ThrowOnError>({
    url: '/api/payment/webhook/{provider}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const receiveRefundWebhook = <ThrowOnError extends boolean = true>(options: Options<ReceiveRefundWebhookData, ThrowOnError>): RequestResult<ReceiveRefundWebhookResponses, ReceiveRefundWebhookErrors, ThrowOnError> => (options.client ?? client).post<ReceiveRefundWebhookResponses, ReceiveRefundWebhookErrors, ThrowOnError>({
    url: '/api/refund/webhook/{provider}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAccountVerifications = <ThrowOnError extends boolean = true>(options?: Options<ListAccountVerificationsData, ThrowOnError>): RequestResult<ListAccountVerificationsResponses, ListAccountVerificationsErrors, ThrowOnError> => (options?.client ?? client).get<ListAccountVerificationsResponses, ListAccountVerificationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/verifications',
    ...options
});
export const submitAccountVerification = <ThrowOnError extends boolean = true>(options: Options<SubmitAccountVerificationData, ThrowOnError>): RequestResult<SubmitAccountVerificationResponses, SubmitAccountVerificationErrors, ThrowOnError> => (options.client ?? client).post<SubmitAccountVerificationResponses, SubmitAccountVerificationErrors, ThrowOnError>({
    ...formDataBodySerializer,
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/verifications',
    ...options,
    headers: {
        'Content-Type': null,
        ...options.headers
    }
});
export const getAccountVerification = <ThrowOnError extends boolean = true>(options: Options<GetAccountVerificationData, ThrowOnError>): RequestResult<GetAccountVerificationResponses, GetAccountVerificationErrors, ThrowOnError> => (options.client ?? client).get<GetAccountVerificationResponses, GetAccountVerificationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/verifications/{case_id}',
    ...options
});
export const syncAccountVerificationProvider = <ThrowOnError extends boolean = true>(options: Options<SyncAccountVerificationProviderData, ThrowOnError>): RequestResult<SyncAccountVerificationProviderResponses, SyncAccountVerificationProviderErrors, ThrowOnError> => (options.client ?? client).post<SyncAccountVerificationProviderResponses, SyncAccountVerificationProviderErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/verifications/{case_id}/provider-sync',
    ...options
});
export const downloadAccountVerificationMaterial = <ThrowOnError extends boolean = true>(options: Options<DownloadAccountVerificationMaterialData, ThrowOnError>): RequestResult<DownloadAccountVerificationMaterialResponses, DownloadAccountVerificationMaterialErrors, ThrowOnError> => (options.client ?? client).get<DownloadAccountVerificationMaterialResponses, DownloadAccountVerificationMaterialErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/verifications/{case_id}/materials/{material_id}',
    ...options
});
export const listAdminAccountVerifications = <ThrowOnError extends boolean = true>(options?: Options<ListAdminAccountVerificationsData, ThrowOnError>): RequestResult<ListAdminAccountVerificationsResponses, ListAdminAccountVerificationsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminAccountVerificationsResponses, ListAdminAccountVerificationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/account-verifications',
    ...options
});
export const getAdminAccountVerification = <ThrowOnError extends boolean = true>(options: Options<GetAdminAccountVerificationData, ThrowOnError>): RequestResult<GetAdminAccountVerificationResponses, GetAdminAccountVerificationErrors, ThrowOnError> => (options.client ?? client).get<GetAdminAccountVerificationResponses, GetAdminAccountVerificationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/account-verifications/{case_id}',
    ...options
});
export const downloadAdminAccountVerificationMaterial = <ThrowOnError extends boolean = true>(options: Options<DownloadAdminAccountVerificationMaterialData, ThrowOnError>): RequestResult<DownloadAdminAccountVerificationMaterialResponses, DownloadAdminAccountVerificationMaterialErrors, ThrowOnError> => (options.client ?? client).get<DownloadAdminAccountVerificationMaterialResponses, DownloadAdminAccountVerificationMaterialErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/account-verifications/{case_id}/materials/{material_id}',
    ...options
});
export const getAccountVerificationEligibility = <ThrowOnError extends boolean = true>(options?: Options<GetAccountVerificationEligibilityData, ThrowOnError>): RequestResult<GetAccountVerificationEligibilityResponses, GetAccountVerificationEligibilityErrors, ThrowOnError> => (options?.client ?? client).get<GetAccountVerificationEligibilityResponses, GetAccountVerificationEligibilityErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/verifications/eligibility',
    ...options
});
export const decideAccountVerification = <ThrowOnError extends boolean = true>(options: Options<DecideAccountVerificationData, ThrowOnError>): RequestResult<DecideAccountVerificationResponses, DecideAccountVerificationErrors, ThrowOnError> => (options.client ?? client).post<DecideAccountVerificationResponses, DecideAccountVerificationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/account-verifications/{case_id}/decision',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminVerificationSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminVerificationSettingsData, ThrowOnError>): RequestResult<GetAdminVerificationSettingsResponses, GetAdminVerificationSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminVerificationSettingsResponses, GetAdminVerificationSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/account-verification-settings',
    ...options
});
export const updateAdminVerificationSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminVerificationSettingsData, ThrowOnError>): RequestResult<UpdateAdminVerificationSettingsResponses, UpdateAdminVerificationSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminVerificationSettingsResponses, UpdateAdminVerificationSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/account-verification-settings',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const startPasskeyAuthentication = <ThrowOnError extends boolean = true>(options: Options<StartPasskeyAuthenticationData, ThrowOnError>): RequestResult<StartPasskeyAuthenticationResponses, StartPasskeyAuthenticationErrors, ThrowOnError> => (options.client ?? client).post<StartPasskeyAuthenticationResponses, StartPasskeyAuthenticationErrors, ThrowOnError>({
    url: '/api/auth/passkey/options',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const finishPasskeyAuthentication = <ThrowOnError extends boolean = true>(options: Options<FinishPasskeyAuthenticationData, ThrowOnError>): RequestResult<FinishPasskeyAuthenticationResponses, FinishPasskeyAuthenticationErrors, ThrowOnError> => (options.client ?? client).post<FinishPasskeyAuthenticationResponses, FinishPasskeyAuthenticationErrors, ThrowOnError>({
    url: '/api/auth/passkey/verify',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listAdminPlatformAuditLogs = <ThrowOnError extends boolean = true>(options?: Options<ListAdminPlatformAuditLogsData, ThrowOnError>): RequestResult<ListAdminPlatformAuditLogsResponses, ListAdminPlatformAuditLogsErrors, ThrowOnError> => (options?.client ?? client).get<ListAdminPlatformAuditLogsResponses, ListAdminPlatformAuditLogsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/audit-logs',
    ...options
});
export const listSelfPlatformAuditLogs = <ThrowOnError extends boolean = true>(options?: Options<ListSelfPlatformAuditLogsData, ThrowOnError>): RequestResult<ListSelfPlatformAuditLogsResponses, ListSelfPlatformAuditLogsErrors, ThrowOnError> => (options?.client ?? client).get<ListSelfPlatformAuditLogsResponses, ListSelfPlatformAuditLogsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/audit-logs',
    ...options
});
export const startOidcLogin = <ThrowOnError extends boolean = true>(options?: Options<StartOidcLoginData, ThrowOnError>): RequestResult<StartOidcLoginResponses, StartOidcLoginErrors, ThrowOnError> => (options?.client ?? client).post<StartOidcLoginResponses, StartOidcLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/oidc/start', ...options });
export const startLinuxDoLogin = <ThrowOnError extends boolean = true>(options?: Options<StartLinuxDoLoginData, ThrowOnError>): RequestResult<StartLinuxDoLoginResponses, StartLinuxDoLoginErrors, ThrowOnError> => (options?.client ?? client).post<StartLinuxDoLoginResponses, StartLinuxDoLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/linuxdo/start', ...options });
export const startWeChatOAuthLogin = <ThrowOnError extends boolean = true>(options?: Options<StartWeChatOAuthLoginData, ThrowOnError>): RequestResult<StartWeChatOAuthLoginResponses, StartWeChatOAuthLoginErrors, ThrowOnError> => (options?.client ?? client).post<StartWeChatOAuthLoginResponses, StartWeChatOAuthLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/wechat/start', ...options });
export const startTelegramLogin = <ThrowOnError extends boolean = true>(options?: Options<StartTelegramLoginData, ThrowOnError>): RequestResult<StartTelegramLoginResponses, StartTelegramLoginErrors, ThrowOnError> => (options?.client ?? client).post<StartTelegramLoginResponses, StartTelegramLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/telegram/start', ...options });
export const startGoogleLogin = <ThrowOnError extends boolean = true>(options?: Options<StartGoogleLoginData, ThrowOnError>): RequestResult<StartGoogleLoginResponses, StartGoogleLoginErrors, ThrowOnError> => (options?.client ?? client).post<StartGoogleLoginResponses, StartGoogleLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/google/start', ...options });
export const startCustomOAuth2Login = <ThrowOnError extends boolean = true>(options: Options<StartCustomOAuth2LoginData, ThrowOnError>): RequestResult<StartCustomOAuth2LoginResponses, StartCustomOAuth2LoginErrors, ThrowOnError> => (options.client ?? client).post<StartCustomOAuth2LoginResponses, StartCustomOAuth2LoginErrors, ThrowOnError>({ url: '/api/auth/oauth/custom/{provider_key}/start', ...options });
export const completeOidcLogin = <ThrowOnError extends boolean = true>(options?: Options<CompleteOidcLoginData, ThrowOnError>): RequestResult<unknown, CompleteOidcLoginErrors, ThrowOnError> => (options?.client ?? client).get<unknown, CompleteOidcLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/oidc/callback', ...options });
export const completeLinuxDoLogin = <ThrowOnError extends boolean = true>(options?: Options<CompleteLinuxDoLoginData, ThrowOnError>): RequestResult<unknown, CompleteLinuxDoLoginErrors, ThrowOnError> => (options?.client ?? client).get<unknown, CompleteLinuxDoLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/linuxdo/callback', ...options });
export const completeWeChatOAuthLogin = <ThrowOnError extends boolean = true>(options?: Options<CompleteWeChatOAuthLoginData, ThrowOnError>): RequestResult<unknown, CompleteWeChatOAuthLoginErrors, ThrowOnError> => (options?.client ?? client).get<unknown, CompleteWeChatOAuthLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/wechat/callback', ...options });
export const completeTelegramLogin = <ThrowOnError extends boolean = true>(options?: Options<CompleteTelegramLoginData, ThrowOnError>): RequestResult<unknown, CompleteTelegramLoginErrors, ThrowOnError> => (options?.client ?? client).get<unknown, CompleteTelegramLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/telegram/callback', ...options });
export const completeGoogleLogin = <ThrowOnError extends boolean = true>(options?: Options<CompleteGoogleLoginData, ThrowOnError>): RequestResult<unknown, CompleteGoogleLoginErrors, ThrowOnError> => (options?.client ?? client).get<unknown, CompleteGoogleLoginErrors, ThrowOnError>({ url: '/api/auth/oauth/google/callback', ...options });
export const completeCustomOAuth2Login = <ThrowOnError extends boolean = true>(options: Options<CompleteCustomOAuth2LoginData, ThrowOnError>): RequestResult<unknown, CompleteCustomOAuth2LoginErrors, ThrowOnError> => (options.client ?? client).get<unknown, CompleteCustomOAuth2LoginErrors, ThrowOnError>({ url: '/api/auth/oauth/custom/{provider_key}/callback', ...options });
export const getAdminOidcLoginSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminOidcLoginSettingsData, ThrowOnError>): RequestResult<GetAdminOidcLoginSettingsResponses, GetAdminOidcLoginSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminOidcLoginSettingsResponses, GetAdminOidcLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/oidc',
    ...options
});
export const updateAdminOidcLoginSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminOidcLoginSettingsData, ThrowOnError>): RequestResult<UpdateAdminOidcLoginSettingsResponses, UpdateAdminOidcLoginSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminOidcLoginSettingsResponses, UpdateAdminOidcLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/oidc',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminLinuxDoLoginSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminLinuxDoLoginSettingsData, ThrowOnError>): RequestResult<GetAdminLinuxDoLoginSettingsResponses, GetAdminLinuxDoLoginSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminLinuxDoLoginSettingsResponses, GetAdminLinuxDoLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/linuxdo',
    ...options
});
export const updateAdminLinuxDoLoginSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminLinuxDoLoginSettingsData, ThrowOnError>): RequestResult<UpdateAdminLinuxDoLoginSettingsResponses, UpdateAdminLinuxDoLoginSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminLinuxDoLoginSettingsResponses, UpdateAdminLinuxDoLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/linuxdo',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminWeChatOAuthLoginSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminWeChatOAuthLoginSettingsData, ThrowOnError>): RequestResult<GetAdminWeChatOAuthLoginSettingsResponses, GetAdminWeChatOAuthLoginSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminWeChatOAuthLoginSettingsResponses, GetAdminWeChatOAuthLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/wechat',
    ...options
});
export const updateAdminWeChatOAuthLoginSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminWeChatOAuthLoginSettingsData, ThrowOnError>): RequestResult<UpdateAdminWeChatOAuthLoginSettingsResponses, UpdateAdminWeChatOAuthLoginSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminWeChatOAuthLoginSettingsResponses, UpdateAdminWeChatOAuthLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/wechat',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminTelegramOAuthLoginSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminTelegramOAuthLoginSettingsData, ThrowOnError>): RequestResult<GetAdminTelegramOAuthLoginSettingsResponses, GetAdminTelegramOAuthLoginSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminTelegramOAuthLoginSettingsResponses, GetAdminTelegramOAuthLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/telegram',
    ...options
});
export const updateAdminTelegramOAuthLoginSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminTelegramOAuthLoginSettingsData, ThrowOnError>): RequestResult<UpdateAdminTelegramOAuthLoginSettingsResponses, UpdateAdminTelegramOAuthLoginSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminTelegramOAuthLoginSettingsResponses, UpdateAdminTelegramOAuthLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/telegram',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getAdminGoogleOAuthLoginSettings = <ThrowOnError extends boolean = true>(options?: Options<GetAdminGoogleOAuthLoginSettingsData, ThrowOnError>): RequestResult<GetAdminGoogleOAuthLoginSettingsResponses, GetAdminGoogleOAuthLoginSettingsErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminGoogleOAuthLoginSettingsResponses, GetAdminGoogleOAuthLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/google',
    ...options
});
export const updateAdminGoogleOAuthLoginSettings = <ThrowOnError extends boolean = true>(options: Options<UpdateAdminGoogleOAuthLoginSettingsData, ThrowOnError>): RequestResult<UpdateAdminGoogleOAuthLoginSettingsResponses, UpdateAdminGoogleOAuthLoginSettingsErrors, ThrowOnError> => (options.client ?? client).put<UpdateAdminGoogleOAuthLoginSettingsResponses, UpdateAdminGoogleOAuthLoginSettingsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/authentication-settings/oauth/google',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
