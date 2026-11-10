use serde_json::json;
use utoipa::openapi::OpenApi;

pub(super) fn document() -> OpenApi {
    let policy = json!({"type":"object","additionalProperties":false,"required":["mode","models","disabled_models"],"properties":{
        "mode":{"type":"string","enum":["inherit","allowlist","denylist"]},
        "models":{"type":"array","maxItems":1000,"items":{"type":"string","minLength":1,"maxLength":255}},
        "disabled_models":{"type":"array","maxItems":1000,"items":{"type":"string","minLength":1,"maxLength":255}}
    }});
    let settings = json!({"type":"object","additionalProperties":false,"required":["policy","version","auto_add_discovered","refresh_interval_secs"],"properties":{
        "policy":{"$ref":"#/components/schemas/UpstreamModelPolicy"},"version":{"type":"integer","format":"int64","minimum":0},
        "auto_add_discovered":{"type":"boolean"},"refresh_interval_secs":{"type":["integer","null"],"minimum":300,"maximum":2592000}
    }});
    let mut snapshot = settings.clone();
    snapshot["additionalProperties"] = json!(true);
    for (name, schema) in [
        (
            "items",
            json!({"type":"array","items":{"$ref":"#/components/schemas/CredentialModelItem"}}),
        ),
        (
            "last_success_at",
            json!({"type":["integer","null"],"format":"int64"}),
        ),
        (
            "last_checked_at",
            json!({"type":["integer","null"],"format":"int64"}),
        ),
        (
            "status",
            json!({"type":"string","enum":["unknown","available","empty","unavailable"]}),
        ),
        ("error_code", json!({"type":["string","null"]})),
    ] {
        snapshot["properties"][name] = schema;
        snapshot["required"]
            .as_array_mut()
            .expect("required array")
            .push(json!(name));
    }
    let item = json!({"type":"object","required":["upstream_model","canonical_models","enabled","blocked_reason","source"],"properties":{
        "upstream_model":{"type":"string"},"canonical_models":{"type":"array","items":{"type":"string"}},
        "enabled":{"type":"boolean"},"blocked_reason":{"type":["string","null"]},"source":{"type":"string"}
    }});
    let mut paths = serde_json::Map::new();
    for (path, method, operation, response, write) in [
        (
            "/api/admin/channels/{channel_id}/model-policy",
            "get",
            "getAdminChannelModelPolicy",
            "UpstreamModelPolicySettings",
            false,
        ),
        (
            "/api/admin/channels/{channel_id}/model-policy",
            "put",
            "updateAdminChannelModelPolicy",
            "UpstreamModelPolicySettings",
            true,
        ),
        (
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/model-policy",
            "get",
            "getAdminCredentialModelPolicy",
            "UpstreamModelPolicySettings",
            false,
        ),
        (
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/model-policy",
            "put",
            "updateAdminCredentialModelPolicy",
            "UpstreamModelPolicySettings",
            true,
        ),
        (
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/models",
            "get",
            "listAdminCredentialModels",
            "CredentialModelsSnapshot",
            false,
        ),
        (
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/models/refresh",
            "post",
            "refreshAdminCredentialModels",
            "CredentialModelsSnapshot",
            false,
        ),
    ] {
        let mut parameters = vec![
            json!({"name":"channel_id","in":"path","required":true,"schema":{"type":"integer","minimum":1}}),
        ];
        if path.contains("{credential_id}") {
            parameters.push(json!({"name":"credential_id","in":"path","required":true,"schema":{"type":"integer","minimum":1}}));
        }
        let mut entry = json!({"operationId":operation,"tags":["凭据模型"],"parameters":parameters,"security":[{"bearerAuth":[]}],"responses":{
            "200":{"description":"Current policy and upstream discovery evidence","content":{"application/json":{"schema":{"$ref":format!("#/components/schemas/{response}")}}}}
        }});
        for status in [400, 401, 403, 404, 409, 503] {
            entry["responses"][status.to_string()] = json!({"description":"Request rejected"});
        }
        if write {
            entry["requestBody"] = json!({"required":true,"content":{"application/json":{"schema":{"$ref":"#/components/schemas/UpstreamModelPolicySettings"}}}});
        }
        paths.entry(path.to_owned()).or_insert_with(|| json!({}))[method] = entry;
    }
    serde_json::from_value(json!({"openapi":"3.1.0","info":{"title":"Credential models","version":"1"},"paths":paths,
        "components":{"schemas":{"UpstreamModelPolicy":policy,"UpstreamModelPolicySettings":settings,"CredentialModelItem":item,"CredentialModelsSnapshot":snapshot}}
    })).expect("static model contracts are valid OpenAPI")
}
