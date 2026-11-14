//! Exact management operation grants. Gateway authentication never consults this registry.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock},
};

use af_admin::SessionRole;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

/// The policy of one registered HTTP method and route template.
#[derive(Clone, Debug)]
pub(crate) struct AccessTokenRoutePolicy {
    pub required_scopes: Vec<String>,
    pub admin_only: bool,
    pub session_only: bool,
    pub operation_id: String,
    pub summary: String,
}

/// Immutable grants owned by one assembled router. Never shared across unrelated
/// router instances through process-global registration.
#[derive(Clone)]
pub(crate) struct AccessTokenRouteOverlay {
    routes: Arc<BTreeMap<String, AccessTokenRoutePolicy>>,
    document: Arc<utoipa::openapi::OpenApi>,
}

impl AccessTokenRouteOverlay {
    pub(crate) fn from_documents(documents: &[utoipa::openapi::OpenApi]) -> Self {
        let mut merged_document = crate::openapi::raw_openapi_document();
        let core = serde_json::to_value(&merged_document).expect("OpenAPI is serializable");
        let mut occupied = BTreeSet::new();
        if let Some(paths) = core.get("paths").and_then(Value::as_object) {
            for (path, item) in paths {
                for method in METHODS {
                    if item.get(method).is_some() {
                        occupied.insert(route_key(method, path));
                    }
                }
            }
        }
        let mut operation_ids: BTreeSet<String> = registry()
            .values()
            .map(|rule| rule.operation_id.clone())
            .collect();
        let mut admin_scopes: BTreeSet<String> = registry()
            .values()
            .filter(|rule| rule.admin_only)
            .flat_map(|rule| rule.required_scopes.iter().cloned())
            .collect();
        let available_user_scopes: BTreeSet<String> = scope_catalog(SessionRole::User)
            .into_iter()
            .map(|item| item.scope)
            .collect();
        let mut routes = BTreeMap::new();
        for document in documents {
            let document =
                serde_json::to_value(document).expect("extension OpenAPI is serializable");
            let Some(paths) = document.get("paths").and_then(Value::as_object) else {
                continue;
            };
            for (path, item) in paths {
                for method in METHODS {
                    let Some(operation) = item.get(method) else {
                        continue;
                    };
                    assert!(
                        occupied.insert(route_key(method, path)),
                        "extension operation conflicts with an existing HTTP route"
                    );
                    if !path.starts_with("/api/") || !has_session_security(operation) {
                        continue;
                    }
                    let operation_id = operation
                        .get("operationId")
                        .and_then(Value::as_str)
                        .expect("management extension operations require a stable operationId");
                    assert!(
                        operation_ids.insert(operation_id.to_owned()),
                        "extension management operationId must be unique"
                    );
                    let summary = operation
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or(operation_id);
                    let mut rule = operation_policy(method, path, operation_id, summary);
                    if rule.admin_only && !rule.session_only {
                        let scope = &rule.required_scopes[0];
                        assert!(
                            scope.len() <= 128
                                && snake_case(operation_id)
                                    .bytes()
                                    .all(|byte| byte.is_ascii_lowercase()
                                        || byte.is_ascii_digit()
                                        || byte == b'_'),
                            "extension operationId must produce a valid admin scope"
                        );
                        assert!(
                            admin_scopes.insert(scope.clone()),
                            "extension admin operation scope must be unique"
                        );
                    } else if !is_session_only(path) {
                        let scopes = operation
                            .get("x-anyflows-pat-scopes")
                            .and_then(Value::as_array)
                            .and_then(|values| {
                                values
                                    .iter()
                                    .map(|value| value.as_str().map(str::to_owned))
                                    .collect::<Option<Vec<_>>>()
                            });
                        if let Some(scopes) = scopes.filter(|scopes| {
                            !scopes.is_empty()
                                && scopes.len() <= 1_024
                                && scopes
                                    .iter()
                                    .all(|scope| available_user_scopes.contains(scope))
                                && scopes.iter().collect::<BTreeSet<_>>().len() == scopes.len()
                        }) {
                            rule.required_scopes = scopes;
                            rule.session_only = false;
                        } else {
                            rule.required_scopes.clear();
                            rule.session_only = true;
                        }
                    }
                    routes.insert(route_key(method, path), rule);
                }
            }
        }
        for document in documents {
            merged_document.merge(document.clone());
        }
        let mut overlay = Self {
            routes: Arc::new(routes),
            document: Arc::new(merged_document.clone()),
        };
        annotate_document_with_overlay(&mut merged_document, Some(&overlay));
        overlay.document = Arc::new(merged_document);
        overlay
    }

