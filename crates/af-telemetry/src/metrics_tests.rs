use std::{env, process::Command, thread, time::Duration};

use ::metrics::{Key, Label};

use crate::{
    BILLING_FLUSH_LAG, CHANNEL_HEALTH, CONCURRENCY_SLOTS, FIRST_TOKEN_SECONDS,
    MetricConcurrencyLevel, MetricLabelError, MetricProtocol, MetricRequestStatus,
    MetricUpstreamErrorType, MetricsHandle, PROMETHEUS_CONTENT_TYPE, PROMETHEUS_PATH,
    QUOTA_CONSUMED_TOTAL, REQUEST_DURATION_SECONDS, REQUESTS_TOTAL, TelemetryError,
    UPSTREAM_ERRORS_TOTAL, init_metrics,
    metrics::{
        MAX_CHANNEL_LABELS, MAX_GROUP_LABELS, MAX_MODEL_LABELS, MAX_SERIES_PER_FAMILY,
        build_metrics_recorder, describe_metric_families,
    },
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
