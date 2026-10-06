use std::{env, process::Command, thread, time::Duration};

use ::metrics::{Key, Label};

use crate::{
    ANALYTICS_EXPORT_BACKLOG, BILLING_FLUSH_LAG, CHANNEL_HEALTH, CLIENT_SIMULATION_ATTEMPTS_TOTAL,
    CLIENT_SIMULATION_BODY_PATCHES_TOTAL, CONCURRENCY_SLOTS, FIRST_TOKEN_SECONDS,
    MetricClientSimulationBodyProfile, MetricClientSimulationBodyResult,
    MetricClientSimulationProfile, MetricClientSimulationResult, MetricConcurrencyLevel,
    MetricLabelError, MetricOrganizationApprovalTimeoutOutcome, MetricPaymentConfirmationOutcome,
    MetricProtocol, MetricRequestRateLimitFailure, MetricRequestRateLimitSubject,
    MetricRequestStatus, MetricUpstreamErrorType, MetricsHandle,
    ORGANIZATION_APPROVAL_TIMEOUT_TOTAL, PAYMENT_WEBHOOK_CONFIRMATIONS_TOTAL,
    PROMETHEUS_CONTENT_TYPE, PROMETHEUS_PATH, QUOTA_CONSUMED_TOTAL, REQUEST_DURATION_SECONDS,
    REQUEST_RATE_LIMIT_ADMISSIONS_TOTAL, REQUEST_RATE_LIMIT_FAILURES_TOTAL,
    REQUEST_RATE_LIMIT_REJECTIONS_TOTAL, REQUEST_RATE_LIMIT_RETRY_AFTER_SECONDS,
    REQUEST_RATE_LIMIT_RULES_PER_CHECK, REQUESTS_TOTAL, TelemetryError, UPSTREAM_ERRORS_TOTAL,
    init_metrics,
    metrics::{
        MAX_CHANNEL_LABELS, MAX_GROUP_LABELS, MAX_MODEL_LABELS, MAX_SERIES_PER_FAMILY,
        build_metrics_recorder, describe_metric_families,
    },
    record_client_simulation_attempt, record_client_simulation_body_patch,
    record_organization_approval_timeout, record_payment_confirmation,
    record_request_rate_limit_admission, record_request_rate_limit_check,
    record_request_rate_limit_failure, record_request_rate_limit_rejection,
    set_analytics_export_backlog,
};

const GLOBAL_METRICS_INIT_CHILD: &str = "ANYFLOWS_METRICS_INIT_CHILD";

fn emit_sample_metrics(handle: &MetricsHandle) {
    let model = handle.register_model("gpt-test").unwrap();
    let channel = handle.register_channel("channel-main").unwrap();
    let group = handle.register_group("group-standard").unwrap();

    for _ in 0..2 {
        handle.record_request(
            MetricProtocol::OpenAi,
            &model,
            MetricRequestStatus::Success,
            Duration::from_millis(250),
        );
    }
    handle.record_first_token(MetricProtocol::OpenAi, &model, Duration::from_millis(100));
    handle.set_channel_health(&channel, true);
    for _ in 0..3 {
        handle.record_upstream_error(&channel, MetricUpstreamErrorType::Timeout);
    }
    handle.add_quota_consumed(&group, 42);
    handle.set_concurrency_slots(MetricConcurrencyLevel::Account, 7);
    handle.set_billing_flush_lag(Duration::from_millis(1_500));
    record_client_simulation_attempt(
        MetricClientSimulationProfile::AnthropicCliHeadersV1,
        MetricClientSimulationResult::Applied,
    );
    record_client_simulation_body_patch(
        MetricClientSimulationBodyProfile::AnthropicCliSystemDateV1,
        MetricClientSimulationBodyResult::Applied,
    );
    set_analytics_export_backlog(5);
    record_request_rate_limit_check(2);
    record_request_rate_limit_admission();
    record_request_rate_limit_rejection(
        MetricRequestRateLimitSubject::Group,
        Duration::from_millis(1_500),
    );
    record_request_rate_limit_failure(MetricRequestRateLimitFailure::StoreProtocol);
    record_organization_approval_timeout(MetricOrganizationApprovalTimeoutOutcome::Applied, 1);
}

