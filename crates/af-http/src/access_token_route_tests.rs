//! Compare literal production Router registrations with the Rust operation contract.

use std::{collections::BTreeSet, path::Path};

use super::access_token_routes::policy;
use serde_json::Value;

#[test]
fn personal_access_token_policies_cover_actual_management_router_registrations() {
    let raw = serde_json::to_value(crate::openapi::raw_openapi_document()).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut paths = Vec::new();
    collect_sources(&source, &mut paths);
    let mut checked = 0;
    let mut missing = BTreeSet::new();
    let mut playground_operations = BTreeSet::new();
    for file in paths {
        let text = std::fs::read_to_string(&file).unwrap();
        let production = text.split("#[cfg(test)]").next().unwrap_or(&text);
        for (method, path) in route_registrations(production) {
            if !path.starts_with("/api/") {
                continue;
            }
            if is_interactive_playground_operation(&method, &path) {
                assert!(
                    policy(&method, &path).is_none(),
                    "PATs must not authorize session playground model calls: {method} {path}"
                );
                playground_operations.insert(format!("{method} {path}"));
                continue;
            }
            if path.starts_with("/api/explorer/") {
                assert!(policy(&method, &path).is_some());
                continue;
            }
            let contract = raw
                .get("paths")
                .and_then(|paths| paths.get(&path))
                .and_then(|item| item.get(method.to_ascii_lowercase()));
            if contract.is_none() {
                missing.insert(format!("{method} {path} ({})", file.display()));
                continue;
            }
            let bearer = contract
                .and_then(|operation| operation.get("security"))
                .and_then(Value::as_array)
                .is_some_and(|requirements| {
                    requirements
                        .iter()
                        .any(|requirement| requirement.get("bearerAuth").is_some())
                });
            if is_public_or_external_callback(&method, &path) {
                assert!(
                    !bearer,
                    "public routes must not require session Bearer: {method} {path}"
                );
                assert!(
                    policy(&method, &path).is_none(),
                    "public routes must not receive a PAT policy: {method} {path}"
                );
                continue;
            }
            if path == "/api/access-token/self" {
                assert!(
                    contract
                        .and_then(|operation| operation.get("security"))
                        .and_then(Value::as_array)
                        .is_some_and(|requirements| requirements.iter().any(|requirement| {
                            requirement.get("personalAccessTokenBearer").is_some()
                        })),
                    "PAT self inspection must declare its independent authentication scheme"
                );
                assert!(policy(&method, &path).is_some_and(|rule| !rule.session_only));
                continue;
            }
            if requires_management_bearer(&method, &path) && !bearer {
                missing.insert(format!(
                    "{method} {path}: protected source route missing bearerAuth security"
                ));
                continue;
            }
            if bearer {
                checked += 1;
                let Some(rule) = policy(&method, &path) else {
                    missing.insert(format!("{method} {path}: no PAT policy"));
                    continue;
                };
                if (path.starts_with("/api/admin/") || path.starts_with("/api/platform/"))
                    && !path.contains("/access-tokens")
                {
                    assert!(rule.admin_only && !rule.session_only, "{method} {path}");
                    assert_eq!(rule.required_scopes.len(), 1, "{method} {path}");
                    assert!(rule.required_scopes[0].starts_with("admin:"));
                }
                if path == "/api/auth/session" {
                    assert!(
                        rule.session_only,
                        "login session inspection must remain interactive"
                    );
                }
                match (method.as_str(), path.as_str()) {
                    ("GET", "/api/account/usage-logs" | "/api/account/audit-logs") => {
                        assert_eq!(rule.required_scopes, ["usage:read"]);
                        assert!(!rule.admin_only && !rule.session_only);
                    }
                    ("POST", "/api/merchant/services/{id}/key") => {
                        assert_eq!(
                            rule.required_scopes,
                            ["merchant:write", "merchant_secrets:write"]
                        );
                        assert!(!rule.admin_only && !rule.session_only);
                    }
                    ("GET", "/api/admin/audit-logs") => {
                        assert!(rule.admin_only && !rule.session_only);
                        assert_eq!(rule.required_scopes.len(), 1);
                        assert!(rule.required_scopes[0].starts_with("admin:"));
                    }
                    _ => {}
                }
            }
        }
    }
    assert_eq!(
        playground_operations.len(),
        8,
        "every explicitly allowed session playground model operation must remain registered"
    );
    assert!(
        checked > 200,
        "the production router scan must not silently become empty"
    );
    assert!(
        missing.is_empty(),
        "management operations missing a contract/policy:\n{}",
        missing.into_iter().collect::<Vec<_>>().join("\n")
    );
}

