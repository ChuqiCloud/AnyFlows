use serde_json::{Value, json};
use utoipa::openapi::OpenApi;

fn reference(name: &str) -> Value {
    json!({"$ref":format!("#/components/schemas/{name}")})
}
fn integer() -> Value {
    json!({"type":"integer","format":"int64"})
}
fn object(fields: &[(&str, Value)], optional: &[&str]) -> Value {
    let properties: serde_json::Map<_, _> = fields
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect();
    let required: Vec<_> = fields
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !optional.contains(name))
        .collect();
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn array(schema: Value) -> Value {
    json!({"type":"array","items":schema})
}
fn nullable(schema: Value) -> Value {
    json!({"anyOf":[schema,{"type":"null"}]})
}

pub(super) fn document() -> OpenApi {
    let mut schemas = serde_json::Map::new();
    schemas.insert(
        "MerchantModelRatio".to_owned(),
        object(
            &[
                ("model", json!({"type":"string","maxLength":255})),
                (
                    "ratio_micros",
                    nullable(
                        json!({"type":"integer","format":"int64","minimum":1,"maximum":100000000}),
                    ),
                ),
            ],
            &["ratio_micros"],
        ),
    );
    let price_write = vec![
        ("expected_revision_id", integer()),
        ("effective_at", integer()),
        ("default_ratio_micros", integer()),
        ("model_ratios", array(reference("MerchantModelRatio"))),
    ];
    schemas.insert(
        "MerchantServicePriceWrite".to_owned(),
        object(&price_write, &[]),
    );
    schemas.insert("MerchantServicePrice".to_owned(),object(&[("id",integer()),("service_id",integer()),("revision_id",integer()),
        ("effective_at",integer()),("default_ratio_micros",integer()),("model_ratios",array(reference("MerchantModelRatio"))),
        ("status",json!({"type":"string","enum":["scheduled","active","superseded","cancelled","invalidated"]})),
        ("reason",nullable(json!({"type":"string"}))),("created_at",integer()),("updated_at",integer())],&[]));
    let notice_write = vec![
        ("starts_at", integer()),
        ("ends_at", integer()),
        ("message", json!({"type":"string","maxLength":2048})),
        ("blocks_requests", json!({"type":"boolean"})),
    ];
    schemas.insert(
        "MerchantMaintenanceWrite".to_owned(),
        object(&notice_write, &[]),
    );
    let mut notice = notice_write;
    notice.extend([
        ("id", integer()),
        ("service_id", integer()),
        ("created_at", integer()),
        ("updated_at", integer()),
        (
            "status",
            json!({"type":"string","enum":["scheduled","active","completed","cancelled"]}),
        ),
    ]);
    schemas.insert("MerchantMaintenance".to_owned(), object(&notice, &[]));
    for (name, item) in [
        ("MerchantServicePricePage", "MerchantServicePrice"),
        ("MerchantMaintenancePage", "MerchantMaintenance"),
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
    let mut paths = serde_json::Map::<String, Value>::new();
    for (prefix, scope) in [
        ("/api/merchant/services", "Owned"),
        ("/api/admin/merchant-services", "Admin"),
    ] {
        for (method, suffix, operation, request, response, status) in [
            (
                "get",
                "/{id}/prices",
                "listServicePrices",
                None,
                "MerchantServicePricePage",
                200,
            ),
            (
                "post",
                "/{id}/prices",
                "scheduleServicePrice",
                Some("MerchantServicePriceWrite"),
                "MerchantServicePrice",
                201,
            ),
            (
                "post",
                "/{id}/prices/{price}/cancel",
                "cancelServicePrice",
                None,
                "MerchantServicePrice",
                200,
            ),
            (
                "get",
                "/{id}/maintenance",
                "listServiceMaintenance",
                None,
                "MerchantMaintenancePage",
                200,
            ),
            (
                "post",
                "/{id}/maintenance",
                "createServiceMaintenance",
                Some("MerchantMaintenanceWrite"),
                "MerchantMaintenance",
                201,
            ),
            (
                "put",
                "/{id}/maintenance/{notice}",
                "updateServiceMaintenance",
                Some("MerchantMaintenanceWrite"),
                "MerchantMaintenance",
                200,
            ),
            (
                "post",
                "/{id}/maintenance/{notice}/cancel",
                "cancelServiceMaintenance",
                None,
                "MerchantMaintenance",
                200,
            ),
        ] {
            let mut parameters = Vec::new();
            for segment in suffix.split('/') {
                if let Some(name) = segment
                    .strip_prefix('{')
                    .and_then(|value| value.strip_suffix('}'))
                {
                    parameters.push(json!({"name":name,"in":"path","required":true,"schema":{"type":"integer","format":"int64","minimum":1}}));
                }
            }
            if response.ends_with("Page") {
                parameters.extend([json!({"name":"page","in":"query","required":false,"schema":{"type":"integer","minimum":1}}),
                    json!({"name":"page_size","in":"query","required":false,"schema":{"type":"integer","minimum":1,"maximum":100}})]);
            }
            let mut responses = serde_json::Map::new();
            responses.insert(status.to_string(),json!({"description":"Merchant service policy","content":{"application/json":{"schema":reference(response)}}}));
            for error in [400, 401, 403, 404, 409, 503] {
                responses.insert(error.to_string(),json!({"description":"Request rejected","content":{"application/json":{"schema":{"type":"object"}}}}));
            }
            let mut operation = json!({"operationId":format!("{operation}{scope}"),"tags":["商户共享池"],"parameters":parameters,
                "security":[{"bearerAuth":[]}],"responses":responses});
            if let Some(request) = request {
                operation["requestBody"] = json!({"required":true,"content":{"application/json":{"schema":reference(request)}}});
            }
            paths
                .entry(format!("{prefix}{suffix}"))
                .or_insert_with(|| json!({}))[method] = operation;
        }
    }
    serde_json::from_value(json!({"openapi":"3.1.0","info":{"title":"Merchant service policies","version":"1"},"paths":paths,"components":{"schemas":schemas}}))
        .expect("merchant policy OpenAPI contract is static")
}
