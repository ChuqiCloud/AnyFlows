use serde_json::{Value, json};

pub(super) fn document() -> utoipa::openapi::OpenApi {
    let provider =
        json!({"type":"string","enum":["auto","codex","new_api","one_api","sub2api","none"]});
    let nullable_string = json!({"type":["string","null"]});
    let config = json!({"type":"object","required":["version","provider","scope","pause_when_exhausted"],"properties":{
        "version":{"type":"integer","format":"int64","minimum":0},"provider":provider,
        "scope":{"type":"string","enum":["key","account"]},"profile_id":{"type":["integer","null"],"format":"int64","minimum":1},
        "refresh_interval_secs":{"type":["integer","null"],"minimum":60,"maximum":604800},"quota_per_unit":nullable_string,
        "currency":nullable_string,"low_balance_threshold":nullable_string,"pause_when_exhausted":{"type":"boolean"},"inherited":{"type":"boolean"}}});
    let profile = json!({"type":"object","required":["id","name","provider","base_url","version","has_secret"],"properties":{
        "id":{"type":"integer","format":"int64"},"name":{"type":"string","maxLength":128},"provider":{"type":"string","enum":["new_api","one_api","sub2api"]},
        "base_url":{"type":"string","maxLength":2048},"platform_user_id":nullable_string,"quota_per_unit":nullable_string,
        "currency":nullable_string,"version":{"type":"integer","format":"int64"},"has_secret":{"type":"boolean"}}});
    let mut write = profile.clone();
    write["required"] = json!(["name", "provider", "base_url", "version"]);
    write["properties"]
        .as_object_mut()
        .expect("object")
        .remove("id");
    write["properties"]
        .as_object_mut()
        .expect("object")
        .remove("has_secret");
    write["properties"]["secret"] = json!({"type":["string","null"],"maxLength":8192,"writeOnly":true,"description":"Create requires a secret; omit or null to preserve on update."});
    let mut paths = serde_json::Map::new();
    for (method, path, id, request, response, status) in [
        (
            "get",
            "/api/admin/channels/{channel_id}/usage-config",
            "getAdminChannelUsageConfig",
            None,
            Some("UpstreamUsageConfig"),
            200,
        ),
        (
            "put",
            "/api/admin/channels/{channel_id}/usage-config",
            "updateAdminChannelUsageConfig",
            Some("UpstreamUsageConfig"),
            Some("UpstreamUsageConfig"),
            200,
        ),
        (
            "get",
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/usage-config",
            "getAdminCredentialUsageConfig",
            None,
            Some("UpstreamUsageConfig"),
            200,
        ),
        (
            "put",
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/usage-config",
            "updateAdminCredentialUsageConfig",
            Some("UpstreamUsageConfig"),
            Some("UpstreamUsageConfig"),
            200,
        ),
        (
            "delete",
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/usage-config",
            "resetAdminCredentialUsageConfig",
            None,
            None,
            204,
        ),
        (
            "post",
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/usage/refresh",
            "refreshAdminCredentialUsage",
            None,
            Some("CredentialUsageSnapshot"),
            200,
        ),
        (
            "get",
            "/api/admin/upstream-usage-profiles",
            "listAdminUpstreamUsageProfiles",
            None,
            Some("UpstreamUsageProfilePage"),
            200,
        ),
        (
            "post",
            "/api/admin/upstream-usage-profiles",
            "createAdminUpstreamUsageProfile",
            Some("UpstreamUsageProfileWrite"),
            Some("UpstreamUsageProfile"),
            201,
        ),
        (
            "put",
            "/api/admin/upstream-usage-profiles/{id}",
            "updateAdminUpstreamUsageProfile",
            Some("UpstreamUsageProfileWrite"),
            Some("UpstreamUsageProfile"),
            200,
        ),
        (
            "delete",
            "/api/admin/upstream-usage-profiles/{id}",
            "deleteAdminUpstreamUsageProfile",
            None,
            None,
            204,
        ),
    ] {
        let params:Vec<Value>=path.split('/').filter_map(|part|part.strip_prefix('{').and_then(|p|p.strip_suffix('}')))
            .map(|name|json!({"name":name,"in":"path","required":true,"schema":{"type":"integer","format":"int64","minimum":1}})).collect();
        let mut success = json!({"description":"Upstream usage operation result"});
        if let Some(schema) = response {
            success["content"] = json!({"application/json":{"schema":{"$ref":format!("#/components/schemas/{schema}")}}});
        }
        let mut responses = serde_json::Map::new();
        responses.insert(status.to_string(), success);
        for status in [400, 401, 403, 404, 409, 503] {
            responses.insert(status.to_string(),json!({"description":"Request rejected","content":{"application/json":{"schema":{"$ref":"#/components/schemas/ManagementErrorBody"}}}}));
        }
        let mut operation = json!({"operationId":id,"tags":["凭据管理"],"security":[{"bearerAuth":[]}],"parameters":params,"responses":responses});
        if let Some(schema) = request {
            operation["requestBody"] = json!({"required":true,"content":{"application/json":{"schema":{"$ref":format!("#/components/schemas/{schema}")}}}});
        }
        paths.entry(path.to_owned()).or_insert_with(|| json!({}))[method] = operation;
    }
    serde_json::from_value(json!({"openapi":"3.1.0","info":{"title":"Upstream usage","version":"1"},"paths":paths,
        "components":{"schemas":{"UpstreamUsageConfig":config,"UpstreamUsageProfile":profile,"UpstreamUsageProfileWrite":write,
        "UpstreamUsageProfilePage":{"type":"object","required":["items"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/UpstreamUsageProfile"}}}}}}})).expect("static usage contract")
}
