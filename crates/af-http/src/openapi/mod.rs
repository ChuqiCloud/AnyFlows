//! 从 Rust 路由契约生成管理 API 的 OpenAPI 文档。

mod balance_alert_settings;
mod channels;
mod credential_proxies;
mod credentials;
mod dashboard;
mod debug_traces;
mod email_settings;
mod groups;
mod invitations;
mod model_metadata;
mod model_prices;
mod model_sync;
mod models;
mod network_settings;
mod oauth_connections;
mod organization_budget;
mod organization_wallet;
mod organizations;
mod password_reset;
mod payment_settings;
mod payment_webhook;
mod playground_conversations;
mod playground_shares;
mod redemptions;
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
mod user_profile;
mod user_tokens;
mod user_topups;
mod user_wallet;
mod users;
mod videos;
mod wallet;

use utoipa::openapi::{
    Components, Info, OpenApi, Paths,
    security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    tag::Tag,
};

/// 构建可导出、可供客户端生成和 Apifox 导入的完整管理 API 文档。
pub fn openapi_document() -> OpenApi {
    let mut document = OpenApi::new(Info::new("AnyFlows 管理 API", "0.1.0"), Paths::new());
    document.merge(session::document());
    document.merge(setup::document());
    document.merge(registration::document());
    document.merge(rerank::document());
    document.merge(responses_compact::document());
    document.merge(speech::document());
    document.merge(videos::document());
    document.merge(redemptions::document());
    document.merge(subscriptions::document());
    document.merge(password_reset::document());
    document.merge(user_profile::document());
    document.merge(user_wallet::document());
    document.merge(user_topups::document());
    document.merge(invitations::document());
    document.merge(email_settings::document());
    document.merge(network_settings::document());
    document.merge(payment_settings::document());
    document.merge(oauth_connections::document());
    document.merge(organizations::document());
    document.merge(balance_alert_settings::document());
    document.merge(site_settings::document());
    document.merge(dashboard::document());
    document.merge(users::document());
    document.merge(wallet::document());
    document.merge(groups::document());
    document.merge(routes::document());
    document.merge(models::document());
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
    document.merge(debug_traces::document());
    document.merge(payment_webhook::document());
    document.merge(organization_wallet::document());
    document.merge(organization_budget::document());

    document.tags = Some(vec![
        Tag::new("公开注册"),
        Tag::new("OpenAI Responses"),
        Tag::new("OpenAI Audio"),
        Tag::new("视频任务"),
        Tag::new("Rerank"),
        Tag::new("认证设置"),
        Tag::new("密码重置"),
        Tag::new("个人资料与安全"),
        Tag::new("我的钱包"),
        Tag::new("兑换码"),
        Tag::new("订阅管理"),
        Tag::new("邀请中心"),
        Tag::new("站点设置"),
        Tag::new("邮件设置"),
        Tag::new("网络与代理"),
        Tag::new("支付设置"),
        Tag::new("OAuth 连接"),
        Tag::new("企业钱包"),
        Tag::new("企业预算"),
        Tag::new("余额预警"),
        Tag::new("管理会话"),
        Tag::new("首次安装"),
        Tag::new("管理看板"),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn document_keeps_the_management_contract_surface() {
        let document = openapi_document();
        assert_eq!(document.paths.paths.len(), 107);
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
        assert_eq!(operation_count, 152);
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
            "compactResponse",
            "rerank",
            "synthesizeSpeech",
            "submitVideoTask",
            "listVideoTasks",
            "pollVideoTask",
            "createAdminChannel",
            "createAdminCredential",
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
            "confirmPasswordReset",
            "completeAdminOAuthManualCallback",
            "getUserProfile",
            "listAccountOrganizations",
            "getOrganizationOverview",
            "getOrganizationWallet",
            "getOrganizationBudgets",
            "createOrganizationBudgetPolicy",
            "getOrganizationBudgetReservation",
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
            "transferOrganizationOwnership",
            "listOrganizationTeams",
            "createOrganizationTeam",
            "changeOrganizationTeam",
            "disableOrganizationTeam",
            "getUserTopupConfiguration",
            "getUserWallet",
            "importMissingAdminModels",
            "getUserInvitations",
            "updateUserProfile",
            "changeUserPassword",
            "updateUserNotificationPreferences",
            "createPlaygroundShare",
            "createUserToken",
            "createPlaygroundToken",
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
            "getAdminDashboard",
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
            "listMissingAdminModels",
            "listAdminTokens",
            "listAdminUsageLogs",
            "listUserUsageLogs",
            "listAdminUsers",
            "listModels",
            "listModelProviders",
            "listPlaygroundConversations",
            "listUserTokens",
            "receivePaymentWebhook",
            "listUserWalletEntries",
            "listCurrentUserSubscriptions",
            "loginManagementSession",
            "probeAdminChannel",
            "previewAdminLiteLlmModelPrices",
            "previewAdminModelPrices",
            "previewAdminModelPriceExpression",
            "registerUser",
            "redeemUserRedemptionCode",
            "revokePlaygroundShare",
            "savePlaygroundConversation",
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
            "updateAdminToken",
            "updateAdminUser",
            "updateUserToken",
            "adjustAdminWallet",
            "bindAdminUserSubscription",
            "transitionAdminUserSubscriptionLifecycle",
            "listAdminWalletEntries",
            "listOrganizationWalletEntries",
            "adjustOrganizationWallet",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        assert_eq!(operation_ids, expected);

        let schemas = document["components"]["schemas"].as_object().unwrap();
        assert_eq!(schemas.len(), 310);
        assert!(schemas.keys().is_sorted());
        assert!(schemas.contains_key("OrganizationEntitlementResponse"));
        assert!(schemas.contains_key("OrganizationInvitationAcceptRequest"));
        assert!(schemas.contains_key("OrganizationInvitationResponse"));
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
        assert!(schemas.contains_key("AdminUserCreateRequest"));
        assert!(schemas.contains_key("AdminUserUpdateRequest"));
        assert!(schemas.contains_key("AdminWalletEntry"));
        assert!(schemas.contains_key("AdminWalletEntryType"));
        assert!(schemas.contains_key("AdminWalletListResponse"));
        assert!(schemas.contains_key("AdminWalletAdjustmentRequest"));
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
        assert!(schemas.contains_key("SubscriptionCycle"));
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
        assert!(schemas.contains_key("UserPasswordChangeRequest"));
        assert!(schemas.contains_key("UserNotificationPreferencesRequest"));
        assert!(schemas.contains_key("UserInvitationSummaryResponse"));
        assert!(schemas.contains_key("UserInvitationRebateResponse"));
        assert!(schemas.contains_key("AdminAuthenticationSettings"));
        assert!(schemas.contains_key("AdminAuthenticationSettingsRequest"));
        assert!(schemas.contains_key("PublicSiteSettings"));
        assert!(schemas.contains_key("PublicBrandSettings"));
        assert!(schemas.contains_key("PublicAuthenticationCapabilities"));
        assert!(schemas.contains_key("AdminSiteSettings"));
        assert!(schemas.contains_key("AdminSiteSettingsRequest"));
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
