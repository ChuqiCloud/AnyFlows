use std::{fmt, future::Future, pin::Pin, time::Duration};

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::TrustedClientIp;

/// 公开企业 SSO 启动的客户端与标识组合限流窗口。
pub const ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
/// 单个客户端在窗口内允许的公开启动次数。
pub const ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_PER_IP: u32 = 30;
/// 单个规范化公开标识在窗口内允许的公开启动次数。
pub const ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_PER_IDENTIFIER: u32 = 10;

const IP_CONTEXT: &[u8] = b"anyflows-organization-sso-public-login-ip-v1";
const IDENTIFIER_CONTEXT: &[u8] = b"anyflows-organization-sso-public-login-identifier-v1";

/// 公开启动限流只携带不可逆指纹，不保存原始地址或公开标识。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct OrganizationSsoPublicLoginRateLimitKey {
    client_ip: [u8; 32],
    identifier: [u8; 32],
}

impl OrganizationSsoPublicLoginRateLimitKey {
    /// 从可信客户端地址和已严格规范化的公开标识派生组合限流指纹。
    #[must_use]
    pub fn new(secret: &[u8; 32], client_ip: TrustedClientIp, normalized_identifier: &str) -> Self {
        Self {
            client_ip: digest(secret, IP_CONTEXT, &client_ip_bytes(client_ip)),
            identifier: digest(secret, IDENTIFIER_CONTEXT, normalized_identifier.as_bytes()),
        }
    }

    /// 返回客户端指纹，供 Redis 键构造使用。
    #[must_use]
    pub const fn client_ip_fingerprint(self) -> [u8; 32] {
        self.client_ip
    }

    /// 返回规范化公开标识指纹，供 Redis 键构造使用。
    #[must_use]
    pub const fn identifier_fingerprint(self) -> [u8; 32] {
        self.identifier
    }
}

impl fmt::Debug for OrganizationSsoPublicLoginRateLimitKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoPublicLoginRateLimitKey(<redacted>)")
    }
}

/// 公开启动限流的原子判定。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationSsoPublicLoginRateLimitDecision {
    /// 两个固定窗口均已原子递增。
    Admitted,
    /// 任一窗口已满；调用方不得创建登录事务。
    Limited { retry_after_seconds: u32 },
    /// 无法安全判断共享状态，必须失败关闭。
    Rejected,
}

/// 公开启动限流存储的异步调用约定。
pub type OrganizationSsoPublicLoginRateLimitFuture<'a> =
    Pin<Box<dyn Future<Output = OrganizationSsoPublicLoginRateLimitDecision> + Send + 'a>>;

/// 以客户端 IP 与规范化公开标识执行原子组合限流。
pub trait OrganizationSsoPublicLoginRateLimitStore: Send + Sync {
    fn admit<'a>(
        &'a self,
        key: OrganizationSsoPublicLoginRateLimitKey,
    ) -> OrganizationSsoPublicLoginRateLimitFuture<'a>;
}

fn client_ip_bytes(client_ip: TrustedClientIp) -> [u8; 17] {
    let mut bytes = [0_u8; 17];
    let length = client_ip.write_fingerprint_input(&mut bytes);
    bytes[length..].fill(0);
    bytes
}

fn digest(secret: &[u8; 32], context: &[u8], value: &[u8]) -> [u8; 32] {
    let mut hasher = Hmac::<Sha256>::new_from_slice(secret).expect("固定长度 HMAC 密钥有效");
    hasher.update(context);
    hasher.update(&[0]);
    hasher.update(value);
    hasher.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;

    #[test]
    fn key_uses_canonical_identifier_without_exposing_input() {
        let client_ip = TrustedClientIp::new(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let secret = [7_u8; 32];
        let first = OrganizationSsoPublicLoginRateLimitKey::new(&secret, client_ip, "example.com");
        let second = OrganizationSsoPublicLoginRateLimitKey::new(&secret, client_ip, "example.com");
        assert_eq!(
            first.identifier_fingerprint(),
            second.identifier_fingerprint()
        );
        let debug = format!("{first:?}");
        assert!(!debug.contains("Example"));
        assert_ne!(first.client_ip_fingerprint(), [0; 32]);
    }
}