fn metric_line<'a>(rendered: &'a str, prefix: &str) -> &'a str {
    rendered
        .lines()
        .find(|line| line.starts_with(prefix))
        .unwrap_or_else(|| panic!("缺少 Prometheus 指标行: {prefix}"))
}

fn sorted_lines(rendered: &str) -> Vec<&str> {
    let mut lines = rendered
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines
}

fn test_key(name: &str, labels: &[(&str, &str)]) -> Key {
    let labels = labels
        .iter()
        .map(|(key, value)| Label::new((*key).to_owned(), (*value).to_owned()))
        .collect::<Vec<_>>();
    Key::from_parts(name.to_owned(), labels)
}

#[test]
fn payment_confirmation_metric_accepts_only_fixed_outcomes() {
    let (recorder, handle) = build_metrics_recorder();
    ::metrics::with_local_recorder(&recorder, || {
        describe_metric_families();
        for outcome in [
            MetricPaymentConfirmationOutcome::Applied,
            MetricPaymentConfirmationOutcome::Acknowledged,
            MetricPaymentConfirmationOutcome::Existing,
            MetricPaymentConfirmationOutcome::Rejected,
            MetricPaymentConfirmationOutcome::IgnoredNonTerminal,
            MetricPaymentConfirmationOutcome::NotFound,
            MetricPaymentConfirmationOutcome::VerificationRejected,
            MetricPaymentConfirmationOutcome::Conflict,
            MetricPaymentConfirmationOutcome::BindingConflict,
            MetricPaymentConfirmationOutcome::OutcomeUnknown,
            MetricPaymentConfirmationOutcome::Unavailable,
            MetricPaymentConfirmationOutcome::Invariant,
            MetricPaymentConfirmationOutcome::InvalidRequest,
        ] {
            record_payment_confirmation(outcome);
        }
    });
    let rendered = handle.render();
    let lines = rendered
        .lines()
        .filter(|line| line.starts_with(&format!("{PAYMENT_WEBHOOK_CONFIRMATIONS_TOTAL}{{")))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 13);
    assert!(lines.iter().all(|line| line.contains("outcome=\"")));
    assert!(!rendered.contains("provider_event_id"));
}

#[test]
fn organization_approval_timeout_metric_accepts_only_fixed_outcomes() {
    let (recorder, handle) = build_metrics_recorder();
    ::metrics::with_local_recorder(&recorder, || {
        describe_metric_families();
        for outcome in [
            MetricOrganizationApprovalTimeoutOutcome::LeaseHeld,
            MetricOrganizationApprovalTimeoutOutcome::ScanFailed,
            MetricOrganizationApprovalTimeoutOutcome::Applied,
            MetricOrganizationApprovalTimeoutOutcome::Existing,
            MetricOrganizationApprovalTimeoutOutcome::Skipped,
            MetricOrganizationApprovalTimeoutOutcome::Failed,
            MetricOrganizationApprovalTimeoutOutcome::OutcomeUnknownReplay,
            MetricOrganizationApprovalTimeoutOutcome::Truncated,
        ] {
            record_organization_approval_timeout(outcome, 2);
        }
        record_organization_approval_timeout(MetricOrganizationApprovalTimeoutOutcome::Applied, 0);
    });
    let rendered = handle.render();
    let lines = rendered
        .lines()
        .filter(|line| line.starts_with(&format!("{ORGANIZATION_APPROVAL_TIMEOUT_TOTAL}{{")))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 8);
    assert!(lines.iter().all(|line| line.contains("outcome=\"")));
    assert!(lines.iter().all(|line| line.ends_with(" 2")));
}

