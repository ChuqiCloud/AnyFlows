//! Merchant contracts live in the domain crate; HTTP owns their OpenAPI representation.
use serde_json::{Value, json};
use utoipa::openapi::OpenApi;

fn reference(name: &str) -> Value {
    json!({"$ref":format!("#/components/schemas/{name}")})
}
fn nullable(schema: Value) -> Value {
    json!({"anyOf":[schema,{"type":"null"}]})
}
fn array(schema: Value) -> Value {
    json!({"type":"array","items":schema})
}
fn object(fields: &[(&str, Value)], optional: &[&str]) -> Value {
    let properties: serde_json::Map<String, Value> = fields
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect();
    let required: Vec<&str> = fields
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !optional.contains(name))
        .collect();
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn integer() -> Value {
    json!({"type":"integer","format":"int64"})
}
fn string() -> Value {
    json!({"type":"string"})
}
fn boolean() -> Value {
    json!({"type":"boolean"})
}
fn enumeration(values: &[&str]) -> Value {
    json!({"type":"string","enum":values})
}

pub(super) fn document() -> OpenApi {
    let mut schemas = serde_json::Map::new();
    let profile = object(
        &[
            ("id", integer()),
            ("owner_user_id", integer()),
            ("name", string()),
            ("description", string()),
            (
                "status",
                enumeration(&["pending", "approved", "rejected", "suspended"]),
            ),
            ("review_reason", nullable(string())),
            ("created_at", integer()),
            ("updated_at", integer()),
        ],
        &[],
    );
    schemas.insert("MerchantProfile".to_owned(), profile);
    let pool_fields = vec![
        ("group_id", integer()),
        ("name", string()),
        ("description", string()),
        ("enabled", boolean()),
        (
            "commission_bps",
            json!({"type":"integer","minimum":0,"maximum":10000}),
        ),
        ("commission_inherit_global", boolean()),
        ("min_ratio_micros", integer()),
        ("max_ratio_micros", integer()),
        ("settlement_delay_secs", integer()),
    ];
    schemas.insert(
        "MerchantPoolWriteRequest".to_owned(),
        object(&pool_fields, &["commission_inherit_global"]),
    );
    let mut pool = pool_fields.clone();
    pool.extend([
        ("id", integer()),
        ("effective_commission_bps", integer()),
        ("created_at", integer()),
        ("updated_at", integer()),
    ]);
    schemas.insert("MerchantPool".to_owned(), object(&pool, &[]));
    schemas.insert(
        "MerchantApplicationRequest".to_owned(),
        object(&[("name", string()), ("description", string())], &[]),
    );
    schemas.insert(
        "MerchantPoolSettings".to_owned(),
        object(
            &[(
                "commission_bps",
                json!({"type":"integer","minimum":0,"maximum":10000}),
            )],
            &[],
        ),
    );
    schemas.insert(
        "MerchantPoolTerms".to_owned(),
        object(
            &[(
                "commission_bps",
                nullable(json!({"type":"integer","minimum":0,"maximum":10000})),
            )],
            &[],
        ),
    );
    let common_model = vec![
        ("model", string()),
        ("context_window", nullable(integer())),
        ("max_output_tokens", nullable(integer())),
        ("supports_streaming", boolean()),
        ("supports_tools", boolean()),
        ("supports_vision", boolean()),
    ];
    let mut model_write = common_model.clone();
    model_write.extend([
        ("upstream_model", string()),
        ("ratio_micros", nullable(integer())),
    ]);
    schemas.insert(
        "MerchantModelWrite".to_owned(),
        object(
            &model_write,
            &["context_window", "max_output_tokens", "ratio_micros"],
        ),
    );
    schemas.insert(
        "MerchantModelPrices".to_owned(),
        object(
            &[
                ("currency", string()),
                ("unit", string()),
                ("billing_mode", enumeration(&["token", "free", "video"])),
                (
                    "video_per_second",
                    nullable(object(
                        &[
                            ("p480", string()),
                            ("p720", string()),
                            ("p1080", nullable(string())),
                        ],
                        &[],
                    )),
                ),
                ("input", string()),
                ("output", string()),
                ("cache_read", string()),
                ("cache_write_5m", string()),
                ("cache_write_1h", string()),
                ("version", integer()),
            ],
            &[],
        ),
    );
    let mut offer = common_model;
    offer.extend([
        ("sales_ratio_micros", integer()),
        ("prices", nullable(reference("MerchantModelPrices"))),
    ]);
    schemas.insert("MerchantModelOffer".to_owned(), object(&offer, &[]));
    schemas.insert(
        "MerchantQuoteRequest".to_owned(),
        object(
            &[
                ("pool_id", integer()),
                ("default_ratio_micros", integer()),
                ("models", array(reference("MerchantModelWrite"))),
            ],
            &[],
        ),
    );
    schemas.insert(
        "MerchantQuoteModel".to_owned(),
        object(
            &[
                ("model", string()),
                ("sales_ratio_micros", integer()),
                ("base_prices", reference("MerchantModelPrices")),
                ("prices", reference("MerchantModelPrices")),
                ("net_prices", reference("MerchantModelPrices")),
            ],
            &[],
        ),
    );
    schemas.insert(
        "MerchantQuote".to_owned(),
        object(
            &[
                ("models", array(reference("MerchantQuoteModel"))),
                ("effective_commission_bps", integer()),
            ],
            &[],
        ),
    );
    let revision_fields = vec![
        ("name", string()),
        ("description", string()),
        ("base_url", string()),
        (
            "protocol",
            enumeration(&[
                "openai_chat",
                "openai_responses",
                "openai_images",
                "openai_audio",
                "openai_speech",
                "xai_video",
            ]),
        ),
        ("default_ratio_micros", integer()),
        ("models", array(reference("MerchantModelWrite"))),
        ("concurrency", nullable(integer())),
        ("timeout_secs", integer()),
    ];
    let mut service_write = revision_fields.clone();
    service_write.extend([
        ("pool_id", integer()),
        (
            "api_key",
            nullable(json!({"type":"string","writeOnly":true,"maxLength":8192})),
        ),
    ]);
    schemas.insert(
        "MerchantServiceWriteRequest".to_owned(),
        object(&service_write, &["api_key", "concurrency"]),
    );
    let mut revision = revision_fields;
    revision.extend([
        ("id", integer()),
        ("revision", integer()),
        (
            "status",
            enumeration(&["draft", "pending", "published", "rejected", "superseded"]),
        ),
        ("has_api_key", boolean()),
        ("review_reason", nullable(string())),
        ("created_at", integer()),
        ("updated_at", integer()),
    ]);
    schemas.insert("MerchantRevision".to_owned(), object(&revision, &[]));
    schemas.insert(
        "MerchantOwnedService".to_owned(),
        object(
            &[
                ("id", integer()),
                ("merchant_id", integer()),
                ("pool_id", integer()),
                ("name", string()),
                ("description", string()),
                (
                    "status",
                    enumeration(&[
                        "draft",
                        "pending",
                        "published",
                        "paused",
                        "suspended",
                        "rejected",
                    ]),
                ),
                ("published", nullable(reference("MerchantRevision"))),
                ("draft", nullable(reference("MerchantRevision"))),
                ("active_price", nullable(reference("MerchantServicePrice"))),
                ("maintenance", array(reference("MerchantMaintenance"))),
                ("created_at", integer()),
                ("updated_at", integer()),
            ],
            &[],
        ),
    );
    schemas.insert(
        "MerchantCatalogService".to_owned(),
        object(
            &[
                ("id", integer()),
                ("pool_id", integer()),
                ("merchant_id", integer()),
                ("merchant_name", string()),
                ("name", string()),
                ("description", string()),
                ("status", string()),
                (
                    "protocol",
                    enumeration(&[
                        "openai_chat",
                        "openai_responses",
                        "openai_images",
                        "openai_audio",
                        "openai_speech",
                        "xai_video",
                    ]),
                ),
                ("revision", integer()),
                ("price_version", integer()),
                ("maintenance", array(reference("MerchantMaintenance"))),
                ("models", array(reference("MerchantModelOffer"))),
                ("updated_at", integer()),
            ],
            &[],
        ),
    );
    schemas.insert(
        "MerchantReviewRequest".to_owned(),
        object(
            &[
                ("decision", enumeration(&["approve", "reject", "suspend"])),
                ("reason", string()),
                ("expected_revision", nullable(integer())),
            ],
            &["expected_revision"],
        ),
    );
    schemas.insert(
        "MerchantPauseRequest".to_owned(),
        object(&[("paused", boolean())], &[]),
    );
    schemas.insert(
        "MerchantKeyRotationRequest".to_owned(),
        object(
            &[(
                "api_key",
                json!({"type":"string","writeOnly":true,"minLength":1,"maxLength":8192}),
            )],
            &[],
        ),
    );
    for (name, item) in [
        ("MerchantPoolPage", "MerchantPool"),
        ("MerchantCatalogServicePage", "MerchantCatalogService"),
        ("MerchantOwnedServicePage", "MerchantOwnedService"),
        ("MerchantProfilePage", "MerchantProfile"),
    ] {
        schemas.insert(
            name.to_owned(),
            object(
                &[
                    ("items", array(reference(item))),
                    ("total", integer()),
                    ("page", integer()),
                    ("page_size", integer()),
                ],
                &[],
            ),
        );
    }
    let operations = [
        (
            "get",
            "/api/merchant-pools",
            "listMerchantPools",
            None,
            "MerchantPoolPage",
            200,
        ),
        (
            "get",
            "/api/merchant-pools/{pool}/services",
            "listMerchantPoolServices",
            None,
            "MerchantCatalogServicePage",
            200,
        ),
        (
            "get",
            "/api/merchant-pools/{pool}/services/{id}",
            "getMerchantPoolService",
            None,
            "MerchantCatalogService",
            200,
        ),
        (
            "get",
            "/api/merchant/profile",
            "getMerchantProfile",
            None,
            "MerchantProfile",
            200,
        ),
        (
            "post",
            "/api/merchant/apply",
            "applyMerchant",
            Some("MerchantApplicationRequest"),
            "MerchantProfile",
            201,
        ),
        (
            "post",
            "/api/merchant/quote",
            "quoteMerchantService",
            Some("MerchantQuoteRequest"),
            "MerchantQuote",
            200,
        ),
        (
            "get",
            "/api/merchant/available-pools",
            "listMerchantAvailablePools",
            None,
            "MerchantPoolPage",
            200,
        ),
        (
            "get",
            "/api/merchant/services",
            "listOwnedMerchantServices",
            None,
            "MerchantOwnedServicePage",
            200,
        ),
        (
            "post",
            "/api/merchant/services",
            "createMerchantService",
            Some("MerchantServiceWriteRequest"),
            "MerchantOwnedService",
            201,
        ),
        (
            "get",
            "/api/merchant/services/{id}",
            "getOwnedMerchantService",
            None,
            "MerchantOwnedService",
            200,
        ),
        (
            "put",
            "/api/merchant/services/{id}",
            "updateMerchantService",
            Some("MerchantServiceWriteRequest"),
            "MerchantOwnedService",
            200,
        ),
        (
            "post",
            "/api/merchant/services/{id}/submit",
            "submitMerchantService",
            None,
            "MerchantOwnedService",
            200,
        ),
        (
            "post",
            "/api/merchant/services/{id}/pause",
            "pauseMerchantService",
            Some("MerchantPauseRequest"),
            "MerchantOwnedService",
            200,
        ),
        (
            "post",
            "/api/merchant/services/{id}/key",
            "rotateMerchantServiceKey",
            Some("MerchantKeyRotationRequest"),
            "MerchantOwnedService",
            200,
        ),
        (
            "get",
            "/api/admin/merchant-pools",
            "listAdminMerchantPools",
            None,
            "MerchantPoolPage",
            200,
        ),
        (
            "post",
            "/api/admin/merchant-pools",
            "createAdminMerchantPool",
            Some("MerchantPoolWriteRequest"),
            "MerchantPool",
            201,
        ),
        (
            "put",
            "/api/admin/merchant-pools/{id}",
            "updateAdminMerchantPool",
            Some("MerchantPoolWriteRequest"),
            "MerchantPool",
            200,
        ),
        (
            "delete",
            "/api/admin/merchant-pools/{id}",
            "deleteAdminMerchantPool",
            None,
            "",
            204,
        ),
        (
            "delete",
            "/api/admin/merchants/{id}",
            "deleteAdminMerchant",
            None,
            "",
            204,
        ),
        (
            "get",
            "/api/admin/merchants",
            "listAdminMerchants",
            None,
            "MerchantProfilePage",
            200,
        ),
        (
            "post",
            "/api/admin/merchants/{id}/review",
            "reviewAdminMerchant",
            Some("MerchantReviewRequest"),
            "MerchantProfile",
            200,
        ),
        (
            "get",
            "/api/admin/merchant-services",
            "listAdminMerchantServices",
            None,
            "MerchantOwnedServicePage",
            200,
        ),
        (
            "get",
            "/api/admin/merchant-services/{id}",
            "getAdminMerchantService",
            None,
            "MerchantOwnedService",
            200,
        ),
        (
            "post",
            "/api/admin/merchant-services/{id}/review",
            "reviewAdminMerchantService",
            Some("MerchantReviewRequest"),
            "MerchantOwnedService",
            200,
        ),
        (
            "post",
            "/api/admin/merchant-services/{id}/resume",
            "resumeAdminMerchantService",
            None,
            "MerchantOwnedService",
            200,
        ),
        (
            "get",
            "/api/admin/merchant-pool-settings",
            "getAdminMerchantPoolSettings",
            None,
            "MerchantPoolSettings",
            200,
        ),
        (
            "put",
            "/api/admin/merchant-pool-settings",
            "updateAdminMerchantPoolSettings",
            Some("MerchantPoolSettings"),
            "MerchantPoolSettings",
            200,
        ),
        (
            "get",
            "/api/admin/merchant-pools/{pool}/merchants/{merchant}/terms",
            "getAdminMerchantPoolTerms",
            None,
            "MerchantPoolTerms",
            200,
        ),
        (
            "put",
            "/api/admin/merchant-pools/{pool}/merchants/{merchant}/terms",
            "updateAdminMerchantPoolTerms",
            Some("MerchantPoolTerms"),
            "MerchantPoolTerms",
            200,
        ),
    ];
    let mut paths = serde_json::Map::<String, Value>::new();
    for (method, path, operation, request, response, status) in operations {
        let response_schema = if operation == "getMerchantProfile" {
            nullable(reference(response))
        } else {
            reference(response)
        };
        let mut responses = serde_json::Map::new();
        let success = if status == 204 {
            json!({"description":"Resource deleted; historical records retained"})
        } else {
            json!({"description":"Merchant pool operation result","content":{"application/json":{"schema":response_schema}}})
        };
        responses.insert(status.to_string(), success);
        for error in [400, 401, 403, 404, 409, 503] {
            responses.insert(error.to_string(),json!({"description":"Request rejected","content":{"application/json":{"schema":{"type":"object"}}}}));
        }
        let mut parameters = Vec::new();
        for segment in path.split('/') {
            if let Some(name) = segment
                .strip_prefix('{')
                .and_then(|name| name.strip_suffix('}'))
            {
                parameters.push(json!({"name":name,"in":"path","required":true,"schema":{"type":"integer","format":"int64","minimum":1}}));
            }
        }
        if response.ends_with("Page") {
            parameters.extend([json!({"name":"page","in":"query","required":false,"schema":{"type":"integer","minimum":1}}),
                json!({"name":"page_size","in":"query","required":false,"schema":{"type":"integer","minimum":1,"maximum":100}})]);
            for name in [
                "model",
                "protocol",
                "capability",
                "search",
                "status",
                "sort",
            ] {
                parameters.push(
                    json!({"name":name,"in":"query","required":false,"schema":{"type":"string"}}),
                );
                if name == "status" && operation == "listAdminMerchantPools" {
                    parameters.last_mut().expect("status parameter")["description"] =
                        json!("Use deleted to list retired pools; omit to list current pools.");
                }
            }
            for name in ["min_ratio_micros", "max_ratio_micros"] {
                parameters.push(json!({"name":name,"in":"query","required":false,"schema":{"type":"integer","minimum":0,"maximum":100000000}}));
            }
            parameters.push(json!({"name":"window_seconds","in":"query","required":false,"schema":{"type":"integer","enum":[900,3600,86400,604800]}}));
            parameters.push(json!({"name":"streaming","in":"query","required":false,"schema":{"type":"boolean"}}));
        }
        let mut operation = json!({"operationId":operation,"tags":["商户共享池"],"parameters":parameters,"security":[{"bearerAuth":[]}],"responses":responses});
        if let Some(request) = request {
            operation["requestBody"] = json!({"required":true,"content":{"application/json":{"schema":reference(request)}}});
        }
        paths.entry(path.to_owned()).or_insert_with(|| json!({}))[method] = operation;
    }
    serde_json::from_value(json!({"openapi":"3.1.0","info":{"title":"Merchant pool","version":"1"},"paths":paths,"components":{"schemas":schemas}}))
        .expect("merchant OpenAPI contract is a static valid document")
}

#[cfg(test)]
mod tests {
    #[test]
    fn merchant_delete_contracts_have_no_response_body() {
        let value = serde_json::to_value(super::document()).unwrap();
        for (path, operation) in [
            ("/api/admin/merchant-pools/{id}", "deleteAdminMerchantPool"),
            ("/api/admin/merchants/{id}", "deleteAdminMerchant"),
        ] {
            let delete = &value["paths"][path]["delete"];
            assert_eq!(delete["operationId"], operation);
            assert!(delete["responses"]["204"].get("content").is_none());
            assert!(delete["requestBody"].is_null());
            assert!(!delete["responses"]["403"].is_null());
            assert!(!delete["responses"]["409"].is_null());
        }
    }
}
