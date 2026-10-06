use std::fmt;

use af_domain::CredentialKind;
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};
use jsonwebtoken::EncodingKey;
use zeroize::Zeroize;

use crate::{AdaptorError, AdaptorResult};

/// 解密后交给适配器的凭据最大长度；持久化密文边界不由本类型承担。
pub const MAX_CREDENTIAL_SECRET_BYTES: usize = 16 * 1_024;
/// AWS access key ID 的内存边界。
pub const MAX_AWS_ACCESS_KEY_ID_BYTES: usize = 128;
/// AWS secret access key 的内存边界。
pub const MAX_AWS_SECRET_ACCESS_KEY_BYTES: usize = 4 * 1_024;
/// AWS 临时会话令牌的内存边界。
pub const MAX_AWS_SESSION_TOKEN_BYTES: usize = 16 * 1_024;
/// Google Service Account 客户端邮箱的内存边界。
pub const MAX_GOOGLE_SERVICE_ACCOUNT_EMAIL_BYTES: usize = 320;
/// Google Service Account 私钥 ID 的内存边界。
pub const MAX_GOOGLE_PRIVATE_KEY_ID_BYTES: usize = 128;
/// Google Service Account RSA 私钥 PEM 的内存边界。
pub const MAX_GOOGLE_PRIVATE_KEY_BYTES: usize = 16 * 1_024;

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
        HeaderName::from_static("x-amz-content-sha256"),
        HeaderName::from_static("x-amz-date"),
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
/// 当前放行 API Key、OAuth access token、结构化 Bedrock SigV4 凭据与 Google
/// Service Account。复杂凭据必须保留独立字段，禁止用不透明字符串占位。
/// 本类型不负责读取数据库；凭据解密与租户边界由上层账号模块完成。
#[derive(Clone, Eq, PartialEq)]
pub struct Credential {
    value: CredentialValue,
}

#[derive(Clone, Eq, PartialEq)]
enum CredentialValue {
    ApiKey(SecretString),
    Oauth(SecretString),
    Bedrock(BedrockCredential),
    ServiceAccount(ServiceAccountCredential),
}

/// AWS Bedrock SigV4 所需的结构化短期或长期凭据。
#[derive(Clone, Eq, PartialEq)]
struct BedrockCredential {
    access_key_id: SecretString,
    secret_access_key: SecretString,
    session_token: Option<SecretString>,
}

/// Google OAuth 2.0 JWT bearer 流程所需的结构化 Service Account 凭据。
#[derive(Clone, Eq, PartialEq)]
struct ServiceAccountCredential {
    client_email: SecretString,
    private_key_id: Option<SecretString>,
    private_key: SecretString,
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

    fn new_ascii_component(value: String, max_bytes: usize) -> AdaptorResult<Self> {
        if value.is_empty()
            || value.len() > max_bytes
            || !value.is_ascii()
            || value
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
        {
            return Err(AdaptorError::InvalidCredential);
        }
        Ok(Self(value))
    }

