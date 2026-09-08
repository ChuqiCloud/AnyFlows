use std::{future::Future, pin::Pin};

use af_domain::TrustedClientIp;

/// Turnstile 服务端验证的稳定结果，不携带上游错误正文或 token 内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnstileVerification {
    /// 上游确认 token 有效。
    Passed,
    /// 上游确认 token 无效、过期或已被消费。
    Rejected,
    /// 服务端无法可靠完成上游验证。
    Unavailable,
}

/// Turnstile 验证结果的异步返回类型。
pub type TurnstileVerificationFuture<'a> =
    Pin<Box<dyn Future<Output = TurnstileVerification> + Send + 'a>>;

/// 登录与注册入口使用的服务端 Turnstile 验证器。
pub trait TurnstileVerifier: Send + Sync {
    /// 使用已完成可信代理解析的客户端 IP 验证一次性 token。
    fn verify<'a>(
        &'a self,
        token: &'a str,
        client_ip: TrustedClientIp,
    ) -> TurnstileVerificationFuture<'a>;
}