#[test]
fn local_recorder_renders_fixed_metric_contract() {
    let (recorder, handle) = build_metrics_recorder();
    ::metrics::with_local_recorder(&recorder, || {
        describe_metric_families();
        emit_sample_metrics(&handle);
    });
    handle.run_upkeep();
    handle.run_upkeep();
    let rendered = handle.render();

    assert_eq!(PROMETHEUS_PATH, "/metrics");
    assert_eq!(
        PROMETHEUS_CONTENT_TYPE,
        "text/plain; version=0.0.4; charset=utf-8"
    );
    for (name, kind, description) in [
        (REQUESTS_TOTAL, "counter", "AnyFlows 请求总数。"),
        (
            REQUEST_DURATION_SECONDS,
            "histogram",
            "AnyFlows 请求总耗时。",
        ),
        (FIRST_TOKEN_SECONDS, "histogram", "AnyFlows 流式首字耗时。"),
        (CHANNEL_HEALTH, "gauge", "AnyFlows 渠道健康状态。"),
        (
            UPSTREAM_ERRORS_TOTAL,
            "counter",
            "AnyFlows 分类后的上游错误总数。",
        ),
        (
            QUOTA_CONSUMED_TOTAL,
            "counter",
            "AnyFlows 已结算的整数 quota 消耗。",
        ),
        (CONCURRENCY_SLOTS, "gauge", "AnyFlows 各级并发槽位占用量。"),
        (BILLING_FLUSH_LAG, "gauge", "AnyFlows 批量计费落盘滞后。"),
        (
            CLIENT_SIMULATION_ATTEMPTS_TOTAL,
            "counter",
            "AnyFlows 客户端仿真 Attempt 总数。",
        ),
        (
            ANALYTICS_EXPORT_BACKLOG,
            "gauge",
            "AnyFlows ClickHouse 事实投递积压数量。",
        ),
        (
            REQUEST_RATE_LIMIT_ADMISSIONS_TOTAL,
            "counter",
            "AnyFlows 有限请求速率策略成功准入总数。",
        ),
        (
            REQUEST_RATE_LIMIT_REJECTIONS_TOTAL,
            "counter",
            "AnyFlows 请求速率限制拒绝总数。",
        ),
        (
            REQUEST_RATE_LIMIT_FAILURES_TOTAL,
            "counter",
            "AnyFlows 请求速率限制失败关闭总数。",
        ),
        (
            REQUEST_RATE_LIMIT_RULES_PER_CHECK,
            "histogram",
            "AnyFlows 单次请求速率限制检查的规则数量。",
        ),
        (
            REQUEST_RATE_LIMIT_RETRY_AFTER_SECONDS,
            "histogram",
            "AnyFlows 请求速率限制拒绝后的等待时间。",
        ),
        (
            ORGANIZATION_APPROVAL_TIMEOUT_TOTAL,
            "counter",
            "AnyFlows 企业审批超时扫描结果总数。",
        ),
    ] {
        assert!(
            rendered.contains(&format!("# HELP {name} {description}")),
            "缺少 HELP 描述: {name}\n{rendered}"
        );
        assert!(
            rendered.contains(&format!("# TYPE {name} {kind}")),
            "指标类型错误: {name}\n{rendered}"
        );
    }

    let requests = metric_line(&rendered, "requests_total{");
    assert!(requests.contains("protocol=\"openai\""));
    assert!(requests.contains("model=\"gpt-test\""));
    assert!(requests.contains("status=\"success\""));
    assert!(requests.ends_with(" 2"));
    assert!(rendered.contains("request_duration_seconds_bucket{"));
    assert!(rendered.contains("le=\"0.005\""));
    assert!(rendered.contains("le=\"900\""));
    assert!(rendered.contains("first_token_seconds_bucket{"));
    assert!(rendered.contains("le=\"0.01\""));
    assert!(rendered.contains("le=\"60\""));
    assert!(metric_line(&rendered, "channel_health{").ends_with(" 1"));
    assert!(metric_line(&rendered, "upstream_errors_total{").ends_with(" 3"));
    assert!(metric_line(&rendered, "quota_consumed_total{").ends_with(" 42"));
    assert!(metric_line(&rendered, "concurrency_slots{").ends_with(" 7"));
    assert_eq!(
        metric_line(&rendered, "billing_flush_lag "),
        "billing_flush_lag 1.5"
    );
    let simulation = metric_line(&rendered, "client_simulation_attempts_total{");
    assert!(simulation.contains("profile=\"anthropic_cli_headers_v1\""));
    assert!(simulation.contains("result=\"applied\""));
    assert!(simulation.ends_with(" 1"));
    let body_patch = metric_line(&rendered, "client_simulation_body_patches_total{");
    assert!(body_patch.contains("profile=\"anthropic_cli_system_date_v1\""));
    assert!(body_patch.contains("result=\"applied\""));
    assert!(body_patch.ends_with(" 1"));
    assert_eq!(
        metric_line(&rendered, "analytics_export_backlog "),
        "analytics_export_backlog 5"
    );
    assert_eq!(
        metric_line(&rendered, "request_rate_limit_admissions_total "),
        "request_rate_limit_admissions_total 1"
    );
    let rejection = metric_line(&rendered, "request_rate_limit_rejections_total{");
    assert!(rejection.contains("subject=\"group\""));
    assert!(rejection.ends_with(" 1"));
    let failure = metric_line(&rendered, "request_rate_limit_failures_total{");
    assert!(failure.contains("reason=\"store_protocol\""));
    assert!(failure.ends_with(" 1"));
    assert!(rendered.contains("request_rate_limit_rules_per_check_bucket{"));
    assert!(rendered.contains("request_rate_limit_rules_per_check_bucket{le=\"8\"} 1"));
    assert!(rendered.contains("request_rate_limit_retry_after_seconds_bucket{"));
    assert!(rendered.contains("subject=\"group\""));
    assert!(rendered.contains("le=\"604800\""));
    let approval_timeout = metric_line(&rendered, "organization_approval_timeout_total{");
    assert!(approval_timeout.contains("outcome=\"applied\""));
    assert!(approval_timeout.ends_with(" 1"));
    let cloned_render = handle.clone().render();
    assert_eq!(sorted_lines(&rendered), sorted_lines(&cloned_render));
}

