use super::*;

#[test]
fn upstream_usage_codex_windows_preserve_unknown_and_do_not_expose_private_fields() {
    let value = serde_json::json!({"rate_limit":{"primary_window":{"used_percent":25.5,"limit_window_seconds":18000,"reset_at":2000}},
        "credits":{"balance":"12.50"},"email":"private@example.invalid"});
    let snapshot = transport::parse_codex(&value).unwrap();
    assert_eq!(snapshot.windows.len(), 1);
    assert_eq!(snapshot.credits_balance.as_deref(), Some("12.5"));
    assert!(snapshot.remaining.is_none());
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("private@example")
    );
    assert!(transport::parse_codex(&serde_json::json!({"rate_limit":{"primary_window":{"used_percent":101,"limit_window_seconds":18000}}})).is_err());
}

#[test]
fn upstream_usage_management_urls_keep_prefix_and_strip_only_known_inference_suffix() {
    for (base, expected) in [
        (
            "https://panel.example/",
            "https://panel.example/api/usage/token/",
        ),
        (
            "https://panel.example/v1",
            "https://panel.example/api/usage/token/",
        ),
        (
            "https://panel.example/prefix/v1/",
            "https://panel.example/prefix/api/usage/token/",
        ),
        (
            "https://panel.example:8443/prefix/",
            "https://panel.example:8443/prefix/api/usage/token/",
        ),
    ] {
        assert_eq!(
            transport::management_url(base, "/api/usage/token/").unwrap(),
            expected
        );
    }
    for invalid in [
        "http://panel.example",
        "https://127.0.0.1",
        "https://u:p@panel.example",
        "https://panel.example/?token=x",
    ] {
        assert!(transport::validate_base_url(invalid).is_err());
    }
}

#[test]
fn upstream_usage_exhaustion_requires_known_fresh_scope_and_no_unlimited_flag() {
    let mut snapshot = CredentialUsageSnapshot {
        remaining: Some("0".to_owned()),
        ..CredentialUsageSnapshot::unavailable()
    };
    assert!(is_exhausted(&snapshot, 1000));
    snapshot.unlimited = true;
    assert!(!is_exhausted(&snapshot, 1000));
    snapshot.unlimited = false;
    snapshot.remaining = None;
    assert!(!is_exhausted(&snapshot, 1000));
}

fn target(
    provider: UpstreamUsageProvider,
    scope: af_domain::UpstreamUsageScope,
) -> UpstreamUsageTarget {
    UpstreamUsageTarget {
        channel_id: ChannelId::new(1).unwrap(),
        credential_id: CredentialId::new(2).unwrap(),
        secret_owner_id: CredentialId::new(2).unwrap(),
        kind: af_domain::CredentialKind::ApiKey,
        quota_dimension: "global".to_owned(),
        oauth_provider: None,
        oauth_account_key: None,
        base_url: Some("https://panel.example/v1".to_owned()),
        envelope: af_db::EncryptedCredentialEnvelope::new("test", [0; 24], vec![0; 32]).unwrap(),
        proxy_id: None,
        enabled: true,
        config: UpstreamUsageConfig {
            provider,
            scope,
            ..Default::default()
        },
        profile: None,
        profile_secret: None,
        identity: "binding".to_owned(),
        query_identity: "source".to_owned(),
        cache_key: "test".to_owned(),
        operator_user_id: None,
    }
}

#[test]
fn upstream_usage_newapi_raw_quota_converts_once_and_distinguishes_zero_unlimited_and_missing() {
    let mut target = target(
        UpstreamUsageProvider::NewApi,
        af_domain::UpstreamUsageScope::Key,
    );
    let payload = serde_json::json!({"code":true,"data":{"object":"token_usage","total_granted":1000000,"total_used":250000,"total_available":750000,"unlimited_quota":false,"expires_at":2000}});
    let raw = transport::parse_panel(&payload, &target).unwrap();
    assert_eq!(raw.remaining.as_deref(), Some("750000"));
    assert_eq!(raw.unit.as_deref(), Some("quota"));
    assert!(raw.currency.is_none());
    target.config.quota_per_unit = Some("500000".to_owned());
    target.config.currency = Some("USD".to_owned());
    let converted = transport::parse_panel(&payload, &target).unwrap();
    assert_eq!(converted.total.as_deref(), Some("2"));
    assert_eq!(converted.used.as_deref(), Some("0.5"));
    assert_eq!(converted.remaining.as_deref(), Some("1.5"));
    assert_eq!(converted.unit.as_deref(), Some("USD"));
    assert_eq!(converted.scope.as_deref(), Some("key"));
    assert_eq!(converted.expires_at, Some(2000));
    let zero = transport::parse_panel(
        &serde_json::json!({"object":"token_usage","total_available":0,"unlimited_quota":false}),
        &target,
    )
    .unwrap();
    assert_eq!(zero.remaining.as_deref(), Some("0"));
    assert!(!zero.unlimited);
    let unlimited = transport::parse_panel(
        &serde_json::json!({"object":"token_usage","total_available":0,"unlimited_quota":true}),
        &target,
    )
    .unwrap();
    assert!(unlimited.unlimited);
    assert!(unlimited.remaining.is_none());
    assert!(unlimited.total.is_none());
    assert!(
        transport::parse_panel(
            &serde_json::json!({"object":"token_usage","unlimited_quota":false}),
            &target
        )
        .is_err()
    );
    assert!(
        transport::parse_panel(
            &serde_json::json!({"object":"billing_subscription","hard_limit_usd":10}),
            &target
        )
        .is_err()
    );
}

