// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import type { Client, ClientMeta, Options as Options2, RequestResult, TDataShape } from './client';
import { client } from './client.gen';
import type { AcceptOrganizationInvitationData, AcceptOrganizationInvitationErrors, AcceptOrganizationInvitationResponses, AdjustAdminWalletData, AdjustAdminWalletErrors, AdjustAdminWalletResponses, AdjustOrganizationWalletData, AdjustOrganizationWalletErrors, AdjustOrganizationWalletResponses, ApplyAdminModelPricesData, ApplyAdminModelPricesErrors, ApplyAdminModelPricesResponses, ApplyAdminModelSyncPreviewData, ApplyAdminModelSyncPreviewErrors, ApplyAdminModelSyncPreviewResponses, ApproveAdminRefundData, ApproveAdminRefundErrors, ApproveAdminRefundResponses, AuthenticateOrganizationServiceAccountRuntimeData, AuthenticateOrganizationServiceAccountRuntimeErrors, AuthenticateOrganizationServiceAccountRuntimeResponses, BeginAdminOAuthAuthorizationData, BeginAdminOAuthAuthorizationErrors, BeginAdminOAuthAuthorizationResponses, BindAdminUserSubscriptionData, BindAdminUserSubscriptionErrors, BindAdminUserSubscriptionResponses, BindOrganizationSsoIdentityData, BindOrganizationSsoIdentityErrors, BindOrganizationSsoIdentityResponses, ChangeOrganizationMemberData, ChangeOrganizationMemberErrors, ChangeOrganizationMemberResponses, ChangeOrganizationServiceAccountStatusData, ChangeOrganizationServiceAccountStatusErrors, ChangeOrganizationServiceAccountStatusResponses, ChangeOrganizationTeamData, ChangeOrganizationTeamErrors, ChangeOrganizationTeamResponses, ChangeUserPasswordData, ChangeUserPasswordErrors, ChangeUserPasswordResponses, CompactResponseData, CompactResponseErrors, CompactResponseResponses, CompleteAdminOAuthManualCallbackData, CompleteAdminOAuthManualCallbackErrors, CompleteAdminOAuthManualCallbackResponses, CompleteCustomOAuth2LoginData, CompleteCustomOAuth2LoginErrors, CompleteDiscordOAuthLoginData, CompleteDiscordOAuthLoginErrors, CompleteGitHubOAuthLoginData, CompleteGitHubOAuthLoginErrors, CompleteGoogleLoginData, CompleteGoogleLoginErrors, CompleteLinuxDoLoginData, CompleteLinuxDoLoginErrors, CompleteOidcLoginData, CompleteOidcLoginErrors, CompleteOrganizationSsoOidcLoginData, CompleteOrganizationSsoOidcLoginErrors, CompleteOrganizationSsoOidcLoginResponses, CompleteOrganizationSsoSamlLoginData, CompleteOrganizationSsoSamlLoginErrors, CompleteTelegramLoginData, CompleteTelegramLoginErrors, CompleteWeChatOAuthLoginData, CompleteWeChatOAuthLoginErrors, ConfirmPasswordResetData, ConfirmPasswordResetErrors, ConfirmPasswordResetResponses, CreateAdminChannelData, CreateAdminChannelErrors, CreateAdminChannelResponses, CreateAdminCredentialData, CreateAdminCredentialErrors, CreateAdminCredentialProxyData, CreateAdminCredentialProxyErrors, CreateAdminCredentialProxyResponses, CreateAdminCredentialResponses, CreateAdminGroupData, CreateAdminGroupErrors, CreateAdminGroupResponses, CreateAdminModelData, CreateAdminModelErrors, CreateAdminModelResponses, CreateAdminModelSyncPreviewData, CreateAdminModelSyncPreviewErrors, CreateAdminModelSyncPreviewResponses, CreateAdminRedemptionBatchData, CreateAdminRedemptionBatchErrors, CreateAdminRedemptionBatchResponses, CreateAdminRouteData, CreateAdminRouteErrors, CreateAdminRouteResponses, CreateAdminSubscriptionPlanData, CreateAdminSubscriptionPlanErrors, CreateAdminSubscriptionPlanResponses, CreateAdminTokenData, CreateAdminTokenErrors, CreateAdminTokenResponses, CreateAdminUserData, CreateAdminUserErrors, CreateAdminUserResponses, CreateCurrentSubscriptionOrderData, CreateCurrentSubscriptionOrderErrors, CreateCurrentSubscriptionOrderResponses, CreateOrganizationBudgetPolicyData, CreateOrganizationBudgetPolicyErrors, CreateOrganizationBudgetPolicyResponses, CreateOrganizationCustomRoleData, CreateOrganizationCustomRoleErrors, CreateOrganizationCustomRoleResponses, CreateOrganizationInvitationBatchData, CreateOrganizationInvitationBatchErrors, CreateOrganizationInvitationBatchResponses, CreateOrganizationInvitationData, CreateOrganizationInvitationErrors, CreateOrganizationInvitationResponses, CreateOrganizationServiceAccountData, CreateOrganizationServiceAccountErrors, CreateOrganizationServiceAccountResponses, CreateOrganizationSsoDomainData, CreateOrganizationSsoDomainErrors, CreateOrganizationSsoDomainResponses, CreateOrganizationSsoProviderData, CreateOrganizationSsoProviderErrors, CreateOrganizationSsoProviderResponses, CreateOrganizationTeamData, CreateOrganizationTeamErrors, CreateOrganizationTeamResponses, CreateOrganizationTokenData, CreateOrganizationTokenErrors, CreateOrganizationTokenResponses, CreateOrganizationTopupOrderData, CreateOrganizationTopupOrderErrors, CreateOrganizationTopupOrderResponses, CreatePlaygroundShareData, CreatePlaygroundShareErrors, CreatePlaygroundShareResponses, CreatePlaygroundTokenData, CreatePlaygroundTokenErrors, CreatePlaygroundTokenResponses, CreateScimGroupData, CreateScimGroupErrors, CreateScimGroupResponses, CreateScimUserData, CreateScimUserErrors, CreateScimUserResponses, CreateUserTokenData, CreateUserTokenErrors, CreateUserTokenResponses, CreateUserTopupOrderData, CreateUserTopupOrderErrors, CreateUserTopupOrderResponses, DeleteAdminChannelData, DeleteAdminChannelErrors, DeleteAdminChannelResponses, DeleteAdminCredentialData, DeleteAdminCredentialErrors, DeleteAdminCredentialProxyData, DeleteAdminCredentialProxyErrors, DeleteAdminCredentialProxyResponses, DeleteAdminCredentialResponses, DeleteAdminGroupData, DeleteAdminGroupErrors, DeleteAdminGroupResponses, DeleteAdminModelData, DeleteAdminModelErrors, DeleteAdminModelResponses, DeleteAdminRouteData, DeleteAdminRouteErrors, DeleteAdminRouteResponses, DeleteAdminTokenData, DeleteAdminTokenErrors, DeleteAdminTokenResponses, DeleteAdminUserData, DeleteAdminUserErrors, DeleteAdminUserResponses, DeletePlaygroundConversationData, DeletePlaygroundConversationErrors, DeletePlaygroundConversationResponses, DeleteScimGroupData, DeleteScimGroupErrors, DeleteScimGroupResponses, DeleteUserTokenData, DeleteUserTokenErrors, DeleteUserTokenResponses, DisableAdminRedemptionBatchData, DisableAdminRedemptionBatchErrors, DisableAdminRedemptionBatchResponses, DisableAdminSubscriptionPlanData, DisableAdminSubscriptionPlanErrors, DisableAdminSubscriptionPlanResponses, DisableOrganizationCustomRoleData, DisableOrganizationCustomRoleErrors, DisableOrganizationCustomRoleResponses, DisableOrganizationTeamData, DisableOrganizationTeamErrors, DisableOrganizationTeamResponses, DisableOrganizationTokenData, DisableOrganizationTokenErrors, DisableOrganizationTokenResponses, DisableOwnOrganizationTokenData, DisableOwnOrganizationTokenErrors, DisableOwnOrganizationTokenResponses, DisableUserTwoFactorData, DisableUserTwoFactorErrors, DisableUserTwoFactorResponses, EnableUserTwoFactorData, EnableUserTwoFactorErrors, EnableUserTwoFactorResponses, EstablishOrganizationSsoRecoveryData, EstablishOrganizationSsoRecoveryErrors, EstablishOrganizationSsoRecoveryResponses, ExchangeOAuthLoginTicketData, ExchangeOAuthLoginTicketErrors, ExchangeOAuthLoginTicketResponses, ExchangeOrganizationSsoSessionData, ExchangeOrganizationSsoSessionErrors, ExchangeOrganizationSsoSessionResponses, FinishPasskeyAuthenticationData, FinishPasskeyAuthenticationErrors, FinishPasskeyAuthenticationResponses, FinishUserPasskeyRegistrationData, FinishUserPasskeyRegistrationErrors, FinishUserPasskeyRegistrationResponses, GetAdminAuthenticationSettingsData, GetAdminAuthenticationSettingsErrors, GetAdminAuthenticationSettingsResponses, GetAdminBalanceAlertSettingsData, GetAdminBalanceAlertSettingsErrors, GetAdminBalanceAlertSettingsResponses, GetAdminChannelData, GetAdminChannelErrors, GetAdminChannelResponses, GetAdminCredentialData, GetAdminCredentialErrors, GetAdminCredentialProxyData, GetAdminCredentialProxyErrors, GetAdminCredentialProxyResponses, GetAdminCredentialResponses, GetAdminCustomOAuth2ProviderData, GetAdminCustomOAuth2ProviderErrors, GetAdminCustomOAuth2ProviderResponses, GetAdminDashboardData, GetAdminDashboardErrors, GetAdminDashboardResponses, GetAdminDebugTraceData, GetAdminDebugTraceErrors, GetAdminDebugTraceResponses, GetAdminDebugTraceSettingsData, GetAdminDebugTraceSettingsErrors, GetAdminDebugTraceSettingsResponses, GetAdminDiscordOAuthLoginSettingsData, GetAdminDiscordOAuthLoginSettingsErrors, GetAdminDiscordOAuthLoginSettingsResponses, GetAdminEmailSettingsData, GetAdminEmailSettingsErrors, GetAdminEmailSettingsResponses, GetAdminGitHubOAuthLoginSettingsData, GetAdminGitHubOAuthLoginSettingsErrors, GetAdminGitHubOAuthLoginSettingsResponses, GetAdminGoogleOAuthLoginSettingsData, GetAdminGoogleOAuthLoginSettingsErrors, GetAdminGoogleOAuthLoginSettingsResponses, GetAdminGroupData, GetAdminGroupErrors, GetAdminGroupResponses, GetAdminLinuxDoLoginSettingsData, GetAdminLinuxDoLoginSettingsErrors, GetAdminLinuxDoLoginSettingsResponses, GetAdminModelData, GetAdminModelErrors, GetAdminModelResponses, GetAdminNetworkSettingsData, GetAdminNetworkSettingsErrors, GetAdminNetworkSettingsResponses, GetAdminOidcLoginSettingsData, GetAdminOidcLoginSettingsErrors, GetAdminOidcLoginSettingsResponses, GetAdminPaymentSettingsData, GetAdminPaymentSettingsErrors, GetAdminPaymentSettingsResponses, GetAdminRouteData, GetAdminRouteErrors, GetAdminRouteResponses, GetAdminSiteSettingsData, GetAdminSiteSettingsErrors, GetAdminSiteSettingsResponses, GetAdminTelegramOAuthLoginSettingsData, GetAdminTelegramOAuthLoginSettingsErrors, GetAdminTelegramOAuthLoginSettingsResponses, GetAdminTokenData, GetAdminTokenErrors, GetAdminTokenResponses, GetAdminUserData, GetAdminUserErrors, GetAdminUserResponses, GetAdminWeChatOAuthLoginSettingsData, GetAdminWeChatOAuthLoginSettingsErrors, GetAdminWeChatOAuthLoginSettingsResponses, GetCurrentSubscriptionOrderData, GetCurrentSubscriptionOrderErrors, GetCurrentSubscriptionOrderResponses, GetInitialSetupStatusData, GetInitialSetupStatusErrors, GetInitialSetupStatusResponses, GetManagementSessionData, GetManagementSessionErrors, GetManagementSessionResponses, GetOrganizationBudgetReservationData, GetOrganizationBudgetReservationErrors, GetOrganizationBudgetReservationResponses, GetOrganizationBudgetsData, GetOrganizationBudgetsErrors, GetOrganizationBudgetsResponses, GetOrganizationCustomRoleData, GetOrganizationCustomRoleErrors, GetOrganizationCustomRoleResponses, GetOrganizationOverviewData, GetOrganizationOverviewErrors, GetOrganizationOverviewResponses, GetOrganizationServiceAccountBudgetBindingData, GetOrganizationServiceAccountBudgetBindingErrors, GetOrganizationServiceAccountBudgetBindingResponses, GetOrganizationServiceAccountData, GetOrganizationServiceAccountErrors, GetOrganizationServiceAccountResponses, GetOrganizationSsoPolicyData, GetOrganizationSsoPolicyErrors, GetOrganizationSsoPolicyResponses, GetOrganizationSsoProviderData, GetOrganizationSsoProviderErrors, GetOrganizationSsoProviderResponses, GetOrganizationTokenData, GetOrganizationTokenErrors, GetOrganizationTokenResponses, GetOrganizationWalletData, GetOrganizationWalletErrors, GetOrganizationWalletResponses, GetPlaygroundConversationData, GetPlaygroundConversationErrors, GetPlaygroundConversationResponses, GetPlaygroundShareData, GetPlaygroundShareErrors, GetPlaygroundShareResponses, GetPublicSiteSettingsData, GetPublicSiteSettingsErrors, GetPublicSiteSettingsResponses, GetRegistrationStatusData, GetRegistrationStatusErrors, GetRegistrationStatusResponses, GetScimServiceProviderConfigData, GetScimServiceProviderConfigErrors, GetScimServiceProviderConfigResponses, GetUserInvitationsData, GetUserInvitationsErrors, GetUserInvitationsResponses, GetUserProfileData, GetUserProfileErrors, GetUserProfileResponses, GetUserTokenData, GetUserTokenErrors, GetUserTokenResponses, GetUserTopupConfigurationData, GetUserTopupConfigurationErrors, GetUserTopupConfigurationResponses, GetUserTwoFactorData, GetUserTwoFactorErrors, GetUserTwoFactorResponses, GetUserWalletData, GetUserWalletErrors, GetUserWalletResponses, ImportMissingAdminModelsData, ImportMissingAdminModelsErrors, ImportMissingAdminModelsResponses, InitializeAdminSetupData, InitializeAdminSetupErrors, InitializeAdminSetupResponses, ListAccountOrganizationsData, ListAccountOrganizationsErrors, ListAccountOrganizationsResponses, ListAdminChannelsData, ListAdminChannelsErrors, ListAdminChannelsResponses, ListAdminCredentialProxiesData, ListAdminCredentialProxiesErrors, ListAdminCredentialProxiesResponses, ListAdminCredentialsData, ListAdminCredentialsErrors, ListAdminCredentialsResponses, ListAdminCustomOAuth2ProvidersData, ListAdminCustomOAuth2ProvidersErrors, ListAdminCustomOAuth2ProvidersResponses, ListAdminDebugTracesData, ListAdminDebugTracesErrors, ListAdminDebugTracesResponses, ListAdminGroupsData, ListAdminGroupsErrors, ListAdminGroupsResponses, ListAdminModelPricesData, ListAdminModelPricesErrors, ListAdminModelPricesResponses, ListAdminModelsData, ListAdminModelsErrors, ListAdminModelsResponses, ListAdminOAuthProvidersData, ListAdminOAuthProvidersErrors, ListAdminOAuthProvidersResponses, ListAdminPlatformAuditLogsData, ListAdminPlatformAuditLogsErrors, ListAdminPlatformAuditLogsResponses, ListAdminRedemptionAuditData, ListAdminRedemptionAuditErrors, ListAdminRedemptionAuditResponses, ListAdminRedemptionBatchesData, ListAdminRedemptionBatchesErrors, ListAdminRedemptionBatchesResponses, ListAdminRefundsData, ListAdminRefundsErrors, ListAdminRefundsResponses, ListAdminRoutesData, ListAdminRoutesErrors, ListAdminRoutesResponses, ListAdminSubscriptionPlansData, ListAdminSubscriptionPlansErrors, ListAdminSubscriptionPlansResponses, ListAdminTokensData, ListAdminTokensErrors, ListAdminTokensResponses, ListAdminUsageLogsData, ListAdminUsageLogsErrors, ListAdminUsageLogsResponses, ListAdminUsersData, ListAdminUsersErrors, ListAdminUsersResponses, ListAdminUserSubscriptionsData, ListAdminUserSubscriptionsErrors, ListAdminUserSubscriptionsResponses, ListAdminWalletEntriesData, ListAdminWalletEntriesErrors, ListAdminWalletEntriesResponses, ListCurrentSubscriptionCatalogData, ListCurrentSubscriptionCatalogErrors, ListCurrentSubscriptionCatalogResponses, ListCurrentUserSubscriptionsData, ListCurrentUserSubscriptionsErrors, ListCurrentUserSubscriptionsResponses, ListMissingAdminModelsData, ListMissingAdminModelsErrors, ListMissingAdminModelsResponses, ListModelProvidersData, ListModelProvidersErrors, ListModelProvidersResponses, ListModelsData, ListModelsErrors, ListModelsResponses, ListOrganizationAuditLogsData, ListOrganizationAuditLogsErrors, ListOrganizationAuditLogsResponses, ListOrganizationCustomRolesData, ListOrganizationCustomRolesErrors, ListOrganizationCustomRolesResponses, ListOrganizationInvitationsData, ListOrganizationInvitationsErrors, ListOrganizationInvitationsResponses, ListOrganizationMembersData, ListOrganizationMembersErrors, ListOrganizationMembersResponses, ListOrganizationScimAuditLogsData, ListOrganizationScimAuditLogsErrors, ListOrganizationScimAuditLogsResponses, ListOrganizationServiceAccountsData, ListOrganizationServiceAccountsErrors, ListOrganizationServiceAccountsResponses, ListOrganizationSsoDomainsData, ListOrganizationSsoDomainsErrors, ListOrganizationSsoDomainsResponses, ListOrganizationSsoProvidersData, ListOrganizationSsoProvidersErrors, ListOrganizationSsoProvidersResponses, ListOrganizationTeamsData, ListOrganizationTeamsErrors, ListOrganizationTeamsResponses, ListOrganizationTokenDirectoryData, ListOrganizationTokenDirectoryErrors, ListOrganizationTokenDirectoryResponses, ListOrganizationTokensData, ListOrganizationTokensErrors, ListOrganizationTokensResponses, ListOrganizationUsageLogsData, ListOrganizationUsageLogsErrors, ListOrganizationUsageLogsResponses, ListOrganizationWalletEntriesData, ListOrganizationWalletEntriesErrors, ListOrganizationWalletEntriesResponses, ListPlatformOrganizationAuditLogsData, ListPlatformOrganizationAuditLogsErrors, ListPlatformOrganizationAuditLogsResponses, ListPlatformOrganizationUsageLogsData, ListPlatformOrganizationUsageLogsErrors, ListPlatformOrganizationUsageLogsResponses, ListPlaygroundConversationsData, ListPlaygroundConversationsErrors, ListPlaygroundConversationsResponses, ListScimResourceTypesData, ListScimResourceTypesErrors, ListScimResourceTypesResponses, ListSelfPlatformAuditLogsData, ListSelfPlatformAuditLogsErrors, ListSelfPlatformAuditLogsResponses, ListUserNotificationsData, ListUserNotificationsErrors, ListUserNotificationsResponses, ListUserPasskeysData, ListUserPasskeysErrors, ListUserPasskeysResponses, ListUserTokensData, ListUserTokensErrors, ListUserTokensResponses, ListUserUsageLogsData, ListUserUsageLogsErrors, ListUserUsageLogsResponses, ListUserWalletEntriesData, ListUserWalletEntriesErrors, ListUserWalletEntriesResponses, ListVideoTasksData, ListVideoTasksErrors, ListVideoTasksResponses, LoginManagementSessionData, LoginManagementSessionErrors, LoginManagementSessionResponses, MarkUserNotificationsReadData, MarkUserNotificationsReadErrors, MarkUserNotificationsReadResponses, PollVideoTaskData, PollVideoTaskErrors, PollVideoTaskResponses, PreviewAdminLiteLlmModelPricesData, PreviewAdminLiteLlmModelPricesErrors, PreviewAdminLiteLlmModelPricesResponses, PreviewAdminModelPriceExpressionData, PreviewAdminModelPriceExpressionErrors, PreviewAdminModelPriceExpressionResponses, PreviewAdminModelPricesData, PreviewAdminModelPricesErrors, PreviewAdminModelPricesResponses, PreviewOrganizationInvitationsData, PreviewOrganizationInvitationsErrors, PreviewOrganizationInvitationsResponses, ProbeAdminChannelData, ProbeAdminChannelErrors, ProbeAdminChannelResponses, ReadAdminDebugTraceSnapshotsData, ReadAdminDebugTraceSnapshotsErrors, ReadAdminDebugTraceSnapshotsResponses, ReceivePaymentWebhookData, ReceivePaymentWebhookErrors, ReceivePaymentWebhookResponses, ReceiveRefundWebhookData, ReceiveRefundWebhookErrors, ReceiveRefundWebhookResponses, RedeemUserRedemptionCodeData, RedeemUserRedemptionCodeErrors, RedeemUserRedemptionCodeResponses, RegisterUserData, RegisterUserErrors, RegisterUserResponses, RejectAdminRefundData, RejectAdminRefundErrors, RejectAdminRefundResponses, RenameUserPasskeyData, RenameUserPasskeyErrors, RenameUserPasskeyResponses, ReplaceScimGroupData, ReplaceScimGroupErrors, ReplaceScimGroupResponses, ReplaceScimUserData, ReplaceScimUserErrors, ReplaceScimUserResponses, RequestPasswordResetData, RequestPasswordResetErrors, RequestPasswordResetResponses, RerankData, RerankErrors, RerankResponses, ResendOrganizationInvitationData, ResendOrganizationInvitationErrors, ResendOrganizationInvitationResponses, RevokeOrganizationInvitationData, RevokeOrganizationInvitationErrors, RevokeOrganizationInvitationResponses, RevokePlaygroundShareData, RevokePlaygroundShareErrors, RevokePlaygroundShareResponses, RevokeUserPasskeyData, RevokeUserPasskeyErrors, RevokeUserPasskeyResponses, RotateOrganizationServiceAccountKeyData, RotateOrganizationServiceAccountKeyErrors, RotateOrganizationServiceAccountKeyResponses, SavePlaygroundConversationData, SavePlaygroundConversationErrors, SavePlaygroundConversationResponses, SendAdminEmailTestData, SendAdminEmailTestErrors, SendAdminEmailTestResponses, SendRegistrationEmailVerificationData, SendRegistrationEmailVerificationErrors, SendRegistrationEmailVerificationResponses, SetOrganizationMemberCustomRoleData, SetOrganizationMemberCustomRoleErrors, SetOrganizationMemberCustomRoleResponses, SetOrganizationServiceAccountBudgetBindingData, SetOrganizationServiceAccountBudgetBindingErrors, SetOrganizationServiceAccountBudgetBindingResponses, StartCustomOAuth2LoginData, StartCustomOAuth2LoginErrors, StartCustomOAuth2LoginResponses, StartDiscordOAuthLoginData, StartDiscordOAuthLoginErrors, StartDiscordOAuthLoginResponses, StartGitHubOAuthLoginData, StartGitHubOAuthLoginErrors, StartGitHubOAuthLoginResponses, StartGoogleLoginData, StartGoogleLoginErrors, StartGoogleLoginResponses, StartLinuxDoLoginData, StartLinuxDoLoginErrors, StartLinuxDoLoginResponses, StartOidcLoginData, StartOidcLoginErrors, StartOidcLoginResponses, StartOrganizationSsoLoginData, StartOrganizationSsoLoginErrors, StartOrganizationSsoLoginResponses, StartPasskeyAuthenticationData, StartPasskeyAuthenticationErrors, StartPasskeyAuthenticationResponses, StartTelegramLoginData, StartTelegramLoginErrors, StartTelegramLoginResponses, StartUserPasskeyRegistrationData, StartUserPasskeyRegistrationErrors, StartUserPasskeyRegistrationResponses, StartWeChatOAuthLoginData, StartWeChatOAuthLoginErrors, StartWeChatOAuthLoginResponses, SubmitAdminRefundData, SubmitAdminRefundErrors, SubmitAdminRefundResponses, SubmitCurrentSubscriptionOrderPaymentData, SubmitCurrentSubscriptionOrderPaymentErrors, SubmitCurrentSubscriptionOrderPaymentResponses, SubmitVideoTaskData, SubmitVideoTaskErrors, SubmitVideoTaskResponses, SuspendOrganizationSsoRequiredData, SuspendOrganizationSsoRequiredErrors, SuspendOrganizationSsoRequiredResponses, SynthesizeSpeechData, SynthesizeSpeechErrors, SynthesizeSpeechResponses, TransferOrganizationOwnershipData, TransferOrganizationOwnershipErrors, TransferOrganizationOwnershipResponses, TransitionAdminUserSubscriptionLifecycleData, TransitionAdminUserSubscriptionLifecycleErrors, TransitionAdminUserSubscriptionLifecycleResponses, UpdateAdminAuthenticationSettingsData, UpdateAdminAuthenticationSettingsErrors, UpdateAdminAuthenticationSettingsResponses, UpdateAdminBalanceAlertSettingsData, UpdateAdminBalanceAlertSettingsErrors, UpdateAdminBalanceAlertSettingsResponses, UpdateAdminChannelData, UpdateAdminChannelErrors, UpdateAdminChannelResponses, UpdateAdminCredentialData, UpdateAdminCredentialErrors, UpdateAdminCredentialProxyData, UpdateAdminCredentialProxyErrors, UpdateAdminCredentialProxyResponses, UpdateAdminCredentialResponses, UpdateAdminCustomOAuth2ProviderData, UpdateAdminCustomOAuth2ProviderErrors, UpdateAdminCustomOAuth2ProviderResponses, UpdateAdminDebugTraceSettingsData, UpdateAdminDebugTraceSettingsErrors, UpdateAdminDebugTraceSettingsResponses, UpdateAdminDiscordOAuthLoginSettingsData, UpdateAdminDiscordOAuthLoginSettingsErrors, UpdateAdminDiscordOAuthLoginSettingsResponses, UpdateAdminEmailSettingsData, UpdateAdminEmailSettingsErrors, UpdateAdminEmailSettingsResponses, UpdateAdminGitHubOAuthLoginSettingsData, UpdateAdminGitHubOAuthLoginSettingsErrors, UpdateAdminGitHubOAuthLoginSettingsResponses, UpdateAdminGoogleOAuthLoginSettingsData, UpdateAdminGoogleOAuthLoginSettingsErrors, UpdateAdminGoogleOAuthLoginSettingsResponses, UpdateAdminGroupData, UpdateAdminGroupErrors, UpdateAdminGroupResponses, UpdateAdminLinuxDoLoginSettingsData, UpdateAdminLinuxDoLoginSettingsErrors, UpdateAdminLinuxDoLoginSettingsResponses, UpdateAdminModelData, UpdateAdminModelErrors, UpdateAdminModelResponses, UpdateAdminNetworkSettingsData, UpdateAdminNetworkSettingsErrors, UpdateAdminNetworkSettingsResponses, UpdateAdminOidcLoginSettingsData, UpdateAdminOidcLoginSettingsErrors, UpdateAdminOidcLoginSettingsResponses, UpdateAdminPaymentSettingsData, UpdateAdminPaymentSettingsErrors, UpdateAdminPaymentSettingsResponses, UpdateAdminRouteData, UpdateAdminRouteErrors, UpdateAdminRouteResponses, UpdateAdminSiteSettingsData, UpdateAdminSiteSettingsErrors, UpdateAdminSiteSettingsResponses, UpdateAdminTelegramOAuthLoginSettingsData, UpdateAdminTelegramOAuthLoginSettingsErrors, UpdateAdminTelegramOAuthLoginSettingsResponses, UpdateAdminTokenData, UpdateAdminTokenErrors, UpdateAdminTokenResponses, UpdateAdminUserData, UpdateAdminUserErrors, UpdateAdminUserResponses, UpdateAdminWeChatOAuthLoginSettingsData, UpdateAdminWeChatOAuthLoginSettingsErrors, UpdateAdminWeChatOAuthLoginSettingsResponses, UpdateOrganizationCustomRoleData, UpdateOrganizationCustomRoleErrors, UpdateOrganizationCustomRoleResponses, UpdateOrganizationSsoPolicyData, UpdateOrganizationSsoPolicyErrors, UpdateOrganizationSsoPolicyResponses, UpdateOrganizationSsoProviderData, UpdateOrganizationSsoProviderErrors, UpdateOrganizationSsoProviderResponses, UpdateOrganizationTokenData, UpdateOrganizationTokenErrors, UpdateOrganizationTokenResponses, UpdateUserNotificationPreferencesData, UpdateUserNotificationPreferencesErrors, UpdateUserNotificationPreferencesResponses, UpdateUserProfileData, UpdateUserProfileErrors, UpdateUserProfileResponses, UpdateUserTokenData, UpdateUserTokenErrors, UpdateUserTokenResponses, VerifyOrganizationSsoDomainData, VerifyOrganizationSsoDomainErrors, VerifyOrganizationSsoDomainResponses } from './types.gen';
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
export const listAccountOrganizations = <ThrowOnError extends boolean = true>(options?: Options<ListAccountOrganizationsData, ThrowOnError>): RequestResult<ListAccountOrganizationsResponses, ListAccountOrganizationsErrors, ThrowOnError> => (options?.client ?? client).get<ListAccountOrganizationsResponses, ListAccountOrganizationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/organizations',
    ...options
});
export const getOrganizationOverview = <ThrowOnError extends boolean = true>(options: Options<GetOrganizationOverviewData, ThrowOnError>): RequestResult<GetOrganizationOverviewResponses, GetOrganizationOverviewErrors, ThrowOnError> => (options.client ?? client).get<GetOrganizationOverviewResponses, GetOrganizationOverviewErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}',
    ...options
});
export const listOrganizationInvitations = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationInvitationsData, ThrowOnError>): RequestResult<ListOrganizationInvitationsResponses, ListOrganizationInvitationsErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationInvitationsResponses, ListOrganizationInvitationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/invitations',
    ...options
});
export const createOrganizationInvitation = <ThrowOnError extends boolean = true>(options: Options<CreateOrganizationInvitationData, ThrowOnError>): RequestResult<CreateOrganizationInvitationResponses, CreateOrganizationInvitationErrors, ThrowOnError> => (options.client ?? client).post<CreateOrganizationInvitationResponses, CreateOrganizationInvitationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/invitations',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const previewOrganizationInvitations = <ThrowOnError extends boolean = true>(options: Options<PreviewOrganizationInvitationsData, ThrowOnError>): RequestResult<PreviewOrganizationInvitationsResponses, PreviewOrganizationInvitationsErrors, ThrowOnError> => (options.client ?? client).post<PreviewOrganizationInvitationsResponses, PreviewOrganizationInvitationsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/invitations/preview',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const createOrganizationInvitationBatch = <ThrowOnError extends boolean = true>(options: Options<CreateOrganizationInvitationBatchData, ThrowOnError>): RequestResult<CreateOrganizationInvitationBatchResponses, CreateOrganizationInvitationBatchErrors, ThrowOnError> => (options.client ?? client).post<CreateOrganizationInvitationBatchResponses, CreateOrganizationInvitationBatchErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/invitations/batches',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const revokeOrganizationInvitation = <ThrowOnError extends boolean = true>(options: Options<RevokeOrganizationInvitationData, ThrowOnError>): RequestResult<RevokeOrganizationInvitationResponses, RevokeOrganizationInvitationErrors, ThrowOnError> => (options.client ?? client).post<RevokeOrganizationInvitationResponses, RevokeOrganizationInvitationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/invitations/{invitation_key}/revoke',
    ...options
});
export const resendOrganizationInvitation = <ThrowOnError extends boolean = true>(options: Options<ResendOrganizationInvitationData, ThrowOnError>): RequestResult<ResendOrganizationInvitationResponses, ResendOrganizationInvitationErrors, ThrowOnError> => (options.client ?? client).post<ResendOrganizationInvitationResponses, ResendOrganizationInvitationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/invitations/{invitation_key}/resend',
    ...options
});
export const acceptOrganizationInvitation = <ThrowOnError extends boolean = true>(options: Options<AcceptOrganizationInvitationData, ThrowOnError>): RequestResult<AcceptOrganizationInvitationResponses, AcceptOrganizationInvitationErrors, ThrowOnError> => (options.client ?? client).post<AcceptOrganizationInvitationResponses, AcceptOrganizationInvitationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/account/organization-invitations/accept',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listOrganizationMembers = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationMembersData, ThrowOnError>): RequestResult<ListOrganizationMembersResponses, ListOrganizationMembersErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationMembersResponses, ListOrganizationMembersErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/members',
    ...options
});
export const changeOrganizationMember = <ThrowOnError extends boolean = true>(options: Options<ChangeOrganizationMemberData, ThrowOnError>): RequestResult<ChangeOrganizationMemberResponses, ChangeOrganizationMemberErrors, ThrowOnError> => (options.client ?? client).patch<ChangeOrganizationMemberResponses, ChangeOrganizationMemberErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/members/{membership_id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const transferOrganizationOwnership = <ThrowOnError extends boolean = true>(options: Options<TransferOrganizationOwnershipData, ThrowOnError>): RequestResult<TransferOrganizationOwnershipResponses, TransferOrganizationOwnershipErrors, ThrowOnError> => (options.client ?? client).post<TransferOrganizationOwnershipResponses, TransferOrganizationOwnershipErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/ownership-transfer',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listOrganizationTeams = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationTeamsData, ThrowOnError>): RequestResult<ListOrganizationTeamsResponses, ListOrganizationTeamsErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationTeamsResponses, ListOrganizationTeamsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/teams',
    ...options
});
export const createOrganizationTeam = <ThrowOnError extends boolean = true>(options: Options<CreateOrganizationTeamData, ThrowOnError>): RequestResult<CreateOrganizationTeamResponses, CreateOrganizationTeamErrors, ThrowOnError> => (options.client ?? client).post<CreateOrganizationTeamResponses, CreateOrganizationTeamErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/teams',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const changeOrganizationTeam = <ThrowOnError extends boolean = true>(options: Options<ChangeOrganizationTeamData, ThrowOnError>): RequestResult<ChangeOrganizationTeamResponses, ChangeOrganizationTeamErrors, ThrowOnError> => (options.client ?? client).patch<ChangeOrganizationTeamResponses, ChangeOrganizationTeamErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/teams/{team_id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const disableOrganizationTeam = <ThrowOnError extends boolean = true>(options: Options<DisableOrganizationTeamData, ThrowOnError>): RequestResult<DisableOrganizationTeamResponses, DisableOrganizationTeamErrors, ThrowOnError> => (options.client ?? client).post<DisableOrganizationTeamResponses, DisableOrganizationTeamErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/teams/{team_id}/disable',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listOrganizationCustomRoles = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationCustomRolesData, ThrowOnError>): RequestResult<ListOrganizationCustomRolesResponses, ListOrganizationCustomRolesErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationCustomRolesResponses, ListOrganizationCustomRolesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/roles',
    ...options
});
export const createOrganizationCustomRole = <ThrowOnError extends boolean = true>(options: Options<CreateOrganizationCustomRoleData, ThrowOnError>): RequestResult<CreateOrganizationCustomRoleResponses, CreateOrganizationCustomRoleErrors, ThrowOnError> => (options.client ?? client).post<CreateOrganizationCustomRoleResponses, CreateOrganizationCustomRoleErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/roles',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getOrganizationCustomRole = <ThrowOnError extends boolean = true>(options: Options<GetOrganizationCustomRoleData, ThrowOnError>): RequestResult<GetOrganizationCustomRoleResponses, GetOrganizationCustomRoleErrors, ThrowOnError> => (options.client ?? client).get<GetOrganizationCustomRoleResponses, GetOrganizationCustomRoleErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/roles/{role_key}',
    ...options
});
export const updateOrganizationCustomRole = <ThrowOnError extends boolean = true>(options: Options<UpdateOrganizationCustomRoleData, ThrowOnError>): RequestResult<UpdateOrganizationCustomRoleResponses, UpdateOrganizationCustomRoleErrors, ThrowOnError> => (options.client ?? client).patch<UpdateOrganizationCustomRoleResponses, UpdateOrganizationCustomRoleErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/roles/{role_key}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const disableOrganizationCustomRole = <ThrowOnError extends boolean = true>(options: Options<DisableOrganizationCustomRoleData, ThrowOnError>): RequestResult<DisableOrganizationCustomRoleResponses, DisableOrganizationCustomRoleErrors, ThrowOnError> => (options.client ?? client).post<DisableOrganizationCustomRoleResponses, DisableOrganizationCustomRoleErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/roles/{role_key}/disable',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listOrganizationServiceAccounts = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationServiceAccountsData, ThrowOnError>): RequestResult<ListOrganizationServiceAccountsResponses, ListOrganizationServiceAccountsErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationServiceAccountsResponses, ListOrganizationServiceAccountsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/service-accounts',
    ...options
});
export const createOrganizationServiceAccount = <ThrowOnError extends boolean = true>(options: Options<CreateOrganizationServiceAccountData, ThrowOnError>): RequestResult<CreateOrganizationServiceAccountResponses, CreateOrganizationServiceAccountErrors, ThrowOnError> => (options.client ?? client).post<CreateOrganizationServiceAccountResponses, CreateOrganizationServiceAccountErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/service-accounts',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getOrganizationServiceAccount = <ThrowOnError extends boolean = true>(options: Options<GetOrganizationServiceAccountData, ThrowOnError>): RequestResult<GetOrganizationServiceAccountResponses, GetOrganizationServiceAccountErrors, ThrowOnError> => (options.client ?? client).get<GetOrganizationServiceAccountResponses, GetOrganizationServiceAccountErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/service-accounts/{service_account_key}',
    ...options
});
export const changeOrganizationServiceAccountStatus = <ThrowOnError extends boolean = true>(options: Options<ChangeOrganizationServiceAccountStatusData, ThrowOnError>): RequestResult<ChangeOrganizationServiceAccountStatusResponses, ChangeOrganizationServiceAccountStatusErrors, ThrowOnError> => (options.client ?? client).post<ChangeOrganizationServiceAccountStatusResponses, ChangeOrganizationServiceAccountStatusErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/service-accounts/{service_account_key}/status',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const rotateOrganizationServiceAccountKey = <ThrowOnError extends boolean = true>(options: Options<RotateOrganizationServiceAccountKeyData, ThrowOnError>): RequestResult<RotateOrganizationServiceAccountKeyResponses, RotateOrganizationServiceAccountKeyErrors, ThrowOnError> => (options.client ?? client).post<RotateOrganizationServiceAccountKeyResponses, RotateOrganizationServiceAccountKeyErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/service-accounts/{service_account_key}/rotate-key',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getOrganizationServiceAccountBudgetBinding = <ThrowOnError extends boolean = true>(options: Options<GetOrganizationServiceAccountBudgetBindingData, ThrowOnError>): RequestResult<GetOrganizationServiceAccountBudgetBindingResponses, GetOrganizationServiceAccountBudgetBindingErrors, ThrowOnError> => (options.client ?? client).get<GetOrganizationServiceAccountBudgetBindingResponses, GetOrganizationServiceAccountBudgetBindingErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/service-accounts/{service_account_key}/budget-binding',
    ...options
});
export const setOrganizationServiceAccountBudgetBinding = <ThrowOnError extends boolean = true>(options: Options<SetOrganizationServiceAccountBudgetBindingData, ThrowOnError>): RequestResult<SetOrganizationServiceAccountBudgetBindingResponses, SetOrganizationServiceAccountBudgetBindingErrors, ThrowOnError> => (options.client ?? client).put<SetOrganizationServiceAccountBudgetBindingResponses, SetOrganizationServiceAccountBudgetBindingErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/service-accounts/{service_account_key}/budget-binding',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const setOrganizationMemberCustomRole = <ThrowOnError extends boolean = true>(options: Options<SetOrganizationMemberCustomRoleData, ThrowOnError>): RequestResult<SetOrganizationMemberCustomRoleResponses, SetOrganizationMemberCustomRoleErrors, ThrowOnError> => (options.client ?? client).patch<SetOrganizationMemberCustomRoleResponses, SetOrganizationMemberCustomRoleErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/members/{membership_id}/custom-role',
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
export const getAdminDashboard = <ThrowOnError extends boolean = true>(options?: Options<GetAdminDashboardData, ThrowOnError>): RequestResult<GetAdminDashboardResponses, GetAdminDashboardErrors, ThrowOnError> => (options?.client ?? client).get<GetAdminDashboardResponses, GetAdminDashboardErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/admin/dashboard',
    ...options
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
export const createPlaygroundToken = <ThrowOnError extends boolean = true>(options?: Options<CreatePlaygroundTokenData, ThrowOnError>): RequestResult<CreatePlaygroundTokenResponses, CreatePlaygroundTokenErrors, ThrowOnError> => (options?.client ?? client).post<CreatePlaygroundTokenResponses, CreatePlaygroundTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/playground/token',
    ...options
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
export const getOrganizationWallet = <ThrowOnError extends boolean = true>(options: Options<GetOrganizationWalletData, ThrowOnError>): RequestResult<GetOrganizationWalletResponses, GetOrganizationWalletErrors, ThrowOnError> => (options.client ?? client).get<GetOrganizationWalletResponses, GetOrganizationWalletErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/wallet',
    ...options
});
export const listOrganizationWalletEntries = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationWalletEntriesData, ThrowOnError>): RequestResult<ListOrganizationWalletEntriesResponses, ListOrganizationWalletEntriesErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationWalletEntriesResponses, ListOrganizationWalletEntriesErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/wallet/entries',
    ...options
});
export const adjustOrganizationWallet = <ThrowOnError extends boolean = true>(options: Options<AdjustOrganizationWalletData, ThrowOnError>): RequestResult<AdjustOrganizationWalletResponses, AdjustOrganizationWalletErrors, ThrowOnError> => (options.client ?? client).post<AdjustOrganizationWalletResponses, AdjustOrganizationWalletErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/wallet/adjustments',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const createOrganizationTopupOrder = <ThrowOnError extends boolean = true>(options: Options<CreateOrganizationTopupOrderData, ThrowOnError>): RequestResult<CreateOrganizationTopupOrderResponses, CreateOrganizationTopupOrderErrors, ThrowOnError> => (options.client ?? client).post<CreateOrganizationTopupOrderResponses, CreateOrganizationTopupOrderErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/topups',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getOrganizationBudgets = <ThrowOnError extends boolean = true>(options: Options<GetOrganizationBudgetsData, ThrowOnError>): RequestResult<GetOrganizationBudgetsResponses, GetOrganizationBudgetsErrors, ThrowOnError> => (options.client ?? client).get<GetOrganizationBudgetsResponses, GetOrganizationBudgetsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/budgets',
    ...options
});
export const createOrganizationBudgetPolicy = <ThrowOnError extends boolean = true>(options: Options<CreateOrganizationBudgetPolicyData, ThrowOnError>): RequestResult<CreateOrganizationBudgetPolicyResponses, CreateOrganizationBudgetPolicyErrors, ThrowOnError> => (options.client ?? client).post<CreateOrganizationBudgetPolicyResponses, CreateOrganizationBudgetPolicyErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/budgets',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getOrganizationBudgetReservation = <ThrowOnError extends boolean = true>(options: Options<GetOrganizationBudgetReservationData, ThrowOnError>): RequestResult<GetOrganizationBudgetReservationResponses, GetOrganizationBudgetReservationErrors, ThrowOnError> => (options.client ?? client).get<GetOrganizationBudgetReservationResponses, GetOrganizationBudgetReservationErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/budgets/reservations/{reservation_id}',
    ...options
});
export const listOrganizationSsoProviders = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationSsoProvidersData, ThrowOnError>): RequestResult<ListOrganizationSsoProvidersResponses, ListOrganizationSsoProvidersErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationSsoProvidersResponses, ListOrganizationSsoProvidersErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/providers',
    ...options
});
export const createOrganizationSsoProvider = <ThrowOnError extends boolean = true>(options: Options<CreateOrganizationSsoProviderData, ThrowOnError>): RequestResult<CreateOrganizationSsoProviderResponses, CreateOrganizationSsoProviderErrors, ThrowOnError> => (options.client ?? client).post<CreateOrganizationSsoProviderResponses, CreateOrganizationSsoProviderErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/providers',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getOrganizationSsoProvider = <ThrowOnError extends boolean = true>(options: Options<GetOrganizationSsoProviderData, ThrowOnError>): RequestResult<GetOrganizationSsoProviderResponses, GetOrganizationSsoProviderErrors, ThrowOnError> => (options.client ?? client).get<GetOrganizationSsoProviderResponses, GetOrganizationSsoProviderErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/providers/{provider_key}',
    ...options
});
export const updateOrganizationSsoProvider = <ThrowOnError extends boolean = true>(options: Options<UpdateOrganizationSsoProviderData, ThrowOnError>): RequestResult<UpdateOrganizationSsoProviderResponses, UpdateOrganizationSsoProviderErrors, ThrowOnError> => (options.client ?? client).put<UpdateOrganizationSsoProviderResponses, UpdateOrganizationSsoProviderErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/providers/{provider_key}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listOrganizationSsoDomains = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationSsoDomainsData, ThrowOnError>): RequestResult<ListOrganizationSsoDomainsResponses, ListOrganizationSsoDomainsErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationSsoDomainsResponses, ListOrganizationSsoDomainsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/domains',
    ...options
});
export const createOrganizationSsoDomain = <ThrowOnError extends boolean = true>(options: Options<CreateOrganizationSsoDomainData, ThrowOnError>): RequestResult<CreateOrganizationSsoDomainResponses, CreateOrganizationSsoDomainErrors, ThrowOnError> => (options.client ?? client).post<CreateOrganizationSsoDomainResponses, CreateOrganizationSsoDomainErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/domains',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const verifyOrganizationSsoDomain = <ThrowOnError extends boolean = true>(options: Options<VerifyOrganizationSsoDomainData, ThrowOnError>): RequestResult<VerifyOrganizationSsoDomainResponses, VerifyOrganizationSsoDomainErrors, ThrowOnError> => (options.client ?? client).post<VerifyOrganizationSsoDomainResponses, VerifyOrganizationSsoDomainErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/domains/{domain_id}/verify',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const startOrganizationSsoLogin = <ThrowOnError extends boolean = true>(options: Options<StartOrganizationSsoLoginData, ThrowOnError>): RequestResult<StartOrganizationSsoLoginResponses, StartOrganizationSsoLoginErrors, ThrowOnError> => (options.client ?? client).post<StartOrganizationSsoLoginResponses, StartOrganizationSsoLoginErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/login/start',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const bindOrganizationSsoIdentity = <ThrowOnError extends boolean = true>(options: Options<BindOrganizationSsoIdentityData, ThrowOnError>): RequestResult<BindOrganizationSsoIdentityResponses, BindOrganizationSsoIdentityErrors, ThrowOnError> => (options.client ?? client).post<BindOrganizationSsoIdentityResponses, BindOrganizationSsoIdentityErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/identities/bind',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const getOrganizationSsoPolicy = <ThrowOnError extends boolean = true>(options: Options<GetOrganizationSsoPolicyData, ThrowOnError>): RequestResult<GetOrganizationSsoPolicyResponses, GetOrganizationSsoPolicyErrors, ThrowOnError> => (options.client ?? client).get<GetOrganizationSsoPolicyResponses, GetOrganizationSsoPolicyErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/policy',
    ...options
});
export const updateOrganizationSsoPolicy = <ThrowOnError extends boolean = true>(options: Options<UpdateOrganizationSsoPolicyData, ThrowOnError>): RequestResult<UpdateOrganizationSsoPolicyResponses, UpdateOrganizationSsoPolicyErrors, ThrowOnError> => (options.client ?? client).put<UpdateOrganizationSsoPolicyResponses, UpdateOrganizationSsoPolicyErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/policy',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const establishOrganizationSsoRecovery = <ThrowOnError extends boolean = true>(options: Options<EstablishOrganizationSsoRecoveryData, ThrowOnError>): RequestResult<EstablishOrganizationSsoRecoveryResponses, EstablishOrganizationSsoRecoveryErrors, ThrowOnError> => (options.client ?? client).post<EstablishOrganizationSsoRecoveryResponses, EstablishOrganizationSsoRecoveryErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/recovery/establish',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const suspendOrganizationSsoRequired = <ThrowOnError extends boolean = true>(options: Options<SuspendOrganizationSsoRequiredData, ThrowOnError>): RequestResult<SuspendOrganizationSsoRequiredResponses, SuspendOrganizationSsoRequiredErrors, ThrowOnError> => (options.client ?? client).post<SuspendOrganizationSsoRequiredResponses, SuspendOrganizationSsoRequiredErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/sso/recovery/suspend-required',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const completeOrganizationSsoOidcLogin = <ThrowOnError extends boolean = true>(options: Options<CompleteOrganizationSsoOidcLoginData, ThrowOnError>): RequestResult<CompleteOrganizationSsoOidcLoginResponses, CompleteOrganizationSsoOidcLoginErrors, ThrowOnError> => (options.client ?? client).get<CompleteOrganizationSsoOidcLoginResponses, CompleteOrganizationSsoOidcLoginErrors, ThrowOnError>({ url: '/api/auth/organization-sso/oidc/callback', ...options });
export const completeOrganizationSsoSamlLogin = <ThrowOnError extends boolean = true>(options: Options<CompleteOrganizationSsoSamlLoginData, ThrowOnError>): RequestResult<unknown, CompleteOrganizationSsoSamlLoginErrors, ThrowOnError> => (options.client ?? client).post<unknown, CompleteOrganizationSsoSamlLoginErrors, ThrowOnError>({
    bodySerializer: null,
    url: '/api/auth/organization-sso/saml/callback',
    ...options,
    headers: {
        'Content-Type': 'text/plain',
        ...options.headers
    }
});
export const exchangeOrganizationSsoSession = <ThrowOnError extends boolean = true>(options: Options<ExchangeOrganizationSsoSessionData, ThrowOnError>): RequestResult<ExchangeOrganizationSsoSessionResponses, ExchangeOrganizationSsoSessionErrors, ThrowOnError> => (options.client ?? client).post<ExchangeOrganizationSsoSessionResponses, ExchangeOrganizationSsoSessionErrors, ThrowOnError>({
    url: '/api/auth/organization-sso/session/exchange',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listOrganizationTokens = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationTokensData, ThrowOnError>): RequestResult<ListOrganizationTokensResponses, ListOrganizationTokensErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationTokensResponses, ListOrganizationTokensErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/tokens',
    ...options
});
export const createOrganizationToken = <ThrowOnError extends boolean = true>(options: Options<CreateOrganizationTokenData, ThrowOnError>): RequestResult<CreateOrganizationTokenResponses, CreateOrganizationTokenErrors, ThrowOnError> => (options.client ?? client).post<CreateOrganizationTokenResponses, CreateOrganizationTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/tokens',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const listOrganizationTokenDirectory = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationTokenDirectoryData, ThrowOnError>): RequestResult<ListOrganizationTokenDirectoryResponses, ListOrganizationTokenDirectoryErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationTokenDirectoryResponses, ListOrganizationTokenDirectoryErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/tokens/directory',
    ...options
});
export const disableOwnOrganizationToken = <ThrowOnError extends boolean = true>(options: Options<DisableOwnOrganizationTokenData, ThrowOnError>): RequestResult<DisableOwnOrganizationTokenResponses, DisableOwnOrganizationTokenErrors, ThrowOnError> => (options.client ?? client).delete<DisableOwnOrganizationTokenResponses, DisableOwnOrganizationTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/tokens/{token_id}',
    ...options
});
export const getOrganizationToken = <ThrowOnError extends boolean = true>(options: Options<GetOrganizationTokenData, ThrowOnError>): RequestResult<GetOrganizationTokenResponses, GetOrganizationTokenErrors, ThrowOnError> => (options.client ?? client).get<GetOrganizationTokenResponses, GetOrganizationTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/tokens/{token_id}',
    ...options
});
export const updateOrganizationToken = <ThrowOnError extends boolean = true>(options: Options<UpdateOrganizationTokenData, ThrowOnError>): RequestResult<UpdateOrganizationTokenResponses, UpdateOrganizationTokenErrors, ThrowOnError> => (options.client ?? client).put<UpdateOrganizationTokenResponses, UpdateOrganizationTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/tokens/{token_id}',
    ...options,
    headers: {
        'Content-Type': 'application/json',
        ...options.headers
    }
});
export const disableOrganizationToken = <ThrowOnError extends boolean = true>(options: Options<DisableOrganizationTokenData, ThrowOnError>): RequestResult<DisableOrganizationTokenResponses, DisableOrganizationTokenErrors, ThrowOnError> => (options.client ?? client).post<DisableOrganizationTokenResponses, DisableOrganizationTokenErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/tokens/{token_id}/disable',
    ...options
});
export const listOrganizationUsageLogs = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationUsageLogsData, ThrowOnError>): RequestResult<ListOrganizationUsageLogsResponses, ListOrganizationUsageLogsErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationUsageLogsResponses, ListOrganizationUsageLogsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/usage-logs',
    ...options
});
export const listOrganizationAuditLogs = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationAuditLogsData, ThrowOnError>): RequestResult<ListOrganizationAuditLogsResponses, ListOrganizationAuditLogsErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationAuditLogsResponses, ListOrganizationAuditLogsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/audit-logs',
    ...options
});
export const listPlatformOrganizationUsageLogs = <ThrowOnError extends boolean = true>(options: Options<ListPlatformOrganizationUsageLogsData, ThrowOnError>): RequestResult<ListPlatformOrganizationUsageLogsResponses, ListPlatformOrganizationUsageLogsErrors, ThrowOnError> => (options.client ?? client).get<ListPlatformOrganizationUsageLogsResponses, ListPlatformOrganizationUsageLogsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/platform/organizations/{organization_id}/usage-logs',
    ...options
});
export const listPlatformOrganizationAuditLogs = <ThrowOnError extends boolean = true>(options: Options<ListPlatformOrganizationAuditLogsData, ThrowOnError>): RequestResult<ListPlatformOrganizationAuditLogsResponses, ListPlatformOrganizationAuditLogsErrors, ThrowOnError> => (options.client ?? client).get<ListPlatformOrganizationAuditLogsResponses, ListPlatformOrganizationAuditLogsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/platform/organizations/{organization_id}/audit-logs',
    ...options
});
export const listOrganizationScimAuditLogs = <ThrowOnError extends boolean = true>(options: Options<ListOrganizationScimAuditLogsData, ThrowOnError>): RequestResult<ListOrganizationScimAuditLogsResponses, ListOrganizationScimAuditLogsErrors, ThrowOnError> => (options.client ?? client).get<ListOrganizationScimAuditLogsResponses, ListOrganizationScimAuditLogsErrors, ThrowOnError>({
    security: [{
            key: 'bearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/scim/audit-logs',
    ...options
});
export const authenticateOrganizationServiceAccountRuntime = <ThrowOnError extends boolean = true>(options: Options<AuthenticateOrganizationServiceAccountRuntimeData, ThrowOnError>): RequestResult<AuthenticateOrganizationServiceAccountRuntimeResponses, AuthenticateOrganizationServiceAccountRuntimeErrors, ThrowOnError> => (options.client ?? client).post<AuthenticateOrganizationServiceAccountRuntimeResponses, AuthenticateOrganizationServiceAccountRuntimeErrors, ThrowOnError>({
    security: [{
            key: 'serviceAccountBearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/api/organizations/{organization_id}/service-accounts/runtime/authenticate',
    ...options
});
export const getScimServiceProviderConfig = <ThrowOnError extends boolean = true>(options: Options<GetScimServiceProviderConfigData, ThrowOnError>): RequestResult<GetScimServiceProviderConfigResponses, GetScimServiceProviderConfigErrors, ThrowOnError> => (options.client ?? client).get<GetScimServiceProviderConfigResponses, GetScimServiceProviderConfigErrors, ThrowOnError>({
    security: [{
            key: 'scimBearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/scim/v2/{organization_id}/ServiceProviderConfig',
    ...options
});
export const listScimResourceTypes = <ThrowOnError extends boolean = true>(options: Options<ListScimResourceTypesData, ThrowOnError>): RequestResult<ListScimResourceTypesResponses, ListScimResourceTypesErrors, ThrowOnError> => (options.client ?? client).get<ListScimResourceTypesResponses, ListScimResourceTypesErrors, ThrowOnError>({
    security: [{
            key: 'scimBearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/scim/v2/{organization_id}/ResourceTypes',
    ...options
});
export const createScimUser = <ThrowOnError extends boolean = true>(options: Options<CreateScimUserData, ThrowOnError>): RequestResult<CreateScimUserResponses, CreateScimUserErrors, ThrowOnError> => (options.client ?? client).post<CreateScimUserResponses, CreateScimUserErrors, ThrowOnError>({
    security: [{
            key: 'scimBearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/scim/v2/{organization_id}/Users',
    ...options,
    headers: {
        'Content-Type': 'application/scim+json',
        ...options.headers
    }
});
export const replaceScimUser = <ThrowOnError extends boolean = true>(options: Options<ReplaceScimUserData, ThrowOnError>): RequestResult<ReplaceScimUserResponses, ReplaceScimUserErrors, ThrowOnError> => (options.client ?? client).put<ReplaceScimUserResponses, ReplaceScimUserErrors, ThrowOnError>({
    security: [{
            key: 'scimBearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/scim/v2/{organization_id}/Users/{resource_id}',
    ...options,
    headers: {
        'Content-Type': 'application/scim+json',
        ...options.headers
    }
});
export const createScimGroup = <ThrowOnError extends boolean = true>(options: Options<CreateScimGroupData, ThrowOnError>): RequestResult<CreateScimGroupResponses, CreateScimGroupErrors, ThrowOnError> => (options.client ?? client).post<CreateScimGroupResponses, CreateScimGroupErrors, ThrowOnError>({
    security: [{
            key: 'scimBearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/scim/v2/{organization_id}/Groups',
    ...options,
    headers: {
        'Content-Type': 'application/scim+json',
        ...options.headers
    }
});
export const deleteScimGroup = <ThrowOnError extends boolean = true>(options: Options<DeleteScimGroupData, ThrowOnError>): RequestResult<DeleteScimGroupResponses, DeleteScimGroupErrors, ThrowOnError> => (options.client ?? client).delete<DeleteScimGroupResponses, DeleteScimGroupErrors, ThrowOnError>({
    security: [{
            key: 'scimBearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/scim/v2/{organization_id}/Groups/{resource_id}',
    ...options
});
export const replaceScimGroup = <ThrowOnError extends boolean = true>(options: Options<ReplaceScimGroupData, ThrowOnError>): RequestResult<ReplaceScimGroupResponses, ReplaceScimGroupErrors, ThrowOnError> => (options.client ?? client).put<ReplaceScimGroupResponses, ReplaceScimGroupErrors, ThrowOnError>({
    security: [{
            key: 'scimBearerAuth',
            scheme: 'bearer',
            type: 'http'
        }],
    url: '/scim/v2/{organization_id}/Groups/{resource_id}',
    ...options,
    headers: {
        'Content-Type': 'application/scim+json',
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