#[test]
fn recorder_rejects_unknown_families_labels_values_and_descriptions() {
    const SECRET_CANARY: &str = "metrics-secret-canary";
    let (recorder, handle) = build_metrics_recorder();
    handle.register_model("model-allowed").unwrap();
    ::metrics::with_local_recorder(&recorder, || {
        ::metrics::describe_counter!(REQUESTS_TOTAL, SECRET_CANARY);
        describe_metric_families();
    });

    recorder.record_counter_from_trusted_test_callsite(
        test_key("unknown_metric", &[("api_key", SECRET_CANARY)]),
        1,
    );
    recorder.record_counter_from_trusted_test_callsite(
        test_key(
            REQUESTS_TOTAL,
            &[
                ("protocol", "openai"),
                ("model", SECRET_CANARY),
                ("status", "success"),
            ],
        ),
        1,
    );
    recorder.record_counter_from_trusted_test_callsite(
        test_key(
            REQUESTS_TOTAL,
            &[
                ("protocol", "openai"),
                ("model", "other"),
                ("status", "success"),
                ("request_id", SECRET_CANARY),
            ],
        ),
        1,
    );
    recorder.record_gauge_from_trusted_test_callsite(
        test_key(
            REQUESTS_TOTAL,
            &[
                ("protocol", "openai"),
                ("model", "other"),
                ("status", "success"),
            ],
        ),
        1.0,
    );
    recorder.record_counter_from_trusted_test_callsite(
        test_key(
            REQUESTS_TOTAL,
            &[
                ("protocol", "future_protocol"),
                ("model", "other"),
                ("status", "success"),
            ],
        ),
        1,
    );
    recorder.record_counter_from_trusted_test_callsite(
        test_key(
            REQUESTS_TOTAL,
            &[
                ("protocol", "openai"),
                ("model", "other"),
                ("model", "other"),
            ],
        ),
        1,
    );
    recorder.record_counter_from_trusted_test_callsite(
        test_key(
            REQUESTS_TOTAL,
            &[("protocol", "openai"), ("model", "other")],
        ),
        1,
    );

    // 同一合法序列用不同标签顺序写入，exporter 前必须归一为唯一序列。
    recorder.record_counter_from_trusted_test_callsite(
        test_key(
            REQUESTS_TOTAL,
            &[
                ("status", "success"),
                ("model", "model-allowed"),
                ("protocol", "openai"),
            ],
        ),
        1,
    );
    recorder.record_counter_from_trusted_test_callsite(
        test_key(
            REQUESTS_TOTAL,
            &[
                ("protocol", "openai"),
                ("model", "model-allowed"),
                ("status", "success"),
            ],
        ),
        1,
    );
    let rendered = handle.render();

    assert!(rendered.contains("# HELP requests_total AnyFlows 请求总数。"));
    assert!(!rendered.contains("unknown_metric"));
    assert!(!rendered.contains("request_id"));
    assert!(!rendered.contains(SECRET_CANARY));
    let request_lines = rendered
        .lines()
        .filter(|line| line.starts_with("requests_total{"))
        .collect::<Vec<_>>();
    assert_eq!(request_lines.len(), 1);
    assert!(request_lines[0].ends_with(" 2"));
}