#[test]
fn upstream_usage_oneapi_user_self_quota_is_remaining_not_total() {
    let target = target(
        UpstreamUsageProvider::OneApi,
        af_domain::UpstreamUsageScope::Account,
    );
    let snapshot=transport::parse_panel(&serde_json::json!({"success":true,"data":{"quota":750,"used_quota":250,"access_token":"must-not-expose"}}),&target).unwrap();
    assert_eq!(snapshot.remaining.as_deref(), Some("750"));
    assert_eq!(snapshot.used.as_deref(), Some("250"));
    assert_eq!(snapshot.total.as_deref(), Some("1000"));
    assert_eq!(snapshot.scope.as_deref(), Some("account"));
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("must-not-expose")
    );
}

#[test]
fn upstream_usage_sub2api_keeps_wallet_and_subscription_metrics_without_inventing_usage() {
    let target = target(
        UpstreamUsageProvider::Sub2api,
        af_domain::UpstreamUsageScope::Account,
    );
    let snapshot = transport::parse_panel(
        &serde_json::json!({"data":{"balance":"12.123456789","email":"private@example.invalid"}}),
        &target,
    )
    .unwrap();
    assert_eq!(snapshot.remaining.as_deref(), Some("12.123456789"));
    assert_eq!(snapshot.unit.as_deref(), Some("USD"));
    assert!(snapshot.used.is_none());
    assert!(snapshot.total.is_none());
    let summary=transport::sanitize_subscription(&serde_json::json!({"data":{"subscriptions":[{
        "group_name":"Plan","expires_at":"2030-01-01T00:00:00Z","daily_limit_usd":"10.50","daily_used_usd":0,
        "monthly_limit_usd":100,"monthly_used_usd":"20.25","email":"private@example.invalid","access_token":"private-secret"
    }],"user":{"email":"private@example.invalid"}}})).unwrap();
    assert_eq!(summary["items"][0]["daily_limit_usd"], "10.5");
    assert_eq!(summary["items"][0]["daily_used_usd"], "0");
    assert_eq!(summary["items"][0]["monthly_used_usd"], "20.25");
    let encoded = summary.to_string();
    assert!(!encoded.contains("email"));
    assert!(!encoded.contains("access_token"));
    assert!(!encoded.contains("private"));
}

#[test]
fn upstream_usage_shared_source_derives_independent_threshold_and_pause_state() {
    let mut target = target(
        UpstreamUsageProvider::NewApi,
        af_domain::UpstreamUsageScope::Account,
    );
    let now = 1000;
    let source = CredentialUsageSnapshot {
        status: CredentialUsageStatus::Available,
        scope: Some("account".to_owned()),
        remaining: Some("0".to_owned()),
        last_success_at: Some(now),
        last_checked_at: Some(now),
        stale: false,
        ..CredentialUsageSnapshot::unavailable()
    };
    target.config.pause_when_exhausted = true;
    assert!(
        derive_snapshot(&target, source.clone(), now)
            .pause_until
            .is_some()
    );
    target.config.pause_when_exhausted = false;
    assert!(
        derive_snapshot(&target, source.clone(), now)
            .pause_until
            .is_none()
    );
    let mut value = source;
    value.remaining = Some("5".to_owned());
    target.config.low_balance_threshold = Some("10".to_owned());
    assert_eq!(
        derive_snapshot(&target, value.clone(), now)
            .alert
            .as_deref(),
        Some("low_balance")
    );
    target.config.low_balance_threshold = Some("1".to_owned());
    assert!(derive_snapshot(&target, value.clone(), now).alert.is_none());
    value.stale = true;
    target.config.pause_when_exhausted = true;
    value.remaining = Some("0".to_owned());
    assert!(derive_snapshot(&target, value, now).pause_until.is_none());
}
