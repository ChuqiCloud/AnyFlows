use super::*;

#[test]
fn upstream_usage_codex_windows_preserve_unknown_and_do_not_expose_private_fields(){
    let value=serde_json::json!({"rate_limit":{"primary_window":{"used_percent":25.5,"limit_window_seconds":18000,"reset_at":2000}},
        "credits":{"balance":"12.50"},"email":"private@example.invalid"});
    let snapshot=transport::parse_codex(&value).unwrap();
    assert_eq!(snapshot.windows.len(),1);
    assert_eq!(snapshot.credits_balance.as_deref(),Some("12.5"));
    assert!(snapshot.remaining.is_none());
    assert!(!serde_json::to_string(&snapshot).unwrap().contains("private@example"));
    assert!(transport::parse_codex(&serde_json::json!({"rate_limit":{"primary_window":{"used_percent":101,"limit_window_seconds":18000}}})).is_err());
}

#[test]
fn upstream_usage_management_urls_keep_prefix_and_strip_only_known_inference_suffix(){
    for(base,expected)in[
        ("https://panel.example/","https://panel.example/api/usage/token/"),
        ("https://panel.example/v1","https://panel.example/api/usage/token/"),
        ("https://panel.example/prefix/v1/","https://panel.example/prefix/api/usage/token/"),
        ("https://panel.example:8443/prefix/","https://panel.example:8443/prefix/api/usage/token/"),
    ]{assert_eq!(transport::management_url(base,"/api/usage/token/").unwrap(),expected);}
    for invalid in["http://panel.example","https://127.0.0.1","https://u:p@panel.example","https://panel.example/?token=x"]{
        assert!(transport::validate_base_url(invalid).is_err());
    }
}

#[test]
fn upstream_usage_exhaustion_requires_known_fresh_scope_and_no_unlimited_flag(){
    let mut snapshot=CredentialUsageSnapshot{remaining:Some("0".to_owned()),..CredentialUsageSnapshot::unavailable()};
    assert!(is_exhausted(&snapshot,1000));
    snapshot.unlimited=true;assert!(!is_exhausted(&snapshot,1000));
    snapshot.unlimited=false;snapshot.remaining=None;assert!(!is_exhausted(&snapshot,1000));
}