#[test]
fn all_fixed_metric_enum_variants_render_canonical_labels() {
    let (recorder, handle) = build_metrics_recorder();
    let model = handle.register_model("enum-model").unwrap();
    let channel = handle.register_channel("enum-channel").unwrap();
    let group = handle.register_group("enum-group").unwrap();

    ::metrics::with_local_recorder(&recorder, || {
        describe_metric_families();
        for protocol in [
            MetricProtocol::OpenAi,
            MetricProtocol::Anthropic,
            MetricProtocol::Gemini,
            MetricProtocol::Bedrock,
            MetricProtocol::Other,
        ] {
            handle.record_request(
                protocol,
                &model,
                MetricRequestStatus::Success,
                Duration::from_millis(1),
            );
            handle.record_first_token(protocol, &model, Duration::from_millis(1));
        }
        for status in [
            MetricRequestStatus::Success,
            MetricRequestStatus::ClientError,
            MetricRequestStatus::UpstreamError,
            MetricRequestStatus::InternalError,
            MetricRequestStatus::Cancelled,
        ] {
            handle.record_request(
                MetricProtocol::OpenAi,
                &model,
                status,
                Duration::from_millis(1),
            );
        }
        for error_type in [
            MetricUpstreamErrorType::Timeout,
            MetricUpstreamErrorType::Authentication,
            MetricUpstreamErrorType::RateLimited,
            MetricUpstreamErrorType::InvalidRequest,
            MetricUpstreamErrorType::Unavailable,
            MetricUpstreamErrorType::Protocol,
            MetricUpstreamErrorType::Other,
        ] {
            handle.record_upstream_error(&channel, error_type);
        }
        for level in [
            MetricConcurrencyLevel::Account,
            MetricConcurrencyLevel::User,
            MetricConcurrencyLevel::Token,
        ] {
            handle.set_concurrency_slots(level, 1);
        }
        for result in [
            MetricClientSimulationResult::NotApplied,
            MetricClientSimulationResult::Applied,
            MetricClientSimulationResult::Failed,
        ] {
            record_client_simulation_attempt(
                MetricClientSimulationProfile::AnthropicCliHeadersV1,
                result,
            );
        }
        for result in [
            MetricClientSimulationBodyResult::Applied,
            MetricClientSimulationBodyResult::Rejected,
        ] {
            record_client_simulation_body_patch(
                MetricClientSimulationBodyProfile::AnthropicCliSystemDateV1,
                result,
            );
        }
        handle.add_quota_consumed(&group, 1);
    });

    let rendered = handle.render();
    for value in [
        "openai",
        "anthropic",
        "gemini",
        "bedrock",
        "other",
        "success",
        "client_error",
        "upstream_error",
        "internal_error",
        "cancelled",
        "timeout",
        "authentication",
        "rate_limited",
        "invalid_request",
        "unavailable",
        "protocol",
        "account",
        "user",
        "token",
        "not_applied",
        "applied",
        "failed",
        "rejected",
    ] {
        assert!(
            rendered.contains(&format!("=\"{value}\"")),
            "缺少固定标签值: {value}"
        );
    }
}