    pub(crate) fn document(&self) -> &utoipa::openapi::OpenApi {
        &self.document
    }
}

#[derive(Clone, Serialize, ToSchema)]
pub(crate) struct AccessTokenScopeResponse {
    pub scope: String,
    pub label: String,
    pub description: String,
    pub admin_only: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AccessTokenPresetResponse {
    pub id: String,
    pub label: String,
    pub scopes: Vec<String>,
}

/// Only paths present in the server's Rust operation contract can authorize PATs.
pub(crate) fn policy(method: &str, path: &str) -> Option<&'static AccessTokenRoutePolicy> {
    registry().get(&route_key(method, path)).or_else(|| {
        // Axum automatically serves HEAD through the registered GET handler.
        if method.eq_ignore_ascii_case("HEAD") {
            registry().get(&route_key("GET", path))
        } else {
            None
        }
    })
}

pub(crate) fn policy_with_overlay<'a>(
    method: &str,
    path: &str,
    overlay: Option<&'a AccessTokenRouteOverlay>,
) -> Option<&'a AccessTokenRoutePolicy> {
    overlay
        .and_then(|overlay| {
            overlay.routes.get(&route_key(method, path)).or_else(|| {
                if method.eq_ignore_ascii_case("HEAD") {
                    overlay.routes.get(&route_key("GET", path))
                } else {
                    None
                }
            })
        })
        .or_else(|| policy(method, path))
}

pub(crate) fn scope_catalog(role: SessionRole) -> Vec<AccessTokenScopeResponse> {
    scope_catalog_with_overlay(role, None)
}

pub(crate) fn scope_catalog_with_overlay(
    role: SessionRole,
    overlay: Option<&AccessTokenRouteOverlay>,
) -> Vec<AccessTokenScopeResponse> {
    let mut scopes = BTreeMap::new();
    for policy in registry()
        .values()
        .chain(
            overlay
                .into_iter()
                .flat_map(|overlay| overlay.routes.values()),
        )
        .filter(|policy| !policy.session_only)
    {
        if policy.admin_only && role != SessionRole::Admin {
            continue;
        }
        for scope in &policy.required_scopes {
            scopes
                .entry(scope.clone())
                .or_insert_with(|| AccessTokenScopeResponse {
                    scope: scope.clone(),
                    label: if policy.admin_only {
                        policy.summary.clone()
                    } else {
                        scope_label(scope).to_owned()
                    },
                    description: if policy.admin_only {
                        format!("{}；仍须当前管理员角色与平台权限", policy.operation_id)
                    } else {
                        "仅限当前用户有权操作的本人资源".to_owned()
                    },
                    admin_only: policy.admin_only,
                });
        }
    }
    scopes.into_values().collect()
}

pub(crate) fn scope_presets(role: SessionRole) -> Vec<AccessTokenPresetResponse> {
    scope_presets_with_overlay(role, None)
}

pub(crate) fn scope_presets_with_overlay(
    role: SessionRole,
    overlay: Option<&AccessTokenRouteOverlay>,
) -> Vec<AccessTokenPresetResponse> {
    let available: BTreeSet<String> = scope_catalog_with_overlay(role, overlay)
        .into_iter()
        .map(|item| item.scope)
        .collect();
    let mut presets = Vec::new();
    for (id, label, grants) in [
        (
            "account_read",
            "账户只读",
            &[
                "account:read",
                "external_accounts:read",
                "models:read",
                "usage:read",
                "billing:read",
            ][..],
        ),
        (
            "billing_read",
            "账单统计",
            &["usage:read", "billing:read"][..],
        ),
        (
            "api_keys",
            "模型 Key 管理",
            &["api_keys:read", "api_keys:write"][..],
        ),
        (
            "merchant",
            "商户运营",
            &["merchant:read", "merchant:write", "merchant_finance:read"][..],
        ),
    ] {
        presets.push(AccessTokenPresetResponse {
            id: id.to_owned(),
            label: label.to_owned(),
            scopes: grants
                .iter()
                .filter(|scope| available.contains(**scope))
                .map(|scope| (*scope).to_owned())
                .collect(),
        });
    }
    if role == SessionRole::Admin {
        presets.push(AccessTokenPresetResponse {
            id: "admin_full".to_owned(),
            label: "管理员全权限".to_owned(),
            scopes: available.into_iter().collect(),
        });
    }
    presets
}

