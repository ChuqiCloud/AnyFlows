use serde_json::{Value, json};
use utoipa::openapi::OpenApi;
fn reference(name: &str) -> Value {
    json!({"$ref":format!("#/components/schemas/{name}")})
}
fn integer() -> Value {
    json!({"type":"integer","format":"int64"})
}
fn nullable(value: Value) -> Value {
    json!({"anyOf":[value,{"type":"null"}]})
}
fn object(fields: &[(&str, Value)]) -> Value {
    let properties: serde_json::Map<String, Value> = fields
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    let required: Vec<&str> = fields.iter().map(|(k, _)| *k).collect();
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub(super) fn document() -> OpenApi {
    let member = object(&[
        ("merchant_id", integer()),
        ("priority", integer()),
        ("weight", integer()),
    ]);
    let fields = vec![
        (
            "name",
            json!({"type":"string","minLength":1,"maxLength":128}),
        ),
        ("pool_id", integer()),
        (
            "strategy",
            json!({"type":"string","enum":["price_first","stable_first","ordered","round_robin","weighted"]}),
        ),
        ("enabled", json!({"type":"boolean"})),
        (
            "max_merchant_attempts",
            json!({"type":"integer","minimum":1,"maximum":16}),
        ),
        ("max_price_ratio_micros", nullable(integer())),
        (
            "members",
            json!({"type":"array","minItems":1,"maxItems":16,"items":reference("MerchantRouteMember")}),
        ),
        (
            "excluded_merchant_ids",
            json!({"type":"array","maxItems":128,"items":integer()}),
        ),
        ("allow_pool_fallback", json!({"type":"boolean"})),
        ("accept_new_merchants", json!({"type":"boolean"})),
    ];
    let mut response_fields = fields.clone();
    response_fields.push(("id", integer()));
    let stats = object(&[
        ("service_id", integer()),
        ("merchant_id", integer()),
        ("sample_count", integer()),
        ("success_count", integer()),
        ("failure_count", integer()),
        (
            "success_rate",
            nullable(json!({"type":"number","minimum":0,"maximum":100})),
        ),
        ("ttft_p50_ms", nullable(integer())),
        ("ttft_p95_ms", nullable(integer())),
        ("latency_p50_ms", nullable(integer())),
        ("latency_p95_ms", nullable(integer())),
        ("status", json!({"type":"string"})),
        ("updated_at", integer()),
    ]);
    let attempt = object(&[
        ("merchant_id", integer()),
        ("merchant_name", json!({"type":"string"})),
        ("service_id", integer()),
        ("service_name", json!({"type":"string"})),
        ("request_id", json!({"type":"string"})),
        (
            "outcome",
            json!({"type":"string","enum":["success","failure","ignored"]}),
        ),
        ("latency_ms", integer()),
        ("completed_at", integer()),
    ]);
    let bucket = object(&[
        ("started_at", integer()),
        ("ended_at", integer()),
        ("success_count", integer()),
        ("failure_count", integer()),
        ("sample_count", integer()),
        ("status", json!({"type":"string"})),
    ]);
    let mut doc = json!({"openapi":"3.1.0","info":{"title":"Merchant routes","version":"1"},"paths":{},"components":{"schemas":{
        "MerchantRoutePreviewRequest":object(&[("route",reference("MerchantRouteWriteRequest")),("model",json!({"type":"string"})),("protocol",json!({"type":"string","enum":["openai_chat","openai_responses"]})),("streaming",json!({"type":"boolean"})),("input_tokens",integer()),("output_tokens",integer())]),
        "MerchantRoutePreviewItem":object(&[("merchant_id",integer()),("merchant_name",json!({"type":"string"})),("service_id",integer()),("service_name",json!({"type":"string"})),("sales_ratio_micros",integer()),("estimated_quota",nullable(integer())),("eligible",json!({"type":"boolean"})),("reason",nullable(json!({"type":"string"})))]),
        "MerchantRoutePreview":object(&[("items",json!({"type":"array","items":reference("MerchantRoutePreviewItem")})),("quota_per_usd",integer()),("as_of",integer())]),
        "MerchantTimelineBucket":bucket,
        "MerchantServiceTimeline":object(&[("service_id",integer()),("merchant_id",integer()),("buckets",json!({"type":"array","items":reference("MerchantTimelineBucket")}))]),
        "MerchantTimelineResponse":object(&[("items",json!({"type":"array","items":reference("MerchantServiceTimeline")})),("as_of",integer()),("window_seconds",integer())]),
        "MerchantRouteMember":member,"MerchantRouteWriteRequest":object(&fields),"MerchantRoute":object(&response_fields),
        "MerchantRouteList":object(&[("items",json!({"type":"array","items":reference("MerchantRoute")})),("capacity",integer())]),
        "MerchantServiceStatistics":stats,"MerchantStatisticsResponse":object(&[("items",json!({"type":"array","items":reference("MerchantServiceStatistics")})),("as_of",integer()),("window_seconds",integer())]),
        "MerchantConsumerAttempt":attempt,"MerchantAttemptList":object(&[("items",json!({"type":"array","items":reference("MerchantConsumerAttempt")}))])
    }}});
    for (path, method, id, response, request, status) in [
        (
            "/api/merchant-pool/routes",
            "get",
            "listMerchantRoutes",
            "MerchantRouteList",
            false,
            "200",
        ),
        (
            "/api/merchant-pool/routes",
            "post",
            "createMerchantRoute",
            "MerchantRoute",
            true,
            "201",
        ),
        (
            "/api/merchant-pool/routes/preview",
            "post",
            "previewMerchantRoute",
            "MerchantRoutePreview",
            true,
            "200",
        ),
        (
            "/api/merchant-pool/routes/{id}",
            "get",
            "getMerchantRoute",
            "MerchantRoute",
            false,
            "200",
        ),
        (
            "/api/merchant-pool/routes/{id}",
            "put",
            "updateMerchantRoute",
            "MerchantRoute",
            true,
            "200",
        ),
        (
            "/api/merchant-pool/routes/{id}",
            "delete",
            "deleteMerchantRoute",
            "",
            false,
            "204",
        ),
        (
            "/api/merchant-pools/{pool_id}/statistics",
            "get",
            "getMerchantPoolStatistics",
            "MerchantStatisticsResponse",
            false,
            "200",
        ),
        (
            "/api/merchant-pools/{pool_id}/timeline",
            "get",
            "getMerchantPoolTimeline",
            "MerchantTimelineResponse",
            false,
            "200",
        ),
        (
            "/api/merchant/statistics",
            "get",
            "getOwnMerchantStatistics",
            "MerchantStatisticsResponse",
            false,
            "200",
        ),
        (
            "/api/merchant-pool/attempts",
            "get",
            "getMerchantAttempts",
            "MerchantAttemptList",
            false,
            "200",
        ),
    ] {
        let mut operation = json!({"operationId":id,"tags":["商户共享池"],"security":[{"bearerAuth":[]}],"responses":{
            "400":{"description":"Invalid request"},"401":{"description":"Invalid session"},
            "403":{"description":"Unavailable"},"404":{"description":"Not found"},"500":{"description":"Unavailable"}}});
        operation["responses"][status] = if response.is_empty() {
            json!({"description":"Deleted"})
        } else {
            json!({"description":"Success","content":{"application/json":{"schema":reference(response)}}})
        };
        if request {
            operation["requestBody"] = json!({"required":true,"content":{"application/json":{"schema":reference(if path.ends_with("preview"){"MerchantRoutePreviewRequest"}else{"MerchantRouteWriteRequest"})}}});
        }
        let mut params = Vec::new();
        if path.contains("{id}") {
            params.push(json!({"name":"id","in":"path","required":true,"schema":integer()}));
        }
        if path.contains("{pool_id}") || path == "/api/merchant/statistics" {
            if path.contains("{pool_id}") {
                params
                    .push(json!({"name":"pool_id","in":"path","required":true,"schema":integer()}));
            }
            params.push(
                json!({"name":"model","in":"query","required":false,"schema":{"type":"string"}}),
            );
            params.push(json!({"name":"window_seconds","in":"query","required":false,"schema":{"type":"integer","enum":[900,3600,86400,604800],"default":86400}}));
            params.push(json!({"name":"protocol","in":"query","required":false,"schema":{"type":"string","enum":["openai_chat","openai_responses"]}}));
            params.push(json!({"name":"streaming","in":"query","required":false,"schema":{"type":"boolean"}}));
        }
        if path.ends_with("attempts") {
            params.push(json!({"name":"request_id","in":"query","required":true,"schema":{"type":"string","maxLength":128}}));
        }
        if !params.is_empty() {
            operation["parameters"] = json!(params);
        }
        if doc["paths"].get(path).is_none() {
            doc["paths"][path] = json!({});
        }
        doc["paths"][path][method] = operation;
    }
    serde_json::from_value(doc).expect("merchant route OpenAPI is valid")
}