#[test]
fn categorical_label_values_are_rejected_at_the_recorder_boundary() {
    const INVALID: &str = "future-value";
    let (recorder, handle) = build_metrics_recorder();
    let model = handle.register_model("known-model").unwrap();
    let _channel = handle.register_channel("known-channel").unwrap();
    let group = handle.register_group("known-group").unwrap();

    let invalid_cases = [
        test_key(
            REQUESTS_TOTAL,
            &[
                ("protocol", INVALID),
                ("model", "known-model"),
                ("status", "success"),
            ],
        ),
        test_key(
            REQUESTS_TOTAL,
            &[
                ("protocol", "openai"),
                ("model", "known-model"),
                ("status", INVALID),
            ],
        ),
        test_key(
            UPSTREAM_ERRORS_TOTAL,
            &[("channel", "known-channel"), ("type", INVALID)],
        ),
        test_key(CONCURRENCY_SLOTS, &[("level", INVALID)]),
        test_key(
            CLIENT_SIMULATION_ATTEMPTS_TOTAL,
            &[("profile", INVALID), ("result", "applied")],
        ),
        test_key(
            CLIENT_SIMULATION_ATTEMPTS_TOTAL,
            &[("profile", "anthropic_cli_headers_v1"), ("result", INVALID)],
        ),
        test_key(
            CLIENT_SIMULATION_BODY_PATCHES_TOTAL,
            &[("profile", INVALID), ("result", "applied")],
        ),
        test_key(
            CLIENT_SIMULATION_BODY_PATCHES_TOTAL,
            &[
                ("profile", "anthropic_cli_system_date_v1"),
                ("result", INVALID),
            ],
        ),
        test_key(REQUEST_RATE_LIMIT_REJECTIONS_TOTAL, &[("subject", INVALID)]),
        test_key(REQUEST_RATE_LIMIT_FAILURES_TOTAL, &[("reason", INVALID)]),
        test_key(PAYMENT_WEBHOOK_CONFIRMATIONS_TOTAL, &[("outcome", INVALID)]),
        test_key(QUOTA_CONSUMED_TOTAL, &[("group", "unknown-group")]),
    ];

    ::metrics::with_local_recorder(&recorder, || {
        describe_metric_families();
        for key in invalid_cases {
            recorder.record_counter_from_trusted_test_callsite(key, 1);
        }
        recorder.record_gauge_from_trusted_test_callsite(
            test_key(CHANNEL_HEALTH, &[("channel", "known-channel")]),
            1.0,
        );
        recorder.record_gauge_from_trusted_test_callsite(
            test_key(CHANNEL_HEALTH, &[("channel", "unknown-channel")]),
            1.0,
        );
    });

    let rendered = handle.render();
    assert!(!rendered.contains(INVALID));
    assert!(!rendered.contains("unknown-group"));
    assert!(!rendered.contains("unknown-channel"));
    assert!(rendered.contains("channel=\"known-channel\""));
    drop(model);
    drop(group);
}

#[test]
fn known_metric_descriptions_units_and_kinds_must_match_contract() {
    let (recorder, handle) = build_metrics_recorder();
    ::metrics::with_local_recorder(&recorder, || {
        ::metrics::describe_gauge!(REQUESTS_TOTAL, ::metrics::Unit::Count, "wrong kind");
        ::metrics::describe_counter!(
            REQUESTS_TOTAL,
            ::metrics::Unit::Seconds,
            "AnyFlows 请求总数。"
        );
        ::metrics::describe_counter!(REQUESTS_TOTAL, ::metrics::Unit::Count, "wrong description");
    });

    let rendered = handle.render();
    assert!(!rendered.contains("# HELP requests_total"));
}

