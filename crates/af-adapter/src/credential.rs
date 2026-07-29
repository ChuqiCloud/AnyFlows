use std::fmt;

use af_domain::CredentialKind;
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};

use crate::{AdaptorError, AdaptorResult};

/// 解密后交给适配器的凭据最大长度；持久化密文边界不由本类型承担。
pub const MAX_CREDENTIAL_SECRET_BYTES: usize = 16 * 1_024;

/// 清除可能由上一层或前一次认证方案留下的凭据头。
///
/// 适配器必须先清理全部已知认证载体，再写入当前渠道唯一允许的认证头，避免
/// API Key、OAuth 或其他供应商凭据在故障切换时交叉泄露。
pub(crate) fn clear_authentication_headers(headers: &mut HeaderMap) {
    for name in [
        HeaderName::from_static("authorization"),
        HeaderName::from_static("api-key"),
        HeaderName::from_static("x-api-key"),
        HeaderName::from_static("x-goog-api-key"),
        HeaderName::from_static("x-auth-token"),
        HeaderName::from_static("x-access-token"),
        HeaderName::from_static("x-client-secret"),
        HeaderName::from_static("x-amz-security-token"),
    ] {
        headers.remove(name);
    }
}

/// 按供应商指定的 API Key 头名构造互斥的 API Key 或 OAuth Bearer 认证头。
///
/// 返回值始终标记为敏感；调用方仍须先清理旧认证载体，再写入该唯一认证头。
pub(crate) fn api_key_or_bearer_header(
    credential: &Credential,
    api_key_header: HeaderName,
) -> AdaptorResult<(HeaderName, HeaderValue)> {
    let (name, value) = match credential.kind() {
        CredentialKind::ApiKey => (api_key_header, credential.expose_secret().to_owned()),
        CredentialKind::Oauth => (
            HeaderName::from_static("authorization"),
            format!("Bearer {}", credential.expose_secret()),
        ),
        kind => return Err(AdaptorError::UnsupportedCredential { kind }),
    };
    let mut value = HeaderValue::from_str(&value).map_err(|_| AdaptorError::InvalidHeader)?;
    value.set_sensitive(true);
    Ok((name, value))
}

/// 已解密但默认脱敏的凭据视图。
///
/// M0 只放行 API Key 与 OAuth access token。结构化的 Bedrock、Service Account
/// 等凭据会在对应适配器切片中增加专用字段，禁止先用不透明字符串占位。
/// 本类型不负责读取数据库；凭据解密与租户边界由上层账号模块完成。
#[derive(Clone, Eq, PartialEq)]
pub struct Credential {
    value: CredentialValue,
}

#[derive(Clone, Eq, PartialEq)]
enum CredentialValue {
    ApiKey(SecretString),
    Oauth(SecretString),
}

/// 不允许通过格式化输出秘密内容的字符串封装。
#[derive(Clone, Eq, PartialEq)]
struct SecretString(String);

impl SecretString {
    fn new(value: String) -> AdaptorResult<Self> {
        if value.is_empty()
            || value.len() > MAX_CREDENTIAL_SECRET_BYTES
            || value.trim() != value
            || !value.is_ascii()
            || value.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(AdaptorError::InvalidCredential);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<已脱敏>")
    }
}

impl Credential {
    /// 构造 API Key 凭据。
    pub fn api_key(value: impl Into<String>) -> AdaptorResult<Self> {
        Self::from_value(CredentialValue::ApiKey, value.into())
    }

    /// 构造 OAuth access token 凭据。
    pub fn oauth(value: impl Into<String>) -> AdaptorResult<Self> {
        Self::from_value(CredentialValue::Oauth, value.into())
    }

    fn from_value<F>(build: F, value: String) -> AdaptorResult<Self>
    where
        F: FnOnce(SecretString) -> CredentialValue,
    {
        Ok(Self {
            value: build(SecretString::new(value)?),
        })
    }

    /// 返回凭据的稳定种类。
    #[must_use]
    pub const fn kind(&self) -> CredentialKind {
        match &self.value {
            CredentialValue::ApiKey(_) => CredentialKind::ApiKey,
            CredentialValue::Oauth(_) => CredentialKind::Oauth,
        }
    }

    /// 返回秘密载荷；调用方只应在构造认证头或签名时短暂使用。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        match &self.value {
            CredentialValue::ApiKey(secret) | CredentialValue::Oauth(secret) => secret.expose(),
        }
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Credential")
            .field("kind", &self.kind())
            .field("secret", &"<已脱敏>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_keeps_kind_and_redacts_secret() {
        let credential = Credential::api_key("test-secret").unwrap();
        assert_eq!(credential.kind(), CredentialKind::ApiKey);
        assert_eq!(credential.expose_secret(), "test-secret");

        let debug = format!("{credential:?}");
        assert!(debug.contains("<已脱敏>"));
        assert!(!debug.contains("test-secret"));
    }

    #[test]
    fn credential_rejects_empty_or_oversized_secret() {
        assert_eq!(
            Credential::api_key(""),
            Err(AdaptorError::InvalidCredential)
        );
        assert_eq!(
            Credential::api_key("x".repeat(MAX_CREDENTIAL_SECRET_BYTES + 1)),
            Err(AdaptorError::InvalidCredential)
        );
        for invalid in [
            " token",
            "token ",
            "token\nsecret",
            "token\0secret",
            "非 ASCII 令牌",
        ] {
            assert_eq!(
                Credential::oauth(invalid),
                Err(AdaptorError::InvalidCredential)
            );
        }
    }
}
