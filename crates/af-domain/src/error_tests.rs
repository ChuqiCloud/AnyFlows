use std::error::Error as _;

use crate::{
    AfError, NetworkFailureKind, QuotaWindowRetryAfter, RateLimitScope, UpstreamError,
    UpstreamRetryAfter, UpstreamServerStatus,
};

#[test]
fn upstream_server_status_accepts_only_5xx_codes() {
    assert_eq!(UpstreamServerStatus::new(499), None);
    assert_eq!(UpstreamServerStatus::new(600), None);

    for code in [UpstreamServerStatus::MIN, 503, UpstreamServerStatus::MAX] {
        let status = UpstreamServerStatus::new(code).expect("5xx 状态码应通过校验");
        assert_eq!(status.get(), code);
        assert_eq!(u16::from(status), code);
        assert_eq!(status.to_string(), code.to_string());
    }
}

#[test]
fn upstream_retry_after_accepts_only_bounded_nonzero_seconds() {
    assert_eq!(UpstreamRetryAfter::from_seconds(0), None);
    assert_eq!(
        UpstreamRetryAfter::from_seconds(UpstreamRetryAfter::MAX_SECONDS + 1),
        None
    );

    let retry_after = UpstreamRetryAfter::from_seconds(90).unwrap();
    assert_eq!(retry_after.seconds(), 90);
    assert_eq!(retry_after.duration(), std::time::Duration::from_secs(90));
}

#[test]
fn quota_window_retry_after_covers_calendar_month_without_relaxing_upstream_bound() {
    let eight_days = 8 * 24 * 60 * 60;
    assert_eq!(UpstreamRetryAfter::from_seconds(eight_days), None);

    let retry_after = QuotaWindowRetryAfter::from_seconds(eight_days).unwrap();
    assert_eq!(u64::from(retry_after.seconds()), eight_days);
    assert_eq!(
        retry_after.duration(),
        std::time::Duration::from_secs(eight_days)
    );
    assert_eq!(QuotaWindowRetryAfter::from_seconds(0), None);
    assert_eq!(
        QuotaWindowRetryAfter::from_seconds(QuotaWindowRetryAfter::MAX_SECONDS + 1),
        None
    );
}

#[test]
fn upstream_errors_have_fixed_chinese_diagnostics_without_sources() {
    for (error, display, debug) in [
        (
            UpstreamError::rate_limited(RateLimitScope::Window),
            "上游请求受到限流",
            "RateLimited { scope: Window, retry_after: None }",
        ),
        (
            UpstreamError::rate_limited_after(
                RateLimitScope::Credential,
                UpstreamRetryAfter::from_seconds(30).unwrap(),
            ),
            "上游请求受到限流",
            "RateLimited { scope: Credential, retry_after: Some(UpstreamRetryAfter(30)) }",
        ),
        (
            UpstreamError::overloaded(),
            "上游服务过载",
            "Overloaded { retry_after: None }",
        ),
        (UpstreamError::AuthExpired, "上游认证已过期", "AuthExpired"),
        (
            UpstreamError::AuthRevoked,
            "上游认证已被撤销",
            "AuthRevoked",
        ),
        (
            UpstreamError::AccountDisabled,
            "上游账号或组织已被停用",
            "AccountDisabled",
        ),
        (
            UpstreamError::QuotaExhausted,
            "上游额度已耗尽",
            "QuotaExhausted",
        ),
        (
            UpstreamError::ModelUnsupported,
            "上游不支持请求的模型",
            "ModelUnsupported",
        ),
        (
            UpstreamError::ProtocolError,
            "上游协议响应无效",
            "ProtocolError",
        ),
        (
            UpstreamError::ServerError {
                status: UpstreamServerStatus::new(503).unwrap(),
            },
            "上游服务返回服务器错误（HTTP 503）",
            "ServerError { status: UpstreamServerStatus(503) }",
        ),
        (
            UpstreamError::BadRequest,
            "上游拒绝了无效请求",
            "BadRequest",
        ),
        (
            UpstreamError::network(NetworkFailureKind::ReadTimeout),
            "上游网络传输失败",
            "Network { kind: ReadTimeout }",
        ),
    ] {
        assert_eq!(error.to_string(), display);
        assert_eq!(format!("{error:?}"), debug);
        assert!(error.source().is_none());
    }
}

#[test]
fn af_error_wraps_only_the_redacted_upstream_classification() {
    let upstream = UpstreamError::AuthRevoked;
    let error = AfError::from(upstream);

    assert_eq!(error, AfError::Upstream(upstream));
    assert_eq!(error.to_string(), "上游认证已被撤销");
    assert_eq!(format!("{error:?}"), "Upstream(AuthRevoked)");

    let source = error.source().expect("上游领域错误应保留为安全错误源");
    assert_eq!(source.to_string(), "上游认证已被撤销");
    assert!(source.source().is_none());
}

#[test]
fn domain_errors_are_thread_safe_static_errors() {
    fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}

    assert_error::<AfError>();
    assert_error::<UpstreamError>();
    assert_eq!(AfError::InvalidRequest.to_string(), "请求无效");
    assert_eq!(AfError::InvalidApiKey.to_string(), "下游 API Key 无效");
    assert_eq!(
        AfError::ModelNotAllowed.to_string(),
        "下游令牌不允许请求的模型"
    );
    assert_eq!(AfError::InsufficientQuota.to_string(), "下游可用额度不足");
    assert_eq!(
        AfError::QuotaWindowLimited {
            retry_after: QuotaWindowRetryAfter::from_seconds(37).unwrap(),
        }
        .to_string(),
        "下游额度窗口已达上限"
    );
    assert_eq!(AfError::ConcurrencyLimited.to_string(), "并发请求已达上限");
    assert_eq!(AfError::Internal.to_string(), "服务内部错误");
    assert!(AfError::InvalidRequest.source().is_none());
    assert!(AfError::InvalidApiKey.source().is_none());
    assert!(AfError::ModelNotAllowed.source().is_none());
    assert!(AfError::InsufficientQuota.source().is_none());
    assert!(
        AfError::QuotaWindowLimited {
            retry_after: QuotaWindowRetryAfter::from_seconds(37).unwrap(),
        }
        .source()
        .is_none()
    );
    assert!(AfError::ConcurrencyLimited.source().is_none());
    assert!(AfError::Internal.source().is_none());
}