pub(crate) fn validate_scopes(role: SessionRole, scopes: &[String]) -> bool {
    validate_scopes_with_overlay(role, scopes, None)
}

pub(crate) fn validate_scopes_with_overlay(
    role: SessionRole,
    scopes: &[String],
    overlay: Option<&AccessTokenRouteOverlay>,
) -> bool {
    if scopes.is_empty() || scopes.len() > 1_024 {
        return false;
    }
    let allowed: BTreeSet<String> = scope_catalog_with_overlay(role, overlay)
        .into_iter()
        .map(|item| item.scope)
        .collect();
    let mut unique = BTreeSet::new();
    scopes
        .iter()
        .all(|scope| allowed.contains(scope) && unique.insert(scope))
}

pub(crate) fn annotate_document(document: &mut utoipa::openapi::OpenApi) {
    annotate_document_with_overlay(document, None);
}

pub(crate) fn annotate_document_with_overlay(
    document: &mut utoipa::openapi::OpenApi,
    overlay: Option<&AccessTokenRouteOverlay>,
) {
    use utoipa::openapi::security::SecurityRequirement;
    for (path, item) in &mut document.paths.paths {
        for (method, operation) in [
            ("GET", &mut item.get),
            ("POST", &mut item.post),
            ("PUT", &mut item.put),
            ("PATCH", &mut item.patch),
            ("DELETE", &mut item.delete),
            ("HEAD", &mut item.head),
            ("OPTIONS", &mut item.options),
        ] {
            let Some(operation) = operation else {
                continue;
            };
            let Some(policy) = policy_with_overlay(method, path, overlay) else {
                continue;
            };
            let extensions = operation.extensions.get_or_insert_with(Default::default);
            if policy.session_only {
                extensions.insert("x-anyflows-pat-session-only".to_owned(), Value::Bool(true));
                extensions.remove("x-anyflows-pat-scopes");
            } else {
                extensions.remove("x-anyflows-pat-session-only");
                extensions.insert(
                    "x-anyflows-pat-scopes".to_owned(),
                    serde_json::json!(policy.required_scopes),
                );
                let security = operation.security.get_or_insert_with(Vec::new);
                let personal_access_token =
                    SecurityRequirement::new("personalAccessTokenBearer", Vec::<String>::new());
                if !security.contains(&personal_access_token) {
                    security.push(personal_access_token);
                }
            }
        }
    }
}

fn registry() -> &'static BTreeMap<String, AccessTokenRoutePolicy> {
    static REGISTRY: OnceLock<BTreeMap<String, AccessTokenRoutePolicy>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let document = serde_json::to_value(crate::openapi::raw_openapi_document())
            .expect("OpenAPI is serializable");
        let mut registry = BTreeMap::new();
        if let Some(paths) = document.get("paths").and_then(Value::as_object) {
            for (path, item) in paths {
                for method in METHODS {
                    let Some(operation) = item.get(method) else {
                        continue;
                    };
                    if !path.starts_with("/api/") || !has_session_security(operation) {
                        continue;
                    }
                    let Some(operation_id) = operation.get("operationId").and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let summary = operation
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or(operation_id);
                    let rule = operation_policy(method, path, operation_id, summary);
                    assert!(
                        registry.insert(route_key(method, path), rule).is_none(),
                        "duplicate management operation"
                    );
                }
            }
        }
        for (method, path, operation_id) in [
            ("GET", "/api/explorer/catalog", "listApiExplorerCatalog"),
            (
                "GET",
                "/api/explorer/catalog/{operation_id}",
                "getApiExplorerOperation",
            ),
        ] {
            registry
                .entry(route_key(method, path))
                .or_insert_with(|| operation_policy(method, path, operation_id, "读取 API 目录"));
        }
        registry.insert(
            route_key("GET", "/api/access-token/self"),
            AccessTokenRoutePolicy {
                required_scopes: Vec::new(),
                admin_only: false,
                session_only: false,
                operation_id: "getPersonalAccessTokenSelf".to_owned(),
                summary: "读取当前 PAT 身份".to_owned(),
            },
        );
        assert_unique_grants(&registry);
        registry
    })
}

