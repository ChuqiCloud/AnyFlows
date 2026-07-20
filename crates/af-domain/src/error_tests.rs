use std::error::Error as _;

use crate::{AfError, UpstreamError, UpstreamServerStatus};

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
fn upstream_errors_have_fixed_chinese_diagnostics_without_sources() {
    for (error, display, debug) in [
        (
            UpstreamError::RateLimited,
            "上游请求受到限流",
            "RateLimited",
        ),
        (UpstreamError::Overloaded, "上游服务过载", "Overloaded"),
        (UpstreamError::AuthExpired, "上游认证已过期", "AuthExpired"),
        (
            UpstreamError::AuthRevoked,
            "上游认证已被撤销",
            "AuthRevoked",
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
        (UpstreamError::Network, "上游网络传输失败", "Network"),
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
    assert_eq!(AfError::Internal.to_string(), "服务内部错误");
    assert!(AfError::InvalidRequest.source().is_none());
    assert!(AfError::InvalidApiKey.source().is_none());
    assert!(AfError::Internal.source().is_none());
}
