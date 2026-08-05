use std::{fmt, mem};

use af_config::{
    CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings,
    MAX_CREDENTIAL_ENCRYPTION_KEY_ID_BYTES,
};
use af_db::{ChannelProbeTargetRecord, EncryptedCredentialEnvelope};
use af_domain::{ChannelId, CredentialKind};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use serde::{Deserialize, Deserializer, de::Error as _};
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

use crate::{
    credential_secret::{
        MAX_AWS_ACCESS_KEY_ID_BYTES, MAX_AWS_SECRET_ACCESS_KEY_BYTES, MAX_AWS_SESSION_TOKEN_BYTES,
        MAX_GOOGLE_PRIVATE_KEY_ID_BYTES, valid_ascii_component, valid_oauth_token,
        valid_service_account_email, valid_service_account_private_key, valid_simple_secret,
    },
    oauth::validate_scope,
};

/// 解密后单个 API Key 或 access token 允许占用的最大字节数。
pub const MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES: usize = 16 * 1_024;

/// 凭据解密失败；错误不包含 key_id、nonce、密文或明文。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CredentialDecryptionError {
    /// 启动配置缺少完整密钥材料。
    #[error("凭据解密密钥未配置")]
    MissingKey,
    /// 启动密钥标识或密钥材料无效。
    #[error("凭据解密密钥无效")]
    InvalidKey,
    /// 密文封套声明的 key_id 与当前启动密钥不一致。
    #[error("凭据密文密钥标识不匹配")]
    KeyMismatch,
    /// 当前切片尚不支持数据库声明的凭据类型。
    #[error("凭据类型暂不支持探活")]
    UnsupportedKind,
    /// 密文绑定参数不满足持久化不变量。
    #[error("凭据密文绑定参数无效")]
    InvalidBinding,
    /// AEAD 认证或解密失败。
    #[error("凭据密文解密失败")]
    Decrypt,
    /// 解密后的 JSON 结构或 secret 字段无效。
    #[error("凭据明文结构无效")]
    InvalidPlaintext,
    /// 数据库声明的凭据类型与明文 JSON kind 不一致。
    #[error("凭据明文类型不匹配")]
    KindMismatch,
}

/// 单密钥 v1 凭据解密器；内部密钥在释放时清零。
#[derive(Clone)]
pub struct CredentialDecryptor {
    key_id: String,
    key: CredentialKey,
}

impl CredentialDecryptor {
    /// 从启动配置构造解密器，并立即解码 Base64URL 密钥材料。
    pub fn new(settings: &CredentialEncryptionSettings) -> Result<Self, CredentialDecryptionError> {
        let key_id = settings
            .key_id()
            .ok_or(CredentialDecryptionError::MissingKey)?;
        let encoded_key = settings
            .key()
            .ok_or(CredentialDecryptionError::MissingKey)?;
        if !is_valid_key_id(key_id) {
            return Err(CredentialDecryptionError::InvalidKey);
        }
        let key = CredentialKey::from_encoded(encoded_key.expose())?;
        Ok(Self {
            key_id: key_id.to_owned(),
            key,
        })
    }

    /// 解密数据库探活目标中的凭据封套。
    pub fn decrypt_target(
        &self,
        target: &ChannelProbeTargetRecord,
    ) -> Result<DecryptedCredential, CredentialDecryptionError> {
        self.decrypt_envelope(
            target.channel_id(),
            target.credential_id(),
            target.credential_kind(),
            target.envelope(),
        )
    }