    fn new_service_account_private_key(value: String) -> AdaptorResult<Self> {
        if value.is_empty()
            || value.len() > MAX_GOOGLE_PRIVATE_KEY_BYTES
            || !value.is_ascii()
            || value
                .bytes()
                .any(|byte| byte.is_ascii_control() && !matches!(byte, b'\r' | b'\n'))
            || EncodingKey::from_rsa_pem(value.as_bytes()).is_err()
        {
            return Err(AdaptorError::InvalidCredential);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.0.zeroize();
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

    /// 构造 AWS Bedrock SigV4 凭据；会话令牌仅用于 STS 临时凭据。
    pub fn bedrock(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        session_token: Option<String>,
    ) -> AdaptorResult<Self> {
        Ok(Self {
            value: CredentialValue::Bedrock(BedrockCredential {
                access_key_id: SecretString::new_ascii_component(
                    access_key_id.into(),
                    MAX_AWS_ACCESS_KEY_ID_BYTES,
                )?,
                secret_access_key: SecretString::new_ascii_component(
                    secret_access_key.into(),
                    MAX_AWS_SECRET_ACCESS_KEY_BYTES,
                )?,
                session_token: session_token
                    .map(|value| {
                        SecretString::new_ascii_component(value, MAX_AWS_SESSION_TOKEN_BYTES)
                    })
                    .transpose()?,
            }),
        })
    }

    /// 构造 Google Service Account 凭据；token endpoint 固定由 Vertex 适配器提供。
    pub fn service_account(
        client_email: impl Into<String>,
        private_key_id: Option<String>,
        private_key: impl Into<String>,
    ) -> AdaptorResult<Self> {
        let client_email = client_email.into();
        if !is_valid_service_account_email(&client_email) {
            return Err(AdaptorError::InvalidCredential);
        }
        Ok(Self {
            value: CredentialValue::ServiceAccount(ServiceAccountCredential {
                client_email: SecretString::new_ascii_component(
                    client_email,
                    MAX_GOOGLE_SERVICE_ACCOUNT_EMAIL_BYTES,
                )?,
                private_key_id: private_key_id
                    .map(|value| {
                        SecretString::new_ascii_component(value, MAX_GOOGLE_PRIVATE_KEY_ID_BYTES)
                    })
                    .transpose()?,
                private_key: SecretString::new_service_account_private_key(private_key.into())?,
            }),
        })
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
            CredentialValue::Bedrock(_) => CredentialKind::Bedrock,
            CredentialValue::ServiceAccount(_) => CredentialKind::ServiceAccount,
        }
    }

    /// 返回简单令牌或复杂凭据的主秘密字段。
    ///
    /// 调用方只应在构造认证头或签名时短暂使用；Bedrock 与 Service Account
    /// 完整字段必须通过内部结构化访问器读取，不能重新拼成不透明字符串。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        match &self.value {
            CredentialValue::ApiKey(secret) | CredentialValue::Oauth(secret) => secret.expose(),
            CredentialValue::Bedrock(credential) => credential.secret_access_key.expose(),
            CredentialValue::ServiceAccount(credential) => credential.private_key.expose(),
        }
    }

    /// 返回 Bedrock 签名字段；仅适配器内部可短暂读取。
    pub(crate) fn bedrock_parts(&self) -> Option<(&str, &str, Option<&str>)> {
        match &self.value {
            CredentialValue::Bedrock(credential) => Some((
                credential.access_key_id.expose(),
                credential.secret_access_key.expose(),
                credential.session_token.as_ref().map(SecretString::expose),
            )),
            CredentialValue::ApiKey(_)
            | CredentialValue::Oauth(_)
            | CredentialValue::ServiceAccount(_) => None,
        }
    }

    /// 返回 Service Account JWT 签名字段；仅适配器内部可短暂读取。
    pub(crate) fn service_account_parts(&self) -> Option<(&str, Option<&str>, &str)> {
        match &self.value {
            CredentialValue::ServiceAccount(credential) => Some((
                credential.client_email.expose(),
                credential.private_key_id.as_ref().map(SecretString::expose),
                credential.private_key.expose(),
            )),
            CredentialValue::ApiKey(_)
            | CredentialValue::Oauth(_)
            | CredentialValue::Bedrock(_) => None,
        }
    }
}

fn is_valid_service_account_email(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_GOOGLE_SERVICE_ACCOUNT_EMAIL_BYTES
        || value.trim() != value
        || !value.is_ascii()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return false;
    }
    let mut parts = value.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && local.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
        && domain.ends_with(".gserviceaccount.com")
        && domain.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
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

    #[test]
    fn bedrock_credential_preserves_structured_fields_and_redacts_every_secret() {
        let credential = Credential::bedrock(
            "AKIDEXAMPLE00000001",
            "aws-secret-access-key",
            Some("aws-session-token".to_owned()),
        )
        .unwrap();
        assert_eq!(credential.kind(), CredentialKind::Bedrock);
        assert_eq!(
            credential.bedrock_parts(),
            Some((
                "AKIDEXAMPLE00000001",
                "aws-secret-access-key",
                Some("aws-session-token")
            ))
        );

        let debug = format!("{credential:?}");
        for private in [
            "AKIDEXAMPLE00000001",
            "aws-secret-access-key",
            "aws-session-token",
        ] {
            assert!(!debug.contains(private));
        }
    }

    #[test]
    fn bedrock_credential_rejects_missing_oversized_or_whitespace_components() {
        for result in [
            Credential::bedrock("", "secret", None),
            Credential::bedrock("access key", "secret", None),
            Credential::bedrock("access-key", "secret key", None),
            Credential::bedrock("access-key", "secret", Some(String::new())),
            Credential::bedrock("access-key", "secret", Some("session token".to_owned())),
            Credential::bedrock("x".repeat(MAX_AWS_ACCESS_KEY_ID_BYTES + 1), "secret", None),
            Credential::bedrock(
                "access-key",
                "x".repeat(MAX_AWS_SECRET_ACCESS_KEY_BYTES + 1),
                None,
            ),
            Credential::bedrock(
                "access-key",
                "secret",
                Some("x".repeat(MAX_AWS_SESSION_TOKEN_BYTES + 1)),
            ),
        ] {
            assert_eq!(result, Err(AdaptorError::InvalidCredential));
        }
    }
}
