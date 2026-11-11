//! Compare literal production Router registrations with the Rust operation contract.

use std::{collections::BTreeSet, path::Path};

use serde_json::Value;
use super::access_token_routes::policy;

#[test]
fn personal_access_token_policies_cover_actual_management_router_registrations() {
    let raw = serde_json::to_value(crate::openapi::raw_openapi_document()).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut paths = Vec::new();
    collect_sources(&source, &mut paths);
    let mut checked = 0;
    let mut missing = BTreeSet::new();
    for file in paths {
        let text = std::fs::read_to_string(&file).unwrap();
        let production = text.split("#[cfg(test)]").next().unwrap_or(&text);
        for (method, path) in route_registrations(production) {
            if !path.starts_with("/api/") { continue; }
            if path.starts_with("/api/explorer/") { assert!(policy(&method, &path).is_some()); continue; }
            let contract = raw.get("paths").and_then(|paths| paths.get(&path))
                .and_then(|item| item.get(method.to_ascii_lowercase()));
            if contract.is_none() { missing.insert(format!("{method} {path} ({})", file.display())); continue; }
            let bearer = contract.and_then(|operation| operation.get("security")).and_then(Value::as_array)
                .is_some_and(|requirements| requirements.iter().any(|requirement| requirement.get("bearerAuth").is_some()));
            if bearer {
                checked += 1;
                let Some(rule) = policy(&method, &path) else { missing.insert(format!("{method} {path}: no PAT policy")); continue; };
                if path.starts_with("/api/admin/") && !path.contains("/access-tokens") {
                    assert!(rule.admin_only && !rule.session_only, "{method} {path}");
                    assert_eq!(rule.required_scopes.len(), 1, "{method} {path}");
                    assert!(rule.required_scopes[0].starts_with("admin:"));
                }
            }
        }
    }
    assert!(checked > 200, "the production router scan must not silently become empty");
    assert!(missing.is_empty(), "management operations missing a contract/policy:\n{}", missing.into_iter().collect::<Vec<_>>().join("\n"));
}

fn collect_sources(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains("tests") || name == "openapi" { continue; }
        if path.is_dir() { collect_sources(&path, files); }
        else if path.extension().is_some_and(|extension| extension == "rs") { files.push(path); }
    }
}

fn route_registrations(source: &str) -> Vec<(String, String)> {
    let mut remaining = source;
    let mut routes = Vec::new();
    while let Some(start) = remaining.find(".route(") {
        remaining = &remaining[start + ".route(".len()..];
        let Some(end) = closing_parenthesis(remaining) else { break; };
        let block = &remaining[..end];
        let candidate = block.trim_start();
        if let Some(candidate) = candidate.strip_prefix('"') {
            if let Some(quote) = candidate.find('"') {
                let path = &candidate[..quote];
                let method_source = &candidate[quote + 1..];
                for method in ["get", "post", "put", "patch", "delete", "head", "options"] {
                    if has_method_call(method_source, method) { routes.push((method.to_ascii_uppercase(), path.to_owned())); }
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
            && suffix.trim_start().starts_with('(') { return true; }
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
            if escaped { escaped = false; }
            else if character == '\\' { escaped = true; }
            else if character == '"' { quoted = false; }
            continue;
        }
        match character {
            '"' => quoted = true,
            '(' => depth += 1,
            ')' => { depth -= 1; if depth == 0 { return Some(index); } }
            _ => {}
        }
    }
    None
}

#[test]
fn route_scanner_preserves_methods_and_ignores_handler_names() {
    let routes = route_registrations("Router::new().route(\"/api/example\", get(read).post(write)).route(\"/v1/test\", axum::routing::post(handle))");
    assert_eq!(routes, [("GET".to_owned(), "/api/example".to_owned()),
        ("POST".to_owned(), "/api/example".to_owned()), ("POST".to_owned(), "/v1/test".to_owned())]);
}
