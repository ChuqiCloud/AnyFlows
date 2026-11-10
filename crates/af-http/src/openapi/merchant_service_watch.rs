use serde_json::json;
use utoipa::openapi::OpenApi;

pub(super) fn document() -> OpenApi {
    let mut paths = serde_json::Map::new();
    paths.insert(
        "/api/merchant-pool/watches".to_owned(),
        json!({"get":{"operationId":"listMerchantServiceWatches","tags":["商户共享池"],"security":[{"bearerAuth":[]}],"parameters":[],"responses":{"200":{"description":"Watched public services","content":{"application/json":{"schema":{"$ref":"#/components/schemas/MerchantServiceWatchListResponse"}}}},"401":{"description":"Unauthorized"}}}}),
    );
    paths.insert(
        "/api/merchant-pools/{pool}/services/{service}/watch".to_owned(),
        json!({
            "parameters":[
                {"name":"pool","in":"path","required":true,"schema":{"type":"integer","minimum":1}},
                {"name":"service","in":"path","required":true,"schema":{"type":"integer","minimum":1}}
            ],
            "put":{"operationId":"watchMerchantService","tags":["商户共享池"],"security":[{"bearerAuth":[]}],"responses":{"200":{"description":"Watched public service","content":{"application/json":{"schema":{"$ref":"#/components/schemas/MerchantServiceWatch"}}}},"404":{"description":"Service not found or not accessible"},"429":{"description":"Watch limit reached"}}},
            "delete":{"operationId":"unwatchMerchantService","tags":["商户共享池"],"security":[{"bearerAuth":[]}],"responses":{"200":{"description":"Watch removed","content":{"application/json":{"schema":{"$ref":"#/components/schemas/MerchantServiceWatchChangeResponse"}}}},"404":{"description":"Service watch not found"}}}
        }),
    );
    let schemas = json!({
        "MerchantServiceWatch":{"type":"object","additionalProperties":false,"required":["pool_id","pool_name","service_id","merchant_id","service_name","merchant_name","active","created_at"],"properties":{"pool_id":{"type":"integer","format":"int64"},"pool_name":{"type":"string"},"service_id":{"type":"integer","format":"int64"},"merchant_id":{"type":"integer","format":"int64"},"service_name":{"type":"string"},"merchant_name":{"type":"string"},"active":{"type":"boolean"},"created_at":{"type":"integer","format":"int64"}}},
        "MerchantServiceWatchListResponse":{"type":"object","additionalProperties":false,"required":["items","total"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/MerchantServiceWatch"}},"total":{"type":"integer","format":"int64"}}},
        "MerchantServiceWatchChangeResponse":{"type":"object","additionalProperties":false,"required":["changed"],"properties":{"changed":{"type":"boolean"}}}
    });
    serde_json::from_value::<OpenApi>(json!({"openapi":"3.1.0","info":{"title":"Merchant service watches","version":"1"},"paths":paths,"components":{"schemas":schemas}})).expect("static service watch contract")
}