    /// 解密单个凭据封套；测试和未来写入链路可复用同一 AAD 约定。
    pub fn decrypt_envelope(
        &self,
        channel_id: ChannelId,
        credential_id: i64,
        credential_kind: CredentialKind,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedCredential, CredentialDecryptionError> {
        let wire = self.decrypt_wire(channel_id, credential_id, credential_kind, envelope)?;
        let value = match wire {
            PlainCredentialWire::ApiKey { api_key } => DecryptedCredentialValue::ApiKey(api_key),
            PlainCredentialWire::Oauth {
                access_token,
                refresh_token,
                ..
            } => {
                let has_refresh_token = refresh_token.is_some();
                // 普通转发只保留可恢复能力这一位事实，refresh token 本体立即释放并清零。
                drop(refresh_token);
                DecryptedCredentialValue::Oauth {
                    access_token,
                    has_refresh_token,
                }
            }
            PlainCredentialWire::Bedrock {
                access_key_id,
                secret_access_key,
                session_token,
            } => DecryptedCredentialValue::Bedrock {
                access_key_id,
                secret_access_key,
                session_token,
            },
            PlainCredentialWire::ServiceAccount {
                client_email,
                private_key_id,
                private_key,
            } => DecryptedCredentialValue::ServiceAccount {
                client_email,
                private_key_id,
                private_key,
            },
        };
        if value.kind() != credential_kind {
            return Err(CredentialDecryptionError::KindMismatch);
        }
        Ok(DecryptedCredential { value })
    }

    /// 解密完整 OAuth token 集合，供后续刷新协调器使用。
    pub fn decrypt_oauth_envelope(
        &self,
        channel_id: ChannelId,
        credential_id: i64,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedOAuthCredential, CredentialDecryptionError> {
        let wire = self.decrypt_wire(channel_id, credential_id, CredentialKind::Oauth, envelope)?;
        let PlainCredentialWire::Oauth {
            access_token,
            refresh_token,
            expires_at_epoch_seconds,
            scope,
        } = wire
        else {
            return Err(CredentialDecryptionError::KindMismatch);
        };
        if expires_at_epoch_seconds.is_some_and(|value| value < 0) {
            return Err(CredentialDecryptionError::InvalidPlaintext);
        }
        Ok(DecryptedOAuthCredential {
            access_token,
            refresh_token,
            expires_at_epoch_seconds,
            scope,
        })
    }

    fn decrypt_wire(
        &self,
        channel_id: ChannelId,
        credential_id: i64,
        credential_kind: CredentialKind,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<PlainCredentialWire, CredentialDecryptionError> {
        if !matches!(
            credential_kind,
            CredentialKind::ApiKey
                | CredentialKind::Oauth
                | CredentialKind::Bedrock
                | CredentialKind::ServiceAccount
        ) {
            return Err(CredentialDecryptionError::UnsupportedKind);
        }
        if credential_id <= 0 {
            return Err(CredentialDecryptionError::InvalidBinding);
        }
        if envelope.key_id() != self.key_id {
            return Err(CredentialDecryptionError::KeyMismatch);
        }

        let aad = credential_plaintext_aad(channel_id, credential_id, credential_kind);
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| CredentialDecryptionError::InvalidKey)?;
        let mut plaintext = cipher
            .decrypt(
                XNonce::from_slice(envelope.nonce()),
                Payload {
                    msg: envelope.ciphertext(),
                    aad: &aad,
                },
            )
            .map_err(|_| CredentialDecryptionError::Decrypt)?;
        let wire = serde_json::from_slice::<PlainCredentialWire>(&plaintext)
            .map_err(|_| CredentialDecryptionError::InvalidPlaintext);
        plaintext.zeroize();
        wire
    }
}

impl fmt::Debug for CredentialDecryptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialDecryptor")
            .field("key_id", &"<已脱敏>")
            .finish_non_exhaustive()
    }
}

/// 构造 AEAD AAD，绑定渠道、凭据与数据库声明类型，避免密文跨记录搬用。
#[must_use]
pub fn credential_plaintext_aad(
    channel_id: ChannelId,
    credential_id: i64,
    credential_kind: CredentialKind,
) -> Vec<u8> {
    format!(
        "anyflows:credential:v1:channel={}:credential={}:kind={}",
        channel_id.get(),
        credential_id,
        credential_kind.as_str()
    )
    .into_bytes()
}