fn assert_unique_grants(registry: &BTreeMap<String, AccessTokenRoutePolicy>) {
    let mut operation_ids = BTreeSet::new();
    let mut admin_scopes = BTreeSet::new();
    for rule in registry.values() {
        assert!(
            operation_ids.insert(&rule.operation_id),
            "duplicate management operationId"
        );
        if rule.admin_only {
            for scope in &rule.required_scopes {
                assert!(
                    admin_scopes.insert(scope),
                    "duplicate management admin scope"
                );
            }
        }
    }
}

fn has_session_security(operation: &Value) -> bool {
    operation
        .get("security")
        .and_then(Value::as_array)
        .is_some_and(|requirements| {
            requirements
                .iter()
                .any(|requirement| requirement.get("bearerAuth").is_some())
        })
}

fn operation_policy(
    method: &str,
    path: &str,
    operation_id: &str,
    summary: &str,
) -> AccessTokenRoutePolicy {
    let session_only = is_session_only(path);
    let admin_only = path.starts_with("/api/admin/") || path.starts_with("/api/platform/");
    let required_scopes = if session_only {
        Vec::new()
    } else if admin_only {
        vec![format!("admin:{}:execute", snake_case(operation_id))]
    } else {
        user_scopes(method, path)
            .iter()
            .map(|scope| (*scope).to_owned())
            .collect()
    };
    AccessTokenRoutePolicy {
        session_only: session_only || required_scopes.is_empty(),
        required_scopes,
        admin_only,
        operation_id: operation_id.to_owned(),
        summary: summary.to_owned(),
    }
}

fn is_session_only(path: &str) -> bool {
    path.starts_with("/api/auth/")
        || path.starts_with("/api/account/access-tokens")
        || path.starts_with("/api/admin/access-tokens")
        || (path.starts_with("/api/admin/users/") && path.ends_with("/access-tokens/revoke-all"))
}