fn is_public_or_external_callback(method: &str, path: &str) -> bool {
    matches!(
        (method, path),
        ("GET", "/api/model-provider-catalog")
            | ("GET", "/api/account/verifications/alipay/callback")
            | ("GET" | "POST", "/api/payment/webhook/{provider}")
            | ("POST", "/api/refund/webhook/{provider}")
    )
}

fn is_interactive_playground_operation(method: &str, path: &str) -> bool {
    matches!(
        (method, path),
        (
            "POST",
            "/api/playground/v1/chat/completions"
                | "/api/playground/v1/images/generations"
                | "/api/playground/v1/images/edits"
                | "/api/playground/v1/responses"
                | "/api/playground/v1/messages"
                | "/api/playground/v1/videos/generations"
        ) | (
            "GET",
            "/api/playground/v1/videos" | "/api/playground/v1/videos/{task_id}"
        )
    )
}

fn requires_management_bearer(method: &str, path: &str) -> bool {
    path == "/api/auth/session"
        || [
            "/api/admin/",
            "/api/platform/",
            "/api/account/",
            "/api/merchant/",
            "/api/merchant-pool/",
            "/api/merchant-pools",
            "/api/user-routes",
            "/api/tokens",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || matches!(
            path,
            "/api/token-groups" | "/api/models" | "/api/model-providers"
        )
        || (path.starts_with("/api/playground/")
            && !(method == "GET" && path == "/api/playground/shares/{token}"))
}

fn collect_sources(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains("tests") || name == "openapi" {
            continue;
        }
        if path.is_dir() {
            collect_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn route_registrations(source: &str) -> Vec<(String, String)> {
    let mut remaining = source;
    let mut routes = Vec::new();
    while let Some(start) = remaining.find(".route(") {
        remaining = &remaining[start + ".route(".len()..];
        let Some(end) = closing_parenthesis(remaining) else {
            break;
        };
        let block = &remaining[..end];
        let candidate = block.trim_start();
        if let Some(candidate) = candidate.strip_prefix('"')
            && let Some(quote) = candidate.find('"')
        {
            let path = &candidate[..quote];
            let method_source = &candidate[quote + 1..];
            for method in ["get", "post", "put", "patch", "delete", "head", "options"] {
                if has_method_call(method_source, method) {
                    routes.push((method.to_ascii_uppercase(), path.to_owned()));
                }
            }
        }
        remaining = &remaining[end + 1..];
    }
    routes
}

fn has_method_call(source: &str, method: &str) -> bool {
    let mut remaining = source;
    while let Some(position) = remaining.find(method) {
        let before = remaining[..position].chars().next_back();
        let suffix = &remaining[position + method.len()..];
        if before.is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_')
            && suffix.trim_start().starts_with('(')
        {
            return true;
        }
        remaining = suffix;
    }
    false
}

fn closing_parenthesis(source: &str) -> Option<usize> {
    let mut depth = 1_u32;
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in source.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

#[test]
fn route_scanner_preserves_methods_and_ignores_handler_names() {
    let routes = route_registrations(
        "Router::new().route(\"/api/example\", get(read).post(write)).route(\"/v1/test\", axum::routing::post(handle))",
    );
    assert_eq!(
        routes,
        [
            ("GET".to_owned(), "/api/example".to_owned()),
            ("POST".to_owned(), "/api/example".to_owned()),
            ("POST".to_owned(), "/v1/test".to_owned())
        ]
    );
}
