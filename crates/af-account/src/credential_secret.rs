use std::fmt;

use af_domain::CredentialKind;
use jsonwebtoken::EncodingKey;
use zeroize::Zeroizing;

use crate::credential_encryption::PlainOAuthCredential;
use crate::{CredentialEncryptionError, MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES};

/// AWS access key ID 的最大明文字节数。
pub const MAX_AWS_ACCESS_KEY_ID_BYTES: usize = 128;
/// AWS secret access key 的最大明文字节数。
pub const MAX_AWS_SECRET_ACCESS_KEY_BYTES: usize = 4 * 1_024;
/// AWS STS 会话令牌的最大明文字节数。
pub const MAX_AWS_SESSION_TOKEN_BYTES: usize = 16 * 1_024;
/// Google Service Account 客户端邮箱的最大明文字节数。
pub const MAX_GOOGLE_SERVICE_ACCOUNT_EMAIL_BYTES: usize = 320;
/// Google Service Account 私钥 ID 的最大明文字节数。
pub const MAX_GOOGLE_PRIVATE_KEY_ID_BYTES: usize = 128;
/// Google Service Account RSA 私钥 PEM 的最大明文字节数。
pub const MAX_GOOGLE_PRIVATE_KEY_BYTES: usize = 16 * 1_024;

/// 待加密的闭合凭据明文；所有字段默认脱敏并在释放时清零。
pub struct PlainCredentialSecret {
    value: PlainCredentialValue,
}

enum PlainCredentialValue {
    ApiKey(Zeroizing<String>),
    Oauth(Zeroizing<String>),
    OauthBundle(PlainOAuthCredential),
    Bedrock {
        access_key_id: Zeroizing<String>,
        secret_access_key: Zeroizing<String>,
        session_token: Option<Zeroizing<String>>,
    },
    ServiceAccount {
        client_email: Zeroizing<String>,
        private_key_id: Option<Zeroizing<String>>,
        private_key: Zeroizing<String>,
    },
}

/// 加密 wire 使用的短生命周期只读投影。
pub(crate) enum PlainCredentialSecretRef<'a> {
    ApiKey(&'a str),
    Oauth(&'a str),
    OauthBundle(&'a PlainOAuthCredential),
    Bedrock {
        access_key_id: &'a str,
        secret_access_key: &'a str,
        session_token: Option<&'a str>,
    },
    ServiceAccount {
        client_email: &'a str,
        private_key_id: Option<&'a str>,
        private_key: &'a str,
    },
}

impl PlainCredentialSecret {
    /// 兼容既有简单凭据入口；复杂凭据必须使用专用构造器。
    pub fn new(kind: CredentialKind, value: String) -> Result<Self, CredentialEncryptionError> {
        match kind {
            CredentialKind::ApiKey => Self::api_key(value),
            CredentialKind::Oauth => Self::oauth_access_token(value),
            CredentialKind::SetupToken
            | CredentialKind::Bedrock
            | CredentialKind::ServiceAccount
            | CredentialKind::Upstream => Err(CredentialEncryptionError::UnsupportedKind),
        }
    }

    /// 构造 API Key 明文。
    pub fn api_key(value: String) -> Result<Self, CredentialEncryptionError> {
        Ok(Self {
            value: PlainCredentialValue::ApiKey(validate_simple_secret(value)?),
        })
    }

    /// 构造只含 access token 的 OAuth 明文；完整 token 集合仍由 OAuth 持久化端口写入。
    pub fn oauth_access_token(value: String) -> Result<Self, CredentialEncryptionError> {
        Ok(Self {
            value: PlainCredentialValue::Oauth(validate_oauth_token(value)?),
        })
    }

    /// 构造包含 access token、refresh token 和到期时间的完整 OAuth 凭据。
    pub fn oauth_bundle(value: PlainOAuthCredential) -> Self {
        Self {
            value: PlainCredentialValue::OauthBundle(value),
        }
    }

    /// 构造 AWS Bedrock SigV4 凭据。
    pub fn bedrock(
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
    ) -> Result<Self, CredentialEncryptionError> {
        Ok(Self {
            value: PlainCredentialValue::Bedrock {
                access_key_id: validate_ascii_component(
                    access_key_id,
                    MAX_AWS_ACCESS_KEY_ID_BYTES,
                )?,
                secret_access_key: validate_ascii_component(
                    secret_access_key,
                    MAX_AWS_SECRET_ACCESS_KEY_BYTES,
                )?,
                session_token: session_token
                    .map(|value| validate_ascii_component(value, MAX_AWS_SESSION_TOKEN_BYTES))
                    .transpose()?,
            },
        })
    }

    /// 构造 Google Service Account 凭据；令牌端点由 Vertex 适配器固定提供。
    pub fn service_account(
        client_email: String,
        private_key_id: Option<String>,
        private_key: String,
    ) -> Result<Self, CredentialEncryptionError> {
        Ok(Self {
            value: PlainCredentialValue::ServiceAccount {
                client_email: validate_service_account_email(client_email)?,
                private_key_id: private_key_id
                    .map(|value| validate_ascii_component(value, MAX_GOOGLE_PRIVATE_KEY_ID_BYTES))
                    .transpose()?,
                private_key: validate_service_account_private_key(private_key)?,
            },
        })
    }

    /// 返回凭据的稳定类型，不暴露任何字段内容。
    #[must_use]
    pub const fn kind(&self) -> CredentialKind {
        match &self.value {
            PlainCredentialValue::ApiKey(_) => CredentialKind::ApiKey,
            PlainCredentialValue::Oauth(_) | PlainCredentialValue::OauthBundle(_) => {
                CredentialKind::Oauth
            }
            PlainCredentialValue::Bedrock { .. } => CredentialKind::Bedrock,
            PlainCredentialValue::ServiceAccount { .. } => CredentialKind::ServiceAccount,
        }
    }

    pub(crate) fn as_ref(&self) -> PlainCredentialSecretRef<'_> {
        match &self.value {
            PlainCredentialValue::ApiKey(value) => PlainCredentialSecretRef::ApiKey(value),
            PlainCredentialValue::Oauth(value) => PlainCredentialSecretRef::Oauth(value),
            PlainCredentialValue::OauthBundle(value) => {
                PlainCredentialSecretRef::OauthBundle(value)
            }
            PlainCredentialValue::Bedrock {
                access_key_id,
                secret_access_key,
                session_token,
            } => PlainCredentialSecretRef::Bedrock {
                access_key_id,
                secret_access_key,
                session_token: session_token.as_ref().map(|value| value.as_str()),
            },
            PlainCredentialValue::ServiceAccount {
                client_email,
                private_key_id,
                private_key,
            } => PlainCredentialSecretRef::ServiceAccount {
                client_email,
                private_key_id: private_key_id.as_ref().map(|value| value.as_str()),
                private_key,
            },
        }
    }
}

impl fmt::Debug for PlainCredentialSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlainCredentialSecret")
            .field("kind", &self.kind())
            .field("value", &"<已脱敏>")
            .finish()
    }
}