fn user_scopes(method: &str, path: &str) -> &'static [&'static str] {
    let read = method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD");
    match path {
        "/api/account/external-connectors" | "/api/account/external-accounts/{id}/balances" => {
            &["external_accounts:read"]
        }
        "/api/account/external-accounts" => {
            if read {
                &["external_accounts:read"]
            } else {
                &["external_accounts:write"]
            }
        }
        "/api/account/external-accounts/{id}/refresh" => &["external_accounts:write"],
        "/api/account/external-accounts/{id}" => &["external_accounts:write"],
        "/api/access-token/self" => &["account:read"],
        "/api/models"
        | "/api/model-providers"
        | "/api/account/plugin-models/sources"
        | "/api/account/plugin-models/{plugin_id}" => &["models:read"],
        "/api/token-groups" => &["api_keys:read"],
        "/api/tokens" | "/api/tokens/{id}" => {
            if read {
                &["api_keys:read"]
            } else {
                &["api_keys:write"]
            }
        }
        "/api/account/profile" => {
            if read {
                &["account:read"]
            } else {
                &["account:write"]
            }
        }
        "/api/account/usage-logs" | "/api/account/audit-logs" => &["usage:read"],
        "/api/account/wallet"
        | "/api/account/wallet/entries"
        | "/api/account/refund-reconciliations"
        | "/api/account/wallet/topups/config" => &["billing:read"],
        "/api/account/wallet/redemptions" | "/api/account/wallet/topups" => &["billing:write"],
        "/api/account/invitations" => &["invitations:read"],
        "/api/account/notifications" => {
            if read {
                &["notifications:read"]
            } else {
                &["notifications:write"]
            }
        }
        "/api/account/notifications/read" => &["notifications:write"],
        "/api/account/subscriptions"
        | "/api/account/subscription-catalog"
        | "/api/account/subscription-orders/{order_id}" => &["subscriptions:read"],
        "/api/account/subscription-orders"
        | "/api/account/subscription-orders/{order_id}/payment" => &["subscriptions:write"],
        "/api/account/two-factor"
        | "/api/account/passkeys"
        | "/api/account/passkeys/{id}"
        | "/api/account/passkeys/registration/options"
        | "/api/account/passkeys/registration/verify"
        | "/api/account/password"
        | "/api/account/profile/email-verification"
        | "/api/account/profile/email" => {
            if read {
                &["account_security:read"]
            } else {
                &["account_security:write"]
            }
        }
        "/api/account/verifications"
        | "/api/account/verifications/{case_id}"
        | "/api/account/verifications/eligibility"
        | "/api/account/verifications/{case_id}/materials/{material_id}"
        | "/api/account/verifications/{case_id}/provider-sync" => {
            if read {
                &["verification:read"]
            } else {
                &["verification:write"]
            }
        }
        "/api/user-routes"
        | "/api/user-routes/{id}"
        | "/api/merchant-pool/routes"
        | "/api/merchant-pool/routes/{id}"
        | "/api/merchant-pool/routes/preview" => {
            if read {
                &["routes:read"]
            } else {
                &["routes:write"]
            }
        }
        "/api/merchant-pool/attempts"
        | "/api/merchant-pools/{pool_id}/statistics"
        | "/api/merchant-pools/{pool_id}/timeline" => &["usage:read"],
        "/api/merchant/services/{id}/key" => &["merchant:write", "merchant_secrets:write"],
        "/api/merchant/quote" => &["merchant:write"],
        "/api/merchant/services"
        | "/api/merchant/services/{id}"
        | "/api/merchant/profile"
        | "/api/merchant/apply"
        | "/api/merchant/available-pools"
        | "/api/merchant/services/{id}/pause"
        | "/api/merchant/services/{id}/submit"
        | "/api/merchant/services/{id}/maintenance"
        | "/api/merchant/services/{id}/maintenance/{notice}"
        | "/api/merchant/services/{id}/maintenance/{notice}/cancel"
        | "/api/merchant/services/{id}/prices"
        | "/api/merchant/services/{id}/prices/{price}/cancel" => {
            if read {
                &["merchant:read"]
            } else {
                &["merchant:write"]
            }
        }
        "/api/merchant-pools"
        | "/api/merchant-pools/{pool}/model-providers"
        | "/api/merchant-pools/{pool}/services"
        | "/api/merchant-pools/{pool}/services/{id}"
        | "/api/merchant-pool/watches"
        | "/api/merchant-pools/{pool}/services/{service}/watch" => {
            if read {
                &["merchant:read"]
            } else {
                &["merchant:write"]
            }
        }
        "/api/merchant/statistics"
        | "/api/merchant/finance"
        | "/api/merchant/finance/entries"
        | "/api/merchant/finance/report"
        | "/api/merchant/finance/report.csv"
        | "/api/merchant/withdrawals"
        | "/api/merchant/payout-account"
        | "/api/merchant/payouts" => {
            if read {
                &["merchant_finance:read"]
            } else {
                &["merchant_finance:write"]
            }
        }
        "/api/playground/conversations"
        | "/api/playground/conversations/{conversation_id}"
        | "/api/playground/image-history"
        | "/api/playground/image-history/{image_id}"
        | "/api/playground/image-history/{image_id}/image"
        | "/api/playground/image-history/{image_id}/thumbnail"
        | "/api/playground/shares"
        | "/api/playground/shares/{token}" => {
            if read {
                &["playground:read"]
            } else {
                &["playground:write"]
            }
        }
        "/api/explorer/catalog" | "/api/explorer/catalog/{operation_id}" => &["api_catalog:read"],
        _ => &[],
    }
}

fn route_key(method: &str, path: &str) -> String {
    let mut normalized = String::new();
    let mut parameter = false;
    for character in path.chars() {
        if character == '{' {
            parameter = true;
            normalized.push_str("{}");
        } else if character == '}' {
            parameter = false;
        } else if !parameter {
            normalized.push(character);
        }
    }
    format!("{} {normalized}", method.to_ascii_uppercase())
}

