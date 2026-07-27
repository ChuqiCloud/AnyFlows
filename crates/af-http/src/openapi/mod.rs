//! 从 Rust 路由契约生成管理 API 的 OpenAPI 文档。

mod channels;
mod credentials;
mod groups;
pub(crate) mod schema;
mod session;
mod tokens;
mod users;

use utoipa::openapi::{
    Components, Info, OpenApi, Paths,
    security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    tag::Tag,
};

/// 构建可导出、可供客户端生成和 Apifox 导入的完整管理 API 文档。
pub fn openapi_document() -> OpenApi {
    let mut document = OpenApi::new(Info::new("AnyFlows 管理 API", "0.1.0"), Paths::new());
    document.merge(session::document());
    document.merge(users::document());
    document.merge(groups::document());
    document.merge(tokens::document());
    document.merge(channels::document());
    document.merge(credentials::document());

    document.tags = Some(vec![
        Tag::new("管理会话"),
        Tag::new("用户管理"),
        Tag::new("分组管理"),
        Tag::new("令牌管理"),
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
                    .description(Some("管理会话 Bearer JWT"))
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
        assert_eq!(document.paths.paths.len(), 13);
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
        assert_eq!(operation_count, 28);
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
            "createAdminToken",
            "createAdminUser",
            "deleteAdminChannel",
            "deleteAdminCredential",
            "deleteAdminGroup",
            "deleteAdminToken",
            "deleteAdminUser",
            "getAdminChannel",
            "getAdminCredential",
            "getAdminGroup",
            "getAdminToken",
            "getAdminUser",
            "getManagementSession",
            "listAdminChannels",
            "listAdminCredentials",
            "listAdminGroups",
            "listAdminTokens",
            "listAdminUsers",
            "loginManagementSession",
            "probeAdminChannel",
            "updateAdminChannel",
            "updateAdminCredential",
            "updateAdminGroup",
            "updateAdminToken",
            "updateAdminUser",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        assert_eq!(operation_ids, expected);

        let schemas = document["components"]["schemas"].as_object().unwrap();
        assert_eq!(schemas.len(), 35);
        assert!(schemas.keys().is_sorted());
        assert!(schemas.contains_key("AdminCredentialCreateRequest"));
        assert!(schemas.contains_key("AdminCredentialUpdateRequest"));
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
        assert!(
            !update["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "settings" || field == "header_override")
        );
    }
}
