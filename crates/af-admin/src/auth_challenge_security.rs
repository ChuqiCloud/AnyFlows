use std::fmt;

use af_domain::TrustedClientIp;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

/// 认证挑战派生密钥的固定字节数。
pub(crate) const AUTH_CHALLENGE_SECURITY_KEY_BYTES: usize = 32;

const SHA256_BLOCK_BYTES: usize = 64;

/// 认证挑战安全派生密钥的配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum AuthChallengeSecurityKeyError {
    /// 未提供应用级派生密钥。
    #[error("认证挑战安全派生密钥缺失")]
    Missing,
    /// 派生密钥不是 32 字节 Base64URL 无填充文本。
    #[error("认证挑战安全派生密钥无效")]
    Invalid,
}

/// 使用带密钥、带域分离的 SHA-256 派生认证挑战摘要。
pub(crate) struct AuthChallengeSecurityKey(Zeroizing<[u8; AUTH_CHALLENGE_SECURITY_KEY_BYTES]>);

impl AuthChallengeSecurityKey {
    /// 解析启动配置中的 Base64URL 密钥，并在内存中以可清零形式保存。
    pub(crate) fn new(value: Option<&str>) -> Result<Self, AuthChallengeSecurityKeyError> {
        let value = value.ok_or(AuthChallengeSecurityKeyError::Missing)?;
        let mut decoded = [0_u8; AUTH_CHALLENGE_SECURITY_KEY_BYTES];
        let length = URL_SAFE_NO_PAD
            .decode_slice(value, &mut decoded)
            .map_err(|_| AuthChallengeSecurityKeyError::Invalid)?;
        if length != AUTH_CHALLENGE_SECURITY_KEY_BYTES {
            decoded.zeroize();
            return Err(AuthChallengeSecurityKeyError::Invalid);
        }
        Ok(Self(Zeroizing::new(decoded)))
    }

    /// 为客户端 IP 派生摘要，统一 IPv4-mapped IPv6 的表示。
    pub(crate) fn derive_client(&self, domain: &[u8], client_ip: TrustedClientIp) -> [u8; 32] {
        let mut address = [0_u8; 17];
        let address_length = client_ip.write_fingerprint_input(&mut address);
        let digest = self.derive(domain, &[&address[..address_length]]);
        address.zeroize();
        digest
    }

    /// 对多个长度前缀输入执行带域分离的 HMAC-SHA256 派生。
    pub(crate) fn derive(&self, domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
        let mut inner_pad = [0x36_u8; SHA256_BLOCK_BYTES];
        let mut outer_pad = [0x5c_u8; SHA256_BLOCK_BYTES];
        for (index, key_byte) in self.0.iter().copied().enumerate() {
            inner_pad[index] ^= key_byte;
            outer_pad[index] ^= key_byte;
        }

        let mut inner = Sha256::new();
        inner.update(inner_pad);
        inner.update(domain);
        for part in parts {
            inner.update(u64::try_from(part.len()).unwrap_or(u64::MAX).to_be_bytes());
            inner.update(part);
        }
        let inner_digest = inner.finalize();

        let mut outer = Sha256::new();
        outer.update(outer_pad);
        outer.update(inner_digest);
        let digest = outer.finalize().into();
        inner_pad.zeroize();
        outer_pad.zeroize();
        digest
    }
}

impl fmt::Debug for AuthChallengeSecurityKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthChallengeSecurityKey(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::*;

    fn key() -> String {
        URL_SAFE_NO_PAD.encode([0x42; AUTH_CHALLENGE_SECURITY_KEY_BYTES])
    }

    #[test]
    fn keyed_derivation_is_domain_separated_and_ip_normalized() {
        let first = AuthChallengeSecurityKey::new(Some(&key())).unwrap();
        let second_key = URL_SAFE_NO_PAD.encode([0x24; AUTH_CHALLENGE_SECURITY_KEY_BYTES]);
        let second = AuthChallengeSecurityKey::new(Some(&second_key)).unwrap();
        let ipv4 = TrustedClientIp::new("192.0.2.9".parse::<IpAddr>().unwrap());
        let mapped = TrustedClientIp::new("::ffff:192.0.2.9".parse::<IpAddr>().unwrap());

        assert_eq!(
            first.derive_client(b"client", ipv4),
            first.derive_client(b"client", mapped)
        );
        assert_ne!(
            first.derive_client(b"client", ipv4),
            first.derive_client(b"ip", ipv4)
        );
        assert_ne!(
            first.derive(b"subject", &[b"user@example.com"]),
            second.derive(b"subject", &[b"user@example.com"])
        );
        assert_eq!(format!("{first:?}"), "AuthChallengeSecurityKey(<redacted>)");
    }
}
