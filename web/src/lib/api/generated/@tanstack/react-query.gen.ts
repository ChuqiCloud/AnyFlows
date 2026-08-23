// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import { type DefaultError, type InfiniteData, infiniteQueryOptions, queryOptions, type UseMutationOptions } from '@tanstack/react-query';
import { client } from '../client.gen';
import { acceptOrganizationInvitation, adjustAdminWallet, adjustOrganizationWallet, applyAdminModelPrices, applyAdminModelSyncPreview, approveAdminRefund, beginAdminOAuthAuthorization, bindAdminUserSubscription, changeOrganizationMember, changeOrganizationTeam, changeUserPassword, compactResponse, completeAdminOAuthManualCallback, completeCustomOAuth2Login, completeDiscordOAuthLogin, completeGitHubOAuthLogin, completeGoogleLogin, completeLinuxDoLogin, completeOidcLogin, completeTelegramLogin, completeWeChatOAuthLogin, confirmPasswordReset, createAdminChannel, createAdminCredential, createAdminCredentialProxy, createAdminGroup, createAdminModel, createAdminModelSyncPreview, createAdminRedemptionBatch, createAdminRoute, createAdminSubscriptionPlan, createAdminToken, createAdminUser, createCurrentSubscriptionOrder, createOrganizationBudgetPolicy, createOrganizationCustomRole, createOrganizationInvitation, createOrganizationInvitationBatch, createOrganizationTeam, createOrganizationToken, createOrganizationTopupOrder, createPlaygroundShare, createPlaygroundToken, createUserToken, createUserTopupOrder, deleteAdminChannel, deleteAdminCredential, deleteAdminCredentialProxy, deleteAdminGroup, deleteAdminModel, deleteAdminRoute, deleteAdminToken, deleteAdminUser, deletePlaygroundConversation, deleteUserToken, disableAdminRedemptionBatch, disableAdminSubscriptionPlan, disableOrganizationCustomRole, disableOrganizationTeam, disableOrganizationToken, disableOwnOrganizationToken, disableUserTwoFactor, enableUserTwoFactor, exchangeOAuthLoginTicket, finishPasskeyAuthentication, finishUserPasskeyRegistration, getAdminAuthenticationSettings, getAdminBalanceAlertSettings, getAdminChannel, getAdminCredential, getAdminCredentialProxy, getAdminCustomOAuth2Provider, getAdminDashboard, getAdminDebugTrace, getAdminDebugTraceSettings, getAdminDiscordOAuthLoginSettings, getAdminEmailSettings, getAdminGitHubOAuthLoginSettings, getAdminGoogleOAuthLoginSettings, getAdminGroup, getAdminLinuxDoLoginSettings, getAdminModel, getAdminNetworkSettings, getAdminOidcLoginSettings, getAdminPaymentSettings, getAdminRoute, getAdminSiteSettings, getAdminTelegramOAuthLoginSettings, getAdminToken, getAdminUser, getAdminWeChatOAuthLoginSettings, getCurrentSubscriptionOrder, getInitialSetupStatus, getManagementSession, getOrganizationBudgetReservation, getOrganizationBudgets, getOrganizationCustomRole, getOrganizationOverview, getOrganizationToken, getOrganizationWallet, getPlaygroundConversation, getPlaygroundShare, getPublicSiteSettings, getRegistrationStatus, getUserInvitations, getUserProfile, getUserToken, getUserTopupConfiguration, getUserTwoFactor, getUserWallet, importMissingAdminModels, initializeAdminSetup, listAccountOrganizations, listAdminChannels, listAdminCredentialProxies, listAdminCredentials, listAdminCustomOAuth2Providers, listAdminDebugTraces, listAdminGroups, listAdminModelPrices, listAdminModels, listAdminOAuthProviders, listAdminPlatformAuditLogs, listAdminRedemptionAudit, listAdminRedemptionBatches, listAdminRefunds, listAdminRoutes, listAdminSubscriptionPlans, listAdminTokens, listAdminUsageLogs, listAdminUsers, listAdminUserSubscriptions, listAdminWalletEntries, listCurrentSubscriptionCatalog, listCurrentUserSubscriptions, listMissingAdminModels, listModelProviders, listModels, listOrganizationAuditLogs, listOrganizationCustomRoles, listOrganizationInvitations, listOrganizationMembers, listOrganizationTeams, listOrganizationTokenDirectory, listOrganizationTokens, listOrganizationUsageLogs, listOrganizationWalletEntries, listPlatformOrganizationAuditLogs, listPlatformOrganizationUsageLogs, listPlaygroundConversations, listSelfPlatformAuditLogs, listUserNotifications, listUserPasskeys, listUserTokens, listUserUsageLogs, listUserWalletEntries, listVideoTasks, loginManagementSession, markUserNotificationsRead, type Options, pollVideoTask, previewAdminLiteLlmModelPrices, previewAdminModelPriceExpression, previewAdminModelPrices, previewOrganizationInvitations, probeAdminChannel, readAdminDebugTraceSnapshots, receivePaymentWebhook, receiveRefundWebhook, redeemUserRedemptionCode, registerUser, rejectAdminRefund, renameUserPasskey, requestPasswordReset, rerank, resendOrganizationInvitation, revokeOrganizationInvitation, revokePlaygroundShare, revokeUserPasskey, savePlaygroundConversation, sendAdminEmailTest, sendRegistrationEmailVerification, startCustomOAuth2Login, startDiscordOAuthLogin, startGitHubOAuthLogin, startGoogleLogin, startLinuxDoLogin, startOidcLogin, startPasskeyAuthentication, startTelegramLogin, startUserPasskeyRegistration, startWeChatOAuthLogin, submitAdminRefund, submitCurrentSubscriptionOrderPayment, submitVideoTask, synthesizeSpeech, transferOrganizationOwnership, transitionAdminUserSubscriptionLifecycle, updateAdminAuthenticationSettings, updateAdminBalanceAlertSettings, updateAdminChannel, updateAdminCredential, updateAdminCredentialProxy, updateAdminCustomOAuth2Provider, updateAdminDebugTraceSettings, updateAdminDiscordOAuthLoginSettings, updateAdminEmailSettings, updateAdminGitHubOAuthLoginSettings, updateAdminGoogleOAuthLoginSettings, updateAdminGroup, updateAdminLinuxDoLoginSettings, updateAdminModel, updateAdminNetworkSettings, updateAdminOidcLoginSettings, updateAdminPaymentSettings, updateAdminRoute, updateAdminSiteSettings, updateAdminTelegramOAuthLoginSettings, updateAdminToken, updateAdminUser, updateAdminWeChatOAuthLoginSettings, updateOrganizationCustomRole, updateOrganizationToken, updateUserNotificationPreferences, updateUserProfile, updateUserToken } from '../sdk.gen';
import type { AcceptOrganizationInvitationData, AcceptOrganizationInvitationError, AcceptOrganizationInvitationResponse, AdjustAdminWalletData, AdjustAdminWalletError, AdjustAdminWalletResponse, AdjustOrganizationWalletData, AdjustOrganizationWalletError, AdjustOrganizationWalletResponse, ApplyAdminModelPricesData, ApplyAdminModelPricesError, ApplyAdminModelPricesResponse, ApplyAdminModelSyncPreviewData, ApplyAdminModelSyncPreviewError, ApplyAdminModelSyncPreviewResponse, ApproveAdminRefundData, ApproveAdminRefundError, ApproveAdminRefundResponse, BeginAdminOAuthAuthorizationData, BeginAdminOAuthAuthorizationError, BeginAdminOAuthAuthorizationResponse, BindAdminUserSubscriptionData, BindAdminUserSubscriptionError, BindAdminUserSubscriptionResponse, ChangeOrganizationMemberData, ChangeOrganizationMemberError, ChangeOrganizationMemberResponse, ChangeOrganizationTeamData, ChangeOrganizationTeamError, ChangeOrganizationTeamResponse, ChangeUserPasswordData, ChangeUserPasswordError, ChangeUserPasswordResponse, CompactResponseData, CompactResponseError, CompactResponseResponse, CompleteAdminOAuthManualCallbackData, CompleteAdminOAuthManualCallbackError, CompleteAdminOAuthManualCallbackResponse, CompleteCustomOAuth2LoginData, CompleteCustomOAuth2LoginError, CompleteDiscordOAuthLoginData, CompleteDiscordOAuthLoginError, CompleteGitHubOAuthLoginData, CompleteGitHubOAuthLoginError, CompleteGoogleLoginData, CompleteGoogleLoginError, CompleteLinuxDoLoginData, CompleteLinuxDoLoginError, CompleteOidcLoginData, CompleteOidcLoginError, CompleteTelegramLoginData, CompleteTelegramLoginError, CompleteWeChatOAuthLoginData, CompleteWeChatOAuthLoginError, ConfirmPasswordResetData, ConfirmPasswordResetError, ConfirmPasswordResetResponse, CreateAdminChannelData, CreateAdminChannelError, CreateAdminChannelResponse, CreateAdminCredentialData, CreateAdminCredentialError, CreateAdminCredentialProxyData, CreateAdminCredentialProxyError, CreateAdminCredentialProxyResponse, CreateAdminCredentialResponse, CreateAdminGroupData, CreateAdminGroupError, CreateAdminGroupResponse, CreateAdminModelData, CreateAdminModelError, CreateAdminModelResponse, CreateAdminModelSyncPreviewData, CreateAdminModelSyncPreviewError, CreateAdminModelSyncPreviewResponse, CreateAdminRedemptionBatchData, CreateAdminRedemptionBatchError, CreateAdminRedemptionBatchResponse, CreateAdminRouteData, CreateAdminRouteError, CreateAdminRouteResponse, CreateAdminSubscriptionPlanData, CreateAdminSubscriptionPlanError, CreateAdminSubscriptionPlanResponse, CreateAdminTokenData, CreateAdminTokenError, CreateAdminTokenResponse, CreateAdminUserData, CreateAdminUserError, CreateAdminUserResponse, CreateCurrentSubscriptionOrderData, CreateCurrentSubscriptionOrderError, CreateCurrentSubscriptionOrderResponse, CreateOrganizationBudgetPolicyData, CreateOrganizationBudgetPolicyError, CreateOrganizationBudgetPolicyResponse, CreateOrganizationCustomRoleData, CreateOrganizationCustomRoleError, CreateOrganizationCustomRoleResponse, CreateOrganizationInvitationBatchData, CreateOrganizationInvitationBatchError, CreateOrganizationInvitationBatchResponse, CreateOrganizationInvitationData, CreateOrganizationInvitationError, CreateOrganizationInvitationResponse, CreateOrganizationTeamData, CreateOrganizationTeamError, CreateOrganizationTeamResponse, CreateOrganizationTokenData, CreateOrganizationTokenError, CreateOrganizationTokenResponse, CreateOrganizationTopupOrderData, CreateOrganizationTopupOrderError, CreateOrganizationTopupOrderResponse, CreatePlaygroundShareData, CreatePlaygroundShareError, CreatePlaygroundShareResponse, CreatePlaygroundTokenData, CreatePlaygroundTokenError, CreatePlaygroundTokenResponse, CreateUserTokenData, CreateUserTokenError, CreateUserTokenResponse, CreateUserTopupOrderData, CreateUserTopupOrderError, CreateUserTopupOrderResponse, DeleteAdminChannelData, DeleteAdminChannelError, DeleteAdminChannelResponse, DeleteAdminCredentialData, DeleteAdminCredentialError, DeleteAdminCredentialProxyData, DeleteAdminCredentialProxyError, DeleteAdminCredentialProxyResponse, DeleteAdminCredentialResponse, DeleteAdminGroupData, DeleteAdminGroupError, DeleteAdminGroupResponse, DeleteAdminModelData, DeleteAdminModelError, DeleteAdminModelResponse, DeleteAdminRouteData, DeleteAdminRouteError, DeleteAdminRouteResponse, DeleteAdminTokenData, DeleteAdminTokenError, DeleteAdminTokenResponse, DeleteAdminUserData, DeleteAdminUserError, DeleteAdminUserResponse, DeletePlaygroundConversationData, DeletePlaygroundConversationError, DeletePlaygroundConversationResponse, DeleteUserTokenData, DeleteUserTokenError, DeleteUserTokenResponse, DisableAdminRedemptionBatchData, DisableAdminRedemptionBatchError, DisableAdminRedemptionBatchResponse, DisableAdminSubscriptionPlanData, DisableAdminSubscriptionPlanError, DisableAdminSubscriptionPlanResponse, DisableOrganizationCustomRoleData, DisableOrganizationCustomRoleError, DisableOrganizationCustomRoleResponse, DisableOrganizationTeamData, DisableOrganizationTeamError, DisableOrganizationTeamResponse, DisableOrganizationTokenData, DisableOrganizationTokenError, DisableOrganizationTokenResponse, DisableOwnOrganizationTokenData, DisableOwnOrganizationTokenError, DisableOwnOrganizationTokenResponse, DisableUserTwoFactorData, DisableUserTwoFactorError, DisableUserTwoFactorResponse, EnableUserTwoFactorData, EnableUserTwoFactorError, EnableUserTwoFactorResponse, ExchangeOAuthLoginTicketData, ExchangeOAuthLoginTicketError, ExchangeOAuthLoginTicketResponse, FinishPasskeyAuthenticationData, FinishPasskeyAuthenticationError, FinishPasskeyAuthenticationResponse, FinishUserPasskeyRegistrationData, FinishUserPasskeyRegistrationError, FinishUserPasskeyRegistrationResponse, GetAdminAuthenticationSettingsData, GetAdminAuthenticationSettingsError, GetAdminAuthenticationSettingsResponse, GetAdminBalanceAlertSettingsData, GetAdminBalanceAlertSettingsError, GetAdminBalanceAlertSettingsResponse, GetAdminChannelData, GetAdminChannelError, GetAdminChannelResponse, GetAdminCredentialData, GetAdminCredentialError, GetAdminCredentialProxyData, GetAdminCredentialProxyError, GetAdminCredentialProxyResponse, GetAdminCredentialResponse, GetAdminCustomOAuth2ProviderData, GetAdminCustomOAuth2ProviderError, GetAdminCustomOAuth2ProviderResponse, GetAdminDashboardData, GetAdminDashboardError, GetAdminDashboardResponse, GetAdminDebugTraceData, GetAdminDebugTraceError, GetAdminDebugTraceResponse, GetAdminDebugTraceSettingsData, GetAdminDebugTraceSettingsError, GetAdminDebugTraceSettingsResponse, GetAdminDiscordOAuthLoginSettingsData, GetAdminDiscordOAuthLoginSettingsError, GetAdminDiscordOAuthLoginSettingsResponse, GetAdminEmailSettingsData, GetAdminEmailSettingsError, GetAdminEmailSettingsResponse, GetAdminGitHubOAuthLoginSettingsData, GetAdminGitHubOAuthLoginSettingsError, GetAdminGitHubOAuthLoginSettingsResponse, GetAdminGoogleOAuthLoginSettingsData, GetAdminGoogleOAuthLoginSettingsError, GetAdminGoogleOAuthLoginSettingsResponse, GetAdminGroupData, GetAdminGroupError, GetAdminGroupResponse, GetAdminLinuxDoLoginSettingsData, GetAdminLinuxDoLoginSettingsError, GetAdminLinuxDoLoginSettingsResponse, GetAdminModelData, GetAdminModelError, GetAdminModelResponse, GetAdminNetworkSettingsData, GetAdminNetworkSettingsError, GetAdminNetworkSettingsResponse, GetAdminOidcLoginSettingsData, GetAdminOidcLoginSettingsError, GetAdminOidcLoginSettingsResponse, GetAdminPaymentSettingsData, GetAdminPaymentSettingsError, GetAdminPaymentSettingsResponse, GetAdminRouteData, GetAdminRouteError, GetAdminRouteResponse, GetAdminSiteSettingsData, GetAdminSiteSettingsError, GetAdminSiteSettingsResponse, GetAdminTelegramOAuthLoginSettingsData, GetAdminTelegramOAuthLoginSettingsError, GetAdminTelegramOAuthLoginSettingsResponse, GetAdminTokenData, GetAdminTokenError, GetAdminTokenResponse, GetAdminUserData, GetAdminUserError, GetAdminUserResponse, GetAdminWeChatOAuthLoginSettingsData, GetAdminWeChatOAuthLoginSettingsError, GetAdminWeChatOAuthLoginSettingsResponse, GetCurrentSubscriptionOrderData, GetCurrentSubscriptionOrderError, GetCurrentSubscriptionOrderResponse, GetInitialSetupStatusData, GetInitialSetupStatusError, GetInitialSetupStatusResponse, GetManagementSessionData, GetManagementSessionError, GetManagementSessionResponse, GetOrganizationBudgetReservationData, GetOrganizationBudgetReservationError, GetOrganizationBudgetReservationResponse, GetOrganizationBudgetsData, GetOrganizationBudgetsError, GetOrganizationBudgetsResponse, GetOrganizationCustomRoleData, GetOrganizationCustomRoleError, GetOrganizationCustomRoleResponse, GetOrganizationOverviewData, GetOrganizationOverviewError, GetOrganizationOverviewResponse, GetOrganizationTokenData, GetOrganizationTokenError, GetOrganizationTokenResponse, GetOrganizationWalletData, GetOrganizationWalletError, GetOrganizationWalletResponse, GetPlaygroundConversationData, GetPlaygroundConversationError, GetPlaygroundConversationResponse, GetPlaygroundShareData, GetPlaygroundShareError, GetPlaygroundShareResponse, GetPublicSiteSettingsData, GetPublicSiteSettingsError, GetPublicSiteSettingsResponse, GetRegistrationStatusData, GetRegistrationStatusError, GetRegistrationStatusResponse, GetUserInvitationsData, GetUserInvitationsError, GetUserInvitationsResponse, GetUserProfileData, GetUserProfileError, GetUserProfileResponse, GetUserTokenData, GetUserTokenError, GetUserTokenResponse, GetUserTopupConfigurationData, GetUserTopupConfigurationError, GetUserTopupConfigurationResponse, GetUserTwoFactorData, GetUserTwoFactorError, GetUserTwoFactorResponse, GetUserWalletData, GetUserWalletError, GetUserWalletResponse, ImportMissingAdminModelsData, ImportMissingAdminModelsError, ImportMissingAdminModelsResponse, InitializeAdminSetupData, InitializeAdminSetupError, InitializeAdminSetupResponse, ListAccountOrganizationsData, ListAccountOrganizationsError, ListAccountOrganizationsResponse, ListAdminChannelsData, ListAdminChannelsError, ListAdminChannelsResponse, ListAdminCredentialProxiesData, ListAdminCredentialProxiesError, ListAdminCredentialProxiesResponse, ListAdminCredentialsData, ListAdminCredentialsError, ListAdminCredentialsResponse, ListAdminCustomOAuth2ProvidersData, ListAdminCustomOAuth2ProvidersError, ListAdminCustomOAuth2ProvidersResponse, ListAdminDebugTracesData, ListAdminDebugTracesError, ListAdminDebugTracesResponse, ListAdminGroupsData, ListAdminGroupsError, ListAdminGroupsResponse, ListAdminModelPricesData, ListAdminModelPricesError, ListAdminModelPricesResponse, ListAdminModelsData, ListAdminModelsError, ListAdminModelsResponse, ListAdminOAuthProvidersData, ListAdminOAuthProvidersError, ListAdminOAuthProvidersResponse, ListAdminPlatformAuditLogsData, ListAdminPlatformAuditLogsError, ListAdminPlatformAuditLogsResponse, ListAdminRedemptionAuditData, ListAdminRedemptionAuditError, ListAdminRedemptionAuditResponse, ListAdminRedemptionBatchesData, ListAdminRedemptionBatchesError, ListAdminRedemptionBatchesResponse, ListAdminRefundsData, ListAdminRefundsError, ListAdminRefundsResponse, ListAdminRoutesData, ListAdminRoutesError, ListAdminRoutesResponse, ListAdminSubscriptionPlansData, ListAdminSubscriptionPlansError, ListAdminSubscriptionPlansResponse, ListAdminTokensData, ListAdminTokensError, ListAdminTokensResponse, ListAdminUsageLogsData, ListAdminUsageLogsError, ListAdminUsageLogsResponse, ListAdminUsersData, ListAdminUsersError, ListAdminUsersResponse, ListAdminUserSubscriptionsData, ListAdminUserSubscriptionsError, ListAdminUserSubscriptionsResponse, ListAdminWalletEntriesData, ListAdminWalletEntriesError, ListAdminWalletEntriesResponse, ListCurrentSubscriptionCatalogData, ListCurrentSubscriptionCatalogError, ListCurrentSubscriptionCatalogResponse, ListCurrentUserSubscriptionsData, ListCurrentUserSubscriptionsError, ListCurrentUserSubscriptionsResponse, ListMissingAdminModelsData, ListMissingAdminModelsError, ListMissingAdminModelsResponse, ListModelProvidersData, ListModelProvidersError, ListModelProvidersResponse, ListModelsData, ListModelsError, ListModelsResponse, ListOrganizationAuditLogsData, ListOrganizationAuditLogsError, ListOrganizationAuditLogsResponse, ListOrganizationCustomRolesData, ListOrganizationCustomRolesError, ListOrganizationCustomRolesResponse, ListOrganizationInvitationsData, ListOrganizationInvitationsError, ListOrganizationInvitationsResponse, ListOrganizationMembersData, ListOrganizationMembersError, ListOrganizationMembersResponse, ListOrganizationTeamsData, ListOrganizationTeamsError, ListOrganizationTeamsResponse, ListOrganizationTokenDirectoryData, ListOrganizationTokenDirectoryError, ListOrganizationTokenDirectoryResponse, ListOrganizationTokensData, ListOrganizationTokensError, ListOrganizationTokensResponse, ListOrganizationUsageLogsData, ListOrganizationUsageLogsError, ListOrganizationUsageLogsResponse, ListOrganizationWalletEntriesData, ListOrganizationWalletEntriesError, ListOrganizationWalletEntriesResponse, ListPlatformOrganizationAuditLogsData, ListPlatformOrganizationAuditLogsError, ListPlatformOrganizationAuditLogsResponse, ListPlatformOrganizationUsageLogsData, ListPlatformOrganizationUsageLogsError, ListPlatformOrganizationUsageLogsResponse, ListPlaygroundConversationsData, ListPlaygroundConversationsError, ListPlaygroundConversationsResponse, ListSelfPlatformAuditLogsData, ListSelfPlatformAuditLogsError, ListSelfPlatformAuditLogsResponse, ListUserNotificationsData, ListUserNotificationsError, ListUserNotificationsResponse, ListUserPasskeysData, ListUserPasskeysError, ListUserPasskeysResponse, ListUserTokensData, ListUserTokensError, ListUserTokensResponse, ListUserUsageLogsData, ListUserUsageLogsError, ListUserUsageLogsResponse, ListUserWalletEntriesData, ListUserWalletEntriesError, ListUserWalletEntriesResponse, ListVideoTasksData, ListVideoTasksError, ListVideoTasksResponse, LoginManagementSessionData, LoginManagementSessionError, LoginManagementSessionResponse, MarkUserNotificationsReadData, MarkUserNotificationsReadError, MarkUserNotificationsReadResponse, PollVideoTaskData, PollVideoTaskError, PollVideoTaskResponse, PreviewAdminLiteLlmModelPricesData, PreviewAdminLiteLlmModelPricesError, PreviewAdminLiteLlmModelPricesResponse, PreviewAdminModelPriceExpressionData, PreviewAdminModelPriceExpressionError, PreviewAdminModelPriceExpressionResponse, PreviewAdminModelPricesData, PreviewAdminModelPricesError, PreviewAdminModelPricesResponse, PreviewOrganizationInvitationsData, PreviewOrganizationInvitationsError, PreviewOrganizationInvitationsResponse, ProbeAdminChannelData, ProbeAdminChannelError, ProbeAdminChannelResponse, ReadAdminDebugTraceSnapshotsData, ReadAdminDebugTraceSnapshotsError, ReadAdminDebugTraceSnapshotsResponse, ReceivePaymentWebhookData, ReceiveRefundWebhookData, RedeemUserRedemptionCodeData, RedeemUserRedemptionCodeError, RedeemUserRedemptionCodeResponse, RegisterUserData, RegisterUserError, RegisterUserResponse, RejectAdminRefundData, RejectAdminRefundError, RejectAdminRefundResponse, RenameUserPasskeyData, RenameUserPasskeyError, RenameUserPasskeyResponse, RequestPasswordResetData, RequestPasswordResetError, RequestPasswordResetResponse, RerankData, RerankError2, RerankResponse2, ResendOrganizationInvitationData, ResendOrganizationInvitationError, ResendOrganizationInvitationResponse, RevokeOrganizationInvitationData, RevokeOrganizationInvitationError, RevokeOrganizationInvitationResponse, RevokePlaygroundShareData, RevokePlaygroundShareError, RevokePlaygroundShareResponse, RevokeUserPasskeyData, RevokeUserPasskeyError, RevokeUserPasskeyResponse, SavePlaygroundConversationData, SavePlaygroundConversationError, SavePlaygroundConversationResponse, SendAdminEmailTestData, SendAdminEmailTestError, SendAdminEmailTestResponse, SendRegistrationEmailVerificationData, SendRegistrationEmailVerificationError, SendRegistrationEmailVerificationResponse, StartCustomOAuth2LoginData, StartCustomOAuth2LoginError, StartCustomOAuth2LoginResponse, StartDiscordOAuthLoginData, StartDiscordOAuthLoginError, StartDiscordOAuthLoginResponse, StartGitHubOAuthLoginData, StartGitHubOAuthLoginError, StartGitHubOAuthLoginResponse, StartGoogleLoginData, StartGoogleLoginError, StartGoogleLoginResponse, StartLinuxDoLoginData, StartLinuxDoLoginError, StartLinuxDoLoginResponse, StartOidcLoginData, StartOidcLoginError, StartOidcLoginResponse, StartPasskeyAuthenticationData, StartPasskeyAuthenticationError, StartPasskeyAuthenticationResponse, StartTelegramLoginData, StartTelegramLoginError, StartTelegramLoginResponse, StartUserPasskeyRegistrationData, StartUserPasskeyRegistrationError, StartUserPasskeyRegistrationResponse, StartWeChatOAuthLoginData, StartWeChatOAuthLoginError, StartWeChatOAuthLoginResponse, SubmitAdminRefundData, SubmitAdminRefundError, SubmitAdminRefundResponse, SubmitCurrentSubscriptionOrderPaymentData, SubmitCurrentSubscriptionOrderPaymentError, SubmitCurrentSubscriptionOrderPaymentResponse, SubmitVideoTaskData, SubmitVideoTaskError, SubmitVideoTaskResponse, SynthesizeSpeechData, SynthesizeSpeechError, SynthesizeSpeechResponse, TransferOrganizationOwnershipData, TransferOrganizationOwnershipError, TransferOrganizationOwnershipResponse, TransitionAdminUserSubscriptionLifecycleData, TransitionAdminUserSubscriptionLifecycleError, TransitionAdminUserSubscriptionLifecycleResponse, UpdateAdminAuthenticationSettingsData, UpdateAdminAuthenticationSettingsError, UpdateAdminAuthenticationSettingsResponse, UpdateAdminBalanceAlertSettingsData, UpdateAdminBalanceAlertSettingsError, UpdateAdminBalanceAlertSettingsResponse, UpdateAdminChannelData, UpdateAdminChannelError, UpdateAdminChannelResponse, UpdateAdminCredentialData, UpdateAdminCredentialError, UpdateAdminCredentialProxyData, UpdateAdminCredentialProxyError, UpdateAdminCredentialProxyResponse, UpdateAdminCredentialResponse, UpdateAdminCustomOAuth2ProviderData, UpdateAdminCustomOAuth2ProviderError, UpdateAdminCustomOAuth2ProviderResponse, UpdateAdminDebugTraceSettingsData, UpdateAdminDebugTraceSettingsError, UpdateAdminDebugTraceSettingsResponse, UpdateAdminDiscordOAuthLoginSettingsData, UpdateAdminDiscordOAuthLoginSettingsError, UpdateAdminDiscordOAuthLoginSettingsResponse, UpdateAdminEmailSettingsData, UpdateAdminEmailSettingsError, UpdateAdminEmailSettingsResponse, UpdateAdminGitHubOAuthLoginSettingsData, UpdateAdminGitHubOAuthLoginSettingsError, UpdateAdminGitHubOAuthLoginSettingsResponse, UpdateAdminGoogleOAuthLoginSettingsData, UpdateAdminGoogleOAuthLoginSettingsError, UpdateAdminGoogleOAuthLoginSettingsResponse, UpdateAdminGroupData, UpdateAdminGroupError, UpdateAdminGroupResponse, UpdateAdminLinuxDoLoginSettingsData, UpdateAdminLinuxDoLoginSettingsError, UpdateAdminLinuxDoLoginSettingsResponse, UpdateAdminModelData, UpdateAdminModelError, UpdateAdminModelResponse, UpdateAdminNetworkSettingsData, UpdateAdminNetworkSettingsError, UpdateAdminNetworkSettingsResponse, UpdateAdminOidcLoginSettingsData, UpdateAdminOidcLoginSettingsError, UpdateAdminOidcLoginSettingsResponse, UpdateAdminPaymentSettingsData, UpdateAdminPaymentSettingsError, UpdateAdminPaymentSettingsResponse, UpdateAdminRouteData, UpdateAdminRouteError, UpdateAdminRouteResponse, UpdateAdminSiteSettingsData, UpdateAdminSiteSettingsError, UpdateAdminSiteSettingsResponse, UpdateAdminTelegramOAuthLoginSettingsData, UpdateAdminTelegramOAuthLoginSettingsError, UpdateAdminTelegramOAuthLoginSettingsResponse, UpdateAdminTokenData, UpdateAdminTokenError, UpdateAdminTokenResponse, UpdateAdminUserData, UpdateAdminUserError, UpdateAdminUserResponse, UpdateAdminWeChatOAuthLoginSettingsData, UpdateAdminWeChatOAuthLoginSettingsError, UpdateAdminWeChatOAuthLoginSettingsResponse, UpdateOrganizationCustomRoleData, UpdateOrganizationCustomRoleError, UpdateOrganizationCustomRoleResponse, UpdateOrganizationTokenData, UpdateOrganizationTokenError, UpdateOrganizationTokenResponse, UpdateUserNotificationPreferencesData, UpdateUserNotificationPreferencesError, UpdateUserNotificationPreferencesResponse, UpdateUserProfileData, UpdateUserProfileError, UpdateUserProfileResponse, UpdateUserTokenData, UpdateUserTokenError, UpdateUserTokenResponse } from '../types.gen';
export const loginManagementSessionMutation = (options?: Partial<Options<LoginManagementSessionData>>): UseMutationOptions<LoginManagementSessionResponse, LoginManagementSessionError, Options<LoginManagementSessionData>> => {
    const mutationOptions: UseMutationOptions<LoginManagementSessionResponse, LoginManagementSessionError, Options<LoginManagementSessionData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await loginManagementSession({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export type QueryKey<TOptions extends Options> = [
    Pick<TOptions, 'baseUrl' | 'body' | 'headers' | 'path' | 'query'> & {
        _id: string;
        _infinite?: boolean;
        tags?: ReadonlyArray<string>;
    }
];
const createQueryKey = <TOptions extends Options>(id: string, options?: TOptions, infinite?: boolean, tags?: ReadonlyArray<string>): [
    QueryKey<TOptions>[0]
] => {
    const params: QueryKey<TOptions>[0] = { _id: id, baseUrl: options?.baseUrl || (options?.client ?? client).getConfig().baseUrl } as QueryKey<TOptions>[0];
    if (infinite) {
        params._infinite = infinite;
    }
    if (tags) {
        params.tags = tags;
    }
    if (options?.body) {
        params.body = options.body;
    }
    if (options?.headers) {
        params.headers = options.headers;
    }
    if (options?.path) {
        params.path = options.path;
    }
    if (options?.query) {
        params.query = options.query;
    }
    return [params];
};
export const getManagementSessionQueryKey = (options?: Options<GetManagementSessionData>) => createQueryKey('getManagementSession', options);
export const getManagementSessionOptions = (options?: Options<GetManagementSessionData>) => queryOptions<GetManagementSessionResponse, GetManagementSessionError, GetManagementSessionResponse, ReturnType<typeof getManagementSessionQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getManagementSession({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getManagementSessionQueryKey(options)
});
export const getInitialSetupStatusQueryKey = (options?: Options<GetInitialSetupStatusData>) => createQueryKey('getInitialSetupStatus', options);
export const getInitialSetupStatusOptions = (options?: Options<GetInitialSetupStatusData>) => queryOptions<GetInitialSetupStatusResponse, GetInitialSetupStatusError, GetInitialSetupStatusResponse, ReturnType<typeof getInitialSetupStatusQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getInitialSetupStatus({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getInitialSetupStatusQueryKey(options)
});
export const initializeAdminSetupMutation = (options?: Partial<Options<InitializeAdminSetupData>>): UseMutationOptions<InitializeAdminSetupResponse, InitializeAdminSetupError, Options<InitializeAdminSetupData>> => {
    const mutationOptions: UseMutationOptions<InitializeAdminSetupResponse, InitializeAdminSetupError, Options<InitializeAdminSetupData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await initializeAdminSetup({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getRegistrationStatusQueryKey = (options?: Options<GetRegistrationStatusData>) => createQueryKey('getRegistrationStatus', options);
export const getRegistrationStatusOptions = (options?: Options<GetRegistrationStatusData>) => queryOptions<GetRegistrationStatusResponse, GetRegistrationStatusError, GetRegistrationStatusResponse, ReturnType<typeof getRegistrationStatusQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getRegistrationStatus({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getRegistrationStatusQueryKey(options)
});
export const sendRegistrationEmailVerificationMutation = (options?: Partial<Options<SendRegistrationEmailVerificationData>>): UseMutationOptions<SendRegistrationEmailVerificationResponse, SendRegistrationEmailVerificationError, Options<SendRegistrationEmailVerificationData>> => {
    const mutationOptions: UseMutationOptions<SendRegistrationEmailVerificationResponse, SendRegistrationEmailVerificationError, Options<SendRegistrationEmailVerificationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await sendRegistrationEmailVerification({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const registerUserMutation = (options?: Partial<Options<RegisterUserData>>): UseMutationOptions<RegisterUserResponse, RegisterUserError, Options<RegisterUserData>> => {
    const mutationOptions: UseMutationOptions<RegisterUserResponse, RegisterUserError, Options<RegisterUserData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await registerUser({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminAuthenticationSettingsQueryKey = (options?: Options<GetAdminAuthenticationSettingsData>) => createQueryKey('getAdminAuthenticationSettings', options);
export const getAdminAuthenticationSettingsOptions = (options?: Options<GetAdminAuthenticationSettingsData>) => queryOptions<GetAdminAuthenticationSettingsResponse, GetAdminAuthenticationSettingsError, GetAdminAuthenticationSettingsResponse, ReturnType<typeof getAdminAuthenticationSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminAuthenticationSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminAuthenticationSettingsQueryKey(options)
});
export const updateAdminAuthenticationSettingsMutation = (options?: Partial<Options<UpdateAdminAuthenticationSettingsData>>): UseMutationOptions<UpdateAdminAuthenticationSettingsResponse, UpdateAdminAuthenticationSettingsError, Options<UpdateAdminAuthenticationSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminAuthenticationSettingsResponse, UpdateAdminAuthenticationSettingsError, Options<UpdateAdminAuthenticationSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminAuthenticationSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const rerankMutation = (options?: Partial<Options<RerankData>>): UseMutationOptions<RerankResponse2, RerankError2, Options<RerankData>> => {
    const mutationOptions: UseMutationOptions<RerankResponse2, RerankError2, Options<RerankData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await rerank({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const compactResponseMutation = (options?: Partial<Options<CompactResponseData>>): UseMutationOptions<CompactResponseResponse, CompactResponseError, Options<CompactResponseData>> => {
    const mutationOptions: UseMutationOptions<CompactResponseResponse, CompactResponseError, Options<CompactResponseData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await compactResponse({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const synthesizeSpeechMutation = (options?: Partial<Options<SynthesizeSpeechData>>): UseMutationOptions<SynthesizeSpeechResponse, SynthesizeSpeechError, Options<SynthesizeSpeechData>> => {
    const mutationOptions: UseMutationOptions<SynthesizeSpeechResponse, SynthesizeSpeechError, Options<SynthesizeSpeechData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await synthesizeSpeech({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listVideoTasksQueryKey = (options?: Options<ListVideoTasksData>) => createQueryKey('listVideoTasks', options);
export const listVideoTasksOptions = (options?: Options<ListVideoTasksData>) => queryOptions<ListVideoTasksResponse, ListVideoTasksError, ListVideoTasksResponse, ReturnType<typeof listVideoTasksQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listVideoTasks({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listVideoTasksQueryKey(options)
});
const createInfiniteParams = <K extends Pick<QueryKey<Options>[0], 'body' | 'headers' | 'path' | 'query'>>(queryKey: QueryKey<Options>, page: K) => {
    const params = { ...queryKey[0] };
    if (page.body) {
        params.body = {
            ...queryKey[0].body as any,
            ...page.body as any
        };
    }
    if (page.headers) {
        params.headers = {
            ...queryKey[0].headers,
            ...page.headers
        };
    }
    if (page.path) {
        params.path = {
            ...queryKey[0].path as any,
            ...page.path as any
        };
    }
    if (page.query) {
        params.query = {
            ...queryKey[0].query as any,
            ...page.query as any
        };
    }
    return params as unknown as typeof page;
};
export const listVideoTasksInfiniteQueryKey = (options?: Options<ListVideoTasksData>): QueryKey<Options<ListVideoTasksData>> => createQueryKey('listVideoTasks', options, true);
export const listVideoTasksInfiniteOptions = (options?: Options<ListVideoTasksData>) => {
    const opts = infiniteQueryOptions<ListVideoTasksResponse, ListVideoTasksError, InfiniteData<ListVideoTasksResponse>, QueryKey<Options<ListVideoTasksData>>, string | Pick<QueryKey<Options<ListVideoTasksData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListVideoTasksData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listVideoTasks({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listVideoTasksInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const submitVideoTaskMutation = (options?: Partial<Options<SubmitVideoTaskData>>): UseMutationOptions<SubmitVideoTaskResponse, SubmitVideoTaskError, Options<SubmitVideoTaskData>> => {
    const mutationOptions: UseMutationOptions<SubmitVideoTaskResponse, SubmitVideoTaskError, Options<SubmitVideoTaskData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await submitVideoTask({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const pollVideoTaskQueryKey = (options: Options<PollVideoTaskData>) => createQueryKey('pollVideoTask', options);
export const pollVideoTaskOptions = (options: Options<PollVideoTaskData>) => queryOptions<PollVideoTaskResponse, PollVideoTaskError, PollVideoTaskResponse, ReturnType<typeof pollVideoTaskQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await pollVideoTask({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: pollVideoTaskQueryKey(options)
});
export const listAdminRedemptionAuditQueryKey = (options?: Options<ListAdminRedemptionAuditData>) => createQueryKey('listAdminRedemptionAudit', options);
export const listAdminRedemptionAuditOptions = (options?: Options<ListAdminRedemptionAuditData>) => queryOptions<ListAdminRedemptionAuditResponse, ListAdminRedemptionAuditError, ListAdminRedemptionAuditResponse, ReturnType<typeof listAdminRedemptionAuditQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminRedemptionAudit({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminRedemptionAuditQueryKey(options)
});
export const listAdminRedemptionAuditInfiniteQueryKey = (options?: Options<ListAdminRedemptionAuditData>): QueryKey<Options<ListAdminRedemptionAuditData>> => createQueryKey('listAdminRedemptionAudit', options, true);
export const listAdminRedemptionAuditInfiniteOptions = (options?: Options<ListAdminRedemptionAuditData>) => {
    const opts = infiniteQueryOptions<ListAdminRedemptionAuditResponse, ListAdminRedemptionAuditError, InfiniteData<ListAdminRedemptionAuditResponse>, QueryKey<Options<ListAdminRedemptionAuditData>>, number | Pick<QueryKey<Options<ListAdminRedemptionAuditData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminRedemptionAuditData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminRedemptionAudit({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminRedemptionAuditInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const listAdminRedemptionBatchesQueryKey = (options?: Options<ListAdminRedemptionBatchesData>) => createQueryKey('listAdminRedemptionBatches', options);
export const listAdminRedemptionBatchesOptions = (options?: Options<ListAdminRedemptionBatchesData>) => queryOptions<ListAdminRedemptionBatchesResponse, ListAdminRedemptionBatchesError, ListAdminRedemptionBatchesResponse, ReturnType<typeof listAdminRedemptionBatchesQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminRedemptionBatches({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminRedemptionBatchesQueryKey(options)
});
export const listAdminRedemptionBatchesInfiniteQueryKey = (options?: Options<ListAdminRedemptionBatchesData>): QueryKey<Options<ListAdminRedemptionBatchesData>> => createQueryKey('listAdminRedemptionBatches', options, true);
export const listAdminRedemptionBatchesInfiniteOptions = (options?: Options<ListAdminRedemptionBatchesData>) => {
    const opts = infiniteQueryOptions<ListAdminRedemptionBatchesResponse, ListAdminRedemptionBatchesError, InfiniteData<ListAdminRedemptionBatchesResponse>, QueryKey<Options<ListAdminRedemptionBatchesData>>, number | Pick<QueryKey<Options<ListAdminRedemptionBatchesData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminRedemptionBatchesData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminRedemptionBatches({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminRedemptionBatchesInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminRedemptionBatchMutation = (options?: Partial<Options<CreateAdminRedemptionBatchData>>): UseMutationOptions<CreateAdminRedemptionBatchResponse, CreateAdminRedemptionBatchError, Options<CreateAdminRedemptionBatchData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminRedemptionBatchResponse, CreateAdminRedemptionBatchError, Options<CreateAdminRedemptionBatchData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminRedemptionBatch({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const disableAdminRedemptionBatchMutation = (options?: Partial<Options<DisableAdminRedemptionBatchData>>): UseMutationOptions<DisableAdminRedemptionBatchResponse, DisableAdminRedemptionBatchError, Options<DisableAdminRedemptionBatchData>> => {
    const mutationOptions: UseMutationOptions<DisableAdminRedemptionBatchResponse, DisableAdminRedemptionBatchError, Options<DisableAdminRedemptionBatchData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await disableAdminRedemptionBatch({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const redeemUserRedemptionCodeMutation = (options?: Partial<Options<RedeemUserRedemptionCodeData>>): UseMutationOptions<RedeemUserRedemptionCodeResponse, RedeemUserRedemptionCodeError, Options<RedeemUserRedemptionCodeData>> => {
    const mutationOptions: UseMutationOptions<RedeemUserRedemptionCodeResponse, RedeemUserRedemptionCodeError, Options<RedeemUserRedemptionCodeData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await redeemUserRedemptionCode({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminRefundsQueryKey = (options?: Options<ListAdminRefundsData>) => createQueryKey('listAdminRefunds', options);
export const listAdminRefundsOptions = (options?: Options<ListAdminRefundsData>) => queryOptions<ListAdminRefundsResponse, ListAdminRefundsError, ListAdminRefundsResponse, ReturnType<typeof listAdminRefundsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminRefunds({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminRefundsQueryKey(options)
});
export const listAdminRefundsInfiniteQueryKey = (options?: Options<ListAdminRefundsData>): QueryKey<Options<ListAdminRefundsData>> => createQueryKey('listAdminRefunds', options, true);
export const listAdminRefundsInfiniteOptions = (options?: Options<ListAdminRefundsData>) => {
    const opts = infiniteQueryOptions<ListAdminRefundsResponse, ListAdminRefundsError, InfiniteData<ListAdminRefundsResponse>, QueryKey<Options<ListAdminRefundsData>>, number | Pick<QueryKey<Options<ListAdminRefundsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminRefundsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminRefunds({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminRefundsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const approveAdminRefundMutation = (options?: Partial<Options<ApproveAdminRefundData>>): UseMutationOptions<ApproveAdminRefundResponse, ApproveAdminRefundError, Options<ApproveAdminRefundData>> => {
    const mutationOptions: UseMutationOptions<ApproveAdminRefundResponse, ApproveAdminRefundError, Options<ApproveAdminRefundData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await approveAdminRefund({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const rejectAdminRefundMutation = (options?: Partial<Options<RejectAdminRefundData>>): UseMutationOptions<RejectAdminRefundResponse, RejectAdminRefundError, Options<RejectAdminRefundData>> => {
    const mutationOptions: UseMutationOptions<RejectAdminRefundResponse, RejectAdminRefundError, Options<RejectAdminRefundData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await rejectAdminRefund({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const submitAdminRefundMutation = (options?: Partial<Options<SubmitAdminRefundData>>): UseMutationOptions<SubmitAdminRefundResponse, SubmitAdminRefundError, Options<SubmitAdminRefundData>> => {
    const mutationOptions: UseMutationOptions<SubmitAdminRefundResponse, SubmitAdminRefundError, Options<SubmitAdminRefundData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await submitAdminRefund({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminSubscriptionPlansQueryKey = (options?: Options<ListAdminSubscriptionPlansData>) => createQueryKey('listAdminSubscriptionPlans', options);
export const listAdminSubscriptionPlansOptions = (options?: Options<ListAdminSubscriptionPlansData>) => queryOptions<ListAdminSubscriptionPlansResponse, ListAdminSubscriptionPlansError, ListAdminSubscriptionPlansResponse, ReturnType<typeof listAdminSubscriptionPlansQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminSubscriptionPlans({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminSubscriptionPlansQueryKey(options)
});
export const listAdminSubscriptionPlansInfiniteQueryKey = (options?: Options<ListAdminSubscriptionPlansData>): QueryKey<Options<ListAdminSubscriptionPlansData>> => createQueryKey('listAdminSubscriptionPlans', options, true);
export const listAdminSubscriptionPlansInfiniteOptions = (options?: Options<ListAdminSubscriptionPlansData>) => {
    const opts = infiniteQueryOptions<ListAdminSubscriptionPlansResponse, ListAdminSubscriptionPlansError, InfiniteData<ListAdminSubscriptionPlansResponse>, QueryKey<Options<ListAdminSubscriptionPlansData>>, number | Pick<QueryKey<Options<ListAdminSubscriptionPlansData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminSubscriptionPlansData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminSubscriptionPlans({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminSubscriptionPlansInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminSubscriptionPlanMutation = (options?: Partial<Options<CreateAdminSubscriptionPlanData>>): UseMutationOptions<CreateAdminSubscriptionPlanResponse, CreateAdminSubscriptionPlanError, Options<CreateAdminSubscriptionPlanData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminSubscriptionPlanResponse, CreateAdminSubscriptionPlanError, Options<CreateAdminSubscriptionPlanData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminSubscriptionPlan({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const disableAdminSubscriptionPlanMutation = (options?: Partial<Options<DisableAdminSubscriptionPlanData>>): UseMutationOptions<DisableAdminSubscriptionPlanResponse, DisableAdminSubscriptionPlanError, Options<DisableAdminSubscriptionPlanData>> => {
    const mutationOptions: UseMutationOptions<DisableAdminSubscriptionPlanResponse, DisableAdminSubscriptionPlanError, Options<DisableAdminSubscriptionPlanData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await disableAdminSubscriptionPlan({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminUserSubscriptionsQueryKey = (options: Options<ListAdminUserSubscriptionsData>) => createQueryKey('listAdminUserSubscriptions', options);
export const listAdminUserSubscriptionsOptions = (options: Options<ListAdminUserSubscriptionsData>) => queryOptions<ListAdminUserSubscriptionsResponse, ListAdminUserSubscriptionsError, ListAdminUserSubscriptionsResponse, ReturnType<typeof listAdminUserSubscriptionsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminUserSubscriptions({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminUserSubscriptionsQueryKey(options)
});
export const listAdminUserSubscriptionsInfiniteQueryKey = (options: Options<ListAdminUserSubscriptionsData>): QueryKey<Options<ListAdminUserSubscriptionsData>> => createQueryKey('listAdminUserSubscriptions', options, true);
export const listAdminUserSubscriptionsInfiniteOptions = (options: Options<ListAdminUserSubscriptionsData>) => {
    const opts = infiniteQueryOptions<ListAdminUserSubscriptionsResponse, ListAdminUserSubscriptionsError, InfiniteData<ListAdminUserSubscriptionsResponse>, QueryKey<Options<ListAdminUserSubscriptionsData>>, number | Pick<QueryKey<Options<ListAdminUserSubscriptionsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminUserSubscriptionsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminUserSubscriptions({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminUserSubscriptionsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const bindAdminUserSubscriptionMutation = (options?: Partial<Options<BindAdminUserSubscriptionData>>): UseMutationOptions<BindAdminUserSubscriptionResponse, BindAdminUserSubscriptionError, Options<BindAdminUserSubscriptionData>> => {
    const mutationOptions: UseMutationOptions<BindAdminUserSubscriptionResponse, BindAdminUserSubscriptionError, Options<BindAdminUserSubscriptionData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await bindAdminUserSubscription({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const transitionAdminUserSubscriptionLifecycleMutation = (options?: Partial<Options<TransitionAdminUserSubscriptionLifecycleData>>): UseMutationOptions<TransitionAdminUserSubscriptionLifecycleResponse, TransitionAdminUserSubscriptionLifecycleError, Options<TransitionAdminUserSubscriptionLifecycleData>> => {
    const mutationOptions: UseMutationOptions<TransitionAdminUserSubscriptionLifecycleResponse, TransitionAdminUserSubscriptionLifecycleError, Options<TransitionAdminUserSubscriptionLifecycleData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await transitionAdminUserSubscriptionLifecycle({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listCurrentUserSubscriptionsQueryKey = (options?: Options<ListCurrentUserSubscriptionsData>) => createQueryKey('listCurrentUserSubscriptions', options);
export const listCurrentUserSubscriptionsOptions = (options?: Options<ListCurrentUserSubscriptionsData>) => queryOptions<ListCurrentUserSubscriptionsResponse, ListCurrentUserSubscriptionsError, ListCurrentUserSubscriptionsResponse, ReturnType<typeof listCurrentUserSubscriptionsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listCurrentUserSubscriptions({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listCurrentUserSubscriptionsQueryKey(options)
});
export const listCurrentUserSubscriptionsInfiniteQueryKey = (options?: Options<ListCurrentUserSubscriptionsData>): QueryKey<Options<ListCurrentUserSubscriptionsData>> => createQueryKey('listCurrentUserSubscriptions', options, true);
export const listCurrentUserSubscriptionsInfiniteOptions = (options?: Options<ListCurrentUserSubscriptionsData>) => {
    const opts = infiniteQueryOptions<ListCurrentUserSubscriptionsResponse, ListCurrentUserSubscriptionsError, InfiniteData<ListCurrentUserSubscriptionsResponse>, QueryKey<Options<ListCurrentUserSubscriptionsData>>, number | Pick<QueryKey<Options<ListCurrentUserSubscriptionsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListCurrentUserSubscriptionsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listCurrentUserSubscriptions({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listCurrentUserSubscriptionsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const listCurrentSubscriptionCatalogQueryKey = (options?: Options<ListCurrentSubscriptionCatalogData>) => createQueryKey('listCurrentSubscriptionCatalog', options);
export const listCurrentSubscriptionCatalogOptions = (options?: Options<ListCurrentSubscriptionCatalogData>) => queryOptions<ListCurrentSubscriptionCatalogResponse, ListCurrentSubscriptionCatalogError, ListCurrentSubscriptionCatalogResponse, ReturnType<typeof listCurrentSubscriptionCatalogQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listCurrentSubscriptionCatalog({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listCurrentSubscriptionCatalogQueryKey(options)
});
export const createCurrentSubscriptionOrderMutation = (options?: Partial<Options<CreateCurrentSubscriptionOrderData>>): UseMutationOptions<CreateCurrentSubscriptionOrderResponse, CreateCurrentSubscriptionOrderError, Options<CreateCurrentSubscriptionOrderData>> => {
    const mutationOptions: UseMutationOptions<CreateCurrentSubscriptionOrderResponse, CreateCurrentSubscriptionOrderError, Options<CreateCurrentSubscriptionOrderData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createCurrentSubscriptionOrder({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getCurrentSubscriptionOrderQueryKey = (options: Options<GetCurrentSubscriptionOrderData>) => createQueryKey('getCurrentSubscriptionOrder', options);
export const getCurrentSubscriptionOrderOptions = (options: Options<GetCurrentSubscriptionOrderData>) => queryOptions<GetCurrentSubscriptionOrderResponse, GetCurrentSubscriptionOrderError, GetCurrentSubscriptionOrderResponse, ReturnType<typeof getCurrentSubscriptionOrderQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getCurrentSubscriptionOrder({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getCurrentSubscriptionOrderQueryKey(options)
});
export const submitCurrentSubscriptionOrderPaymentMutation = (options?: Partial<Options<SubmitCurrentSubscriptionOrderPaymentData>>): UseMutationOptions<SubmitCurrentSubscriptionOrderPaymentResponse, SubmitCurrentSubscriptionOrderPaymentError, Options<SubmitCurrentSubscriptionOrderPaymentData>> => {
    const mutationOptions: UseMutationOptions<SubmitCurrentSubscriptionOrderPaymentResponse, SubmitCurrentSubscriptionOrderPaymentError, Options<SubmitCurrentSubscriptionOrderPaymentData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await submitCurrentSubscriptionOrderPayment({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const requestPasswordResetMutation = (options?: Partial<Options<RequestPasswordResetData>>): UseMutationOptions<RequestPasswordResetResponse, RequestPasswordResetError, Options<RequestPasswordResetData>> => {
    const mutationOptions: UseMutationOptions<RequestPasswordResetResponse, RequestPasswordResetError, Options<RequestPasswordResetData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await requestPasswordReset({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const confirmPasswordResetMutation = (options?: Partial<Options<ConfirmPasswordResetData>>): UseMutationOptions<ConfirmPasswordResetResponse, ConfirmPasswordResetError, Options<ConfirmPasswordResetData>> => {
    const mutationOptions: UseMutationOptions<ConfirmPasswordResetResponse, ConfirmPasswordResetError, Options<ConfirmPasswordResetData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await confirmPasswordReset({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getUserProfileQueryKey = (options?: Options<GetUserProfileData>) => createQueryKey('getUserProfile', options);
export const getUserProfileOptions = (options?: Options<GetUserProfileData>) => queryOptions<GetUserProfileResponse, GetUserProfileError, GetUserProfileResponse, ReturnType<typeof getUserProfileQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getUserProfile({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getUserProfileQueryKey(options)
});
export const updateUserProfileMutation = (options?: Partial<Options<UpdateUserProfileData>>): UseMutationOptions<UpdateUserProfileResponse, UpdateUserProfileError, Options<UpdateUserProfileData>> => {
    const mutationOptions: UseMutationOptions<UpdateUserProfileResponse, UpdateUserProfileError, Options<UpdateUserProfileData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateUserProfile({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const changeUserPasswordMutation = (options?: Partial<Options<ChangeUserPasswordData>>): UseMutationOptions<ChangeUserPasswordResponse, ChangeUserPasswordError, Options<ChangeUserPasswordData>> => {
    const mutationOptions: UseMutationOptions<ChangeUserPasswordResponse, ChangeUserPasswordError, Options<ChangeUserPasswordData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await changeUserPassword({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const disableUserTwoFactorMutation = (options?: Partial<Options<DisableUserTwoFactorData>>): UseMutationOptions<DisableUserTwoFactorResponse, DisableUserTwoFactorError, Options<DisableUserTwoFactorData>> => {
    const mutationOptions: UseMutationOptions<DisableUserTwoFactorResponse, DisableUserTwoFactorError, Options<DisableUserTwoFactorData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await disableUserTwoFactor({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getUserTwoFactorQueryKey = (options?: Options<GetUserTwoFactorData>) => createQueryKey('getUserTwoFactor', options);
export const getUserTwoFactorOptions = (options?: Options<GetUserTwoFactorData>) => queryOptions<GetUserTwoFactorResponse, GetUserTwoFactorError, GetUserTwoFactorResponse, ReturnType<typeof getUserTwoFactorQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getUserTwoFactor({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getUserTwoFactorQueryKey(options)
});
export const enableUserTwoFactorMutation = (options?: Partial<Options<EnableUserTwoFactorData>>): UseMutationOptions<EnableUserTwoFactorResponse, EnableUserTwoFactorError, Options<EnableUserTwoFactorData>> => {
    const mutationOptions: UseMutationOptions<EnableUserTwoFactorResponse, EnableUserTwoFactorError, Options<EnableUserTwoFactorData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await enableUserTwoFactor({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listUserNotificationsQueryKey = (options?: Options<ListUserNotificationsData>) => createQueryKey('listUserNotifications', options);
export const listUserNotificationsOptions = (options?: Options<ListUserNotificationsData>) => queryOptions<ListUserNotificationsResponse, ListUserNotificationsError, ListUserNotificationsResponse, ReturnType<typeof listUserNotificationsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listUserNotifications({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listUserNotificationsQueryKey(options)
});
export const listUserNotificationsInfiniteQueryKey = (options?: Options<ListUserNotificationsData>): QueryKey<Options<ListUserNotificationsData>> => createQueryKey('listUserNotifications', options, true);
export const listUserNotificationsInfiniteOptions = (options?: Options<ListUserNotificationsData>) => {
    const opts = infiniteQueryOptions<ListUserNotificationsResponse, ListUserNotificationsError, InfiniteData<ListUserNotificationsResponse>, QueryKey<Options<ListUserNotificationsData>>, string | Pick<QueryKey<Options<ListUserNotificationsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListUserNotificationsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listUserNotifications({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listUserNotificationsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const updateUserNotificationPreferencesMutation = (options?: Partial<Options<UpdateUserNotificationPreferencesData>>): UseMutationOptions<UpdateUserNotificationPreferencesResponse, UpdateUserNotificationPreferencesError, Options<UpdateUserNotificationPreferencesData>> => {
    const mutationOptions: UseMutationOptions<UpdateUserNotificationPreferencesResponse, UpdateUserNotificationPreferencesError, Options<UpdateUserNotificationPreferencesData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateUserNotificationPreferences({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listUserPasskeysQueryKey = (options?: Options<ListUserPasskeysData>) => createQueryKey('listUserPasskeys', options);
export const listUserPasskeysOptions = (options?: Options<ListUserPasskeysData>) => queryOptions<ListUserPasskeysResponse, ListUserPasskeysError, ListUserPasskeysResponse, ReturnType<typeof listUserPasskeysQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listUserPasskeys({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listUserPasskeysQueryKey(options)
});
export const startUserPasskeyRegistrationMutation = (options?: Partial<Options<StartUserPasskeyRegistrationData>>): UseMutationOptions<StartUserPasskeyRegistrationResponse, StartUserPasskeyRegistrationError, Options<StartUserPasskeyRegistrationData>> => {
    const mutationOptions: UseMutationOptions<StartUserPasskeyRegistrationResponse, StartUserPasskeyRegistrationError, Options<StartUserPasskeyRegistrationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await startUserPasskeyRegistration({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const finishUserPasskeyRegistrationMutation = (options?: Partial<Options<FinishUserPasskeyRegistrationData>>): UseMutationOptions<FinishUserPasskeyRegistrationResponse, FinishUserPasskeyRegistrationError, Options<FinishUserPasskeyRegistrationData>> => {
    const mutationOptions: UseMutationOptions<FinishUserPasskeyRegistrationResponse, FinishUserPasskeyRegistrationError, Options<FinishUserPasskeyRegistrationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await finishUserPasskeyRegistration({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const revokeUserPasskeyMutation = (options?: Partial<Options<RevokeUserPasskeyData>>): UseMutationOptions<RevokeUserPasskeyResponse, RevokeUserPasskeyError, Options<RevokeUserPasskeyData>> => {
    const mutationOptions: UseMutationOptions<RevokeUserPasskeyResponse, RevokeUserPasskeyError, Options<RevokeUserPasskeyData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await revokeUserPasskey({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const renameUserPasskeyMutation = (options?: Partial<Options<RenameUserPasskeyData>>): UseMutationOptions<RenameUserPasskeyResponse, RenameUserPasskeyError, Options<RenameUserPasskeyData>> => {
    const mutationOptions: UseMutationOptions<RenameUserPasskeyResponse, RenameUserPasskeyError, Options<RenameUserPasskeyData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await renameUserPasskey({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getUserWalletQueryKey = (options?: Options<GetUserWalletData>) => createQueryKey('getUserWallet', options);
export const getUserWalletOptions = (options?: Options<GetUserWalletData>) => queryOptions<GetUserWalletResponse, GetUserWalletError, GetUserWalletResponse, ReturnType<typeof getUserWalletQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getUserWallet({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getUserWalletQueryKey(options)
});
export const listUserWalletEntriesQueryKey = (options?: Options<ListUserWalletEntriesData>) => createQueryKey('listUserWalletEntries', options);
export const listUserWalletEntriesOptions = (options?: Options<ListUserWalletEntriesData>) => queryOptions<ListUserWalletEntriesResponse, ListUserWalletEntriesError, ListUserWalletEntriesResponse, ReturnType<typeof listUserWalletEntriesQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listUserWalletEntries({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listUserWalletEntriesQueryKey(options)
});
export const listUserWalletEntriesInfiniteQueryKey = (options?: Options<ListUserWalletEntriesData>): QueryKey<Options<ListUserWalletEntriesData>> => createQueryKey('listUserWalletEntries', options, true);
export const listUserWalletEntriesInfiniteOptions = (options?: Options<ListUserWalletEntriesData>) => {
    const opts = infiniteQueryOptions<ListUserWalletEntriesResponse, ListUserWalletEntriesError, InfiniteData<ListUserWalletEntriesResponse>, QueryKey<Options<ListUserWalletEntriesData>>, number | Pick<QueryKey<Options<ListUserWalletEntriesData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListUserWalletEntriesData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listUserWalletEntries({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listUserWalletEntriesInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const markUserNotificationsReadMutation = (options?: Partial<Options<MarkUserNotificationsReadData>>): UseMutationOptions<MarkUserNotificationsReadResponse, MarkUserNotificationsReadError, Options<MarkUserNotificationsReadData>> => {
    const mutationOptions: UseMutationOptions<MarkUserNotificationsReadResponse, MarkUserNotificationsReadError, Options<MarkUserNotificationsReadData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await markUserNotificationsRead({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getUserTopupConfigurationQueryKey = (options?: Options<GetUserTopupConfigurationData>) => createQueryKey('getUserTopupConfiguration', options);
export const getUserTopupConfigurationOptions = (options?: Options<GetUserTopupConfigurationData>) => queryOptions<GetUserTopupConfigurationResponse, GetUserTopupConfigurationError, GetUserTopupConfigurationResponse, ReturnType<typeof getUserTopupConfigurationQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getUserTopupConfiguration({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getUserTopupConfigurationQueryKey(options)
});
export const createUserTopupOrderMutation = (options?: Partial<Options<CreateUserTopupOrderData>>): UseMutationOptions<CreateUserTopupOrderResponse, CreateUserTopupOrderError, Options<CreateUserTopupOrderData>> => {
    const mutationOptions: UseMutationOptions<CreateUserTopupOrderResponse, CreateUserTopupOrderError, Options<CreateUserTopupOrderData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createUserTopupOrder({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getUserInvitationsQueryKey = (options?: Options<GetUserInvitationsData>) => createQueryKey('getUserInvitations', options);
export const getUserInvitationsOptions = (options?: Options<GetUserInvitationsData>) => queryOptions<GetUserInvitationsResponse, GetUserInvitationsError, GetUserInvitationsResponse, ReturnType<typeof getUserInvitationsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getUserInvitations({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getUserInvitationsQueryKey(options)
});
export const getAdminEmailSettingsQueryKey = (options?: Options<GetAdminEmailSettingsData>) => createQueryKey('getAdminEmailSettings', options);
export const getAdminEmailSettingsOptions = (options?: Options<GetAdminEmailSettingsData>) => queryOptions<GetAdminEmailSettingsResponse, GetAdminEmailSettingsError, GetAdminEmailSettingsResponse, ReturnType<typeof getAdminEmailSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminEmailSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminEmailSettingsQueryKey(options)
});
export const updateAdminEmailSettingsMutation = (options?: Partial<Options<UpdateAdminEmailSettingsData>>): UseMutationOptions<UpdateAdminEmailSettingsResponse, UpdateAdminEmailSettingsError, Options<UpdateAdminEmailSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminEmailSettingsResponse, UpdateAdminEmailSettingsError, Options<UpdateAdminEmailSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminEmailSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const sendAdminEmailTestMutation = (options?: Partial<Options<SendAdminEmailTestData>>): UseMutationOptions<SendAdminEmailTestResponse, SendAdminEmailTestError, Options<SendAdminEmailTestData>> => {
    const mutationOptions: UseMutationOptions<SendAdminEmailTestResponse, SendAdminEmailTestError, Options<SendAdminEmailTestData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await sendAdminEmailTest({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminNetworkSettingsQueryKey = (options?: Options<GetAdminNetworkSettingsData>) => createQueryKey('getAdminNetworkSettings', options);
export const getAdminNetworkSettingsOptions = (options?: Options<GetAdminNetworkSettingsData>) => queryOptions<GetAdminNetworkSettingsResponse, GetAdminNetworkSettingsError, GetAdminNetworkSettingsResponse, ReturnType<typeof getAdminNetworkSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminNetworkSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminNetworkSettingsQueryKey(options)
});
export const updateAdminNetworkSettingsMutation = (options?: Partial<Options<UpdateAdminNetworkSettingsData>>): UseMutationOptions<UpdateAdminNetworkSettingsResponse, UpdateAdminNetworkSettingsError, Options<UpdateAdminNetworkSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminNetworkSettingsResponse, UpdateAdminNetworkSettingsError, Options<UpdateAdminNetworkSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminNetworkSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminPaymentSettingsQueryKey = (options?: Options<GetAdminPaymentSettingsData>) => createQueryKey('getAdminPaymentSettings', options);
export const getAdminPaymentSettingsOptions = (options?: Options<GetAdminPaymentSettingsData>) => queryOptions<GetAdminPaymentSettingsResponse, GetAdminPaymentSettingsError, GetAdminPaymentSettingsResponse, ReturnType<typeof getAdminPaymentSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminPaymentSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminPaymentSettingsQueryKey(options)
});
export const updateAdminPaymentSettingsMutation = (options?: Partial<Options<UpdateAdminPaymentSettingsData>>): UseMutationOptions<UpdateAdminPaymentSettingsResponse, UpdateAdminPaymentSettingsError, Options<UpdateAdminPaymentSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminPaymentSettingsResponse, UpdateAdminPaymentSettingsError, Options<UpdateAdminPaymentSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminPaymentSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminOAuthProvidersQueryKey = (options?: Options<ListAdminOAuthProvidersData>) => createQueryKey('listAdminOAuthProviders', options);
export const listAdminOAuthProvidersOptions = (options?: Options<ListAdminOAuthProvidersData>) => queryOptions<ListAdminOAuthProvidersResponse, ListAdminOAuthProvidersError, ListAdminOAuthProvidersResponse, ReturnType<typeof listAdminOAuthProvidersQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminOAuthProviders({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminOAuthProvidersQueryKey(options)
});
export const beginAdminOAuthAuthorizationMutation = (options?: Partial<Options<BeginAdminOAuthAuthorizationData>>): UseMutationOptions<BeginAdminOAuthAuthorizationResponse, BeginAdminOAuthAuthorizationError, Options<BeginAdminOAuthAuthorizationData>> => {
    const mutationOptions: UseMutationOptions<BeginAdminOAuthAuthorizationResponse, BeginAdminOAuthAuthorizationError, Options<BeginAdminOAuthAuthorizationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await beginAdminOAuthAuthorization({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const completeAdminOAuthManualCallbackMutation = (options?: Partial<Options<CompleteAdminOAuthManualCallbackData>>): UseMutationOptions<CompleteAdminOAuthManualCallbackResponse, CompleteAdminOAuthManualCallbackError, Options<CompleteAdminOAuthManualCallbackData>> => {
    const mutationOptions: UseMutationOptions<CompleteAdminOAuthManualCallbackResponse, CompleteAdminOAuthManualCallbackError, Options<CompleteAdminOAuthManualCallbackData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await completeAdminOAuthManualCallback({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const startGitHubOAuthLoginMutation = (options?: Partial<Options<StartGitHubOAuthLoginData>>): UseMutationOptions<StartGitHubOAuthLoginResponse, StartGitHubOAuthLoginError, Options<StartGitHubOAuthLoginData>> => {
    const mutationOptions: UseMutationOptions<StartGitHubOAuthLoginResponse, StartGitHubOAuthLoginError, Options<StartGitHubOAuthLoginData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await startGitHubOAuthLogin({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const startDiscordOAuthLoginMutation = (options?: Partial<Options<StartDiscordOAuthLoginData>>): UseMutationOptions<StartDiscordOAuthLoginResponse, StartDiscordOAuthLoginError, Options<StartDiscordOAuthLoginData>> => {
    const mutationOptions: UseMutationOptions<StartDiscordOAuthLoginResponse, StartDiscordOAuthLoginError, Options<StartDiscordOAuthLoginData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await startDiscordOAuthLogin({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const completeGitHubOAuthLoginQueryKey = (options?: Options<CompleteGitHubOAuthLoginData>) => createQueryKey('completeGitHubOAuthLogin', options);
export const completeGitHubOAuthLoginOptions = (options?: Options<CompleteGitHubOAuthLoginData>) => queryOptions<unknown, CompleteGitHubOAuthLoginError, unknown, ReturnType<typeof completeGitHubOAuthLoginQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await completeGitHubOAuthLogin({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: completeGitHubOAuthLoginQueryKey(options)
});
export const completeDiscordOAuthLoginQueryKey = (options?: Options<CompleteDiscordOAuthLoginData>) => createQueryKey('completeDiscordOAuthLogin', options);
export const completeDiscordOAuthLoginOptions = (options?: Options<CompleteDiscordOAuthLoginData>) => queryOptions<unknown, CompleteDiscordOAuthLoginError, unknown, ReturnType<typeof completeDiscordOAuthLoginQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await completeDiscordOAuthLogin({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: completeDiscordOAuthLoginQueryKey(options)
});
export const exchangeOAuthLoginTicketMutation = (options?: Partial<Options<ExchangeOAuthLoginTicketData>>): UseMutationOptions<ExchangeOAuthLoginTicketResponse, ExchangeOAuthLoginTicketError, Options<ExchangeOAuthLoginTicketData>> => {
    const mutationOptions: UseMutationOptions<ExchangeOAuthLoginTicketResponse, ExchangeOAuthLoginTicketError, Options<ExchangeOAuthLoginTicketData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await exchangeOAuthLoginTicket({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminGitHubOAuthLoginSettingsQueryKey = (options?: Options<GetAdminGitHubOAuthLoginSettingsData>) => createQueryKey('getAdminGitHubOAuthLoginSettings', options);
export const getAdminGitHubOAuthLoginSettingsOptions = (options?: Options<GetAdminGitHubOAuthLoginSettingsData>) => queryOptions<GetAdminGitHubOAuthLoginSettingsResponse, GetAdminGitHubOAuthLoginSettingsError, GetAdminGitHubOAuthLoginSettingsResponse, ReturnType<typeof getAdminGitHubOAuthLoginSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminGitHubOAuthLoginSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminGitHubOAuthLoginSettingsQueryKey(options)
});
export const updateAdminGitHubOAuthLoginSettingsMutation = (options?: Partial<Options<UpdateAdminGitHubOAuthLoginSettingsData>>): UseMutationOptions<UpdateAdminGitHubOAuthLoginSettingsResponse, UpdateAdminGitHubOAuthLoginSettingsError, Options<UpdateAdminGitHubOAuthLoginSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminGitHubOAuthLoginSettingsResponse, UpdateAdminGitHubOAuthLoginSettingsError, Options<UpdateAdminGitHubOAuthLoginSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminGitHubOAuthLoginSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminDiscordOAuthLoginSettingsQueryKey = (options?: Options<GetAdminDiscordOAuthLoginSettingsData>) => createQueryKey('getAdminDiscordOAuthLoginSettings', options);
export const getAdminDiscordOAuthLoginSettingsOptions = (options?: Options<GetAdminDiscordOAuthLoginSettingsData>) => queryOptions<GetAdminDiscordOAuthLoginSettingsResponse, GetAdminDiscordOAuthLoginSettingsError, GetAdminDiscordOAuthLoginSettingsResponse, ReturnType<typeof getAdminDiscordOAuthLoginSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminDiscordOAuthLoginSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminDiscordOAuthLoginSettingsQueryKey(options)
});
export const updateAdminDiscordOAuthLoginSettingsMutation = (options?: Partial<Options<UpdateAdminDiscordOAuthLoginSettingsData>>): UseMutationOptions<UpdateAdminDiscordOAuthLoginSettingsResponse, UpdateAdminDiscordOAuthLoginSettingsError, Options<UpdateAdminDiscordOAuthLoginSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminDiscordOAuthLoginSettingsResponse, UpdateAdminDiscordOAuthLoginSettingsError, Options<UpdateAdminDiscordOAuthLoginSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminDiscordOAuthLoginSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAccountOrganizationsQueryKey = (options?: Options<ListAccountOrganizationsData>) => createQueryKey('listAccountOrganizations', options);
export const listAccountOrganizationsOptions = (options?: Options<ListAccountOrganizationsData>) => queryOptions<ListAccountOrganizationsResponse, ListAccountOrganizationsError, ListAccountOrganizationsResponse, ReturnType<typeof listAccountOrganizationsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAccountOrganizations({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAccountOrganizationsQueryKey(options)
});
export const getOrganizationOverviewQueryKey = (options: Options<GetOrganizationOverviewData>) => createQueryKey('getOrganizationOverview', options);
export const getOrganizationOverviewOptions = (options: Options<GetOrganizationOverviewData>) => queryOptions<GetOrganizationOverviewResponse, GetOrganizationOverviewError, GetOrganizationOverviewResponse, ReturnType<typeof getOrganizationOverviewQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getOrganizationOverview({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getOrganizationOverviewQueryKey(options)
});
export const listOrganizationInvitationsQueryKey = (options: Options<ListOrganizationInvitationsData>) => createQueryKey('listOrganizationInvitations', options);
export const listOrganizationInvitationsOptions = (options: Options<ListOrganizationInvitationsData>) => queryOptions<ListOrganizationInvitationsResponse, ListOrganizationInvitationsError, ListOrganizationInvitationsResponse, ReturnType<typeof listOrganizationInvitationsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listOrganizationInvitations({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listOrganizationInvitationsQueryKey(options)
});
export const createOrganizationInvitationMutation = (options?: Partial<Options<CreateOrganizationInvitationData>>): UseMutationOptions<CreateOrganizationInvitationResponse, CreateOrganizationInvitationError, Options<CreateOrganizationInvitationData>> => {
    const mutationOptions: UseMutationOptions<CreateOrganizationInvitationResponse, CreateOrganizationInvitationError, Options<CreateOrganizationInvitationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createOrganizationInvitation({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const previewOrganizationInvitationsMutation = (options?: Partial<Options<PreviewOrganizationInvitationsData>>): UseMutationOptions<PreviewOrganizationInvitationsResponse, PreviewOrganizationInvitationsError, Options<PreviewOrganizationInvitationsData>> => {
    const mutationOptions: UseMutationOptions<PreviewOrganizationInvitationsResponse, PreviewOrganizationInvitationsError, Options<PreviewOrganizationInvitationsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await previewOrganizationInvitations({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const createOrganizationInvitationBatchMutation = (options?: Partial<Options<CreateOrganizationInvitationBatchData>>): UseMutationOptions<CreateOrganizationInvitationBatchResponse, CreateOrganizationInvitationBatchError, Options<CreateOrganizationInvitationBatchData>> => {
    const mutationOptions: UseMutationOptions<CreateOrganizationInvitationBatchResponse, CreateOrganizationInvitationBatchError, Options<CreateOrganizationInvitationBatchData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createOrganizationInvitationBatch({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const revokeOrganizationInvitationMutation = (options?: Partial<Options<RevokeOrganizationInvitationData>>): UseMutationOptions<RevokeOrganizationInvitationResponse, RevokeOrganizationInvitationError, Options<RevokeOrganizationInvitationData>> => {
    const mutationOptions: UseMutationOptions<RevokeOrganizationInvitationResponse, RevokeOrganizationInvitationError, Options<RevokeOrganizationInvitationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await revokeOrganizationInvitation({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const resendOrganizationInvitationMutation = (options?: Partial<Options<ResendOrganizationInvitationData>>): UseMutationOptions<ResendOrganizationInvitationResponse, ResendOrganizationInvitationError, Options<ResendOrganizationInvitationData>> => {
    const mutationOptions: UseMutationOptions<ResendOrganizationInvitationResponse, ResendOrganizationInvitationError, Options<ResendOrganizationInvitationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await resendOrganizationInvitation({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const acceptOrganizationInvitationMutation = (options?: Partial<Options<AcceptOrganizationInvitationData>>): UseMutationOptions<AcceptOrganizationInvitationResponse, AcceptOrganizationInvitationError, Options<AcceptOrganizationInvitationData>> => {
    const mutationOptions: UseMutationOptions<AcceptOrganizationInvitationResponse, AcceptOrganizationInvitationError, Options<AcceptOrganizationInvitationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await acceptOrganizationInvitation({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listOrganizationMembersQueryKey = (options: Options<ListOrganizationMembersData>) => createQueryKey('listOrganizationMembers', options);
export const listOrganizationMembersOptions = (options: Options<ListOrganizationMembersData>) => queryOptions<ListOrganizationMembersResponse, ListOrganizationMembersError, ListOrganizationMembersResponse, ReturnType<typeof listOrganizationMembersQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listOrganizationMembers({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listOrganizationMembersQueryKey(options)
});
export const listOrganizationMembersInfiniteQueryKey = (options: Options<ListOrganizationMembersData>): QueryKey<Options<ListOrganizationMembersData>> => createQueryKey('listOrganizationMembers', options, true);
export const listOrganizationMembersInfiniteOptions = (options: Options<ListOrganizationMembersData>) => {
    const opts = infiniteQueryOptions<ListOrganizationMembersResponse, ListOrganizationMembersError, InfiniteData<ListOrganizationMembersResponse>, QueryKey<Options<ListOrganizationMembersData>>, number | Pick<QueryKey<Options<ListOrganizationMembersData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListOrganizationMembersData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listOrganizationMembers({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listOrganizationMembersInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const changeOrganizationMemberMutation = (options?: Partial<Options<ChangeOrganizationMemberData>>): UseMutationOptions<ChangeOrganizationMemberResponse, ChangeOrganizationMemberError, Options<ChangeOrganizationMemberData>> => {
    const mutationOptions: UseMutationOptions<ChangeOrganizationMemberResponse, ChangeOrganizationMemberError, Options<ChangeOrganizationMemberData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await changeOrganizationMember({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const transferOrganizationOwnershipMutation = (options?: Partial<Options<TransferOrganizationOwnershipData>>): UseMutationOptions<TransferOrganizationOwnershipResponse, TransferOrganizationOwnershipError, Options<TransferOrganizationOwnershipData>> => {
    const mutationOptions: UseMutationOptions<TransferOrganizationOwnershipResponse, TransferOrganizationOwnershipError, Options<TransferOrganizationOwnershipData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await transferOrganizationOwnership({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listOrganizationTeamsQueryKey = (options: Options<ListOrganizationTeamsData>) => createQueryKey('listOrganizationTeams', options);
export const listOrganizationTeamsOptions = (options: Options<ListOrganizationTeamsData>) => queryOptions<ListOrganizationTeamsResponse, ListOrganizationTeamsError, ListOrganizationTeamsResponse, ReturnType<typeof listOrganizationTeamsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listOrganizationTeams({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listOrganizationTeamsQueryKey(options)
});
export const listOrganizationTeamsInfiniteQueryKey = (options: Options<ListOrganizationTeamsData>): QueryKey<Options<ListOrganizationTeamsData>> => createQueryKey('listOrganizationTeams', options, true);
export const listOrganizationTeamsInfiniteOptions = (options: Options<ListOrganizationTeamsData>) => {
    const opts = infiniteQueryOptions<ListOrganizationTeamsResponse, ListOrganizationTeamsError, InfiniteData<ListOrganizationTeamsResponse>, QueryKey<Options<ListOrganizationTeamsData>>, number | Pick<QueryKey<Options<ListOrganizationTeamsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListOrganizationTeamsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listOrganizationTeams({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listOrganizationTeamsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createOrganizationTeamMutation = (options?: Partial<Options<CreateOrganizationTeamData>>): UseMutationOptions<CreateOrganizationTeamResponse, CreateOrganizationTeamError, Options<CreateOrganizationTeamData>> => {
    const mutationOptions: UseMutationOptions<CreateOrganizationTeamResponse, CreateOrganizationTeamError, Options<CreateOrganizationTeamData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createOrganizationTeam({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const changeOrganizationTeamMutation = (options?: Partial<Options<ChangeOrganizationTeamData>>): UseMutationOptions<ChangeOrganizationTeamResponse, ChangeOrganizationTeamError, Options<ChangeOrganizationTeamData>> => {
    const mutationOptions: UseMutationOptions<ChangeOrganizationTeamResponse, ChangeOrganizationTeamError, Options<ChangeOrganizationTeamData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await changeOrganizationTeam({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const disableOrganizationTeamMutation = (options?: Partial<Options<DisableOrganizationTeamData>>): UseMutationOptions<DisableOrganizationTeamResponse, DisableOrganizationTeamError, Options<DisableOrganizationTeamData>> => {
    const mutationOptions: UseMutationOptions<DisableOrganizationTeamResponse, DisableOrganizationTeamError, Options<DisableOrganizationTeamData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await disableOrganizationTeam({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listOrganizationCustomRolesQueryKey = (options: Options<ListOrganizationCustomRolesData>) => createQueryKey('listOrganizationCustomRoles', options);
export const listOrganizationCustomRolesOptions = (options: Options<ListOrganizationCustomRolesData>) => queryOptions<ListOrganizationCustomRolesResponse, ListOrganizationCustomRolesError, ListOrganizationCustomRolesResponse, ReturnType<typeof listOrganizationCustomRolesQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listOrganizationCustomRoles({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listOrganizationCustomRolesQueryKey(options)
});
export const listOrganizationCustomRolesInfiniteQueryKey = (options: Options<ListOrganizationCustomRolesData>): QueryKey<Options<ListOrganizationCustomRolesData>> => createQueryKey('listOrganizationCustomRoles', options, true);
export const listOrganizationCustomRolesInfiniteOptions = (options: Options<ListOrganizationCustomRolesData>) => {
    const opts = infiniteQueryOptions<ListOrganizationCustomRolesResponse, ListOrganizationCustomRolesError, InfiniteData<ListOrganizationCustomRolesResponse>, QueryKey<Options<ListOrganizationCustomRolesData>>, number | Pick<QueryKey<Options<ListOrganizationCustomRolesData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListOrganizationCustomRolesData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listOrganizationCustomRoles({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listOrganizationCustomRolesInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createOrganizationCustomRoleMutation = (options?: Partial<Options<CreateOrganizationCustomRoleData>>): UseMutationOptions<CreateOrganizationCustomRoleResponse, CreateOrganizationCustomRoleError, Options<CreateOrganizationCustomRoleData>> => {
    const mutationOptions: UseMutationOptions<CreateOrganizationCustomRoleResponse, CreateOrganizationCustomRoleError, Options<CreateOrganizationCustomRoleData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createOrganizationCustomRole({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getOrganizationCustomRoleQueryKey = (options: Options<GetOrganizationCustomRoleData>) => createQueryKey('getOrganizationCustomRole', options);
export const getOrganizationCustomRoleOptions = (options: Options<GetOrganizationCustomRoleData>) => queryOptions<GetOrganizationCustomRoleResponse, GetOrganizationCustomRoleError, GetOrganizationCustomRoleResponse, ReturnType<typeof getOrganizationCustomRoleQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getOrganizationCustomRole({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getOrganizationCustomRoleQueryKey(options)
});
export const updateOrganizationCustomRoleMutation = (options?: Partial<Options<UpdateOrganizationCustomRoleData>>): UseMutationOptions<UpdateOrganizationCustomRoleResponse, UpdateOrganizationCustomRoleError, Options<UpdateOrganizationCustomRoleData>> => {
    const mutationOptions: UseMutationOptions<UpdateOrganizationCustomRoleResponse, UpdateOrganizationCustomRoleError, Options<UpdateOrganizationCustomRoleData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateOrganizationCustomRole({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const disableOrganizationCustomRoleMutation = (options?: Partial<Options<DisableOrganizationCustomRoleData>>): UseMutationOptions<DisableOrganizationCustomRoleResponse, DisableOrganizationCustomRoleError, Options<DisableOrganizationCustomRoleData>> => {
    const mutationOptions: UseMutationOptions<DisableOrganizationCustomRoleResponse, DisableOrganizationCustomRoleError, Options<DisableOrganizationCustomRoleData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await disableOrganizationCustomRole({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminBalanceAlertSettingsQueryKey = (options?: Options<GetAdminBalanceAlertSettingsData>) => createQueryKey('getAdminBalanceAlertSettings', options);
export const getAdminBalanceAlertSettingsOptions = (options?: Options<GetAdminBalanceAlertSettingsData>) => queryOptions<GetAdminBalanceAlertSettingsResponse, GetAdminBalanceAlertSettingsError, GetAdminBalanceAlertSettingsResponse, ReturnType<typeof getAdminBalanceAlertSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminBalanceAlertSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminBalanceAlertSettingsQueryKey(options)
});
export const updateAdminBalanceAlertSettingsMutation = (options?: Partial<Options<UpdateAdminBalanceAlertSettingsData>>): UseMutationOptions<UpdateAdminBalanceAlertSettingsResponse, UpdateAdminBalanceAlertSettingsError, Options<UpdateAdminBalanceAlertSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminBalanceAlertSettingsResponse, UpdateAdminBalanceAlertSettingsError, Options<UpdateAdminBalanceAlertSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminBalanceAlertSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getPublicSiteSettingsQueryKey = (options?: Options<GetPublicSiteSettingsData>) => createQueryKey('getPublicSiteSettings', options);
export const getPublicSiteSettingsOptions = (options?: Options<GetPublicSiteSettingsData>) => queryOptions<GetPublicSiteSettingsResponse, GetPublicSiteSettingsError, GetPublicSiteSettingsResponse, ReturnType<typeof getPublicSiteSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getPublicSiteSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getPublicSiteSettingsQueryKey(options)
});
export const getAdminSiteSettingsQueryKey = (options?: Options<GetAdminSiteSettingsData>) => createQueryKey('getAdminSiteSettings', options);
export const getAdminSiteSettingsOptions = (options?: Options<GetAdminSiteSettingsData>) => queryOptions<GetAdminSiteSettingsResponse, GetAdminSiteSettingsError, GetAdminSiteSettingsResponse, ReturnType<typeof getAdminSiteSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminSiteSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminSiteSettingsQueryKey(options)
});
export const updateAdminSiteSettingsMutation = (options?: Partial<Options<UpdateAdminSiteSettingsData>>): UseMutationOptions<UpdateAdminSiteSettingsResponse, UpdateAdminSiteSettingsError, Options<UpdateAdminSiteSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminSiteSettingsResponse, UpdateAdminSiteSettingsError, Options<UpdateAdminSiteSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminSiteSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminDashboardQueryKey = (options?: Options<GetAdminDashboardData>) => createQueryKey('getAdminDashboard', options);
export const getAdminDashboardOptions = (options?: Options<GetAdminDashboardData>) => queryOptions<GetAdminDashboardResponse, GetAdminDashboardError, GetAdminDashboardResponse, ReturnType<typeof getAdminDashboardQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminDashboard({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminDashboardQueryKey(options)
});
export const listAdminUsersQueryKey = (options?: Options<ListAdminUsersData>) => createQueryKey('listAdminUsers', options);
export const listAdminUsersOptions = (options?: Options<ListAdminUsersData>) => queryOptions<ListAdminUsersResponse, ListAdminUsersError, ListAdminUsersResponse, ReturnType<typeof listAdminUsersQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminUsers({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminUsersQueryKey(options)
});
export const listAdminUsersInfiniteQueryKey = (options?: Options<ListAdminUsersData>): QueryKey<Options<ListAdminUsersData>> => createQueryKey('listAdminUsers', options, true);
export const listAdminUsersInfiniteOptions = (options?: Options<ListAdminUsersData>) => {
    const opts = infiniteQueryOptions<ListAdminUsersResponse, ListAdminUsersError, InfiniteData<ListAdminUsersResponse>, QueryKey<Options<ListAdminUsersData>>, number | Pick<QueryKey<Options<ListAdminUsersData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminUsersData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminUsers({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminUsersInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminUserMutation = (options?: Partial<Options<CreateAdminUserData>>): UseMutationOptions<CreateAdminUserResponse, CreateAdminUserError, Options<CreateAdminUserData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminUserResponse, CreateAdminUserError, Options<CreateAdminUserData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminUser({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminUserMutation = (options?: Partial<Options<DeleteAdminUserData>>): UseMutationOptions<DeleteAdminUserResponse, DeleteAdminUserError, Options<DeleteAdminUserData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminUserResponse, DeleteAdminUserError, Options<DeleteAdminUserData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminUser({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminUserQueryKey = (options: Options<GetAdminUserData>) => createQueryKey('getAdminUser', options);
export const getAdminUserOptions = (options: Options<GetAdminUserData>) => queryOptions<GetAdminUserResponse, GetAdminUserError, GetAdminUserResponse, ReturnType<typeof getAdminUserQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminUser({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminUserQueryKey(options)
});
export const updateAdminUserMutation = (options?: Partial<Options<UpdateAdminUserData>>): UseMutationOptions<UpdateAdminUserResponse, UpdateAdminUserError, Options<UpdateAdminUserData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminUserResponse, UpdateAdminUserError, Options<UpdateAdminUserData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminUser({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminWalletEntriesQueryKey = (options: Options<ListAdminWalletEntriesData>) => createQueryKey('listAdminWalletEntries', options);
export const listAdminWalletEntriesOptions = (options: Options<ListAdminWalletEntriesData>) => queryOptions<ListAdminWalletEntriesResponse, ListAdminWalletEntriesError, ListAdminWalletEntriesResponse, ReturnType<typeof listAdminWalletEntriesQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminWalletEntries({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminWalletEntriesQueryKey(options)
});
export const listAdminWalletEntriesInfiniteQueryKey = (options: Options<ListAdminWalletEntriesData>): QueryKey<Options<ListAdminWalletEntriesData>> => createQueryKey('listAdminWalletEntries', options, true);
export const listAdminWalletEntriesInfiniteOptions = (options: Options<ListAdminWalletEntriesData>) => {
    const opts = infiniteQueryOptions<ListAdminWalletEntriesResponse, ListAdminWalletEntriesError, InfiniteData<ListAdminWalletEntriesResponse>, QueryKey<Options<ListAdminWalletEntriesData>>, number | Pick<QueryKey<Options<ListAdminWalletEntriesData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminWalletEntriesData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminWalletEntries({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminWalletEntriesInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const adjustAdminWalletMutation = (options?: Partial<Options<AdjustAdminWalletData>>): UseMutationOptions<AdjustAdminWalletResponse, AdjustAdminWalletError, Options<AdjustAdminWalletData>> => {
    const mutationOptions: UseMutationOptions<AdjustAdminWalletResponse, AdjustAdminWalletError, Options<AdjustAdminWalletData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await adjustAdminWallet({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminGroupsQueryKey = (options?: Options<ListAdminGroupsData>) => createQueryKey('listAdminGroups', options);
export const listAdminGroupsOptions = (options?: Options<ListAdminGroupsData>) => queryOptions<ListAdminGroupsResponse, ListAdminGroupsError, ListAdminGroupsResponse, ReturnType<typeof listAdminGroupsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminGroups({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminGroupsQueryKey(options)
});
export const listAdminGroupsInfiniteQueryKey = (options?: Options<ListAdminGroupsData>): QueryKey<Options<ListAdminGroupsData>> => createQueryKey('listAdminGroups', options, true);
export const listAdminGroupsInfiniteOptions = (options?: Options<ListAdminGroupsData>) => {
    const opts = infiniteQueryOptions<ListAdminGroupsResponse, ListAdminGroupsError, InfiniteData<ListAdminGroupsResponse>, QueryKey<Options<ListAdminGroupsData>>, number | Pick<QueryKey<Options<ListAdminGroupsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminGroupsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminGroups({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminGroupsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminGroupMutation = (options?: Partial<Options<CreateAdminGroupData>>): UseMutationOptions<CreateAdminGroupResponse, CreateAdminGroupError, Options<CreateAdminGroupData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminGroupResponse, CreateAdminGroupError, Options<CreateAdminGroupData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminGroup({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminGroupMutation = (options?: Partial<Options<DeleteAdminGroupData>>): UseMutationOptions<DeleteAdminGroupResponse, DeleteAdminGroupError, Options<DeleteAdminGroupData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminGroupResponse, DeleteAdminGroupError, Options<DeleteAdminGroupData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminGroup({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminGroupQueryKey = (options: Options<GetAdminGroupData>) => createQueryKey('getAdminGroup', options);
export const getAdminGroupOptions = (options: Options<GetAdminGroupData>) => queryOptions<GetAdminGroupResponse, GetAdminGroupError, GetAdminGroupResponse, ReturnType<typeof getAdminGroupQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminGroup({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminGroupQueryKey(options)
});
export const updateAdminGroupMutation = (options?: Partial<Options<UpdateAdminGroupData>>): UseMutationOptions<UpdateAdminGroupResponse, UpdateAdminGroupError, Options<UpdateAdminGroupData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminGroupResponse, UpdateAdminGroupError, Options<UpdateAdminGroupData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminGroup({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminRoutesQueryKey = (options?: Options<ListAdminRoutesData>) => createQueryKey('listAdminRoutes', options);
export const listAdminRoutesOptions = (options?: Options<ListAdminRoutesData>) => queryOptions<ListAdminRoutesResponse, ListAdminRoutesError, ListAdminRoutesResponse, ReturnType<typeof listAdminRoutesQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminRoutes({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminRoutesQueryKey(options)
});
export const listAdminRoutesInfiniteQueryKey = (options?: Options<ListAdminRoutesData>): QueryKey<Options<ListAdminRoutesData>> => createQueryKey('listAdminRoutes', options, true);
export const listAdminRoutesInfiniteOptions = (options?: Options<ListAdminRoutesData>) => {
    const opts = infiniteQueryOptions<ListAdminRoutesResponse, ListAdminRoutesError, InfiniteData<ListAdminRoutesResponse>, QueryKey<Options<ListAdminRoutesData>>, number | Pick<QueryKey<Options<ListAdminRoutesData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminRoutesData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminRoutes({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminRoutesInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminRouteMutation = (options?: Partial<Options<CreateAdminRouteData>>): UseMutationOptions<CreateAdminRouteResponse, CreateAdminRouteError, Options<CreateAdminRouteData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminRouteResponse, CreateAdminRouteError, Options<CreateAdminRouteData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminRoute({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminRouteMutation = (options?: Partial<Options<DeleteAdminRouteData>>): UseMutationOptions<DeleteAdminRouteResponse, DeleteAdminRouteError, Options<DeleteAdminRouteData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminRouteResponse, DeleteAdminRouteError, Options<DeleteAdminRouteData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminRoute({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminRouteQueryKey = (options: Options<GetAdminRouteData>) => createQueryKey('getAdminRoute', options);
export const getAdminRouteOptions = (options: Options<GetAdminRouteData>) => queryOptions<GetAdminRouteResponse, GetAdminRouteError, GetAdminRouteResponse, ReturnType<typeof getAdminRouteQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminRoute({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminRouteQueryKey(options)
});
export const updateAdminRouteMutation = (options?: Partial<Options<UpdateAdminRouteData>>): UseMutationOptions<UpdateAdminRouteResponse, UpdateAdminRouteError, Options<UpdateAdminRouteData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminRouteResponse, UpdateAdminRouteError, Options<UpdateAdminRouteData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminRoute({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listModelsQueryKey = (options?: Options<ListModelsData>) => createQueryKey('listModels', options);
export const listModelsOptions = (options?: Options<ListModelsData>) => queryOptions<ListModelsResponse, ListModelsError, ListModelsResponse, ReturnType<typeof listModelsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listModels({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listModelsQueryKey(options)
});
export const listModelsInfiniteQueryKey = (options?: Options<ListModelsData>): QueryKey<Options<ListModelsData>> => createQueryKey('listModels', options, true);
export const listModelsInfiniteOptions = (options?: Options<ListModelsData>) => {
    const opts = infiniteQueryOptions<ListModelsResponse, ListModelsError, InfiniteData<ListModelsResponse>, QueryKey<Options<ListModelsData>>, string | Pick<QueryKey<Options<ListModelsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListModelsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listModels({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listModelsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const listModelProvidersQueryKey = (options?: Options<ListModelProvidersData>) => createQueryKey('listModelProviders', options);
export const listModelProvidersOptions = (options?: Options<ListModelProvidersData>) => queryOptions<ListModelProvidersResponse, ListModelProvidersError, ListModelProvidersResponse, ReturnType<typeof listModelProvidersQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listModelProviders({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listModelProvidersQueryKey(options)
});
export const listAdminModelsQueryKey = (options?: Options<ListAdminModelsData>) => createQueryKey('listAdminModels', options);
export const listAdminModelsOptions = (options?: Options<ListAdminModelsData>) => queryOptions<ListAdminModelsResponse, ListAdminModelsError, ListAdminModelsResponse, ReturnType<typeof listAdminModelsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminModels({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminModelsQueryKey(options)
});
export const listAdminModelsInfiniteQueryKey = (options?: Options<ListAdminModelsData>): QueryKey<Options<ListAdminModelsData>> => createQueryKey('listAdminModels', options, true);
export const listAdminModelsInfiniteOptions = (options?: Options<ListAdminModelsData>) => {
    const opts = infiniteQueryOptions<ListAdminModelsResponse, ListAdminModelsError, InfiniteData<ListAdminModelsResponse>, QueryKey<Options<ListAdminModelsData>>, number | Pick<QueryKey<Options<ListAdminModelsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminModelsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminModels({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminModelsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminModelMutation = (options?: Partial<Options<CreateAdminModelData>>): UseMutationOptions<CreateAdminModelResponse, CreateAdminModelError, Options<CreateAdminModelData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminModelResponse, CreateAdminModelError, Options<CreateAdminModelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminModel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminModelMutation = (options?: Partial<Options<DeleteAdminModelData>>): UseMutationOptions<DeleteAdminModelResponse, DeleteAdminModelError, Options<DeleteAdminModelData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminModelResponse, DeleteAdminModelError, Options<DeleteAdminModelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminModel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminModelQueryKey = (options: Options<GetAdminModelData>) => createQueryKey('getAdminModel', options);
export const getAdminModelOptions = (options: Options<GetAdminModelData>) => queryOptions<GetAdminModelResponse, GetAdminModelError, GetAdminModelResponse, ReturnType<typeof getAdminModelQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminModel({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminModelQueryKey(options)
});
export const updateAdminModelMutation = (options?: Partial<Options<UpdateAdminModelData>>): UseMutationOptions<UpdateAdminModelResponse, UpdateAdminModelError, Options<UpdateAdminModelData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminModelResponse, UpdateAdminModelError, Options<UpdateAdminModelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminModel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listMissingAdminModelsQueryKey = (options?: Options<ListMissingAdminModelsData>) => createQueryKey('listMissingAdminModels', options);
export const listMissingAdminModelsOptions = (options?: Options<ListMissingAdminModelsData>) => queryOptions<ListMissingAdminModelsResponse, ListMissingAdminModelsError, ListMissingAdminModelsResponse, ReturnType<typeof listMissingAdminModelsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listMissingAdminModels({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listMissingAdminModelsQueryKey(options)
});
export const listMissingAdminModelsInfiniteQueryKey = (options?: Options<ListMissingAdminModelsData>): QueryKey<Options<ListMissingAdminModelsData>> => createQueryKey('listMissingAdminModels', options, true);
export const listMissingAdminModelsInfiniteOptions = (options?: Options<ListMissingAdminModelsData>) => {
    const opts = infiniteQueryOptions<ListMissingAdminModelsResponse, ListMissingAdminModelsError, InfiniteData<ListMissingAdminModelsResponse>, QueryKey<Options<ListMissingAdminModelsData>>, string | Pick<QueryKey<Options<ListMissingAdminModelsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListMissingAdminModelsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listMissingAdminModels({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listMissingAdminModelsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const importMissingAdminModelsMutation = (options?: Partial<Options<ImportMissingAdminModelsData>>): UseMutationOptions<ImportMissingAdminModelsResponse, ImportMissingAdminModelsError, Options<ImportMissingAdminModelsData>> => {
    const mutationOptions: UseMutationOptions<ImportMissingAdminModelsResponse, ImportMissingAdminModelsError, Options<ImportMissingAdminModelsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await importMissingAdminModels({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const createAdminModelSyncPreviewMutation = (options?: Partial<Options<CreateAdminModelSyncPreviewData>>): UseMutationOptions<CreateAdminModelSyncPreviewResponse, CreateAdminModelSyncPreviewError, Options<CreateAdminModelSyncPreviewData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminModelSyncPreviewResponse, CreateAdminModelSyncPreviewError, Options<CreateAdminModelSyncPreviewData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminModelSyncPreview({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const applyAdminModelSyncPreviewMutation = (options?: Partial<Options<ApplyAdminModelSyncPreviewData>>): UseMutationOptions<ApplyAdminModelSyncPreviewResponse, ApplyAdminModelSyncPreviewError, Options<ApplyAdminModelSyncPreviewData>> => {
    const mutationOptions: UseMutationOptions<ApplyAdminModelSyncPreviewResponse, ApplyAdminModelSyncPreviewError, Options<ApplyAdminModelSyncPreviewData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await applyAdminModelSyncPreview({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminModelPricesQueryKey = (options?: Options<ListAdminModelPricesData>) => createQueryKey('listAdminModelPrices', options);
export const listAdminModelPricesOptions = (options?: Options<ListAdminModelPricesData>) => queryOptions<ListAdminModelPricesResponse, ListAdminModelPricesError, ListAdminModelPricesResponse, ReturnType<typeof listAdminModelPricesQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminModelPrices({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminModelPricesQueryKey(options)
});
export const listAdminModelPricesInfiniteQueryKey = (options?: Options<ListAdminModelPricesData>): QueryKey<Options<ListAdminModelPricesData>> => createQueryKey('listAdminModelPrices', options, true);
export const listAdminModelPricesInfiniteOptions = (options?: Options<ListAdminModelPricesData>) => {
    const opts = infiniteQueryOptions<ListAdminModelPricesResponse, ListAdminModelPricesError, InfiniteData<ListAdminModelPricesResponse>, QueryKey<Options<ListAdminModelPricesData>>, string | Pick<QueryKey<Options<ListAdminModelPricesData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminModelPricesData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminModelPrices({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminModelPricesInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const previewAdminModelPricesMutation = (options?: Partial<Options<PreviewAdminModelPricesData>>): UseMutationOptions<PreviewAdminModelPricesResponse, PreviewAdminModelPricesError, Options<PreviewAdminModelPricesData>> => {
    const mutationOptions: UseMutationOptions<PreviewAdminModelPricesResponse, PreviewAdminModelPricesError, Options<PreviewAdminModelPricesData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await previewAdminModelPrices({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const previewAdminLiteLlmModelPricesMutation = (options?: Partial<Options<PreviewAdminLiteLlmModelPricesData>>): UseMutationOptions<PreviewAdminLiteLlmModelPricesResponse, PreviewAdminLiteLlmModelPricesError, Options<PreviewAdminLiteLlmModelPricesData>> => {
    const mutationOptions: UseMutationOptions<PreviewAdminLiteLlmModelPricesResponse, PreviewAdminLiteLlmModelPricesError, Options<PreviewAdminLiteLlmModelPricesData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await previewAdminLiteLlmModelPrices({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const previewAdminModelPriceExpressionMutation = (options?: Partial<Options<PreviewAdminModelPriceExpressionData>>): UseMutationOptions<PreviewAdminModelPriceExpressionResponse, PreviewAdminModelPriceExpressionError, Options<PreviewAdminModelPriceExpressionData>> => {
    const mutationOptions: UseMutationOptions<PreviewAdminModelPriceExpressionResponse, PreviewAdminModelPriceExpressionError, Options<PreviewAdminModelPriceExpressionData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await previewAdminModelPriceExpression({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const applyAdminModelPricesMutation = (options?: Partial<Options<ApplyAdminModelPricesData>>): UseMutationOptions<ApplyAdminModelPricesResponse, ApplyAdminModelPricesError, Options<ApplyAdminModelPricesData>> => {
    const mutationOptions: UseMutationOptions<ApplyAdminModelPricesResponse, ApplyAdminModelPricesError, Options<ApplyAdminModelPricesData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await applyAdminModelPrices({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const createPlaygroundShareMutation = (options?: Partial<Options<CreatePlaygroundShareData>>): UseMutationOptions<CreatePlaygroundShareResponse, CreatePlaygroundShareError, Options<CreatePlaygroundShareData>> => {
    const mutationOptions: UseMutationOptions<CreatePlaygroundShareResponse, CreatePlaygroundShareError, Options<CreatePlaygroundShareData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createPlaygroundShare({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const revokePlaygroundShareMutation = (options?: Partial<Options<RevokePlaygroundShareData>>): UseMutationOptions<RevokePlaygroundShareResponse, RevokePlaygroundShareError, Options<RevokePlaygroundShareData>> => {
    const mutationOptions: UseMutationOptions<RevokePlaygroundShareResponse, RevokePlaygroundShareError, Options<RevokePlaygroundShareData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await revokePlaygroundShare({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getPlaygroundShareQueryKey = (options: Options<GetPlaygroundShareData>) => createQueryKey('getPlaygroundShare', options);
export const getPlaygroundShareOptions = (options: Options<GetPlaygroundShareData>) => queryOptions<GetPlaygroundShareResponse, GetPlaygroundShareError, GetPlaygroundShareResponse, ReturnType<typeof getPlaygroundShareQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getPlaygroundShare({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getPlaygroundShareQueryKey(options)
});
export const listPlaygroundConversationsQueryKey = (options?: Options<ListPlaygroundConversationsData>) => createQueryKey('listPlaygroundConversations', options);
export const listPlaygroundConversationsOptions = (options?: Options<ListPlaygroundConversationsData>) => queryOptions<ListPlaygroundConversationsResponse, ListPlaygroundConversationsError, ListPlaygroundConversationsResponse, ReturnType<typeof listPlaygroundConversationsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listPlaygroundConversations({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listPlaygroundConversationsQueryKey(options)
});
export const deletePlaygroundConversationMutation = (options?: Partial<Options<DeletePlaygroundConversationData>>): UseMutationOptions<DeletePlaygroundConversationResponse, DeletePlaygroundConversationError, Options<DeletePlaygroundConversationData>> => {
    const mutationOptions: UseMutationOptions<DeletePlaygroundConversationResponse, DeletePlaygroundConversationError, Options<DeletePlaygroundConversationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deletePlaygroundConversation({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getPlaygroundConversationQueryKey = (options: Options<GetPlaygroundConversationData>) => createQueryKey('getPlaygroundConversation', options);
export const getPlaygroundConversationOptions = (options: Options<GetPlaygroundConversationData>) => queryOptions<GetPlaygroundConversationResponse, GetPlaygroundConversationError, GetPlaygroundConversationResponse, ReturnType<typeof getPlaygroundConversationQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getPlaygroundConversation({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getPlaygroundConversationQueryKey(options)
});
export const savePlaygroundConversationMutation = (options?: Partial<Options<SavePlaygroundConversationData>>): UseMutationOptions<SavePlaygroundConversationResponse, SavePlaygroundConversationError, Options<SavePlaygroundConversationData>> => {
    const mutationOptions: UseMutationOptions<SavePlaygroundConversationResponse, SavePlaygroundConversationError, Options<SavePlaygroundConversationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await savePlaygroundConversation({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listUserTokensQueryKey = (options?: Options<ListUserTokensData>) => createQueryKey('listUserTokens', options);
export const listUserTokensOptions = (options?: Options<ListUserTokensData>) => queryOptions<ListUserTokensResponse, ListUserTokensError, ListUserTokensResponse, ReturnType<typeof listUserTokensQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listUserTokens({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listUserTokensQueryKey(options)
});
export const listUserTokensInfiniteQueryKey = (options?: Options<ListUserTokensData>): QueryKey<Options<ListUserTokensData>> => createQueryKey('listUserTokens', options, true);
export const listUserTokensInfiniteOptions = (options?: Options<ListUserTokensData>) => {
    const opts = infiniteQueryOptions<ListUserTokensResponse, ListUserTokensError, InfiniteData<ListUserTokensResponse>, QueryKey<Options<ListUserTokensData>>, number | Pick<QueryKey<Options<ListUserTokensData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListUserTokensData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listUserTokens({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listUserTokensInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createUserTokenMutation = (options?: Partial<Options<CreateUserTokenData>>): UseMutationOptions<CreateUserTokenResponse, CreateUserTokenError, Options<CreateUserTokenData>> => {
    const mutationOptions: UseMutationOptions<CreateUserTokenResponse, CreateUserTokenError, Options<CreateUserTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createUserToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const createPlaygroundTokenMutation = (options?: Partial<Options<CreatePlaygroundTokenData>>): UseMutationOptions<CreatePlaygroundTokenResponse, CreatePlaygroundTokenError, Options<CreatePlaygroundTokenData>> => {
    const mutationOptions: UseMutationOptions<CreatePlaygroundTokenResponse, CreatePlaygroundTokenError, Options<CreatePlaygroundTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createPlaygroundToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteUserTokenMutation = (options?: Partial<Options<DeleteUserTokenData>>): UseMutationOptions<DeleteUserTokenResponse, DeleteUserTokenError, Options<DeleteUserTokenData>> => {
    const mutationOptions: UseMutationOptions<DeleteUserTokenResponse, DeleteUserTokenError, Options<DeleteUserTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteUserToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getUserTokenQueryKey = (options: Options<GetUserTokenData>) => createQueryKey('getUserToken', options);
export const getUserTokenOptions = (options: Options<GetUserTokenData>) => queryOptions<GetUserTokenResponse, GetUserTokenError, GetUserTokenResponse, ReturnType<typeof getUserTokenQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getUserToken({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getUserTokenQueryKey(options)
});
export const updateUserTokenMutation = (options?: Partial<Options<UpdateUserTokenData>>): UseMutationOptions<UpdateUserTokenResponse, UpdateUserTokenError, Options<UpdateUserTokenData>> => {
    const mutationOptions: UseMutationOptions<UpdateUserTokenResponse, UpdateUserTokenError, Options<UpdateUserTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateUserToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminTokensQueryKey = (options?: Options<ListAdminTokensData>) => createQueryKey('listAdminTokens', options);
export const listAdminTokensOptions = (options?: Options<ListAdminTokensData>) => queryOptions<ListAdminTokensResponse, ListAdminTokensError, ListAdminTokensResponse, ReturnType<typeof listAdminTokensQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminTokens({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminTokensQueryKey(options)
});
export const listAdminTokensInfiniteQueryKey = (options?: Options<ListAdminTokensData>): QueryKey<Options<ListAdminTokensData>> => createQueryKey('listAdminTokens', options, true);
export const listAdminTokensInfiniteOptions = (options?: Options<ListAdminTokensData>) => {
    const opts = infiniteQueryOptions<ListAdminTokensResponse, ListAdminTokensError, InfiniteData<ListAdminTokensResponse>, QueryKey<Options<ListAdminTokensData>>, number | Pick<QueryKey<Options<ListAdminTokensData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminTokensData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminTokens({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminTokensInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminTokenMutation = (options?: Partial<Options<CreateAdminTokenData>>): UseMutationOptions<CreateAdminTokenResponse, CreateAdminTokenError, Options<CreateAdminTokenData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminTokenResponse, CreateAdminTokenError, Options<CreateAdminTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminTokenMutation = (options?: Partial<Options<DeleteAdminTokenData>>): UseMutationOptions<DeleteAdminTokenResponse, DeleteAdminTokenError, Options<DeleteAdminTokenData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminTokenResponse, DeleteAdminTokenError, Options<DeleteAdminTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminTokenQueryKey = (options: Options<GetAdminTokenData>) => createQueryKey('getAdminToken', options);
export const getAdminTokenOptions = (options: Options<GetAdminTokenData>) => queryOptions<GetAdminTokenResponse, GetAdminTokenError, GetAdminTokenResponse, ReturnType<typeof getAdminTokenQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminToken({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminTokenQueryKey(options)
});
export const updateAdminTokenMutation = (options?: Partial<Options<UpdateAdminTokenData>>): UseMutationOptions<UpdateAdminTokenResponse, UpdateAdminTokenError, Options<UpdateAdminTokenData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminTokenResponse, UpdateAdminTokenError, Options<UpdateAdminTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminUsageLogsQueryKey = (options?: Options<ListAdminUsageLogsData>) => createQueryKey('listAdminUsageLogs', options);
export const listAdminUsageLogsOptions = (options?: Options<ListAdminUsageLogsData>) => queryOptions<ListAdminUsageLogsResponse, ListAdminUsageLogsError, ListAdminUsageLogsResponse, ReturnType<typeof listAdminUsageLogsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminUsageLogs({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminUsageLogsQueryKey(options)
});
export const listAdminUsageLogsInfiniteQueryKey = (options?: Options<ListAdminUsageLogsData>): QueryKey<Options<ListAdminUsageLogsData>> => createQueryKey('listAdminUsageLogs', options, true);
export const listAdminUsageLogsInfiniteOptions = (options?: Options<ListAdminUsageLogsData>) => {
    const opts = infiniteQueryOptions<ListAdminUsageLogsResponse, ListAdminUsageLogsError, InfiniteData<ListAdminUsageLogsResponse>, QueryKey<Options<ListAdminUsageLogsData>>, number | Pick<QueryKey<Options<ListAdminUsageLogsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminUsageLogsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminUsageLogs({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminUsageLogsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const listUserUsageLogsQueryKey = (options?: Options<ListUserUsageLogsData>) => createQueryKey('listUserUsageLogs', options);
export const listUserUsageLogsOptions = (options?: Options<ListUserUsageLogsData>) => queryOptions<ListUserUsageLogsResponse, ListUserUsageLogsError, ListUserUsageLogsResponse, ReturnType<typeof listUserUsageLogsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listUserUsageLogs({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listUserUsageLogsQueryKey(options)
});
export const listUserUsageLogsInfiniteQueryKey = (options?: Options<ListUserUsageLogsData>): QueryKey<Options<ListUserUsageLogsData>> => createQueryKey('listUserUsageLogs', options, true);
export const listUserUsageLogsInfiniteOptions = (options?: Options<ListUserUsageLogsData>) => {
    const opts = infiniteQueryOptions<ListUserUsageLogsResponse, ListUserUsageLogsError, InfiniteData<ListUserUsageLogsResponse>, QueryKey<Options<ListUserUsageLogsData>>, number | Pick<QueryKey<Options<ListUserUsageLogsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListUserUsageLogsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listUserUsageLogs({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listUserUsageLogsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const listAdminChannelsQueryKey = (options?: Options<ListAdminChannelsData>) => createQueryKey('listAdminChannels', options);
export const listAdminChannelsOptions = (options?: Options<ListAdminChannelsData>) => queryOptions<ListAdminChannelsResponse, ListAdminChannelsError, ListAdminChannelsResponse, ReturnType<typeof listAdminChannelsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminChannels({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminChannelsQueryKey(options)
});
export const listAdminChannelsInfiniteQueryKey = (options?: Options<ListAdminChannelsData>): QueryKey<Options<ListAdminChannelsData>> => createQueryKey('listAdminChannels', options, true);
export const listAdminChannelsInfiniteOptions = (options?: Options<ListAdminChannelsData>) => {
    const opts = infiniteQueryOptions<ListAdminChannelsResponse, ListAdminChannelsError, InfiniteData<ListAdminChannelsResponse>, QueryKey<Options<ListAdminChannelsData>>, number | Pick<QueryKey<Options<ListAdminChannelsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminChannelsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminChannels({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminChannelsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminChannelMutation = (options?: Partial<Options<CreateAdminChannelData>>): UseMutationOptions<CreateAdminChannelResponse, CreateAdminChannelError, Options<CreateAdminChannelData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminChannelResponse, CreateAdminChannelError, Options<CreateAdminChannelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminChannel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminChannelMutation = (options?: Partial<Options<DeleteAdminChannelData>>): UseMutationOptions<DeleteAdminChannelResponse, DeleteAdminChannelError, Options<DeleteAdminChannelData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminChannelResponse, DeleteAdminChannelError, Options<DeleteAdminChannelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminChannel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminChannelQueryKey = (options: Options<GetAdminChannelData>) => createQueryKey('getAdminChannel', options);
export const getAdminChannelOptions = (options: Options<GetAdminChannelData>) => queryOptions<GetAdminChannelResponse, GetAdminChannelError, GetAdminChannelResponse, ReturnType<typeof getAdminChannelQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminChannel({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminChannelQueryKey(options)
});
export const updateAdminChannelMutation = (options?: Partial<Options<UpdateAdminChannelData>>): UseMutationOptions<UpdateAdminChannelResponse, UpdateAdminChannelError, Options<UpdateAdminChannelData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminChannelResponse, UpdateAdminChannelError, Options<UpdateAdminChannelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminChannel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const probeAdminChannelMutation = (options?: Partial<Options<ProbeAdminChannelData>>): UseMutationOptions<ProbeAdminChannelResponse, ProbeAdminChannelError, Options<ProbeAdminChannelData>> => {
    const mutationOptions: UseMutationOptions<ProbeAdminChannelResponse, ProbeAdminChannelError, Options<ProbeAdminChannelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await probeAdminChannel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminCredentialsQueryKey = (options: Options<ListAdminCredentialsData>) => createQueryKey('listAdminCredentials', options);
export const listAdminCredentialsOptions = (options: Options<ListAdminCredentialsData>) => queryOptions<ListAdminCredentialsResponse, ListAdminCredentialsError, ListAdminCredentialsResponse, ReturnType<typeof listAdminCredentialsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminCredentials({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminCredentialsQueryKey(options)
});
export const listAdminCredentialsInfiniteQueryKey = (options: Options<ListAdminCredentialsData>): QueryKey<Options<ListAdminCredentialsData>> => createQueryKey('listAdminCredentials', options, true);
export const listAdminCredentialsInfiniteOptions = (options: Options<ListAdminCredentialsData>) => {
    const opts = infiniteQueryOptions<ListAdminCredentialsResponse, ListAdminCredentialsError, InfiniteData<ListAdminCredentialsResponse>, QueryKey<Options<ListAdminCredentialsData>>, number | Pick<QueryKey<Options<ListAdminCredentialsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminCredentialsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminCredentials({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminCredentialsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminCredentialMutation = (options?: Partial<Options<CreateAdminCredentialData>>): UseMutationOptions<CreateAdminCredentialResponse, CreateAdminCredentialError, Options<CreateAdminCredentialData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminCredentialResponse, CreateAdminCredentialError, Options<CreateAdminCredentialData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminCredential({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminCredentialMutation = (options?: Partial<Options<DeleteAdminCredentialData>>): UseMutationOptions<DeleteAdminCredentialResponse, DeleteAdminCredentialError, Options<DeleteAdminCredentialData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminCredentialResponse, DeleteAdminCredentialError, Options<DeleteAdminCredentialData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminCredential({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminCredentialQueryKey = (options: Options<GetAdminCredentialData>) => createQueryKey('getAdminCredential', options);
export const getAdminCredentialOptions = (options: Options<GetAdminCredentialData>) => queryOptions<GetAdminCredentialResponse, GetAdminCredentialError, GetAdminCredentialResponse, ReturnType<typeof getAdminCredentialQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminCredential({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminCredentialQueryKey(options)
});
export const updateAdminCredentialMutation = (options?: Partial<Options<UpdateAdminCredentialData>>): UseMutationOptions<UpdateAdminCredentialResponse, UpdateAdminCredentialError, Options<UpdateAdminCredentialData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminCredentialResponse, UpdateAdminCredentialError, Options<UpdateAdminCredentialData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminCredential({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminCredentialProxiesQueryKey = (options?: Options<ListAdminCredentialProxiesData>) => createQueryKey('listAdminCredentialProxies', options);
export const listAdminCredentialProxiesOptions = (options?: Options<ListAdminCredentialProxiesData>) => queryOptions<ListAdminCredentialProxiesResponse, ListAdminCredentialProxiesError, ListAdminCredentialProxiesResponse, ReturnType<typeof listAdminCredentialProxiesQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminCredentialProxies({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminCredentialProxiesQueryKey(options)
});
export const listAdminCredentialProxiesInfiniteQueryKey = (options?: Options<ListAdminCredentialProxiesData>): QueryKey<Options<ListAdminCredentialProxiesData>> => createQueryKey('listAdminCredentialProxies', options, true);
export const listAdminCredentialProxiesInfiniteOptions = (options?: Options<ListAdminCredentialProxiesData>) => {
    const opts = infiniteQueryOptions<ListAdminCredentialProxiesResponse, ListAdminCredentialProxiesError, InfiniteData<ListAdminCredentialProxiesResponse>, QueryKey<Options<ListAdminCredentialProxiesData>>, number | Pick<QueryKey<Options<ListAdminCredentialProxiesData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminCredentialProxiesData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminCredentialProxies({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminCredentialProxiesInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminCredentialProxyMutation = (options?: Partial<Options<CreateAdminCredentialProxyData>>): UseMutationOptions<CreateAdminCredentialProxyResponse, CreateAdminCredentialProxyError, Options<CreateAdminCredentialProxyData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminCredentialProxyResponse, CreateAdminCredentialProxyError, Options<CreateAdminCredentialProxyData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminCredentialProxy({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminCredentialProxyMutation = (options?: Partial<Options<DeleteAdminCredentialProxyData>>): UseMutationOptions<DeleteAdminCredentialProxyResponse, DeleteAdminCredentialProxyError, Options<DeleteAdminCredentialProxyData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminCredentialProxyResponse, DeleteAdminCredentialProxyError, Options<DeleteAdminCredentialProxyData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminCredentialProxy({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminCredentialProxyQueryKey = (options: Options<GetAdminCredentialProxyData>) => createQueryKey('getAdminCredentialProxy', options);
export const getAdminCredentialProxyOptions = (options: Options<GetAdminCredentialProxyData>) => queryOptions<GetAdminCredentialProxyResponse, GetAdminCredentialProxyError, GetAdminCredentialProxyResponse, ReturnType<typeof getAdminCredentialProxyQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminCredentialProxy({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminCredentialProxyQueryKey(options)
});
export const updateAdminCredentialProxyMutation = (options?: Partial<Options<UpdateAdminCredentialProxyData>>): UseMutationOptions<UpdateAdminCredentialProxyResponse, UpdateAdminCredentialProxyError, Options<UpdateAdminCredentialProxyData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminCredentialProxyResponse, UpdateAdminCredentialProxyError, Options<UpdateAdminCredentialProxyData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminCredentialProxy({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminCustomOAuth2ProvidersQueryKey = (options?: Options<ListAdminCustomOAuth2ProvidersData>) => createQueryKey('listAdminCustomOAuth2Providers', options);
export const listAdminCustomOAuth2ProvidersOptions = (options?: Options<ListAdminCustomOAuth2ProvidersData>) => queryOptions<ListAdminCustomOAuth2ProvidersResponse, ListAdminCustomOAuth2ProvidersError, ListAdminCustomOAuth2ProvidersResponse, ReturnType<typeof listAdminCustomOAuth2ProvidersQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminCustomOAuth2Providers({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminCustomOAuth2ProvidersQueryKey(options)
});
export const getAdminCustomOAuth2ProviderQueryKey = (options: Options<GetAdminCustomOAuth2ProviderData>) => createQueryKey('getAdminCustomOAuth2Provider', options);
export const getAdminCustomOAuth2ProviderOptions = (options: Options<GetAdminCustomOAuth2ProviderData>) => queryOptions<GetAdminCustomOAuth2ProviderResponse, GetAdminCustomOAuth2ProviderError, GetAdminCustomOAuth2ProviderResponse, ReturnType<typeof getAdminCustomOAuth2ProviderQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminCustomOAuth2Provider({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminCustomOAuth2ProviderQueryKey(options)
});
export const updateAdminCustomOAuth2ProviderMutation = (options?: Partial<Options<UpdateAdminCustomOAuth2ProviderData>>): UseMutationOptions<UpdateAdminCustomOAuth2ProviderResponse, UpdateAdminCustomOAuth2ProviderError, Options<UpdateAdminCustomOAuth2ProviderData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminCustomOAuth2ProviderResponse, UpdateAdminCustomOAuth2ProviderError, Options<UpdateAdminCustomOAuth2ProviderData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminCustomOAuth2Provider({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminDebugTraceSettingsQueryKey = (options?: Options<GetAdminDebugTraceSettingsData>) => createQueryKey('getAdminDebugTraceSettings', options);
export const getAdminDebugTraceSettingsOptions = (options?: Options<GetAdminDebugTraceSettingsData>) => queryOptions<GetAdminDebugTraceSettingsResponse, GetAdminDebugTraceSettingsError, GetAdminDebugTraceSettingsResponse, ReturnType<typeof getAdminDebugTraceSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminDebugTraceSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminDebugTraceSettingsQueryKey(options)
});
export const updateAdminDebugTraceSettingsMutation = (options?: Partial<Options<UpdateAdminDebugTraceSettingsData>>): UseMutationOptions<UpdateAdminDebugTraceSettingsResponse, UpdateAdminDebugTraceSettingsError, Options<UpdateAdminDebugTraceSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminDebugTraceSettingsResponse, UpdateAdminDebugTraceSettingsError, Options<UpdateAdminDebugTraceSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminDebugTraceSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminDebugTracesQueryKey = (options?: Options<ListAdminDebugTracesData>) => createQueryKey('listAdminDebugTraces', options);
export const listAdminDebugTracesOptions = (options?: Options<ListAdminDebugTracesData>) => queryOptions<ListAdminDebugTracesResponse, ListAdminDebugTracesError, ListAdminDebugTracesResponse, ReturnType<typeof listAdminDebugTracesQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminDebugTraces({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminDebugTracesQueryKey(options)
});
export const listAdminDebugTracesInfiniteQueryKey = (options?: Options<ListAdminDebugTracesData>): QueryKey<Options<ListAdminDebugTracesData>> => createQueryKey('listAdminDebugTraces', options, true);
export const listAdminDebugTracesInfiniteOptions = (options?: Options<ListAdminDebugTracesData>) => {
    const opts = infiniteQueryOptions<ListAdminDebugTracesResponse, ListAdminDebugTracesError, InfiniteData<ListAdminDebugTracesResponse>, QueryKey<Options<ListAdminDebugTracesData>>, number | Pick<QueryKey<Options<ListAdminDebugTracesData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminDebugTracesData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminDebugTraces({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminDebugTracesInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const getAdminDebugTraceQueryKey = (options: Options<GetAdminDebugTraceData>) => createQueryKey('getAdminDebugTrace', options);
export const getAdminDebugTraceOptions = (options: Options<GetAdminDebugTraceData>) => queryOptions<GetAdminDebugTraceResponse, GetAdminDebugTraceError, GetAdminDebugTraceResponse, ReturnType<typeof getAdminDebugTraceQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminDebugTrace({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminDebugTraceQueryKey(options)
});
export const readAdminDebugTraceSnapshotsMutation = (options?: Partial<Options<ReadAdminDebugTraceSnapshotsData>>): UseMutationOptions<ReadAdminDebugTraceSnapshotsResponse, ReadAdminDebugTraceSnapshotsError, Options<ReadAdminDebugTraceSnapshotsData>> => {
    const mutationOptions: UseMutationOptions<ReadAdminDebugTraceSnapshotsResponse, ReadAdminDebugTraceSnapshotsError, Options<ReadAdminDebugTraceSnapshotsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await readAdminDebugTraceSnapshots({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const receivePaymentWebhookMutation = (options?: Partial<Options<ReceivePaymentWebhookData>>): UseMutationOptions<unknown, DefaultError, Options<ReceivePaymentWebhookData>> => {
    const mutationOptions: UseMutationOptions<unknown, DefaultError, Options<ReceivePaymentWebhookData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await receivePaymentWebhook({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const receiveRefundWebhookMutation = (options?: Partial<Options<ReceiveRefundWebhookData>>): UseMutationOptions<unknown, DefaultError, Options<ReceiveRefundWebhookData>> => {
    const mutationOptions: UseMutationOptions<unknown, DefaultError, Options<ReceiveRefundWebhookData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await receiveRefundWebhook({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getOrganizationWalletQueryKey = (options: Options<GetOrganizationWalletData>) => createQueryKey('getOrganizationWallet', options);
export const getOrganizationWalletOptions = (options: Options<GetOrganizationWalletData>) => queryOptions<GetOrganizationWalletResponse, GetOrganizationWalletError, GetOrganizationWalletResponse, ReturnType<typeof getOrganizationWalletQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getOrganizationWallet({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getOrganizationWalletQueryKey(options)
});
export const listOrganizationWalletEntriesQueryKey = (options: Options<ListOrganizationWalletEntriesData>) => createQueryKey('listOrganizationWalletEntries', options);
export const listOrganizationWalletEntriesOptions = (options: Options<ListOrganizationWalletEntriesData>) => queryOptions<ListOrganizationWalletEntriesResponse, ListOrganizationWalletEntriesError, ListOrganizationWalletEntriesResponse, ReturnType<typeof listOrganizationWalletEntriesQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listOrganizationWalletEntries({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listOrganizationWalletEntriesQueryKey(options)
});
export const listOrganizationWalletEntriesInfiniteQueryKey = (options: Options<ListOrganizationWalletEntriesData>): QueryKey<Options<ListOrganizationWalletEntriesData>> => createQueryKey('listOrganizationWalletEntries', options, true);
export const listOrganizationWalletEntriesInfiniteOptions = (options: Options<ListOrganizationWalletEntriesData>) => {
    const opts = infiniteQueryOptions<ListOrganizationWalletEntriesResponse, ListOrganizationWalletEntriesError, InfiniteData<ListOrganizationWalletEntriesResponse>, QueryKey<Options<ListOrganizationWalletEntriesData>>, number | Pick<QueryKey<Options<ListOrganizationWalletEntriesData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListOrganizationWalletEntriesData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listOrganizationWalletEntries({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listOrganizationWalletEntriesInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const adjustOrganizationWalletMutation = (options?: Partial<Options<AdjustOrganizationWalletData>>): UseMutationOptions<AdjustOrganizationWalletResponse, AdjustOrganizationWalletError, Options<AdjustOrganizationWalletData>> => {
    const mutationOptions: UseMutationOptions<AdjustOrganizationWalletResponse, AdjustOrganizationWalletError, Options<AdjustOrganizationWalletData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await adjustOrganizationWallet({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const createOrganizationTopupOrderMutation = (options?: Partial<Options<CreateOrganizationTopupOrderData>>): UseMutationOptions<CreateOrganizationTopupOrderResponse, CreateOrganizationTopupOrderError, Options<CreateOrganizationTopupOrderData>> => {
    const mutationOptions: UseMutationOptions<CreateOrganizationTopupOrderResponse, CreateOrganizationTopupOrderError, Options<CreateOrganizationTopupOrderData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createOrganizationTopupOrder({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getOrganizationBudgetsQueryKey = (options: Options<GetOrganizationBudgetsData>) => createQueryKey('getOrganizationBudgets', options);
export const getOrganizationBudgetsOptions = (options: Options<GetOrganizationBudgetsData>) => queryOptions<GetOrganizationBudgetsResponse, GetOrganizationBudgetsError, GetOrganizationBudgetsResponse, ReturnType<typeof getOrganizationBudgetsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getOrganizationBudgets({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getOrganizationBudgetsQueryKey(options)
});
export const createOrganizationBudgetPolicyMutation = (options?: Partial<Options<CreateOrganizationBudgetPolicyData>>): UseMutationOptions<CreateOrganizationBudgetPolicyResponse, CreateOrganizationBudgetPolicyError, Options<CreateOrganizationBudgetPolicyData>> => {
    const mutationOptions: UseMutationOptions<CreateOrganizationBudgetPolicyResponse, CreateOrganizationBudgetPolicyError, Options<CreateOrganizationBudgetPolicyData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createOrganizationBudgetPolicy({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getOrganizationBudgetReservationQueryKey = (options: Options<GetOrganizationBudgetReservationData>) => createQueryKey('getOrganizationBudgetReservation', options);
export const getOrganizationBudgetReservationOptions = (options: Options<GetOrganizationBudgetReservationData>) => queryOptions<GetOrganizationBudgetReservationResponse, GetOrganizationBudgetReservationError, GetOrganizationBudgetReservationResponse, ReturnType<typeof getOrganizationBudgetReservationQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getOrganizationBudgetReservation({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getOrganizationBudgetReservationQueryKey(options)
});
export const listOrganizationTokensQueryKey = (options: Options<ListOrganizationTokensData>) => createQueryKey('listOrganizationTokens', options);
export const listOrganizationTokensOptions = (options: Options<ListOrganizationTokensData>) => queryOptions<ListOrganizationTokensResponse, ListOrganizationTokensError, ListOrganizationTokensResponse, ReturnType<typeof listOrganizationTokensQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listOrganizationTokens({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listOrganizationTokensQueryKey(options)
});
export const listOrganizationTokensInfiniteQueryKey = (options: Options<ListOrganizationTokensData>): QueryKey<Options<ListOrganizationTokensData>> => createQueryKey('listOrganizationTokens', options, true);
export const listOrganizationTokensInfiniteOptions = (options: Options<ListOrganizationTokensData>) => {
    const opts = infiniteQueryOptions<ListOrganizationTokensResponse, ListOrganizationTokensError, InfiniteData<ListOrganizationTokensResponse>, QueryKey<Options<ListOrganizationTokensData>>, number | Pick<QueryKey<Options<ListOrganizationTokensData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListOrganizationTokensData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listOrganizationTokens({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listOrganizationTokensInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createOrganizationTokenMutation = (options?: Partial<Options<CreateOrganizationTokenData>>): UseMutationOptions<CreateOrganizationTokenResponse, CreateOrganizationTokenError, Options<CreateOrganizationTokenData>> => {
    const mutationOptions: UseMutationOptions<CreateOrganizationTokenResponse, CreateOrganizationTokenError, Options<CreateOrganizationTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createOrganizationToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listOrganizationTokenDirectoryQueryKey = (options: Options<ListOrganizationTokenDirectoryData>) => createQueryKey('listOrganizationTokenDirectory', options);
export const listOrganizationTokenDirectoryOptions = (options: Options<ListOrganizationTokenDirectoryData>) => queryOptions<ListOrganizationTokenDirectoryResponse, ListOrganizationTokenDirectoryError, ListOrganizationTokenDirectoryResponse, ReturnType<typeof listOrganizationTokenDirectoryQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listOrganizationTokenDirectory({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listOrganizationTokenDirectoryQueryKey(options)
});
export const listOrganizationTokenDirectoryInfiniteQueryKey = (options: Options<ListOrganizationTokenDirectoryData>): QueryKey<Options<ListOrganizationTokenDirectoryData>> => createQueryKey('listOrganizationTokenDirectory', options, true);
export const listOrganizationTokenDirectoryInfiniteOptions = (options: Options<ListOrganizationTokenDirectoryData>) => {
    const opts = infiniteQueryOptions<ListOrganizationTokenDirectoryResponse, ListOrganizationTokenDirectoryError, InfiniteData<ListOrganizationTokenDirectoryResponse>, QueryKey<Options<ListOrganizationTokenDirectoryData>>, number | Pick<QueryKey<Options<ListOrganizationTokenDirectoryData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListOrganizationTokenDirectoryData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listOrganizationTokenDirectory({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listOrganizationTokenDirectoryInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const disableOwnOrganizationTokenMutation = (options?: Partial<Options<DisableOwnOrganizationTokenData>>): UseMutationOptions<DisableOwnOrganizationTokenResponse, DisableOwnOrganizationTokenError, Options<DisableOwnOrganizationTokenData>> => {
    const mutationOptions: UseMutationOptions<DisableOwnOrganizationTokenResponse, DisableOwnOrganizationTokenError, Options<DisableOwnOrganizationTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await disableOwnOrganizationToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getOrganizationTokenQueryKey = (options: Options<GetOrganizationTokenData>) => createQueryKey('getOrganizationToken', options);
export const getOrganizationTokenOptions = (options: Options<GetOrganizationTokenData>) => queryOptions<GetOrganizationTokenResponse, GetOrganizationTokenError, GetOrganizationTokenResponse, ReturnType<typeof getOrganizationTokenQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getOrganizationToken({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getOrganizationTokenQueryKey(options)
});
export const updateOrganizationTokenMutation = (options?: Partial<Options<UpdateOrganizationTokenData>>): UseMutationOptions<UpdateOrganizationTokenResponse, UpdateOrganizationTokenError, Options<UpdateOrganizationTokenData>> => {
    const mutationOptions: UseMutationOptions<UpdateOrganizationTokenResponse, UpdateOrganizationTokenError, Options<UpdateOrganizationTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateOrganizationToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const disableOrganizationTokenMutation = (options?: Partial<Options<DisableOrganizationTokenData>>): UseMutationOptions<DisableOrganizationTokenResponse, DisableOrganizationTokenError, Options<DisableOrganizationTokenData>> => {
    const mutationOptions: UseMutationOptions<DisableOrganizationTokenResponse, DisableOrganizationTokenError, Options<DisableOrganizationTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await disableOrganizationToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listOrganizationUsageLogsQueryKey = (options: Options<ListOrganizationUsageLogsData>) => createQueryKey('listOrganizationUsageLogs', options);
export const listOrganizationUsageLogsOptions = (options: Options<ListOrganizationUsageLogsData>) => queryOptions<ListOrganizationUsageLogsResponse, ListOrganizationUsageLogsError, ListOrganizationUsageLogsResponse, ReturnType<typeof listOrganizationUsageLogsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listOrganizationUsageLogs({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listOrganizationUsageLogsQueryKey(options)
});
export const listOrganizationUsageLogsInfiniteQueryKey = (options: Options<ListOrganizationUsageLogsData>): QueryKey<Options<ListOrganizationUsageLogsData>> => createQueryKey('listOrganizationUsageLogs', options, true);
export const listOrganizationUsageLogsInfiniteOptions = (options: Options<ListOrganizationUsageLogsData>) => {
    const opts = infiniteQueryOptions<ListOrganizationUsageLogsResponse, ListOrganizationUsageLogsError, InfiniteData<ListOrganizationUsageLogsResponse>, QueryKey<Options<ListOrganizationUsageLogsData>>, number | Pick<QueryKey<Options<ListOrganizationUsageLogsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListOrganizationUsageLogsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listOrganizationUsageLogs({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listOrganizationUsageLogsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const listOrganizationAuditLogsQueryKey = (options: Options<ListOrganizationAuditLogsData>) => createQueryKey('listOrganizationAuditLogs', options);
export const listOrganizationAuditLogsOptions = (options: Options<ListOrganizationAuditLogsData>) => queryOptions<ListOrganizationAuditLogsResponse, ListOrganizationAuditLogsError, ListOrganizationAuditLogsResponse, ReturnType<typeof listOrganizationAuditLogsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listOrganizationAuditLogs({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listOrganizationAuditLogsQueryKey(options)
});
export const listOrganizationAuditLogsInfiniteQueryKey = (options: Options<ListOrganizationAuditLogsData>): QueryKey<Options<ListOrganizationAuditLogsData>> => createQueryKey('listOrganizationAuditLogs', options, true);
export const listOrganizationAuditLogsInfiniteOptions = (options: Options<ListOrganizationAuditLogsData>) => {
    const opts = infiniteQueryOptions<ListOrganizationAuditLogsResponse, ListOrganizationAuditLogsError, InfiniteData<ListOrganizationAuditLogsResponse>, QueryKey<Options<ListOrganizationAuditLogsData>>, number | Pick<QueryKey<Options<ListOrganizationAuditLogsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListOrganizationAuditLogsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listOrganizationAuditLogs({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listOrganizationAuditLogsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const listPlatformOrganizationUsageLogsQueryKey = (options: Options<ListPlatformOrganizationUsageLogsData>) => createQueryKey('listPlatformOrganizationUsageLogs', options);
export const listPlatformOrganizationUsageLogsOptions = (options: Options<ListPlatformOrganizationUsageLogsData>) => queryOptions<ListPlatformOrganizationUsageLogsResponse, ListPlatformOrganizationUsageLogsError, ListPlatformOrganizationUsageLogsResponse, ReturnType<typeof listPlatformOrganizationUsageLogsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listPlatformOrganizationUsageLogs({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listPlatformOrganizationUsageLogsQueryKey(options)
});
export const listPlatformOrganizationUsageLogsInfiniteQueryKey = (options: Options<ListPlatformOrganizationUsageLogsData>): QueryKey<Options<ListPlatformOrganizationUsageLogsData>> => createQueryKey('listPlatformOrganizationUsageLogs', options, true);
export const listPlatformOrganizationUsageLogsInfiniteOptions = (options: Options<ListPlatformOrganizationUsageLogsData>) => {
    const opts = infiniteQueryOptions<ListPlatformOrganizationUsageLogsResponse, ListPlatformOrganizationUsageLogsError, InfiniteData<ListPlatformOrganizationUsageLogsResponse>, QueryKey<Options<ListPlatformOrganizationUsageLogsData>>, number | Pick<QueryKey<Options<ListPlatformOrganizationUsageLogsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListPlatformOrganizationUsageLogsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listPlatformOrganizationUsageLogs({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listPlatformOrganizationUsageLogsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const listPlatformOrganizationAuditLogsQueryKey = (options: Options<ListPlatformOrganizationAuditLogsData>) => createQueryKey('listPlatformOrganizationAuditLogs', options);
export const listPlatformOrganizationAuditLogsOptions = (options: Options<ListPlatformOrganizationAuditLogsData>) => queryOptions<ListPlatformOrganizationAuditLogsResponse, ListPlatformOrganizationAuditLogsError, ListPlatformOrganizationAuditLogsResponse, ReturnType<typeof listPlatformOrganizationAuditLogsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listPlatformOrganizationAuditLogs({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listPlatformOrganizationAuditLogsQueryKey(options)
});
export const listPlatformOrganizationAuditLogsInfiniteQueryKey = (options: Options<ListPlatformOrganizationAuditLogsData>): QueryKey<Options<ListPlatformOrganizationAuditLogsData>> => createQueryKey('listPlatformOrganizationAuditLogs', options, true);
export const listPlatformOrganizationAuditLogsInfiniteOptions = (options: Options<ListPlatformOrganizationAuditLogsData>) => {
    const opts = infiniteQueryOptions<ListPlatformOrganizationAuditLogsResponse, ListPlatformOrganizationAuditLogsError, InfiniteData<ListPlatformOrganizationAuditLogsResponse>, QueryKey<Options<ListPlatformOrganizationAuditLogsData>>, number | Pick<QueryKey<Options<ListPlatformOrganizationAuditLogsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListPlatformOrganizationAuditLogsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listPlatformOrganizationAuditLogs({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listPlatformOrganizationAuditLogsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const startPasskeyAuthenticationMutation = (options?: Partial<Options<StartPasskeyAuthenticationData>>): UseMutationOptions<StartPasskeyAuthenticationResponse, StartPasskeyAuthenticationError, Options<StartPasskeyAuthenticationData>> => {
    const mutationOptions: UseMutationOptions<StartPasskeyAuthenticationResponse, StartPasskeyAuthenticationError, Options<StartPasskeyAuthenticationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await startPasskeyAuthentication({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const finishPasskeyAuthenticationMutation = (options?: Partial<Options<FinishPasskeyAuthenticationData>>): UseMutationOptions<FinishPasskeyAuthenticationResponse, FinishPasskeyAuthenticationError, Options<FinishPasskeyAuthenticationData>> => {
    const mutationOptions: UseMutationOptions<FinishPasskeyAuthenticationResponse, FinishPasskeyAuthenticationError, Options<FinishPasskeyAuthenticationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await finishPasskeyAuthentication({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminPlatformAuditLogsQueryKey = (options?: Options<ListAdminPlatformAuditLogsData>) => createQueryKey('listAdminPlatformAuditLogs', options);
export const listAdminPlatformAuditLogsOptions = (options?: Options<ListAdminPlatformAuditLogsData>) => queryOptions<ListAdminPlatformAuditLogsResponse, ListAdminPlatformAuditLogsError, ListAdminPlatformAuditLogsResponse, ReturnType<typeof listAdminPlatformAuditLogsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminPlatformAuditLogs({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminPlatformAuditLogsQueryKey(options)
});
export const listAdminPlatformAuditLogsInfiniteQueryKey = (options?: Options<ListAdminPlatformAuditLogsData>): QueryKey<Options<ListAdminPlatformAuditLogsData>> => createQueryKey('listAdminPlatformAuditLogs', options, true);
export const listAdminPlatformAuditLogsInfiniteOptions = (options?: Options<ListAdminPlatformAuditLogsData>) => {
    const opts = infiniteQueryOptions<ListAdminPlatformAuditLogsResponse, ListAdminPlatformAuditLogsError, InfiniteData<ListAdminPlatformAuditLogsResponse>, QueryKey<Options<ListAdminPlatformAuditLogsData>>, number | Pick<QueryKey<Options<ListAdminPlatformAuditLogsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminPlatformAuditLogsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminPlatformAuditLogs({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminPlatformAuditLogsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const listSelfPlatformAuditLogsQueryKey = (options?: Options<ListSelfPlatformAuditLogsData>) => createQueryKey('listSelfPlatformAuditLogs', options);
export const listSelfPlatformAuditLogsOptions = (options?: Options<ListSelfPlatformAuditLogsData>) => queryOptions<ListSelfPlatformAuditLogsResponse, ListSelfPlatformAuditLogsError, ListSelfPlatformAuditLogsResponse, ReturnType<typeof listSelfPlatformAuditLogsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listSelfPlatformAuditLogs({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listSelfPlatformAuditLogsQueryKey(options)
});
export const listSelfPlatformAuditLogsInfiniteQueryKey = (options?: Options<ListSelfPlatformAuditLogsData>): QueryKey<Options<ListSelfPlatformAuditLogsData>> => createQueryKey('listSelfPlatformAuditLogs', options, true);
export const listSelfPlatformAuditLogsInfiniteOptions = (options?: Options<ListSelfPlatformAuditLogsData>) => {
    const opts = infiniteQueryOptions<ListSelfPlatformAuditLogsResponse, ListSelfPlatformAuditLogsError, InfiniteData<ListSelfPlatformAuditLogsResponse>, QueryKey<Options<ListSelfPlatformAuditLogsData>>, number | Pick<QueryKey<Options<ListSelfPlatformAuditLogsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListSelfPlatformAuditLogsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listSelfPlatformAuditLogs({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listSelfPlatformAuditLogsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const startOidcLoginMutation = (options?: Partial<Options<StartOidcLoginData>>): UseMutationOptions<StartOidcLoginResponse, StartOidcLoginError, Options<StartOidcLoginData>> => {
    const mutationOptions: UseMutationOptions<StartOidcLoginResponse, StartOidcLoginError, Options<StartOidcLoginData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await startOidcLogin({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const startLinuxDoLoginMutation = (options?: Partial<Options<StartLinuxDoLoginData>>): UseMutationOptions<StartLinuxDoLoginResponse, StartLinuxDoLoginError, Options<StartLinuxDoLoginData>> => {
    const mutationOptions: UseMutationOptions<StartLinuxDoLoginResponse, StartLinuxDoLoginError, Options<StartLinuxDoLoginData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await startLinuxDoLogin({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const startWeChatOAuthLoginMutation = (options?: Partial<Options<StartWeChatOAuthLoginData>>): UseMutationOptions<StartWeChatOAuthLoginResponse, StartWeChatOAuthLoginError, Options<StartWeChatOAuthLoginData>> => {
    const mutationOptions: UseMutationOptions<StartWeChatOAuthLoginResponse, StartWeChatOAuthLoginError, Options<StartWeChatOAuthLoginData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await startWeChatOAuthLogin({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const startTelegramLoginMutation = (options?: Partial<Options<StartTelegramLoginData>>): UseMutationOptions<StartTelegramLoginResponse, StartTelegramLoginError, Options<StartTelegramLoginData>> => {
    const mutationOptions: UseMutationOptions<StartTelegramLoginResponse, StartTelegramLoginError, Options<StartTelegramLoginData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await startTelegramLogin({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const startGoogleLoginMutation = (options?: Partial<Options<StartGoogleLoginData>>): UseMutationOptions<StartGoogleLoginResponse, StartGoogleLoginError, Options<StartGoogleLoginData>> => {
    const mutationOptions: UseMutationOptions<StartGoogleLoginResponse, StartGoogleLoginError, Options<StartGoogleLoginData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await startGoogleLogin({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const startCustomOAuth2LoginMutation = (options?: Partial<Options<StartCustomOAuth2LoginData>>): UseMutationOptions<StartCustomOAuth2LoginResponse, StartCustomOAuth2LoginError, Options<StartCustomOAuth2LoginData>> => {
    const mutationOptions: UseMutationOptions<StartCustomOAuth2LoginResponse, StartCustomOAuth2LoginError, Options<StartCustomOAuth2LoginData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await startCustomOAuth2Login({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const completeOidcLoginQueryKey = (options?: Options<CompleteOidcLoginData>) => createQueryKey('completeOidcLogin', options);
export const completeOidcLoginOptions = (options?: Options<CompleteOidcLoginData>) => queryOptions<unknown, CompleteOidcLoginError, unknown, ReturnType<typeof completeOidcLoginQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await completeOidcLogin({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: completeOidcLoginQueryKey(options)
});
export const completeLinuxDoLoginQueryKey = (options?: Options<CompleteLinuxDoLoginData>) => createQueryKey('completeLinuxDoLogin', options);
export const completeLinuxDoLoginOptions = (options?: Options<CompleteLinuxDoLoginData>) => queryOptions<unknown, CompleteLinuxDoLoginError, unknown, ReturnType<typeof completeLinuxDoLoginQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await completeLinuxDoLogin({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: completeLinuxDoLoginQueryKey(options)
});
export const completeWeChatOAuthLoginQueryKey = (options?: Options<CompleteWeChatOAuthLoginData>) => createQueryKey('completeWeChatOAuthLogin', options);
export const completeWeChatOAuthLoginOptions = (options?: Options<CompleteWeChatOAuthLoginData>) => queryOptions<unknown, CompleteWeChatOAuthLoginError, unknown, ReturnType<typeof completeWeChatOAuthLoginQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await completeWeChatOAuthLogin({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: completeWeChatOAuthLoginQueryKey(options)
});
export const completeTelegramLoginQueryKey = (options?: Options<CompleteTelegramLoginData>) => createQueryKey('completeTelegramLogin', options);
export const completeTelegramLoginOptions = (options?: Options<CompleteTelegramLoginData>) => queryOptions<unknown, CompleteTelegramLoginError, unknown, ReturnType<typeof completeTelegramLoginQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await completeTelegramLogin({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: completeTelegramLoginQueryKey(options)
});
export const completeGoogleLoginQueryKey = (options?: Options<CompleteGoogleLoginData>) => createQueryKey('completeGoogleLogin', options);
export const completeGoogleLoginOptions = (options?: Options<CompleteGoogleLoginData>) => queryOptions<unknown, CompleteGoogleLoginError, unknown, ReturnType<typeof completeGoogleLoginQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await completeGoogleLogin({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: completeGoogleLoginQueryKey(options)
});
export const completeCustomOAuth2LoginQueryKey = (options: Options<CompleteCustomOAuth2LoginData>) => createQueryKey('completeCustomOAuth2Login', options);
export const completeCustomOAuth2LoginOptions = (options: Options<CompleteCustomOAuth2LoginData>) => queryOptions<unknown, CompleteCustomOAuth2LoginError, unknown, ReturnType<typeof completeCustomOAuth2LoginQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await completeCustomOAuth2Login({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: completeCustomOAuth2LoginQueryKey(options)
});
export const getAdminOidcLoginSettingsQueryKey = (options?: Options<GetAdminOidcLoginSettingsData>) => createQueryKey('getAdminOidcLoginSettings', options);
export const getAdminOidcLoginSettingsOptions = (options?: Options<GetAdminOidcLoginSettingsData>) => queryOptions<GetAdminOidcLoginSettingsResponse, GetAdminOidcLoginSettingsError, GetAdminOidcLoginSettingsResponse, ReturnType<typeof getAdminOidcLoginSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminOidcLoginSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminOidcLoginSettingsQueryKey(options)
});
export const updateAdminOidcLoginSettingsMutation = (options?: Partial<Options<UpdateAdminOidcLoginSettingsData>>): UseMutationOptions<UpdateAdminOidcLoginSettingsResponse, UpdateAdminOidcLoginSettingsError, Options<UpdateAdminOidcLoginSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminOidcLoginSettingsResponse, UpdateAdminOidcLoginSettingsError, Options<UpdateAdminOidcLoginSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminOidcLoginSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminLinuxDoLoginSettingsQueryKey = (options?: Options<GetAdminLinuxDoLoginSettingsData>) => createQueryKey('getAdminLinuxDoLoginSettings', options);
export const getAdminLinuxDoLoginSettingsOptions = (options?: Options<GetAdminLinuxDoLoginSettingsData>) => queryOptions<GetAdminLinuxDoLoginSettingsResponse, GetAdminLinuxDoLoginSettingsError, GetAdminLinuxDoLoginSettingsResponse, ReturnType<typeof getAdminLinuxDoLoginSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminLinuxDoLoginSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminLinuxDoLoginSettingsQueryKey(options)
});
export const updateAdminLinuxDoLoginSettingsMutation = (options?: Partial<Options<UpdateAdminLinuxDoLoginSettingsData>>): UseMutationOptions<UpdateAdminLinuxDoLoginSettingsResponse, UpdateAdminLinuxDoLoginSettingsError, Options<UpdateAdminLinuxDoLoginSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminLinuxDoLoginSettingsResponse, UpdateAdminLinuxDoLoginSettingsError, Options<UpdateAdminLinuxDoLoginSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminLinuxDoLoginSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminWeChatOAuthLoginSettingsQueryKey = (options?: Options<GetAdminWeChatOAuthLoginSettingsData>) => createQueryKey('getAdminWeChatOAuthLoginSettings', options);
export const getAdminWeChatOAuthLoginSettingsOptions = (options?: Options<GetAdminWeChatOAuthLoginSettingsData>) => queryOptions<GetAdminWeChatOAuthLoginSettingsResponse, GetAdminWeChatOAuthLoginSettingsError, GetAdminWeChatOAuthLoginSettingsResponse, ReturnType<typeof getAdminWeChatOAuthLoginSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminWeChatOAuthLoginSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminWeChatOAuthLoginSettingsQueryKey(options)
});
export const updateAdminWeChatOAuthLoginSettingsMutation = (options?: Partial<Options<UpdateAdminWeChatOAuthLoginSettingsData>>): UseMutationOptions<UpdateAdminWeChatOAuthLoginSettingsResponse, UpdateAdminWeChatOAuthLoginSettingsError, Options<UpdateAdminWeChatOAuthLoginSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminWeChatOAuthLoginSettingsResponse, UpdateAdminWeChatOAuthLoginSettingsError, Options<UpdateAdminWeChatOAuthLoginSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminWeChatOAuthLoginSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminTelegramOAuthLoginSettingsQueryKey = (options?: Options<GetAdminTelegramOAuthLoginSettingsData>) => createQueryKey('getAdminTelegramOAuthLoginSettings', options);
export const getAdminTelegramOAuthLoginSettingsOptions = (options?: Options<GetAdminTelegramOAuthLoginSettingsData>) => queryOptions<GetAdminTelegramOAuthLoginSettingsResponse, GetAdminTelegramOAuthLoginSettingsError, GetAdminTelegramOAuthLoginSettingsResponse, ReturnType<typeof getAdminTelegramOAuthLoginSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminTelegramOAuthLoginSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminTelegramOAuthLoginSettingsQueryKey(options)
});
export const updateAdminTelegramOAuthLoginSettingsMutation = (options?: Partial<Options<UpdateAdminTelegramOAuthLoginSettingsData>>): UseMutationOptions<UpdateAdminTelegramOAuthLoginSettingsResponse, UpdateAdminTelegramOAuthLoginSettingsError, Options<UpdateAdminTelegramOAuthLoginSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminTelegramOAuthLoginSettingsResponse, UpdateAdminTelegramOAuthLoginSettingsError, Options<UpdateAdminTelegramOAuthLoginSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminTelegramOAuthLoginSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminGoogleOAuthLoginSettingsQueryKey = (options?: Options<GetAdminGoogleOAuthLoginSettingsData>) => createQueryKey('getAdminGoogleOAuthLoginSettings', options);
export const getAdminGoogleOAuthLoginSettingsOptions = (options?: Options<GetAdminGoogleOAuthLoginSettingsData>) => queryOptions<GetAdminGoogleOAuthLoginSettingsResponse, GetAdminGoogleOAuthLoginSettingsError, GetAdminGoogleOAuthLoginSettingsResponse, ReturnType<typeof getAdminGoogleOAuthLoginSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminGoogleOAuthLoginSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminGoogleOAuthLoginSettingsQueryKey(options)
});
export const updateAdminGoogleOAuthLoginSettingsMutation = (options?: Partial<Options<UpdateAdminGoogleOAuthLoginSettingsData>>): UseMutationOptions<UpdateAdminGoogleOAuthLoginSettingsResponse, UpdateAdminGoogleOAuthLoginSettingsError, Options<UpdateAdminGoogleOAuthLoginSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminGoogleOAuthLoginSettingsResponse, UpdateAdminGoogleOAuthLoginSettingsError, Options<UpdateAdminGoogleOAuthLoginSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminGoogleOAuthLoginSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