#[test]
fn rate_limit_metrics_accept_only_fixed_classifications() {
    const SECRET_CANARY: &str = "rate-limit-subject-secret";
    let (recorder, handle) = build_metrics_recorder();
    ::metrics::with_local_recorder(&recorder, || {
        describe_metric_families();
        for subject in [
            MetricRequestRateLimitSubject::User,
            MetricRequestRateLimitSubject::Group,
            MetricRequestRateLimitSubject::Token,
        ] {
            record_request_rate_limit_rejection(subject, Duration::from_secs(1));
        }
        for failure in [
            MetricRequestRateLimitFailure::InvalidRule,
            MetricRequestRateLimitFailure::StoreMissing,
            MetricRequestRateLimitFailure::StoreUnavailable,
            MetricRequestRateLimitFailure::StoreProtocol,
            MetricRequestRateLimitFailure::StoreOther,
            MetricRequestRateLimitFailure::InvalidRetryAfter,
        ] {
            record_request_rate_limit_failure(failure);
        }
    });
    recorder.record_counter_from_trusted_test_callsite(
        test_key(
            REQUEST_RATE_LIMIT_REJECTIONS_TOTAL,
            &[("subject", SECRET_CANARY)],
        ),
        1,
    );
    handle.run_upkeep();
    let rendered = handle.render();

    assert_eq!(
        rendered
            .lines()
            .filter(|line| line.starts_with("request_rate_limit_rejections_total{"))
            .count(),
        3
    );
    assert_eq!(
        rendered
            .lines()
            .filter(|line| line.starts_with("request_rate_limit_failures_total{"))
            .count(),
        6
    );
    assert!(!rendered.contains(SECRET_CANARY));
}

#[test]
fn dynamic_labels_are_bounded_and_unregistered_values_create_no_series() {
    let (recorder, handle) = build_metrics_recorder();
    for index in 0..100_000 {
        recorder.record_counter_from_trusted_test_callsite(
            test_key(
                REQUESTS_TOTAL,
                &[
                    ("protocol", "openai"),
                    ("model", &format!("untrusted-{index}")),
                    ("status", "success"),
                ],
            ),
            1,
        );
    }
    assert!(!handle.render().contains("requests_total{"));

    let accepted = (0..100_000)
        .filter(|index| handle.register_model(&format!("model-{index}")).is_ok())
        .count();
    assert_eq!(accepted, MAX_MODEL_LABELS - 1);
    assert_eq!(
        handle.register_model("capacity-overflow"),
        Err(MetricLabelError::CapacityExceeded)
    );

    let accepted_channels = (0..MAX_CHANNEL_LABELS)
        .filter(|index| handle.register_channel(&format!("channel-{index}")).is_ok())
        .count();
    assert_eq!(accepted_channels, MAX_CHANNEL_LABELS - 1);
    assert!(handle.register_channel("channel-0").is_ok());
    assert_eq!(
        handle.register_channel("channel-capacity-overflow"),
        Err(MetricLabelError::CapacityExceeded)
    );

    let accepted_groups = (0..MAX_GROUP_LABELS)
        .filter(|index| handle.register_group(&format!("group-{index}")).is_ok())
        .count();
    assert_eq!(accepted_groups, MAX_GROUP_LABELS - 1);
    assert!(handle.register_group("group-0").is_ok());
    assert_eq!(
        handle.register_group("group-capacity-overflow"),
        Err(MetricLabelError::CapacityExceeded)
    );

    for invalid in ["", "has space", "line\nbreak", "a".repeat(65).as_str()] {
        assert_eq!(
            handle.register_channel(invalid),
            Err(MetricLabelError::Invalid)
        );
    }
}

#[test]
fn concurrent_series_admission_stops_at_family_limit_and_keeps_existing_series() {
    const STATUSES: [&str; 5] = [
        "success",
        "client_error",
        "upstream_error",
        "internal_error",
        "cancelled",
    ];
    let (recorder, handle) = build_metrics_recorder();
    ::metrics::with_local_recorder(&recorder, describe_metric_families);

    let mut models = (0..MAX_MODEL_LABELS - 1)
        .map(|index| {
            let model = format!("model-{index}");
            handle.register_model(&model).unwrap();
            model
        })
        .collect::<Vec<_>>();
    models.push("other".to_owned());

    let known_key = || {
        test_key(
            REQUESTS_TOTAL,
            &[
                ("protocol", "openai"),
                ("model", "model-0"),
                ("status", "success"),
            ],
        )
    };
    recorder.record_counter_from_trusted_test_callsite(known_key(), 1);

    // 多线程提交超过上限的合法组合，验证容量判定和插入保持原子。
    thread::scope(|scope| {
        for protocol in ["openai", "anthropic", "gemini", "bedrock"] {
            let recorder = &recorder;
            let models = &models;
            scope.spawn(move || {
                for status in STATUSES {
                    for model in models {
                        if protocol == "openai" && status == "success" && model == "model-0" {
                            continue;
                        }
                        recorder.record_counter_from_trusted_test_callsite(
                            test_key(
                                REQUESTS_TOTAL,
                                &[("protocol", protocol), ("model", model), ("status", status)],
                            ),
                            1,
                        );
                    }
                }
            });
        }
    });

    recorder.record_counter_from_trusted_test_callsite(known_key(), 1);
    recorder.record_counter_from_trusted_test_callsite(
        test_key(
            REQUESTS_TOTAL,
            &[
                ("protocol", "other"),
                ("model", "model-0"),
                ("status", "success"),
            ],
        ),
        1,
    );
    let rendered = handle.render();
    let request_lines = rendered
        .lines()
        .filter(|line| line.starts_with("requests_total{"))
        .collect::<Vec<_>>();

    assert_eq!(request_lines.len(), MAX_SERIES_PER_FAMILY);
    assert!(request_lines.iter().any(|line| {
        line.contains("protocol=\"openai\"")
            && line.contains("model=\"model-0\"")
            && line.contains("status=\"success\"")
            && line.ends_with(" 2")
    }));
    assert!(
        request_lines
            .iter()
            .all(|line| !line.contains("protocol=\"other\""))
    );
}