fn snake_case(value: &str) -> String {
    let mut result = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            result.push('_');
        }
        result.push(character.to_ascii_lowercase());
    }
    result
}

fn scope_label(scope: &str) -> &str {
    match scope {
        "account:read" => "读取本人账户",
        "account:write" => "更新本人资料",
        "external_accounts:read" => "读取本人外部平台账户",
        "external_accounts:write" => "管理本人外部平台账户绑定",
        "api_keys:read" => "读取本人模型 Key",
        "api_keys:write" => "管理本人模型 Key",
        "models:read" => "读取模型目录",
        "usage:read" => "读取本人用量与审计",
        "billing:read" => "读取本人账单",
        "billing:write" => "管理本人充值与兑换",
        "merchant:read" => "读取本人商户服务",
        "merchant:write" => "管理本人商户服务",
        "merchant_secrets:write" => "写入本人商户上游凭据",
        "merchant_finance:read" => "读取本人商户财务",
        "merchant_finance:write" => "管理本人商户提现",
        "routes:read" => "读取本人智能路由",
        "routes:write" => "管理本人智能路由",
        "account_security:read" => "读取本人账户安全设置",
        "account_security:write" => "管理本人账户安全设置",
        "notifications:read" => "读取本人通知",
        "notifications:write" => "管理本人通知",
        "playground:read" => "读取本人试炼场历史",
        "playground:write" => "管理本人试炼场历史",
        "verification:read" => "读取本人认证",
        "verification:write" => "提交本人认证",
        "subscriptions:read" => "读取本人订阅",
        "subscriptions:write" => "管理本人订阅",
        "invitations:read" => "读取本人邀请",
        "api_catalog:read" => "读取 API 目录",
        _ => scope,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_are_exact_operations_and_cannot_cross_authentication_domains() {
        assert!(policy("POST", "/v1/chat/completions").is_none());
        assert!(policy("POST", "/api/admin/never-registered").is_none());
        assert!(
            policy("POST", "/api/account/access-tokens").is_some_and(|policy| policy.session_only)
        );
        let rule = policy("GET", "/api/admin/users").unwrap();
        assert!(rule.admin_only);
        assert_eq!(rule.required_scopes, ["admin:list_admin_users:execute"]);
        let user_catalog = scope_catalog(SessionRole::User);
        assert!(user_catalog.iter().all(|scope| !scope.admin_only));
        assert!(!validate_scopes(SessionRole::User, &rule.required_scopes));
        assert!(validate_scopes(SessionRole::Admin, &rule.required_scopes));
    }

    #[test]
    fn external_account_operations_have_separate_user_scopes() {
        assert_eq!(
            policy("GET", "/api/account/external-connectors")
                .unwrap()
                .required_scopes,
            ["external_accounts:read"]
        );
        assert_eq!(
            policy("GET", "/api/account/external-accounts")
                .unwrap()
                .required_scopes,
            ["external_accounts:read"]
        );
        assert_eq!(
            policy("POST", "/api/account/external-accounts")
                .unwrap()
                .required_scopes,
            ["external_accounts:write"]
        );
        assert_eq!(
            policy("POST", "/api/account/external-accounts/{id}/refresh")
                .unwrap()
                .required_scopes,
            ["external_accounts:write"]
        );
        assert_eq!(
            policy("DELETE", "/api/account/external-accounts/{id}")
                .unwrap()
                .required_scopes,
            ["external_accounts:write"]
        );
    }

    #[test]
    fn path_parameter_names_do_not_change_exact_route_identity() {
        assert_eq!(
            route_key("get", "/api/tokens/{id}"),
            route_key("GET", "/api/tokens/{token_id}")
        );
        assert_ne!(
            route_key("GET", "/api/tokens/{id}"),
            route_key("PUT", "/api/tokens/{id}")
        );
    }

    #[test]
    fn extension_overlay_exposes_only_documented_scopes_and_concrete_admin_grants() {
        let document: utoipa::openapi::OpenApi = serde_json::from_value(serde_json::json!({
            "openapi":"3.1.0", "info":{"title":"extension","version":"1"},
            "paths":{
                "/api/admin/extensions/status":{"get":{"operationId":"getExtensionStatus","summary":"Extension status","security":[{"bearerAuth":[]}],"responses":{"200":{"description":"ok"}}}},
                "/api/extensions/me":{"get":{"operationId":"getExtensionProfile","security":[{"bearerAuth":[]}],"x-anyflows-pat-scopes":["account:read"],"responses":{"200":{"description":"ok"}}}},
                "/api/extensions/unscoped":{"get":{"operationId":"getUnscopedExtensionData","security":[{"bearerAuth":[]}],"responses":{"200":{"description":"ok"}}}}
            }
        })).unwrap();
        let overlay = AccessTokenRouteOverlay::from_documents(&[document]);
        let admin =
            policy_with_overlay("GET", "/api/admin/extensions/status", Some(&overlay)).unwrap();
        assert_eq!(
            admin.required_scopes,
            ["admin:get_extension_status:execute"]
        );
        assert!(admin.admin_only && !admin.session_only);
        let user_extension =
            policy_with_overlay("GET", "/api/extensions/me", Some(&overlay)).unwrap();
        assert_eq!(user_extension.required_scopes, ["account:read"]);
        let unscoped =
            policy_with_overlay("GET", "/api/extensions/unscoped", Some(&overlay)).unwrap();
        assert!(unscoped.session_only);
        assert!(!validate_scopes_with_overlay(
            SessionRole::User,
            &["admin:get_extension_status:execute".to_owned()],
            Some(&overlay)
        ));
        assert!(validate_scopes_with_overlay(
            SessionRole::Admin,
            &["admin:get_extension_status:execute".to_owned()],
            Some(&overlay)
        ));
        assert!(
            scope_presets_with_overlay(SessionRole::Admin, Some(&overlay))
                .iter()
                .find(|preset| preset.id == "admin_full")
                .is_some_and(|preset| preset.scopes.contains(&admin.required_scopes[0]))
        );
        let merged = serde_json::to_value(overlay.document()).unwrap();
        assert_eq!(
            merged["components"]["securitySchemes"]["personalAccessTokenBearer"]["scheme"],
            "bearer"
        );
        assert!(merged["paths"]["/api/extensions/me"]["get"]["x-anyflows-pat-scopes"].is_array());
        assert!(
            merged["paths"]["/api/extensions/me"]["get"]["security"]
                .as_array()
                .unwrap()
                .iter()
                .any(|requirement| requirement.get("personalAccessTokenBearer").is_some())
        );
    }

    #[test]
    #[should_panic(expected = "extension operation conflicts with an existing HTTP route")]
    fn extension_overlay_rejects_core_operation_conflicts_at_startup() {
        let document: utoipa::openapi::OpenApi = serde_json::from_value(serde_json::json!({
            "openapi":"3.1.0", "info":{"title":"extension","version":"1"},
            "paths":{"/api/tokens":{"get":{"operationId":"getDuplicateTokens","responses":{"200":{"description":"ok"}}}}}
        })).unwrap();
        AccessTokenRouteOverlay::from_documents(&[document]);
    }

    #[test]
    #[should_panic(expected = "duplicate management operationId")]
    fn core_registry_rejects_reused_operation_ids() {
        let rule = operation_policy("GET", "/api/admin/one", "getAdminResource", "resource");
        assert_unique_grants(&BTreeMap::from([
            ("GET /api/admin/one".to_owned(), rule.clone()),
            ("GET /api/admin/two".to_owned(), rule),
        ]));
    }

    #[test]
    #[should_panic(expected = "duplicate management admin scope")]
    fn core_registry_rejects_distinct_operation_ids_with_the_same_admin_scope() {
        assert_unique_grants(&BTreeMap::from([
            (
                "GET /api/admin/one".to_owned(),
                operation_policy("GET", "/api/admin/one", "getAdminResource", "one"),
            ),
            (
                "GET /api/admin/two".to_owned(),
                operation_policy("GET", "/api/admin/two", "get_admin_resource", "two"),
            ),
        ]));
    }
}
