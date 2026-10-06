//! 从 Rust 路由契约生成管理 API 的 OpenAPI 文档。

mod account_verification;
mod analytics_export;
mod announcements;
mod balance_alert_settings;
mod channels;
mod credential_proxies;
mod credentials;
mod custom_oauth2;
mod dashboard;
mod debug_traces;
mod email_settings;
mod extensions;
mod frontend_templates;
mod gateway_models;
mod groups;
mod invitations;
mod model_metadata;
mod model_prices;
mod model_sync;
mod models;
mod network_settings;
mod oauth_connections;
mod oauth_login;
mod passkey_auth;
mod password_reset;
mod payment_settings;
mod payment_webhook;
mod platform_audit;
mod playground_conversations;
mod playground_shares;
mod redemptions;
mod refund_webhook;
mod refunds;
mod registration;
mod rerank;
mod responses_compact;
mod routes;
pub(crate) mod schema;
mod session;
mod setup;
mod site_settings;
mod speech;
mod subscriptions;
mod tokens;
mod usage_logs;
mod user_notifications;
mod user_profile;
mod user_tokens;
mod user_topups;
mod user_wallet;
mod users;
mod verification_settings;
mod videos;
mod wallet;

use utoipa::openapi::{
    Components, Info, OpenApi, Paths,
    security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    tag::Tag,
};

/// 构建可导出、可供客户端生成和 Apifox 导入的完整管理 API 文档。
pub fn openapi_document() -> OpenApi {
    let mut document = OpenApi::new(
        Info::new("AnyFlows 管理 API", env!("CARGO_PKG_VERSION")),
        Paths::new(),
    );
    document.merge(session::document());
    document.merge(announcements::document());
    document.merge(setup::document());
    document.merge(registration::document());
    document.merge(rerank::document());
    document.merge(responses_compact::document());
    document.merge(speech::document());
    document.merge(videos::document());
    document.merge(redemptions::document());
    document.merge(refunds::document());
    document.merge(subscriptions::document());
    document.merge(password_reset::document());
    document.merge(user_profile::document());
    document.merge(user_wallet::document());
    document.merge(user_notifications::document());
    document.merge(user_topups::document());
    document.merge(invitations::document());
    document.merge(email_settings::document());
    document.merge(extensions::document());
    document.merge(network_settings::document());
    document.merge(payment_settings::document());
    document.merge(oauth_connections::document());
    document.merge(oauth_login::document());
    document.merge(balance_alert_settings::document());
    document.merge(site_settings::document());
    document.merge(frontend_templates::document());
    document.merge(dashboard::document());
    document.merge(analytics_export::document());
    document.merge(users::document());
    document.merge(wallet::document());
    document.merge(groups::document());
    document.merge(routes::document());
    document.merge(models::document());
    document.merge(gateway_models::document());
    document.merge(model_metadata::document());
    document.merge(model_sync::document());
    document.merge(model_prices::document());
    document.merge(playground_shares::document());
    document.merge(playground_conversations::document());
    document.merge(user_tokens::document());
    document.merge(tokens::document());
    document.merge(usage_logs::document());
    document.merge(channels::document());
    document.merge(credentials::document());
    document.merge(credential_proxies::document());
    document.merge(custom_oauth2::document());
    document.merge(debug_traces::document());
    document.merge(payment_webhook::document());
    document.merge(refund_webhook::document());
    document.merge(account_verification::document());
    document.merge(verification_settings::document());
    document.merge(passkey_auth::document());
    document.merge(platform_audit::document());
    document.merge(oauth_login::oidc_document());

    document.tags = Some(vec![
        Tag::new("公开注册"),
        Tag::new("公告"),
        Tag::new("OpenAI Responses"),
        Tag::new("OpenAI Audio"),
        Tag::new("视频任务"),
        Tag::new("Rerank"),
        Tag::new("认证设置"),
        Tag::new("密码重置"),
        Tag::new("个人资料与安全"),
        Tag::new("我的钱包"),
        Tag::new("兑换码"),
        Tag::new("退款审批"),
        Tag::new("退款对账"),
        Tag::new("订阅管理"),
        Tag::new("邀请中心"),
        Tag::new("站点设置"),
        Tag::new("邮件设置"),
        Tag::new("网络与代理"),
        Tag::new("支付设置"),
        Tag::new("OAuth 连接"),
        Tag::new("OAuth 登录"),
        Tag::new("Passkey 登录"),
        Tag::new("平台管理审计"),
        Tag::new("余额预警"),
        Tag::new("管理会话"),
        Tag::new("首次安装"),
        Tag::new("管理看板"),
        Tag::new("分析导出"),
        Tag::new("用户管理"),
        Tag::new("钱包账本"),
        Tag::new("分组管理"),
        Tag::new("智能路由"),
        Tag::new("模型目录"),
        Tag::new("模型管理"),
        Tag::new("Playground 分享"),
        Tag::new("Playground 历史"),
        Tag::new("我的 API Key"),
        Tag::new("令牌管理"),
        Tag::new("用量日志"),
        Tag::new("渠道管理"),
        Tag::new("凭据管理"),
        Tag::new("专属代理"),
        Tag::new("调试追踪"),
        Tag::new("支付回调"),
        Tag::new("退款回执"),
    ]);
    document
        .components
        .get_or_insert_with(Components::new)
        .add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some("登录会话 Bearer JWT"))
                    .build(),
            ),
        );
    document
        .components
        .get_or_insert_with(Components::new)
        .add_security_scheme(
            "apiKeyAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("API Key")
                    .description(Some("AnyFlows 网关 API Key"))
                    .build(),
            ),
        );
    document
}

/// Builds the core OpenAPI document and merges documents supplied by private
/// extensions. The default [`openapi_document`] remains deterministic for
/// public builds and generated contract checks.
pub fn openapi_document_with_extensions(
    extensions: Option<&crate::http_extensions::HttpExtensions>,
) -> OpenApi {
    let mut document = openapi_document();
    if let Some(extensions) = extensions {
        for extension_document in extensions.openapi_documents() {
            document.merge(extension_document);
        }
    }
    document
}