#[test]
fn handles_and_registered_labels_have_opaque_debug_output() {
    const DEBUG_CANARY: &str = "debug-secret-canary";
    let (recorder, handle) = build_metrics_recorder();
    let model = handle.register_model(DEBUG_CANARY).unwrap();
    let channel = handle.register_channel(DEBUG_CANARY).unwrap();
    let group = handle.register_group(DEBUG_CANARY).unwrap();
    ::metrics::with_local_recorder(&recorder, || {
        handle.record_request(
            MetricProtocol::OpenAi,
            &model,
            MetricRequestStatus::Success,
            Duration::from_millis(10),
        );
        handle.set_channel_health(&channel, true);
        handle.add_quota_consumed(&group, 1);
    });
    assert!(handle.render().contains(DEBUG_CANARY));
    let debug = format!("{handle:?}\n{model:?}\n{channel:?}\n{group:?}");

    assert!(!debug.contains(DEBUG_CANARY));
    assert!(debug.contains("MetricsHandle"));
    assert!(debug.contains("ModelLabel"));
    assert!(debug.contains("ChannelLabel"));
    assert!(debug.contains("GroupLabel"));
}

#[test]
fn labels_registered_by_another_handle_are_rejected() {
    let (_, first_handle) = build_metrics_recorder();
    let foreign_model = first_handle.register_model("foreign-model").unwrap();
    let (recorder, second_handle) = build_metrics_recorder();
    ::metrics::with_local_recorder(&recorder, || {
        second_handle.record_request(
            MetricProtocol::OpenAi,
            &foreign_model,
            MetricRequestStatus::Success,
            Duration::from_millis(10),
        );
    });

    assert!(!second_handle.render().contains("requests_total{"));
}

#[test]
fn repeated_global_metrics_initialization_is_typed_in_child_process() {
    if env::var_os(GLOBAL_METRICS_INIT_CHILD).is_some() {
        let first = init_metrics().unwrap();
        emit_sample_metrics(&first);
        let error = init_metrics().unwrap_err();
        let post_conflict_model = first.register_model("post-conflict-model").unwrap();
        first.record_request(
            MetricProtocol::OpenAi,
            &post_conflict_model,
            MetricRequestStatus::Success,
            Duration::from_millis(5),
        );

        assert_eq!(error, TelemetryError::MetricsInitializationConflict);
        assert_eq!(error.to_string(), "Prometheus 指标已初始化");
        assert!(first.render().contains("requests_total"));
        assert!(first.render().contains("post-conflict-model"));
        return;
    }

    let output = Command::new(env::current_exe().unwrap())
        .arg("--exact")
        .arg("metrics_tests::repeated_global_metrics_initialization_is_typed_in_child_process")
        .arg("--nocapture")
        .env_clear()
        .env(GLOBAL_METRICS_INIT_CHILD, "1")
        .output()
        .expect("必须能启动隔离 metrics 初始化测试进程");
    assert!(
        output.status.success(),
        "隔离 metrics 测试失败: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