pub(crate) fn is_valid_key_id(key_id: &str) -> bool {
    !key_id.is_empty()
        && key_id.len() <= MAX_CREDENTIAL_ENCRYPTION_KEY_ID_BYTES
        && key_id.trim() == key_id
        && key_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[derive(Clone)]
pub(crate) struct CredentialKey([u8; CREDENTIAL_ENCRYPTION_KEY_BYTES]);

impl CredentialKey {
    pub(crate) fn from_encoded(value: &str) -> Result<Self, CredentialDecryptionError> {
        let mut decoded = [0_u8; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let valid = URL_SAFE_NO_PAD
            .decode_slice(value, &mut decoded)
            .is_ok_and(|length| length == CREDENTIAL_ENCRYPTION_KEY_BYTES);
        if !valid {
            decoded.zeroize();
            return Err(CredentialDecryptionError::InvalidKey);
        }
        Ok(Self(decoded))
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for CredentialKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum PlainCredentialWire {
    #[serde(rename = "api_key")]
    ApiKey { api_key: SecretText },
    #[serde(rename = "oauth")]
    Oauth {
        access_token: OAuthTokenText,
        #[serde(default)]
        refresh_token: Option<OAuthTokenText>,
        #[serde(default)]
        expires_at_epoch_seconds: Option<i64>,
        #[serde(default)]
        scope: Option<OAuthScopeText>,
    },
    #[serde(rename = "bedrock")]
    Bedrock {
        access_key_id: AsciiComponentText<MAX_AWS_ACCESS_KEY_ID_BYTES>,
        secret_access_key: AsciiComponentText<MAX_AWS_SECRET_ACCESS_KEY_BYTES>,
        #[serde(default)]
        session_token: Option<AsciiComponentText<MAX_AWS_SESSION_TOKEN_BYTES>>,
    },
    #[serde(rename = "service_account")]
    ServiceAccount {
        client_email: ServiceAccountEmailText,
        #[serde(default)]
        private_key_id: Option<AsciiComponentText<MAX_GOOGLE_PRIVATE_KEY_ID_BYTES>>,
        private_key: ServiceAccountPrivateKeyText,
    },
}

/// 已解密但默认脱敏并在释放时清零的凭据。
pub struct DecryptedCredential {
    value: DecryptedCredentialValue,
}

enum DecryptedCredentialValue {
    ApiKey(SecretText),
    Oauth {
        access_token: OAuthTokenText,
        has_refresh_token: bool,
    },
    Bedrock {
        access_key_id: AsciiComponentText<MAX_AWS_ACCESS_KEY_ID_BYTES>,
        secret_access_key: AsciiComponentText<MAX_AWS_SECRET_ACCESS_KEY_BYTES>,
        session_token: Option<AsciiComponentText<MAX_AWS_SESSION_TOKEN_BYTES>>,
    },
    ServiceAccount {
        client_email: ServiceAccountEmailText,
        private_key_id: Option<AsciiComponentText<MAX_GOOGLE_PRIVATE_KEY_ID_BYTES>>,
        private_key: ServiceAccountPrivateKeyText,
    },
}

impl DecryptedCredentialValue {
    const fn kind(&self) -> CredentialKind {
        match self {
            Self::ApiKey(_) => CredentialKind::ApiKey,
            Self::Oauth { .. } => CredentialKind::Oauth,
            Self::Bedrock { .. } => CredentialKind::Bedrock,
            Self::ServiceAccount { .. } => CredentialKind::ServiceAccount,
        }
    }
}

impl DecryptedCredential {
    /// 返回明文声明的凭据类型。
    #[must_use]
    pub const fn kind(&self) -> CredentialKind {
        self.value.kind()
    }

    /// 返回 OAuth 凭据是否具备 refresh token；非 OAuth 凭据始终返回假。
    ///
    /// 本方法只暴露调度恢复能力，不返回、复制或记录 refresh token 本体。
    #[must_use]
    pub const fn oauth_has_refresh_token(&self) -> bool {
        matches!(
            &self.value,
            DecryptedCredentialValue::Oauth {
                has_refresh_token: true,
                ..
            }
        )
    }

    /// 返回简单凭据或复杂凭据的主秘密字段。
    ///
    /// Bedrock 与 Service Account 调用方必须改用结构化访问器，不能重新拼接字段。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        match &self.value {
            DecryptedCredentialValue::ApiKey(value) => value.expose(),
            DecryptedCredentialValue::Oauth { access_token, .. } => access_token.expose(),
            DecryptedCredentialValue::Bedrock {
                secret_access_key, ..
            } => secret_access_key.expose(),
            DecryptedCredentialValue::ServiceAccount { private_key, .. } => private_key.expose(),
        }
    }

    /// 返回 Bedrock SigV4 字段；调用方只应短暂用于构造适配器凭据。
    #[must_use]
    pub fn bedrock_parts(&self) -> Option<(&str, &str, Option<&str>)> {
        match &self.value {
            DecryptedCredentialValue::Bedrock {
                access_key_id,
                secret_access_key,
                session_token,
            } => Some((
                access_key_id.expose(),
                secret_access_key.expose(),
                session_token.as_ref().map(AsciiComponentText::expose),
            )),
            _ => None,
        }
    }

    /// 返回 Service Account JWT 字段；令牌端点不属于凭据内容。
    #[must_use]
    pub fn service_account_parts(&self) -> Option<(&str, Option<&str>, &str)> {
        match &self.value {
            DecryptedCredentialValue::ServiceAccount {
                client_email,
                private_key_id,
                private_key,
            } => Some((
                client_email.expose(),
                private_key_id.as_ref().map(AsciiComponentText::expose),
                private_key.expose(),
            )),
            _ => None,
        }
    }
}

impl fmt::Debug for DecryptedCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DecryptedCredential")
            .field("kind", &self.kind())
            .field("secret", &"<已脱敏>")
            .finish()
    }
}