#[cfg(all(test, any()))]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn document_keeps_the_management_contract_surface() {
        let document = openapi_document();
        assert_eq!(document.paths.paths.len(), 280);
        let operation_count = document
            .paths
            .paths
            .values()
            .map(|item| {
                [
                    &item.get,
                    &item.put,
                    &item.post,
                    &item.delete,
                    &item.options,
                    &item.head,
                    &item.patch,
                    &item.trace,
                ]
                .into_iter()
                .flatten()
                .count()
            })
            .sum::<usize>();
        // 保持操作总数与生成文档同步。
        assert_eq!(operation_count, 363);
        assert!(
            document
                .components
                .as_ref()
                .is_some_and(|components| components.security_schemes.contains_key("bearerAuth"))
        );
        assert!(
            document
                .components
                .as_ref()
                .is_some_and(|components| components.security_schemes.contains_key("apiKeyAuth"))
        );
        assert!(
            document
                .components
                .as_ref()
                .is_some_and(|components| components
                    .security_schemes
                    .contains_key("scimBearerAuth"))
        );
        assert!(document.components.as_ref().is_some_and(|components| {
            components
                .security_schemes
                .contains_key("serviceAccountBearerAuth")
        }));
    }

    #[test]
    fn document_keeps_stable_operation_and_schema_names() {
        let document = serde_json::to_value(openapi_document()).unwrap();
        let operation_ids = document["paths"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|path| path.as_object().unwrap().values())
            .filter_map(|operation| operation.get("operationId"))
            .map(|operation_id| operation_id.as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>();
        let expected = [
            "decideAccountVerification",
            "downloadAccountVerificationMaterial",
            "downloadAdminAccountVerificationMaterial",
            "downloadSelfProvisioningMaterial",
            "downloadSelfVerificationMaterial",
            "getAccountVerification",
            "getAccountVerificationEligibility",
            "getAdminAccountVerification",
            "getAdminVerificationHistory",
            "getAdminVerificationSettings",
            "getSelfVerificationHistory",
            "listAccountVerifications",
            "listAdminAccountVerifications",
            "list_extension_catalog",
            "listSelfOrganizationProvisioning",
            "listSelfProvisioningMaterials",
            "submitAccountVerification",
            "syncAccountVerificationProvider",
            "updatePlatformOrganizationEntitlement",
            "updateAdminVerificationSettings",
            "listPublicAnnouncements",
            "listAdminAnnouncements",
            "createAdminAnnouncement",
            "updateAdminAnnouncement",
            "publishAdminAnnouncement",
            "revokeAdminAnnouncement",
            "compactResponse",
            "rerank",
            "synthesizeSpeech",
            "submitVideoTask",
            "listVideoTasks",
            "pollVideoTask",
            "createAdminChannel",
            "createAdminCredential",
            "importAdminCredentials",
            "exportAdminCredentials",
            "createAdminCredentialProxy",
            "createAdminGroup",
            "createAdminRoute",
            "createAdminModel",
            "createAdminModelSyncPreview",
            "createAdminRedemptionBatch",
            "createAdminSubscriptionPlan",
            "createAdminToken",
            "createAdminUser",
            "beginAdminOAuthAuthorization",
            "startGitHubOAuthLogin",
            "completeGitHubOAuthLogin",
            "startDiscordOAuthLogin",
            "completeDiscordOAuthLogin",
            "startOidcLogin",
            "completeOidcLogin",
            "startLinuxDoLogin",
            "completeLinuxDoLogin",
            "startWeChatOAuthLogin",
            "completeWeChatOAuthLogin",
            "startTelegramLogin",
            "completeTelegramLogin",
            "startGoogleLogin",
            "completeGoogleLogin",
            "startCustomOAuth2Login",
            "completeCustomOAuth2Login",
            "exchangeOAuthLoginTicket",
            "discoverOrganizationSso",
            "getAdminGitHubOAuthLoginSettings",
            "updateAdminGitHubOAuthLoginSettings",
            "getAdminDiscordOAuthLoginSettings",
            "updateAdminDiscordOAuthLoginSettings",
            "getAdminOidcLoginSettings",
            "updateAdminOidcLoginSettings",
            "getAdminLinuxDoLoginSettings",
            "updateAdminLinuxDoLoginSettings",
            "getAdminWeChatOAuthLoginSettings",
            "updateAdminWeChatOAuthLoginSettings",
            "getAdminTelegramOAuthLoginSettings",
            "updateAdminTelegramOAuthLoginSettings",
            "getAdminGoogleOAuthLoginSettings",
            "updateAdminGoogleOAuthLoginSettings",
            "listAdminCustomOAuth2Providers",
            "getAdminCustomOAuth2Provider",
            "updateAdminCustomOAuth2Provider",
            "confirmPasswordReset",
            "completeAdminOAuthManualCallback",
            "getUserProfile",
            "sendUserEmailBindingVerification",
            "confirmUserEmailBinding",
            "listAccountOrganizations",
            "getOrganizationOverview",
            "updateOrganizationProfile",
            "listOrganizationApprovalTemplates",
            "createOrganizationApprovalTemplate",
            "listOrganizationApprovalRequests",
            "createOrganizationApprovalRequest",
            "getOrganizationApprovalRequest",
            "decideOrganizationApprovalRequest",
            "listOrganizationDepartments",
            "createOrganizationDepartment",
            "getOrganizationDepartment",
            "moveOrganizationDepartment",
            "disableOrganizationDepartment",
            "getOrganizationWallet",
            "getOrganizationBudgets",
            "listOrganizationContractPrices",
            "getOrganizationContractPrice",
            "createOrganizationContractPrice",
            "closeOrganizationContractPrice",
            "listOrganizationCreditTerms",
            "getOrganizationCreditTerm",
            "createOrganizationCreditTerm",
            "updateOrganizationCreditTermStatus",
            "listOrganizationCreditInvoices",
            "getOrganizationCreditInvoice",
            "listOrganizationCreditRepayments",
            "getOrganizationCreditRepayment",
            "listOrganizationCreditAllocations",
            "getOrganizationCreditAllocation",
            "createOrganizationBudgetPolicy",
            "getOrganizationBudgetReservation",
            "listOrganizationTokens",
            "createOrganizationToken",
            "listOrganizationTokenDirectory",
            "getOrganizationToken",
            "updateOrganizationToken",
            "disableOwnOrganizationToken",
            "disableOrganizationToken",
            "listOrganizationInvitations",
            "createOrganizationInvitation",
            "previewOrganizationInvitations",
            "createOrganizationInvitationBatch",
            "createOrganizationTopupOrder",
            "revokeOrganizationInvitation",
            "resendOrganizationInvitation",
            "acceptOrganizationInvitation",
            "listOrganizationMembers",
            "changeOrganizationMember",
            "listOrganizationMemberDepartmentMemberships",
            "addOrganizationMemberDepartmentMembership",
            "setPrimaryOrganizationMemberDepartmentMembership",
            "removeOrganizationMemberDepartmentMembership",
            "setOrganizationMemberCustomRole",
            "transferOrganizationOwnership",
            "listOrganizationTeams",
            "createOrganizationTeam",
            "changeOrganizationTeam",
            "disableOrganizationTeam",
            "listOrganizationCustomRoles",
            "getOrganizationCustomRole",
            "createOrganizationCustomRole",
            "updateOrganizationCustomRole",
            "disableOrganizationCustomRole",
            "listOrganizationServiceAccounts",
            "getOrganizationServiceAccount",
            "createOrganizationServiceAccount",
            "changeOrganizationServiceAccountStatus",
            "rotateOrganizationServiceAccountKey",
            "getOrganizationServiceAccountBudgetBinding",
            "setOrganizationServiceAccountBudgetBinding",
            "authenticateOrganizationServiceAccountRuntime",
            "listOrganizationSsoProviders",
            "createOrganizationSsoProvider",
            "getOrganizationSsoProvider",
            "updateOrganizationSsoProvider",
            "listOrganizationSsoDomains",
            "createOrganizationSsoDomain",
            "verifyOrganizationSsoDomain",
            "startPublicOrganizationSsoLogin",
            "startOrganizationSsoLogin",
            "bindOrganizationSsoIdentity",
            "getOrganizationSsoPolicy",
            "updateOrganizationSsoPolicy",
            "establishOrganizationSsoRecovery",
            "suspendOrganizationSsoRequired",
            "completeOrganizationSsoOidcLogin",
            "completeOrganizationSsoSamlLogin",
            "exchangeOrganizationSsoSession",
            "createOrganizationProvisioningRequest",
            "getActiveOrganizationProvisioningRequest",
            "getOrganizationProvisioningRequest",
            "listAdminOrganizationProvisioningRequests",
            "getAdminOrganizationProvisioningRequest",
            "decideOrganizationProvisioningRequest",
            "listAdminOrganizationProvisioningMaterials",
            "downloadAdminOrganizationProvisioningMaterial",
            "getOrganizationVerification",
            "submitOrganizationVerification",
            "listAdminOrganizationVerifications",
            "getAdminOrganizationVerification",
            "decideOrganizationVerification",
            "downloadAdminOrganizationVerificationMaterial",
            "listAdminOrganizationPlans",
            "createAdminOrganizationPlan",
            "disableAdminOrganizationPlan",
            "getOrganizationPlanCatalog",
            "createOrganizationPlanOrder",
            "getOrganizationPlanOrder",
            "submitOrganizationPlanOrderPayment",
            "getOrganizationEntitlement",
            "getPlatformOrganizationEntitlement",
            "getUserTopupConfiguration",
            "getUserWallet",
            "importMissingAdminModels",
            "getUserInvitations",
            "updateUserProfile",
            "changeUserPassword",
            "updateUserNotificationPreferences",
            "listUserNotifications",
            "markUserNotificationsRead",
            "listOrganizationApprovalNotifications",
            "markOrganizationApprovalNotificationsRead",
            "createPlaygroundShare",
            "createUserToken",
            "createUserTopupOrder",
            "deletePlaygroundConversation",
            "deleteAdminChannel",
            "deleteAdminCredential",
            "deleteAdminCredentialProxy",
            "deleteAdminGroup",
            "deleteAdminRoute",
            "deleteAdminModel",
            "deleteAdminToken",
            "deleteAdminUser",
            "deleteUserToken",
            "disableAdminRedemptionBatch",
            "disableAdminSubscriptionPlan",
            "getAdminChannel",
            "getAdminBalanceAlertSettings",
            "getAdminCredential",
            "getAdminCredentialProxy",
            "getAdminCredentialUsage",
            "getAdminDashboard",
            "getAdminServiceLevels",
            "getAdminAnalyticsExportStatus",
            "getAdminDebugTrace",
            "getAdminDebugTraceSettings",
            "getAdminEmailSettings",
            "getAdminNetworkSettings",
            "getAdminPaymentSettings",
            "getAdminGroup",
            "getAdminRoute",
            "getAdminModel",
            "getAdminAuthenticationSettings",
            "getAdminSiteSettings",
            "getAdminToken",
            "getAdminUser",
            "getManagementSession",
            "getPlaygroundShare",
            "getPlaygroundConversation",
            "getUserToken",
            "getInitialSetupStatus",
            "getRegistrationStatus",
            "getPublicSiteSettings",
            "initializeAdminSetup",
            "applyAdminModelSyncPreview",
            "applyAdminModelPrices",
            "listAdminChannels",
            "listAdminCredentials",
            "listAdminFrontendTemplates",
            "getAdminFrontendTemplatePreview",
            "listAdminCredentialProxies",
            "listAdminDebugTraces",
            "listAdminGroups",
            "listAdminRoutes",
            "listAdminModels",
            "listAdminOAuthProviders",
            "listAdminModelPrices",
            "listAdminRedemptionAudit",
            "listAdminRedemptionBatches",
            "listAdminSubscriptionPlans",
            "listAdminUserSubscriptions",
            "listCurrentSubscriptionCatalog",
            "createCurrentSubscriptionOrder",
            "getCurrentSubscriptionOrder",
            "submitCurrentSubscriptionOrderPayment",
            "listMissingAdminModels",
            "listAdminTokens",
            "listAdminUsageLogs",
            "listAdminPlatformAuditLogs",
            "listOrganizationUsageLogs",
            "listOrganizationAuditLogs",
            "listOrganizationScimAuditLogs",
            "listPlatformOrganizationUsageLogs",
            "listPlatformOrganizationAuditLogs",
            "listPlatformOrganizations",
            "listPlatformOrganizationMembers",
            "listPlatformUserOrganizations",
            "listUserUsageLogs",
            "listSelfPlatformAuditLogs",
            "listAdminUsers",
            "listModels",
            "listGatewayModels",
            "listModelProviders",
            "listPlaygroundConversations",
            "listUserTokens",
            "receivePaymentWebhook",
            "receiveRefundWebhook",
            "listUserWalletEntries",
            "listCurrentUserSubscriptions",
            "loginManagementSession",
            "startPasskeyAuthentication",
            "finishPasskeyAuthentication",
            "probeAdminChannel",
            "previewAdminLiteLlmModelPrices",
            "previewAdminModelPrices",
            "previewAdminModelPriceExpression",
            "registerUser",
            "redeemUserRedemptionCode",
            "revokePlaygroundShare",
            "savePlaygroundConversation",
            "scanAdminFrontendTemplates",
            "sendAdminEmailTest",
            "requestPasswordReset",
            "readAdminDebugTraceSnapshots",
            "sendRegistrationEmailVerification",
            "updateAdminChannel",
            "updateAdminBalanceAlertSettings",
            "updateAdminCredential",
            "updateAdminCredentialProxy",
            "updateAdminDebugTraceSettings",
            "updateAdminGroup",
            "updateAdminRoute",
            "updateAdminModel",
            "updateAdminEmailSettings",
            "updateAdminNetworkSettings",
            "updateAdminPaymentSettings",
            "updateAdminAuthenticationSettings",
            "updateAdminSiteSettings",
            "activateAdminFrontendTemplate",
            "updateAdminToken",
            "updateAdminUser",
            "updateUserToken",
            "adjustAdminWallet",
            "listAdminRefunds",
            "manualCompleteAdminRefund",
            "approveAdminRefund",
            "rejectAdminRefund",
            "submitAdminRefund",
            "listAccountRefundReconciliations",
            "listOrganizationRefundReconciliations",
            "listAdminRefundReconciliations",
            "bindAdminUserSubscription",
            "transitionAdminUserSubscriptionLifecycle",
            "listAdminWalletEntries",
            "listOrganizationWalletEntries",
            "adjustOrganizationWallet",
            "disableUserTwoFactor",
            "enableUserTwoFactor",
            "getUserTwoFactor",
            "listUserPasskeys",
            "startUserPasskeyRegistration",
            "finishUserPasskeyRegistration",
            "renameUserPasskey",
            "revokeUserPasskey",
            "getScimServiceProviderConfig",
            "listScimResourceTypes",
            "createScimUser",
            "replaceScimUser",
            "createScimGroup",
            "replaceScimGroup",
            "deleteScimGroup",
            "replayAdminAnalyticsExport",
            "changePlatformOrganizationMember",
            "invitePlatformOrganizationMember",
            "updateAdminSiteNavigation",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        assert_eq!(operation_ids, expected);

        let schemas = document["components"]["schemas"].as_object().unwrap();
        assert_eq!(schemas.len(), 586);
        assert!(schemas.contains_key("ServiceLevelPoint"));
        assert!(schemas.contains_key("ServiceLevelRow"));
        assert!(schemas.contains_key("ServiceLevelReport"));
        assert_eq!(
            document["paths"]["/api/admin/dashboard/service-levels"]["get"]["security"],
            serde_json::json!([{"bearerAuth": []}])
        );
        assert!(schemas.contains_key("AdminCredentialImportRequest"));
        assert!(schemas.contains_key("AdminCredentialImportResponse"));
        assert!(schemas.contains_key("CredentialUsageSnapshot"));
        assert!(schemas.contains_key("CredentialUsageStatus"));
        assert!(schemas.contains_key("CredentialUsageWindow"));
        assert!(schemas.contains_key("AdminVerificationSettings"));
        assert!(schemas.contains_key("AdminVerificationSettingsRequest"));
        assert_eq!(
            document["paths"]["/api/admin/account-verification-settings"]["get"]["operationId"],
            "getAdminVerificationSettings"
        );
        assert_eq!(
            document["paths"]["/api/admin/account-verification-settings"]["put"]["operationId"],
            "updateAdminVerificationSettings"
        );
        assert_eq!(
            schemas["AdminVerificationSettingsRequest"]["properties"]["private_key"]["writeOnly"],
            true
        );
        assert!(schemas.contains_key("OpenAiModel"));
        assert!(schemas.contains_key("OpenAiModelList"));
        assert_eq!(
            document["paths"]["/v1/models"]["get"]["security"],
            serde_json::json!([{"apiKeyAuth": []}])
        );
        assert!(schemas.keys().is_sorted());
        assert!(schemas.contains_key("AdminFailedCallLog"));
        assert!(schemas.contains_key("UserFailedCallLog"));
        assert!(schemas.contains_key("OrganizationSsoDiscoveryErrorResponse"));
        assert!(schemas.contains_key("Announcement"));
        assert!(schemas.contains_key("AnnouncementListResponse"));
        assert!(schemas.contains_key("AnnouncementMutationRequest"));
        assert!(schemas.contains_key("AnnouncementUpdateRequest"));
        assert!(schemas.contains_key("AnnouncementWriteRequest"));
        assert!(schemas.contains_key("AnalyticsExportHealthState"));
        assert!(schemas.contains_key("AnalyticsExportReplayRequest"));
        assert!(schemas.contains_key("AnalyticsExportReplayResponse"));
        assert!(schemas.contains_key("AnalyticsExportStatusResponse"));
        assert!(schemas.contains_key("CreditAllocationListResponse"));
        assert!(schemas.contains_key("CreditAllocationResponse"));
        assert!(schemas.contains_key("CreditInvoiceListResponse"));
        assert!(schemas.contains_key("CreditInvoiceResponse"));
        assert!(schemas.contains_key("CreditInvoiceStatusResponse"));
        assert!(schemas.contains_key("CreditRepaymentListResponse"));
        assert!(schemas.contains_key("CreditRepaymentResponse"));
        assert!(schemas.contains_key("CreditRepaymentSourceKindResponse"));
        assert!(schemas.contains_key("CreditTermCreateRequest"));
        assert!(schemas.contains_key("CreditTermListResponse"));
        assert!(schemas.contains_key("CreditTermPeriodRequest"));
        assert!(schemas.contains_key("CreditTermPeriodResponse"));
        assert!(schemas.contains_key("CreditTermResponse"));
        assert!(schemas.contains_key("CreditTermStatusRequest"));
        assert!(schemas.contains_key("CreditTermStatusResponse"));
        assert!(schemas.contains_key("OrganizationSsoDiscoveryRequest"));
        assert!(schemas.contains_key("OrganizationSsoDiscoveryResponse"));
        assert!(schemas.contains_key("OrganizationSsoPublicLoginStartErrorResponse"));
        assert!(schemas.contains_key("OrganizationSsoPublicLoginStartRequest"));
        assert!(schemas.contains_key("OrganizationSsoPublicLoginStartResponse"));
        assert!(schemas.contains_key("OrganizationApprovalCursorQuery"));
        assert!(schemas.contains_key("OrganizationApprovalDecision"));
        assert!(schemas.contains_key("OrganizationApprovalDecisionKind"));
        assert!(schemas.contains_key("OrganizationApprovalDecisionRequest"));
        assert!(schemas.contains_key("OrganizationApprovalRequest"));
        assert!(schemas.contains_key("OrganizationApprovalRequestCreateRequest"));
        assert!(schemas.contains_key("OrganizationApprovalRequestListResponse"));
        assert!(schemas.contains_key("OrganizationApprovalResourceType"));
        assert!(schemas.contains_key("OrganizationApprovalTemplate"));
        assert!(schemas.contains_key("OrganizationApprovalTemplateCreateRequest"));
        assert!(schemas.contains_key("OrganizationApprovalTemplateListResponse"));
        assert!(schemas.contains_key("OrganizationApprovalNotificationDecision"));
        assert!(schemas.contains_key("OrganizationApprovalNotification"));
        assert!(schemas.contains_key("OrganizationApprovalNotificationListResponse"));
        assert!(schemas.contains_key("OrganizationApprovalNotificationMarkReadRequest"));
        assert!(schemas.contains_key("OrganizationApprovalNotificationMarkReadResponse"));
        assert!(schemas.contains_key("OrganizationProvisioningListQuerySchema"));
        assert!(schemas.contains_key("OrganizationProvisioningStatus"));
        assert!(schemas.contains_key("OrganizationProvisioningDecisionKind"));
        assert!(schemas.contains_key("OrganizationProvisioningCreateRequest"));
        assert!(schemas.contains_key("OrganizationProvisioningEntitlement"));
        assert!(schemas.contains_key("OrganizationProvisioningDecisionRequest"));
        assert!(schemas.contains_key("OrganizationProvisioningRequest"));
        assert!(schemas.contains_key("OrganizationProvisioningRequestListResponse"));
        assert!(schemas.contains_key("OrganizationVerificationCase"));
        assert!(schemas.contains_key("OrganizationVerificationDecisionKind"));
        assert!(schemas.contains_key("OrganizationVerificationDecisionRequest"));
        assert!(schemas.contains_key("OrganizationVerificationListQuerySchema"));
        assert!(schemas.contains_key("OrganizationVerificationListResponse"));
        assert!(schemas.contains_key("OrganizationVerificationMaterial"));
        assert!(schemas.contains_key("OrganizationVerificationResponse"));
        assert!(schemas.contains_key("OrganizationVerificationStatus"));
        assert!(schemas.contains_key("OrganizationVerificationSubmitRequest"));
        assert!(schemas.contains_key("OrganizationPlanListQuerySchema"));
        assert!(schemas.contains_key("OrganizationPlanCreateRequest"));
        assert!(schemas.contains_key("OrganizationPlanDisableRequest"));
        assert!(schemas.contains_key("OrganizationPlanStatus"));
        assert!(schemas.contains_key("OrganizationPlan"));
        assert!(schemas.contains_key("OrganizationPlanListResponse"));
        assert!(schemas.contains_key("OrganizationPlanCatalogResponse"));
        assert!(schemas.contains_key("OrganizationPlanOrderCreateRequest"));
        assert!(schemas.contains_key("OrganizationPlanOrderKind"));
        assert!(schemas.contains_key("OrganizationPlanOrderStatus"));
        assert!(schemas.contains_key("OrganizationPlanOrder"));
        assert!(schemas.contains_key("OrganizationEntitlementProjection"));
        assert!(schemas.contains_key("OrganizationEntitlementSource"));
        assert!(schemas.contains_key("OrganizationEntitlementStatus"));
        assert!(schemas.contains_key("AdminDashboardChannelFlow"));
        assert!(schemas.contains_key("AdminDashboardFlowPath"));
        assert!(schemas.contains_key("AdminDashboardFailure"));
        assert!(schemas.contains_key("AdminDashboardFailureKind"));
        assert!(schemas.contains_key("AdminDashboardHourlyPoint"));
        assert!(schemas.contains_key("AdminDashboardPerformance"));
        assert!(schemas.contains_key("ClientSimulationProfile"));
        assert!(schemas.contains_key("ClientSimulationBodyProfile"));
        assert!(schemas.contains_key("ClientSimulationBodyPatchResult"));
        assert!(schemas.contains_key("ClientSimulationResult"));
        assert!(schemas.contains_key("ContractPriceCloseRequest"));
        assert!(schemas.contains_key("ContractPriceCreateRequest"));
        assert!(schemas.contains_key("ContractPriceListResponse"));
        assert!(schemas.contains_key("ContractPriceProtocol"));
        assert!(schemas.contains_key("ContractPriceResponse"));
        assert!(schemas.contains_key("ContractPriceValues"));
        assert!(schemas.contains_key("PasskeyAuthenticationOptionsRequest"));
        assert!(schemas.contains_key("PasskeyAuthenticationOptionsResponse"));
        assert!(schemas.contains_key("PasskeyAuthenticationVerifyRequest"));
        assert!(schemas.contains_key("OrganizationEntitlementResponse"));
        assert!(schemas.contains_key("OrganizationProfileChangeRequest"));
        assert!(schemas.contains_key("OrganizationProfileResponse"));
        assert!(schemas.contains_key("OrganizationDepartmentCreateRequest"));
        assert!(schemas.contains_key("OrganizationDepartmentDisableRequest"));
        assert!(schemas.contains_key("OrganizationDepartmentListResponse"));
        assert!(schemas.contains_key("OrganizationDepartmentMoveRequest"));
        assert!(schemas.contains_key("OrganizationDepartmentResponse"));
        assert!(schemas.contains_key("OrganizationCustomRoleResponse"));
        assert!(schemas.contains_key("OrganizationServiceAccountCreateRequest"));
        assert!(schemas.contains_key("OrganizationServiceAccountStatusRequest"));
        assert!(schemas.contains_key("OrganizationServiceAccountRotateRequest"));
        assert!(schemas.contains_key("OrganizationServiceAccountResponse"));
        assert!(schemas.contains_key("OrganizationServiceAccountIssuedResponse"));
        assert!(schemas.contains_key("OrganizationServiceAccountListResponse"));
        assert!(schemas.contains_key("OrganizationServiceAccountBudgetBindingRequest"));
        assert!(schemas.contains_key("OrganizationServiceAccountBudgetBindingResponse"));
        assert!(schemas.contains_key("OrganizationServiceAccountRuntimeErrorResponse"));
        assert!(schemas.contains_key("OrganizationSsoProvider"));
        assert!(schemas.contains_key("OrganizationSsoProviderList"));
        assert!(schemas.contains_key("OrganizationSsoProviderRequest"));
        assert!(schemas.contains_key("OrganizationSsoDomain"));
        assert!(schemas.contains_key("OrganizationSsoDomainList"));
        assert!(schemas.contains_key("OrganizationSsoDomainCreated"));
        assert!(schemas.contains_key("OrganizationSsoDomainCreateRequest"));
        assert!(schemas.contains_key("OrganizationSsoDomainVerifyRequest"));
        assert!(schemas.contains_key("OrganizationSsoLoginStartRequest"));
        assert!(schemas.contains_key("OrganizationSsoLoginStartResponse"));
        assert!(schemas.contains_key("OrganizationSsoSamlAuthnRequestResponse"));
        assert!(schemas.contains_key("OrganizationSsoSamlAuthnRequestFieldResponse"));
        assert!(schemas.contains_key("OrganizationSsoIdentityBindingRequest"));
        assert!(schemas.contains_key("OrganizationSsoIdentityBindingResponse"));
        assert!(schemas.contains_key("OrganizationSsoPolicyRequest"));
        assert!(schemas.contains_key("OrganizationSsoPolicyResponse"));
        assert!(schemas.contains_key("OrganizationSsoRecoveryRequest"));
        assert!(schemas.contains_key("OrganizationSsoRecoveryResponse"));
        assert!(schemas.contains_key("OrganizationSsoLoginCallbackErrorResponse"));
        assert!(schemas.contains_key("OrganizationSsoSessionExchangeRequest"));
        assert!(schemas.contains_key("OrganizationSsoSessionExchangeErrorResponse"));
        assert!(schemas.contains_key("OrganizationInvitationAcceptRequest"));
        assert!(schemas.contains_key("OrganizationInvitationResponse"));
        assert!(schemas.contains_key("OrganizationMemberCustomRoleRequest"));
        assert!(schemas.contains_key("OrganizationMemberCustomRoleResponse"));
        assert!(schemas.contains_key("OrganizationMembershipChangeRequest"));
        assert!(schemas.contains_key("OrganizationMembershipListResponse"));
        assert!(schemas.contains_key("OrganizationMembershipResponse"));
        assert!(schemas.contains_key("OrganizationOwnershipTransferRequest"));
        assert!(schemas.contains_key("OrganizationOwnershipTransferResponse"));
        assert!(schemas.contains_key("OrganizationTeamChangeRequest"));
        assert!(schemas.contains_key("OrganizationTeamCreateRequest"));
        assert!(schemas.contains_key("OrganizationTeamDisableRequest"));
        assert!(schemas.contains_key("OrganizationTeamListResponse"));
        assert!(schemas.contains_key("OrganizationTeamResponse"));
        assert!(schemas.contains_key("OrganizationWalletSummaryResponse"));
        assert!(schemas.contains_key("OrganizationWalletEntryTypeResponse"));
        assert!(schemas.contains_key("OrganizationWalletEntryResponse"));
        assert!(schemas.contains_key("OrganizationWalletListResponse"));
        assert!(schemas.contains_key("OrganizationWalletAdjustmentRequest"));
        assert!(schemas.contains_key("OrganizationWalletAdjustmentResponse"));
        assert!(schemas.contains_key("OrganizationBudgetAllocationResponse"));
        assert!(schemas.contains_key("OrganizationBudgetOverviewResponse"));
        assert!(schemas.contains_key("OrganizationBudgetPeriodRequest"));
        assert!(schemas.contains_key("OrganizationBudgetPeriodResponse"));
        assert!(schemas.contains_key("OrganizationBudgetPolicyCreateRequest"));
        assert!(schemas.contains_key("OrganizationBudgetPolicyResponse"));
        assert!(schemas.contains_key("OrganizationBudgetReservationResponse"));
        assert!(schemas.contains_key("OrganizationBudgetReservationStatusResponse"));
        assert!(schemas.contains_key("OrganizationBudgetScopeRequest"));
        assert!(schemas.contains_key("OrganizationBudgetScopeResponse"));
        assert!(schemas.contains_key("OrganizationBudgetWindowResponse"));
        assert!(schemas.contains_key("OrganizationTokenStatusDto"));
        assert!(schemas.contains_key("OrganizationTokenWriteRequest"));
        assert!(schemas.contains_key("OrganizationTokenResponse"));
        assert!(schemas.contains_key("OrganizationTokenListResponse"));
        assert!(schemas.contains_key("IssuedOrganizationTokenResponse"));
        assert!(schemas.contains_key("ServiceProviderConfigResponse"));
        assert!(schemas.contains_key("ResourceTypeListResponse"));
        assert!(schemas.contains_key("ResourceTypeResponse"));
        assert!(schemas.contains_key("ScimSupportResponse"));
        assert!(schemas.contains_key("ScimBulkResponse"));
        assert!(schemas.contains_key("ScimFilterResponse"));
        assert!(schemas.contains_key("ScimAuthenticationSchemeResponse"));
        assert!(schemas.contains_key("ScimErrorResponse"));
        assert!(schemas.contains_key("ScimGroupWriteRequest"));
        assert!(schemas.contains_key("ScimGroupMember"));
        assert!(schemas.contains_key("ScimGroupMeta"));
        assert!(schemas.contains_key("ScimGroupResponse"));
        assert!(schemas.contains_key("OrganizationAuditLogListResponse"));
        assert!(schemas.contains_key("OrganizationAuditLogResponse"));
        assert!(schemas.contains_key("OrganizationAuditOutcomeResponse"));
        assert!(schemas.contains_key("OrganizationScimAuditLogListResponse"));
        assert!(schemas.contains_key("OrganizationScimAuditLogResponse"));
        assert!(schemas.contains_key("PlatformAuditLogListResponse"));
        assert!(schemas.contains_key("PlatformAuditLog"));
        assert!(schemas.contains_key("PlatformAuditOutcome"));
        assert!(schemas.contains_key("OrganizationUsageLogListResponse"));
        assert!(schemas.contains_key("OrganizationUsageLogSummaryResponse"));
        assert!(schemas.contains_key("OrganizationTopupOrderCreateRequest"));
        assert!(schemas.contains_key("OrganizationTopupOrderStatusResponse"));
        assert!(schemas.contains_key("OrganizationTopupPaymentSessionResponse"));
        assert!(schemas.contains_key("OrganizationTopupOrderResponse"));
        assert!(schemas.contains_key("ModelCatalogProvider"));
        assert!(schemas.contains_key("ModelCatalogProviderListResponse"));
        assert!(schemas.contains_key("AdminResponsesCompactProbe"));
        assert!(schemas.contains_key("ResponsesCompactError"));
        assert!(schemas.contains_key("ResponsesCompactErrorBody"));
        assert!(schemas.contains_key("ResponsesCompactInput"));
        assert!(schemas.contains_key("ResponsesCompactInputUsageDetails"));
        assert!(schemas.contains_key("ResponsesCompactItem"));
        assert!(schemas.contains_key("ResponsesCompactMode"));
        assert!(schemas.contains_key("ResponsesCompactOutputUsageDetails"));
        assert!(schemas.contains_key("ResponsesCompactProbeResult"));
        assert!(schemas.contains_key("ResponsesCompactRequest"));
        assert!(schemas.contains_key("ResponsesCompactResponse"));
        assert!(schemas.contains_key("ResponsesCompactUsage"));
        assert!(schemas.contains_key("RerankDocument"));
        assert!(schemas.contains_key("RerankError"));
        assert!(schemas.contains_key("RerankErrorBody"));
        assert!(schemas.contains_key("RerankRequest"));
        assert!(schemas.contains_key("RerankResponse"));
        assert!(schemas.contains_key("RerankResult"));
        assert!(schemas.contains_key("RerankTextDocument"));
        assert!(schemas.contains_key("RerankUsage"));
        assert!(schemas.contains_key("AudioSpeechRequest"));
        assert!(schemas.contains_key("AudioSpeechVoice"));
        assert!(schemas.contains_key("AudioSpeechCustomVoice"));
        assert!(schemas.contains_key("AudioSpeechOutputFormat"));
        assert!(schemas.contains_key("AudioSpeechStreamFormat"));
        assert!(schemas.contains_key("AudioSpeechBinary"));
        assert!(schemas.contains_key("AudioSpeechErrorBody"));
        assert!(schemas.contains_key("AudioSpeechError"));
        assert!(schemas.contains_key("VideoGenerationRequest"));
        assert!(schemas.contains_key("VideoAspectRatio"));
        assert!(schemas.contains_key("VideoResolution"));
        assert!(schemas.contains_key("VideoSubmissionResponse"));
        assert!(schemas.contains_key("VideoTaskStatus"));
        assert!(schemas.contains_key("VideoOutput"));
        assert!(schemas.contains_key("VideoFailureCode"));
        assert!(schemas.contains_key("VideoFailure"));
        assert!(schemas.contains_key("VideoPollResponse"));
        assert!(schemas.contains_key("VideoTaskListItem"));
        assert!(schemas.contains_key("VideoTaskListResponse"));
        assert!(schemas.contains_key("VideoTaskErrorBody"));
        assert!(schemas.contains_key("VideoTaskError"));
        assert!(schemas.contains_key("AdminChannelAutoBanRules"));
        assert!(schemas.contains_key("AdminCredentialCreateRequest"));
        assert!(schemas.contains_key("AdminCredentialUpdateRequest"));
        let writable_protocols =
            schemas["AdminChannelCreateRequest"]["properties"]["protocol"]["oneOf"][0]["enum"]
                .as_array()
                .unwrap();
        assert!(
            writable_protocols
                .iter()
                .any(|value| value == "openai_speech")
        );
        assert!(
            writable_protocols
                .iter()
                .any(|value| value == "jina_rerank")
        );
        assert!(schemas.contains_key("AdminCredentialSecret"));
        assert!(schemas.contains_key("AdminCredentialProxy"));
        assert!(schemas.contains_key("AdminCredentialProxyListResponse"));
        assert!(schemas.contains_key("AdminCredentialProxyWriteRequest"));
        assert!(schemas.contains_key("AdminCredentialProxyScheme"));
        assert!(schemas.contains_key("AdminGroupWindow"));
        assert!(schemas.contains_key("AdminDebugTrace"));
        assert!(schemas.contains_key("AdminDebugTraceAttempt"));
        assert!(schemas.contains_key("AdminDebugTraceDownstreamRequest"));
        assert!(schemas.contains_key("AdminDebugTraceDetailResponse"));
        assert!(schemas.contains_key("AdminDebugTraceListResponse"));
        assert!(schemas.contains_key("AdminDebugTraceSettings"));
        assert!(schemas.contains_key("AdminDebugTraceSettingsRequest"));
        assert!(schemas.contains_key("AdminDebugTraceSnapshotScope"));
        assert!(schemas.contains_key("AdminDebugTraceSnapshotRequest"));
        assert!(schemas.contains_key("AdminDebugTraceAttemptSnapshot"));
        assert!(schemas.contains_key("AdminDebugTraceSnapshotsResponse"));
        assert!(schemas.contains_key("AdminOAuthProvider"));
        assert!(schemas.contains_key("AdminOAuthProviderListResponse"));
        assert!(schemas.contains_key("AdminOAuthProviderStatus"));
        assert!(schemas.contains_key("AdminOAuthAuthorizationRequest"));
        assert!(schemas.contains_key("AdminOAuthAuthorizationResponse"));
        assert!(schemas.contains_key("AdminOAuthManualCallbackRequest"));
        assert!(schemas.contains_key("AdminOAuthCompletionStatus"));
        assert!(schemas.contains_key("AdminOAuthCompletionResponse"));
        assert!(schemas.contains_key("AdminCustomOAuth2Provider"));
        assert!(schemas.contains_key("AdminCustomOAuth2ProviderList"));
        assert!(schemas.contains_key("AdminCustomOAuth2ProviderRequest"));
        assert!(schemas.contains_key("PlatformOrganizationSummaryResponse"));
        assert!(schemas.contains_key("PlatformOrganizationDirectoryResponse"));
        assert!(schemas.contains_key("PlatformUserOrganizationSummaryResponse"));
        assert!(schemas.contains_key("PlatformUserOrganizationListResponse"));
        assert!(schemas.contains_key("AdminUserCreateRequest"));
        assert!(schemas.contains_key("AdminUserUpdateRequest"));
        assert!(schemas.contains_key("AdminWalletEntry"));
        assert!(schemas.contains_key("AdminWalletEntryType"));
        assert!(schemas.contains_key("AdminWalletListResponse"));
        assert!(schemas.contains_key("AdminWalletAdjustmentRequest"));
        assert!(schemas.contains_key("AdminRefundRequest"));
        assert!(schemas.contains_key("AdminRefundListResponse"));
        assert!(schemas.contains_key("AdminRefundDecisionRequest"));
        assert!(schemas.contains_key("AdminRefundOrderKind"));
        assert!(schemas.contains_key("AdminRefundStatus"));
        assert!(schemas.contains_key("AdminRefundApprovalStatus"));
        assert!(schemas.contains_key("RefundReconciliationStatus"));
        assert!(schemas.contains_key("RefundReconciliationEntry"));
        assert!(schemas.contains_key("RefundReconciliationListResponse"));
        assert!(schemas.contains_key("AdminUsageLogVideoResolution"));
        assert!(schemas.contains_key("AdminRedemptionAuditBatch"));
        assert!(schemas.contains_key("AdminRedemptionAuditListResponse"));
        assert!(schemas.contains_key("AdminRedemptionAuditStatus"));
        assert!(schemas.contains_key("AdminRedemptionAuditSummary"));
        assert!(schemas.contains_key("AdminRedemptionBatch"));
        assert!(schemas.contains_key("AdminRedemptionBatchStatus"));
        assert!(schemas.contains_key("AdminRedemptionBatchListResponse"));
        assert!(schemas.contains_key("AdminRedemptionBatchCreateRequest"));
        assert!(schemas.contains_key("IssuedAdminRedemptionBatch"));
        assert!(schemas.contains_key("AdminRedemptionBatchDisableRequest"));
        assert!(schemas.contains_key("AdminRedemptionBatchDisableResponse"));
        assert!(schemas.contains_key("UserRedemptionRequest"));
        assert!(schemas.contains_key("UserRedemptionResult"));
        assert!(schemas.contains_key("SubscriptionCatalogPlan"));
        assert!(schemas.contains_key("SubscriptionCatalogResponse"));
        assert!(schemas.contains_key("SubscriptionCycle"));
        assert!(schemas.contains_key("SubscriptionOrder"));
        assert!(schemas.contains_key("SubscriptionOrderCreateRequest"));
        assert!(schemas.contains_key("SubscriptionOrderStatus"));
        assert!(schemas.contains_key("SubscriptionPlanStatus"));
        assert!(schemas.contains_key("UserSubscriptionStatus"));
        assert!(schemas.contains_key("AdminSubscriptionPlan"));
        assert!(schemas.contains_key("AdminSubscriptionPlanListResponse"));
        assert!(schemas.contains_key("AdminSubscriptionPlanCreateRequest"));
        assert!(schemas.contains_key("AdminSubscriptionPlanDisableRequest"));
        assert!(schemas.contains_key("UserSubscription"));
        assert!(schemas.contains_key("UserSubscriptionListResponse"));
        assert!(schemas.contains_key("AdminUserSubscriptionBindRequest"));
        assert!(schemas.contains_key("AdminUserSubscriptionLifecycleAction"));
        assert!(schemas.contains_key("AdminUserSubscriptionLifecycleRequest"));
        assert!(schemas.contains_key("AdminUserSubscriptionLifecycleResponse"));
        assert!(schemas.contains_key("SetupRequest"));
        assert!(schemas.contains_key("SetupStatusResponse"));
        assert!(schemas.contains_key("RegistrationStatusResponse"));
        assert!(schemas.contains_key("RegistrationEmailVerificationRequest"));
        assert!(schemas.contains_key("RegistrationEmailVerificationResponse"));
        assert!(schemas.contains_key("RegistrationRequest"));
        assert!(schemas.contains_key("PasswordResetRequest"));
        assert!(schemas.contains_key("PasswordResetRequestResponse"));
        assert!(schemas.contains_key("PasswordResetConfirmRequest"));
        assert!(schemas.contains_key("UserProfileResponse"));
        assert!(schemas.contains_key("UserWalletSummary"));
        assert!(schemas.contains_key("UserWalletEntryType"));
        assert!(schemas.contains_key("UserWalletEntry"));
        assert!(schemas.contains_key("UserWalletListResponse"));
        assert!(schemas.contains_key("UserTopupConfiguration"));
        assert!(schemas.contains_key("UserTopupOrderCreateRequest"));
        assert!(schemas.contains_key("UserTopupOrderStatus"));
        assert!(schemas.contains_key("UserTopupOrder"));
        assert!(schemas.contains_key("UserNotificationPreferencesResponse"));
        assert!(schemas.contains_key("UserProfileUpdateRequest"));
        assert!(schemas.contains_key("UserEmailBindingVerificationRequest"));
        assert!(schemas.contains_key("UserEmailBindingVerificationResponse"));
        assert!(schemas.contains_key("UserEmailBindingConfirmRequest"));
        assert!(schemas.contains_key("UserPasswordChangeRequest"));
        assert!(schemas.contains_key("UserNotificationPreferencesRequest"));
        assert!(schemas.contains_key("NotificationKind"));
        assert!(schemas.contains_key("NotificationChannel"));
        assert!(schemas.contains_key("NotificationDeliveryState"));
        assert!(schemas.contains_key("UserNotification"));
        assert!(schemas.contains_key("UserNotificationListResponse"));
        assert!(schemas.contains_key("UserNotificationMarkReadRequest"));
        assert!(schemas.contains_key("UserNotificationMarkReadResponse"));
        assert!(schemas.contains_key("UserTwoFactorEnrollmentResponse"));
        assert!(schemas.contains_key("UserTwoFactorPasswordRequest"));
        assert!(schemas.contains_key("UserTwoFactorStatusResponse"));
        assert!(schemas.contains_key("UserPasskeyListResponse"));
        assert!(schemas.contains_key("UserPasskeyRegistrationOptionsResponse"));
        assert!(schemas.contains_key("UserPasskeyRegistrationVerifyRequest"));
        assert!(schemas.contains_key("UserPasskeyRenameRequest"));
        assert!(schemas.contains_key("UserPasskeyResponse"));
        assert!(schemas.contains_key("UserPasskeyRevokeRequest"));
        assert!(schemas.contains_key("UserInvitationSummaryResponse"));
        assert!(schemas.contains_key("UserInvitationRebateResponse"));
        assert!(schemas.contains_key("AdminAuthenticationSettings"));
        assert!(schemas.contains_key("AdminAuthenticationSettingsRequest"));
        assert!(schemas.contains_key("PublicSiteSettings"));
        assert!(schemas.contains_key("PublicBrandSettings"));
        assert!(schemas.contains_key("PublicAuthenticationCapabilities"));
        assert!(schemas.contains_key("AdminSiteSettings"));
        assert!(schemas.contains_key("AdminSiteSettingsRequest"));
        assert!(schemas.contains_key("FrontendTemplateCatalog"));
        assert!(schemas.contains_key("FrontendTemplateSummary"));
        assert!(schemas.contains_key("FrontendTemplateActivationRequest"));
        assert!(schemas.contains_key("AdminBrandSettingsRequest"));
        assert!(schemas.contains_key("BalanceDisplayMode"));
        assert!(schemas.contains_key("BalanceDisplaySettings"));
        assert!(schemas.contains_key("BalanceDisplaySymbolPosition"));
        assert!(schemas.contains_key("AdminEmailSettings"));
        assert!(schemas.contains_key("AdminEmailSettingsRequest"));
        assert!(schemas.contains_key("AdminEmailTestRequest"));
        assert!(schemas.contains_key("AdminEmailTlsMode"));
        assert!(schemas.contains_key("AdminNetworkSettings"));
        assert!(schemas.contains_key("AdminNetworkSettingsRequest"));
        assert!(schemas.contains_key("AdminNetworkSettingsMode"));
        assert!(schemas.contains_key("AdminPaymentSettings"));
        assert!(schemas.contains_key("AdminPaymentSettingsRequest"));
        assert!(schemas.contains_key("AdminBalanceAlertSettings"));
        assert!(schemas.contains_key("AdminBalanceAlertSettingsRequest"));
        assert!(schemas.contains_key("AdminRoute"));
        assert!(schemas.contains_key("AdminRouteChannelResponse"));
        assert!(schemas.contains_key("AdminRouteListResponse"));
        assert!(schemas.contains_key("AdminRouteWriteRequest"));
        assert!(schemas.contains_key("AdminRouteChannelWriteRequest"));
        assert!(schemas.contains_key("AdminRouteMode"));
        assert!(schemas.contains_key("AdminRouteStrategy"));
        assert!(schemas.contains_key("ModelCatalogListResponse"));
        assert!(schemas.contains_key("ModelCatalogPricingScope"));
        assert!(schemas.contains_key("ModelCatalogModality"));
        assert!(schemas.contains_key("ModelCatalogCapability"));
        assert!(schemas.contains_key("ModelCatalogLifecycle"));
        assert!(schemas.contains_key("ModelCatalogProtocol"));
        assert!(schemas.contains_key("ModelCatalogRuntimeStatus"));
        assert!(schemas.contains_key("AdminModel"));
        assert!(schemas.contains_key("AdminModelCreateRequest"));
        assert!(schemas.contains_key("AdminModelUpdateRequest"));
        assert!(schemas.contains_key("AdminModelVisibility"));
        assert!(schemas.contains_key("AdminModelLifecycle"));
        assert!(schemas.contains_key("AdminModelModality"));
        assert!(schemas.contains_key("AdminMissingModelImportItemRequest"));
        assert!(schemas.contains_key("AdminMissingModelImportRequest"));
        assert!(schemas.contains_key("AdminMissingModelImportResponse"));
        assert!(schemas.contains_key("AdminMissingModelListResponse"));
        assert!(schemas.contains_key("AdminModelSyncPreview"));
        assert!(schemas.contains_key("AdminModelSyncApplyRequest"));
        assert!(schemas.contains_key("AdminModelSyncRelation"));
        assert!(schemas.contains_key("AdminModelPrice"));
        assert!(schemas.contains_key("AdminModelPriceBillingMode"));
        assert!(schemas.contains_key("AdminModelPriceValues"));
        assert!(schemas.contains_key("AdminModelPriceListResponse"));
        assert!(schemas.contains_key("AdminModelPriceSourcePreview"));
        assert!(schemas.contains_key("AdminModelPriceBatchRequest"));
        assert!(schemas.contains_key("AdminModelPriceExpressionPreviewRequest"));
        assert!(schemas.contains_key("AdminModelPriceExpressionPreviewResponse"));
        assert!(schemas.contains_key("AdminRefundManualCompletionRequest"));
        assert_eq!(
            schemas["AdminModelPriceBillingMode"]["enum"],
            serde_json::json!(["per_token", "free", "expression"])
        );
        assert!(
            schemas["AdminModelPrice"]["required"]
                .as_array()
                .is_some_and(|required| required.iter().any(|field| field == "billing_expression"))
        );
        assert_eq!(
            schemas["AdminModelPrice"]["properties"]["billing_expression"]["type"],
            serde_json::json!(["string", "null"])
        );
        assert!(
            schemas["AdminModelPriceWriteItem"]["required"]
                .as_array()
                .is_some_and(|required| {
                    required.iter().any(|field| field == "context_window")
                        && required.iter().all(|field| field != "billing_expression")
                })
        );
        assert!(
            schemas["AdminModelPriceSourceCandidate"]["required"]
                .as_array()
                .is_some_and(|required| required.iter().any(|field| field == "context_window"))
        );
        assert!(schemas.contains_key("PlaygroundShareCreateRequest"));
        assert!(schemas.contains_key("PlaygroundShareReadResponse"));
        assert!(schemas.contains_key("PlaygroundConversationSaveRequest"));
        assert!(schemas.contains_key("PlaygroundConversationListResponse"));
        assert!(schemas.contains_key("PlaygroundConversationResponse"));
        assert!(schemas.contains_key("UserTokenWriteRequest"));
        assert!(schemas.contains_key("IssuedUserToken"));
        assert!(
            schemas["UserTokenWriteRequest"]["properties"]
                .as_object()
                .is_some_and(|properties| {
                    !properties.contains_key("user_id")
                        && !properties.contains_key("group_id")
                        && !properties.contains_key("cross_group_retry")
                        && !properties.contains_key("rate_limit_5h")
                        && !properties.contains_key("max_requests")
                })
        );
        assert_eq!(
            schemas["PlaygroundShareCreateRequest"]["properties"]["ttl_days"]["enum"],
            serde_json::json!([1, 7, 30])
        );
        assert_eq!(
            document["paths"]["/api/models"]["get"]["security"],
            serde_json::json!([{}, { "bearerAuth": [] }])
        );
        assert_eq!(
            document["paths"]["/api/model-providers"]["get"]["security"],
            serde_json::json!([{}, { "bearerAuth": [] }])
        );
        assert!(
            document["paths"]["/api/payment/webhook/{provider}"]["post"]
                .get("security")
                .is_none()
        );
    }

    #[test]
    fn document_preserves_sensitive_and_free_form_object_boundaries() {
        let document = serde_json::to_value(openapi_document()).unwrap();
        let schemas = &document["components"]["schemas"];

        let create = &schemas["AdminCredentialCreateRequest"];
        assert_eq!(create["additionalProperties"], false);
        assert!(
            create["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "secret")
        );
        assert!(
            create["properties"]["secret"]["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value["$ref"] == "#/components/schemas/AdminCredentialSecret")
        );
        assert!(
            create["properties"]["secret"]["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value["type"] == "null")
        );
        assert_eq!(
            schemas["AdminCredential"]["properties"]["oauth_token_pending"]["type"],
            "boolean"
        );
        assert_eq!(
            schemas["AdminCredential"]["properties"]["blocks_spark_shadow"]["type"],
            "boolean"
        );
        assert_eq!(
            schemas["AdminCredentialQuotaDimension"]["enum"],
            serde_json::json!(["global", "spark"])
        );
        assert_eq!(create["properties"]["parent_id"]["example"], 41);
        assert_eq!(
            create["properties"]["quota_dimension"]["$ref"],
            "#/components/schemas/AdminCredentialQuotaDimension"
        );
        assert!(
            create["properties"]["secret"]["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|value| value["description"].as_str())
                .any(|description| description.contains("Spark 影子"))
        );

        let secret_variants = schemas["AdminCredentialSecret"]["oneOf"]
            .as_array()
            .unwrap();
        let expected_variants = [
            ("api_key", &["api_key"][..]),
            ("oauth", &["access_token"][..]),
            (
                "bedrock",
                &["access_key_id", "secret_access_key", "session_token"][..],
            ),
            (
                "service_account",
                &["client_email", "private_key_id", "private_key"][..],
            ),
        ];
        assert_eq!(secret_variants.len(), expected_variants.len());
        for (variant, (kind, sensitive_fields)) in secret_variants.iter().zip(expected_variants) {
            let properties = variant["properties"].as_object().unwrap();
            assert_eq!(properties["kind"]["enum"], serde_json::json!([kind]));
            assert_eq!(properties.len(), sensitive_fields.len() + 1);
            for &field in sensitive_fields {
                assert_eq!(properties[field]["writeOnly"], true);
            }
        }

        let update_secret = &schemas["AdminCredentialUpdateRequest"]["properties"]["secret"];
        assert!(
            update_secret["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value["type"] == "null")
        );
        assert!(
            update_secret["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| { value["$ref"] == "#/components/schemas/AdminCredentialSecret" })
        );
        assert!(
            schemas["AdminCredentialUpdateRequest"]["properties"]["parent_id"]["description"]
                .as_str()
                .is_some_and(|description| description.contains("创建后不可修改"))
        );
        assert_eq!(
            schemas["AdminGroup"]["properties"]["flags"]["additionalProperties"],
            true
        );
        assert_eq!(
            schemas["AdminChannelCreateRequest"]["properties"]["settings"]["writeOnly"],
            true
        );
        let update = &schemas["AdminChannelUpdateRequest"];
        assert_eq!(update["properties"]["settings"]["writeOnly"], true);
        assert_eq!(
            update["properties"]["settings"]["type"],
            serde_json::json!(["object", "null"])
        );
        assert_eq!(
            update["properties"]["header_override"]["type"],
            serde_json::json!(["object", "null"])
        );
        let parameters = &update["properties"]["param_override"];
        assert_eq!(parameters["additionalProperties"], false);
        assert_eq!(parameters["properties"]["temperature"]["maximum"], 2.0);
        assert_eq!(
            parameters["properties"]["max_output_tokens"]["maximum"],
            1_000_000
        );
        assert_eq!(parameters["properties"]["stop_sequences"]["maxItems"], 4);
        assert!(
            !update["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "settings" || field == "header_override")
        );
        assert_eq!(
            schemas["AdminEmailSettingsRequest"]["properties"]["password"]["writeOnly"],
            true
        );
        assert_eq!(
            schemas["AdminCredentialProxyWriteRequest"]["properties"]["password"]["writeOnly"],
            true
        );
        assert!(
            schemas["AdminCredentialProxy"]["properties"]
                .as_object()
                .is_some_and(|properties| !properties.contains_key("password"))
        );
        assert!(
            schemas["AdminEmailSettings"]["properties"]
                .as_object()
                .is_some_and(|properties| !properties.contains_key("password"))
        );
        let model_update = &schemas["AdminModelUpdateRequest"];
        assert_eq!(model_update["additionalProperties"], false);
        assert!(
            model_update["properties"]
                .as_object()
                .is_some_and(|properties| !properties.contains_key("model"))
        );
        assert_eq!(
            model_update["properties"]["input_modalities"]["minItems"],
            1
        );
        assert_eq!(
            schemas["AdminModelModality"]["enum"],
            serde_json::json!(["text", "image", "audio", "video"])
        );
    }
}
