//! 从 Rust 路由契约生成管理 API 的 OpenAPI 文档。

mod balance_alert_settings;
mod channels;
mod credentials;
mod dashboard;
mod email_settings;
mod groups;
mod invitations;
mod model_metadata;
mod model_prices;
mod model_sync;
mod models;
mod network_settings;
mod password_reset;
mod playground_conversations;
mod playground_shares;
mod redemptions;
mod registration;
pub(crate) mod schema;
mod session;
mod setup;
mod site_settings;
mod tokens;
mod usage_logs;
mod user_profile;
mod user_tokens;
mod user_wallet;
mod users;
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
    document.merge(redemptions::document());
    document.merge(password_reset::document());
    document.merge(user_profile::document());
    document.merge(user_wallet::document());
    document.merge(invitations::document());
    document.merge(email_settings::document());
    document.merge(network_settings::document());
    document.merge(balance_alert_settings::document());
    document.merge(site_settings::document());
    document.merge(dashboard::document());
    document.merge(users::document());
    document.merge(wallet::document());
    document.merge(groups::document());
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

    document.tags = Some(vec![
        Tag::new("公开注册"),
        Tag::new("认证设置"),
        Tag::new("密码重置"),
        Tag::new("个人资料与安全"),
        Tag::new("我的钱包"),
        Tag::new("兑换码"),
        Tag::new("邀请中心"),
        Tag::new("站点设置"),
        Tag::new("邮件设置"),
        Tag::new("网络与代理"),
        Tag::new("余额预警"),
        Tag::new("管理会话"),
        Tag::new("首次安装"),
        Tag::new("管理看板"),
        Tag::new("用户管理"),
        Tag::new("钱包账本"),
        Tag::new("分组管理"),
        Tag::new("模型目录"),
        Tag::new("模型管理"),
        Tag::new("Playground 分享"),
        Tag::new("Playground 历史"),
        Tag::new("我的 API Key"),
        Tag::new("令牌管理"),
        Tag::new("用量日志"),
        Tag::new("渠道管理"),
        Tag::new("凭据管理"),
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
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn document_keeps_the_management_contract_surface() {
        let document = openapi_document();
        assert_eq!(document.paths.paths.len(), 55);
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
        assert_eq!(operation_count, 86);
        assert!(
            document
                .components
                .as_ref()
                .is_some_and(|components| components.security_schemes.contains_key("bearerAuth"))
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
            "createAdminChannel",
            "createAdminCredential",
            "createAdminGroup",
            "createAdminModel",
            "createAdminModelSyncPreview",
            "createAdminRedemptionBatch",
            "createAdminToken",
            "createAdminUser",
            "confirmPasswordReset",
            "getUserProfile",
            "getUserWallet",
            "getUserInvitations",
            "updateUserProfile",
            "changeUserPassword",
            "updateUserNotificationPreferences",
            "createPlaygroundShare",
            "createUserToken",
            "deletePlaygroundConversation",
            "deleteAdminChannel",
            "deleteAdminCredential",
            "deleteAdminGroup",
            "deleteAdminModel",
            "deleteAdminToken",
            "deleteAdminUser",
            "deleteUserToken",
            "disableAdminRedemptionBatch",
            "getAdminChannel",
            "getAdminBalanceAlertSettings",
            "getAdminCredential",
            "getAdminDashboard",
            "getAdminEmailSettings",
            "getAdminNetworkSettings",
            "getAdminGroup",
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
            "listAdminGroups",
            "listAdminModels",
            "listAdminModelPrices",
            "listAdminRedemptionBatches",
            "listMissingAdminModels",
            "listAdminTokens",
            "listAdminUsageLogs",
            "listAdminUsers",
            "listModels",
            "listPlaygroundConversations",
            "listUserTokens",
            "listUserWalletEntries",
            "loginManagementSession",
            "probeAdminChannel",
            "previewAdminModelPrices",
            "registerUser",
            "redeemUserRedemptionCode",
            "revokePlaygroundShare",
            "savePlaygroundConversation",
            "sendAdminEmailTest",
            "requestPasswordReset",
            "sendRegistrationEmailVerification",
            "updateAdminChannel",
            "updateAdminBalanceAlertSettings",
            "updateAdminCredential",
            "updateAdminGroup",
            "updateAdminModel",
            "updateAdminEmailSettings",
            "updateAdminNetworkSettings",
            "updateAdminAuthenticationSettings",
            "updateAdminSiteSettings",
            "updateAdminToken",
            "updateAdminUser",
            "updateUserToken",
            "adjustAdminWallet",
            "listAdminWalletEntries",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        assert_eq!(operation_ids, expected);

        let schemas = document["components"]["schemas"].as_object().unwrap();
        assert_eq!(schemas.len(), 143);
        assert!(schemas.keys().is_sorted());
        assert!(schemas.contains_key("AdminCredentialCreateRequest"));
        assert!(schemas.contains_key("AdminCredentialUpdateRequest"));
        assert!(schemas.contains_key("AdminUserCreateRequest"));
        assert!(schemas.contains_key("AdminUserUpdateRequest"));
        assert!(schemas.contains_key("AdminWalletEntry"));
        assert!(schemas.contains_key("AdminWalletEntryType"));
        assert!(schemas.contains_key("AdminWalletListResponse"));
        assert!(schemas.contains_key("AdminWalletAdjustmentRequest"));
        assert!(schemas.contains_key("AdminRedemptionBatch"));
        assert!(schemas.contains_key("AdminRedemptionBatchStatus"));
        assert!(schemas.contains_key("AdminRedemptionBatchListResponse"));
        assert!(schemas.contains_key("AdminRedemptionBatchCreateRequest"));
        assert!(schemas.contains_key("IssuedAdminRedemptionBatch"));
        assert!(schemas.contains_key("AdminRedemptionBatchDisableRequest"));
        assert!(schemas.contains_key("AdminRedemptionBatchDisableResponse"));
        assert!(schemas.contains_key("UserRedemptionRequest"));
        assert!(schemas.contains_key("UserRedemptionResult"));
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
        assert!(schemas.contains_key("AdminEmailSettings"));
        assert!(schemas.contains_key("AdminEmailSettingsRequest"));
        assert!(schemas.contains_key("AdminEmailTestRequest"));
        assert!(schemas.contains_key("AdminEmailTlsMode"));
        assert!(schemas.contains_key("AdminNetworkSettings"));
        assert!(schemas.contains_key("AdminNetworkSettingsRequest"));
        assert!(schemas.contains_key("AdminNetworkSettingsMode"));
        assert!(schemas.contains_key("AdminBalanceAlertSettings"));
        assert!(schemas.contains_key("AdminBalanceAlertSettingsRequest"));
        assert!(schemas.contains_key("ModelCatalogListResponse"));
        assert!(schemas.contains_key("ModelCatalogPricingScope"));
        assert!(schemas.contains_key("ModelCatalogLifecycle"));
        assert!(schemas.contains_key("ModelCatalogProtocol"));
        assert!(schemas.contains_key("ModelCatalogRuntimeStatus"));
        assert!(schemas.contains_key("AdminModel"));
        assert!(schemas.contains_key("AdminModelCreateRequest"));
        assert!(schemas.contains_key("AdminModelUpdateRequest"));
        assert!(schemas.contains_key("AdminModelVisibility"));
        assert!(schemas.contains_key("AdminModelLifecycle"));
        assert!(schemas.contains_key("AdminModelModality"));
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
        assert_eq!(create["properties"]["secret"]["writeOnly"], true);
        assert_eq!(create["properties"]["secret"]["type"], "string");

        let update_secret = &schemas["AdminCredentialUpdateRequest"]["properties"]["secret"];
        assert_eq!(update_secret["writeOnly"], true);
        assert!(
            update_secret["type"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "null")
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