/// 已解密的完整 OAuth token 集合；普通转发链路不使用该类型。
pub struct DecryptedOAuthCredential {
    access_token: OAuthTokenText,
    refresh_token: Option<OAuthTokenText>,
    expires_at_epoch_seconds: Option<i64>,
    scope: Option<OAuthScopeText>,
}

impl DecryptedOAuthCredential {
    /// 返回 access token；调用方不得记录或复制。
    #[must_use]
    pub fn access_token(&self) -> &str {
        self.access_token.expose()
    }

    /// 返回可选 refresh token；调用方不得记录或复制。
    #[must_use]
    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_ref().map(OAuthTokenText::expose)
    }

    /// 返回 Unix epoch 秒表示的绝对过期时间。
    #[must_use]
    pub const fn expires_at_epoch_seconds(&self) -> Option<i64> {
        self.expires_at_epoch_seconds
    }

    /// 返回供应商确认的可选 scope；调用方不得记录或复制。
    #[must_use]
    pub fn scope(&self) -> Option<&str> {
        self.scope.as_ref().map(OAuthScopeText::expose)
    }

    /// 按所有权移出续期材料，并先清零后台刷新不再需要的 access token。
    pub(crate) fn into_refresh_material(
        self,
    ) -> (
        Option<Zeroizing<String>>,
        Option<Zeroizing<String>>,
        Option<i64>,
    ) {
        let Self {
            access_token,
            refresh_token,
            expires_at_epoch_seconds,
            scope,
        } = self;
        // 后台刷新不再需要旧 access token，先释放并清零它，再移动可续期材料。
        drop(access_token);
        (
            refresh_token.map(OAuthTokenText::into_zeroizing),
            scope.map(OAuthScopeText::into_zeroizing),
            expires_at_epoch_seconds,
        )
    }
}