pub(crate) fn valid_simple_secret(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES
        && value.trim() == value
        && value.is_ascii()
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

pub(crate) fn valid_oauth_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

pub(crate) fn valid_ascii_component(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

pub(crate) fn valid_service_account_email(value: &str) -> bool {
    if !valid_ascii_component(value, MAX_GOOGLE_SERVICE_ACCOUNT_EMAIL_BYTES) {
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

pub(crate) fn valid_service_account_private_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_GOOGLE_PRIVATE_KEY_BYTES
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() && !matches!(byte, b'\r' | b'\n'))
        && EncodingKey::from_rsa_pem(value.as_bytes()).is_ok()
}

fn validate_simple_secret(value: String) -> Result<Zeroizing<String>, CredentialEncryptionError> {
    validate_owned(value, valid_simple_secret)
}

fn validate_oauth_token(value: String) -> Result<Zeroizing<String>, CredentialEncryptionError> {
    validate_owned(value, valid_oauth_token)
}

fn validate_ascii_component(
    value: String,
    maximum_bytes: usize,
) -> Result<Zeroizing<String>, CredentialEncryptionError> {
    validate_owned(value, |value| valid_ascii_component(value, maximum_bytes))
}

fn validate_service_account_email(
    value: String,
) -> Result<Zeroizing<String>, CredentialEncryptionError> {
    validate_owned(value, valid_service_account_email)
}

fn validate_service_account_private_key(
    value: String,
) -> Result<Zeroizing<String>, CredentialEncryptionError> {
    validate_owned(value, valid_service_account_private_key)
}

fn validate_owned(
    value: String,
    validate: impl FnOnce(&str) -> bool,
) -> Result<Zeroizing<String>, CredentialEncryptionError> {
    let value = Zeroizing::new(value);
    if validate(value.as_str()) {
        Ok(value)
    } else {
        Err(CredentialEncryptionError::InvalidPlaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVICE_ACCOUNT_PRIVATE_KEY: &str =
        include_str!("../../af-adapter/src/vertex/fixtures/service_account_private.pem");

    #[test]
    fn constructors_validate_closed_provider_shapes_and_redact_debug() {
        let bedrock = PlainCredentialSecret::bedrock(
            "AKIDEXAMPLE00000001".to_owned(),
            "aws-secret-access-key".to_owned(),
            None,
        )
        .unwrap();
        let service_account = PlainCredentialSecret::service_account(
            "runtime@example.iam.gserviceaccount.com".to_owned(),
            None,
            SERVICE_ACCOUNT_PRIVATE_KEY.to_owned(),
        )
        .unwrap();
        assert_eq!(bedrock.kind(), CredentialKind::Bedrock);
        assert_eq!(service_account.kind(), CredentialKind::ServiceAccount);
        let rendered = format!("{bedrock:?}{service_account:?}");
        assert!(!rendered.contains("AKIDEXAMPLE00000001"));
        assert!(!rendered.contains("runtime@example.iam.gserviceaccount.com"));
        assert!(!rendered.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn constructors_reject_invalid_structured_components() {
        for result in [
            PlainCredentialSecret::bedrock("access key".to_owned(), "secret".to_owned(), None),
            PlainCredentialSecret::bedrock("access-key".to_owned(), "secret key".to_owned(), None),
            PlainCredentialSecret::bedrock(
                "access-key".to_owned(),
                "secret".to_owned(),
                Some(String::new()),
            ),
        ] {
            assert_eq!(
                result.unwrap_err(),
                CredentialEncryptionError::InvalidPlaintext
            );
        }
        for result in [
            PlainCredentialSecret::service_account(
                "not-a-service-account@example.com".to_owned(),
                None,
                SERVICE_ACCOUNT_PRIVATE_KEY.to_owned(),
            ),
            PlainCredentialSecret::service_account(
                "runtime@example.iam.gserviceaccount.com".to_owned(),
                Some(String::new()),
                SERVICE_ACCOUNT_PRIVATE_KEY.to_owned(),
            ),
            PlainCredentialSecret::service_account(
                "runtime@example.iam.gserviceaccount.com".to_owned(),
                None,
                "not-an-rsa-private-key".to_owned(),
            ),
        ] {
            assert_eq!(
                result.unwrap_err(),
                CredentialEncryptionError::InvalidPlaintext
            );
        }
    }
}