impl fmt::Debug for DecryptedOAuthCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DecryptedOAuthCredential")
            .field("access_token", &"<已脱敏>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<已脱敏>"),
            )
            .field("expires_at_epoch_seconds", &self.expires_at_epoch_seconds)
            .field("scope", &self.scope.as_ref().map(|_| "<已脱敏>"))
            .finish()
    }
}

/// 明文 secret 字段；校验边界与当前适配器凭据保持一致。
struct SecretText(String);

impl SecretText {
    fn new(mut value: String) -> Result<Self, CredentialDecryptionError> {
        if !valid_simple_secret(&value) {
            value.zeroize();
            return Err(CredentialDecryptionError::InvalidPlaintext);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SecretText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl Drop for SecretText {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SecretText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<已脱敏>")
    }
}

/// OAuth token 文本；任何空白和控制字符都会使整份密文失败关闭。
struct OAuthTokenText(String);

impl OAuthTokenText {
    fn new(mut value: String) -> Result<Self, CredentialDecryptionError> {
        if !valid_oauth_token(&value) {
            value.zeroize();
            return Err(CredentialDecryptionError::InvalidPlaintext);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }

    fn into_zeroizing(mut self) -> Zeroizing<String> {
        Zeroizing::new(mem::take(&mut self.0))
    }
}

impl<'de> Deserialize<'de> for OAuthTokenText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl Drop for OAuthTokenText {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for OAuthTokenText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<已脱敏>")
    }
}

/// 供应商结构化凭据中的无空白 ASCII 组件。
struct AsciiComponentText<const MAXIMUM_BYTES: usize>(String);

impl<const MAXIMUM_BYTES: usize> AsciiComponentText<MAXIMUM_BYTES> {
    fn new(mut value: String) -> Result<Self, CredentialDecryptionError> {
        if !valid_ascii_component(&value, MAXIMUM_BYTES) {
            value.zeroize();
            return Err(CredentialDecryptionError::InvalidPlaintext);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl<'de, const MAXIMUM_BYTES: usize> Deserialize<'de> for AsciiComponentText<MAXIMUM_BYTES> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl<const MAXIMUM_BYTES: usize> Drop for AsciiComponentText<MAXIMUM_BYTES> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<const MAXIMUM_BYTES: usize> fmt::Debug for AsciiComponentText<MAXIMUM_BYTES> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<已脱敏>")
    }
}

/// 已校验 Google Service Account 域名约束的客户端邮箱。
struct ServiceAccountEmailText(String);

impl ServiceAccountEmailText {
    fn new(mut value: String) -> Result<Self, CredentialDecryptionError> {
        if !valid_service_account_email(&value) {
            value.zeroize();
            return Err(CredentialDecryptionError::InvalidPlaintext);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ServiceAccountEmailText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl Drop for ServiceAccountEmailText {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for ServiceAccountEmailText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<已脱敏>")
    }
}

/// 已解析为 RSA 密钥的 Service Account 私钥 PEM。
struct ServiceAccountPrivateKeyText(String);

impl ServiceAccountPrivateKeyText {
    fn new(mut value: String) -> Result<Self, CredentialDecryptionError> {
        if !valid_service_account_private_key(&value) {
            value.zeroize();
            return Err(CredentialDecryptionError::InvalidPlaintext);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ServiceAccountPrivateKeyText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl Drop for ServiceAccountPrivateKeyText {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for ServiceAccountPrivateKeyText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<已脱敏>")
    }
}

/// OAuth scope 允许单个 ASCII 空格分隔，不能复用禁止空白的 token 校验器。
struct OAuthScopeText(String);

impl OAuthScopeText {
    fn new(mut value: String) -> Result<Self, CredentialDecryptionError> {
        if validate_scope(&value).is_err() {
            value.zeroize();
            return Err(CredentialDecryptionError::InvalidPlaintext);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }

    fn into_zeroizing(mut self) -> Zeroizing<String> {
        Zeroizing::new(mem::take(&mut self.0))
    }
}

impl<'de> Deserialize<'de> for OAuthScopeText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl Drop for OAuthScopeText {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for OAuthScopeText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<已脱敏>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_config::CredentialEncryptionSettings;
    use serde_json::json;

    const CHANNEL_ID: i64 = 7;
    const CREDENTIAL_ID: i64 = 11;

    #[test]
    fn decrypts_api_key_and_redacts_debug_output() {
        let key = [0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let decryptor = decryptor(&key, "primary-key");
        let envelope = encrypted_envelope(
            &key,
            "primary-key",
            CredentialKind::ApiKey,
            br#"{"kind":"api_key","api_key":"sk-test-secret"}"#,
        );

        let credential = decryptor
            .decrypt_envelope(
                channel_id(),
                CREDENTIAL_ID,
                CredentialKind::ApiKey,
                &envelope,
            )
            .unwrap();

        assert_eq!(credential.kind(), CredentialKind::ApiKey);
        assert_eq!(credential.expose_secret(), "sk-test-secret");
        assert!(!credential.oauth_has_refresh_token());
        for rendered in [format!("{decryptor:?}"), format!("{credential:?}")] {
            assert!(!rendered.contains("primary-key"));
            assert!(!rendered.contains("sk-test-secret"));
        }
    }

    #[test]
    fn decrypts_oauth_access_token() {
        let key = [0x24; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let decryptor = decryptor(&key, "oauth-key");
        let envelope = encrypted_envelope(
            &key,
            "oauth-key",
            CredentialKind::Oauth,
            br#"{"kind":"oauth","access_token":"ya29.test-token"}"#,
        );

        let credential = decryptor
            .decrypt_envelope(
                channel_id(),
                CREDENTIAL_ID,
                CredentialKind::Oauth,
                &envelope,
            )
            .unwrap();

        assert_eq!(credential.kind(), CredentialKind::Oauth);
        assert_eq!(credential.expose_secret(), "ya29.test-token");
        assert!(!credential.oauth_has_refresh_token());

        let complete = decryptor
            .decrypt_oauth_envelope(channel_id(), CREDENTIAL_ID, &envelope)
            .unwrap();
        assert_eq!(complete.access_token(), "ya29.test-token");
        assert_eq!(complete.refresh_token(), None);
        assert_eq!(complete.expires_at_epoch_seconds(), None);
        assert_eq!(complete.scope(), None);
    }

    #[test]
    fn rejects_wrong_key_id_tampered_aad_and_kind_mismatch() {
        let key = [0x66; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let decryptor = decryptor(&key, "primary-key");
        let envelope = encrypted_envelope(
            &key,
            "primary-key",
            CredentialKind::ApiKey,
            br#"{"kind":"api_key","api_key":"sk-test-secret"}"#,
        );
        let wrong_key = encrypted_envelope(
            &key,
            "other-key",
            CredentialKind::ApiKey,
            br#"{"kind":"api_key","api_key":"sk-test-secret"}"#,
        );
        let kind_mismatch = encrypted_envelope(
            &key,
            "primary-key",
            CredentialKind::Oauth,
            br#"{"kind":"api_key","api_key":"sk-test-secret"}"#,
        );

        assert_eq!(
            decryptor
                .decrypt_envelope(
                    channel_id(),
                    CREDENTIAL_ID,
                    CredentialKind::ApiKey,
                    &wrong_key,
                )
                .unwrap_err(),
            CredentialDecryptionError::KeyMismatch
        );
        assert_eq!(
            decryptor
                .decrypt_envelope(
                    channel_id(),
                    CREDENTIAL_ID + 1,
                    CredentialKind::ApiKey,
                    &envelope,
                )
                .unwrap_err(),
            CredentialDecryptionError::Decrypt
        );
        assert_eq!(
            decryptor
                .decrypt_envelope(
                    ChannelId::new(CHANNEL_ID + 1).unwrap(),
                    CREDENTIAL_ID,
                    CredentialKind::ApiKey,
                    &envelope,
                )
                .unwrap_err(),
            CredentialDecryptionError::Decrypt
        );
        assert_eq!(
            decryptor
                .decrypt_envelope(
                    channel_id(),
                    CREDENTIAL_ID,
                    CredentialKind::Oauth,
                    &envelope,
                )
                .unwrap_err(),
            CredentialDecryptionError::Decrypt
        );
        assert_eq!(
            decryptor
                .decrypt_envelope(
                    channel_id(),
                    CREDENTIAL_ID,
                    CredentialKind::Oauth,
                    &kind_mismatch,
                )
                .unwrap_err(),
            CredentialDecryptionError::KindMismatch
        );
    }

    #[test]
    fn rejects_invalid_configuration_and_plaintext_shape() {
        let missing = serde_json::from_value::<CredentialEncryptionSettings>(json!({})).unwrap();
        assert_eq!(
            CredentialDecryptor::new(&missing).unwrap_err(),
            CredentialDecryptionError::MissingKey
        );

        let invalid_key = serde_json::from_value::<CredentialEncryptionSettings>(json!({
            "key_id": "primary-key",
            "key": "not-base64url"
        }))
        .unwrap();
        assert_eq!(
            CredentialDecryptor::new(&invalid_key).unwrap_err(),
            CredentialDecryptionError::InvalidKey
        );

        let key = [0x77; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let decryptor = decryptor(&key, "primary-key");
        for invalid_plaintext in [
            br#"{"kind":"api_key","api_key":" secret"}"#.as_slice(),
            br#"{"kind":"api_key","api_key":"sk-test","extra":true}"#.as_slice(),
            r#"{"kind":"oauth","access_token":"非ASCII"}"#.as_bytes(),
        ] {
            let envelope = encrypted_envelope(
                &key,
                "primary-key",
                CredentialKind::ApiKey,
                invalid_plaintext,
            );
            assert_eq!(
                decryptor
                    .decrypt_envelope(
                        channel_id(),
                        CREDENTIAL_ID,
                        CredentialKind::ApiKey,
                        &envelope,
                    )
                    .unwrap_err(),
                CredentialDecryptionError::InvalidPlaintext
            );
        }

        for invalid_plaintext in [
            br#"{"kind":"oauth","access_token":"access","extra":true}"#.as_slice(),
            br#"{"kind":"oauth","access_token":"access","expires_at_epoch_seconds":-1}"#.as_slice(),
            br#"{"kind":"oauth","access_token":"access","scope":"openid  profile"}"#.as_slice(),
        ] {
            let envelope = encrypted_envelope(
                &key,
                "primary-key",
                CredentialKind::Oauth,
                invalid_plaintext,
            );
            assert_eq!(
                decryptor
                    .decrypt_oauth_envelope(channel_id(), CREDENTIAL_ID, &envelope)
                    .unwrap_err(),
                CredentialDecryptionError::InvalidPlaintext
            );
        }
    }

    fn decryptor(key: &[u8; CREDENTIAL_ENCRYPTION_KEY_BYTES], key_id: &str) -> CredentialDecryptor {
        let settings = serde_json::from_value::<CredentialEncryptionSettings>(json!({
            "key_id": key_id,
            "key": URL_SAFE_NO_PAD.encode(key)
        }))
        .unwrap();
        CredentialDecryptor::new(&settings).unwrap()
    }

    fn encrypted_envelope(
        key: &[u8; CREDENTIAL_ENCRYPTION_KEY_BYTES],
        key_id: &str,
        aad_kind: CredentialKind,
        plaintext: &[u8],
    ) -> EncryptedCredentialEnvelope {
        let nonce = [0x5a; 24];
        let cipher = XChaCha20Poly1305::new_from_slice(key).unwrap();
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: &credential_plaintext_aad(channel_id(), CREDENTIAL_ID, aad_kind),
                },
            )
            .unwrap();
        EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext).unwrap()
    }

    fn channel_id() -> ChannelId {
        ChannelId::new(CHANNEL_ID).unwrap()
    }
}
