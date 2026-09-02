use std::{fmt, mem};

use af_config::CredentialEncryptionSettings;
use af_db::{CREDENTIAL_ENVELOPE_NONCE_BYTES, EncryptedCredentialEnvelope};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

use crate::credential_decryption::{CredentialKey, is_valid_key_id};
use crate::oauth::CustomOAuth2ProviderKey;

const MAX_SYSTEM_SECRET_BYTES: usize = 4 * 1_024;
const ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_KEY_CONTEXT: &[u8] =
    b"anyflows:organization-sso:public-login-rate-limit:v1";

/// 系统密钥用途参与 AAD，避免不同业务域之间搬用密文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemSecretKind {
    /// 系统 SMTP 认证密码。
    SmtpPassword,
    /// 系统出站代理认证密码。
    ProxyPassword,
    /// Stripe 服务端密钥。
    StripeSecretKey,
    /// Stripe webhook 签名密钥。
    StripeWebhookSecret,
    /// 易支付商户密钥。
    EpayMerchantKey,
    /// GitHub 用户登录 OAuth App 的 Client Secret。
    OAuthLoginGithubClientSecret,
    /// Discord 用户登录 OAuth App 的 Client Secret。
    OAuthLoginDiscordClientSecret,
    /// 通用 OIDC 用户登录 OAuth App 的 Client Secret。
    OAuthLoginOidcClientSecret,
    /// LinuxDO 用户登录 OAuth App 的 Client Secret。
    OAuthLoginLinuxDoClientSecret,
    /// 微信开放平台网页应用的 AppSecret。
    OAuthLoginWechatClientSecret,
    /// Telegram OIDC 应用的 Client Secret。
    OAuthLoginTelegramClientSecret,
    /// Google OIDC 应用的 Client Secret。
    OAuthLoginGoogleClientSecret,
    /// 用户登录 OAuth state 内携带的一次性随机材料封套。
    OAuthLoginState,
    /// 用户 TOTP 配置；用户标识参与 AAD，禁止跨账户搬用。
    TotpSecret(af_domain::UserId),
    /// Passkey 注册状态；用户标识参与 AAD，禁止跨账户搬用。
    PasskeyRegistrationState(af_domain::UserId),
    /// Passkey 登录状态；用户标识参与 AAD，禁止跨账户搬运密文。
    PasskeyAuthenticationState(af_domain::UserId),
    /// 凭据专属代理认证密码；记录标识参与 AAD，禁止密文跨代理搬用。
    CredentialProxyPassword(af_domain::ProxyId),
    /// 企业 SSO Provider 敏感材料；企业、Provider、协议和配置版本共同参与 AAD。
    OrganizationSsoProviderSecret {
        organization_id: af_domain::OrganizationId,
        provider_key: af_domain::OrganizationSsoProviderKey,
        protocol: af_domain::OrganizationSsoProtocol,
        config_version: i64,
    },
}

impl SystemSecretKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::SmtpPassword => "smtp_password",
            Self::ProxyPassword => "proxy_password",
            Self::StripeSecretKey => "stripe_secret_key",
            Self::StripeWebhookSecret => "stripe_webhook_secret",
            Self::EpayMerchantKey => "epay_merchant_key",
            Self::OAuthLoginGithubClientSecret => "oauth_login_github_client_secret",
            Self::OAuthLoginDiscordClientSecret => "oauth_login_discord_client_secret",
            Self::OAuthLoginOidcClientSecret => "oauth_login_oidc_client_secret",
            Self::OAuthLoginLinuxDoClientSecret => "oauth_login_linuxdo_client_secret",
            Self::OAuthLoginWechatClientSecret => "oauth_login_wechat_client_secret",
            Self::OAuthLoginTelegramClientSecret => "oauth_login_telegram_client_secret",
            Self::OAuthLoginGoogleClientSecret => "oauth_login_google_client_secret",
            Self::OAuthLoginState => "oauth_login_state",
            Self::TotpSecret(_) => "totp_secret",
            Self::PasskeyRegistrationState(_) => "passkey_registration_state",
            Self::PasskeyAuthenticationState(_) => "passkey_authentication_state",
            Self::CredentialProxyPassword(_) => "credential_proxy_password",
            Self::OrganizationSsoProviderSecret { .. } => "organization_sso_provider_secret",
        }
    }
}

/// 系统运行时密钥加解密失败；不携带明文、密文或密钥标识。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SystemSecretError {
    #[error("系统密钥加密材料未配置")]
    MissingKey,
    #[error("系统密钥加密材料无效")]
    InvalidKey,
    #[error("系统密钥明文无效")]
    InvalidPlaintext,
    #[error("系统安全随机数不可用")]
    Entropy,
    #[error("系统密钥加密失败")]
    Encrypt,
    #[error("系统密钥密文封套无效")]
    InvalidEnvelope,
    #[error("系统密钥密钥标识不匹配")]
    KeyMismatch,
    #[error("系统密钥解密失败")]
    Decrypt,
}

/// 待加密系统密钥；默认脱敏并在释放时清零。
pub struct PlainSystemSecret {
    value: String,
}

impl PlainSystemSecret {
    /// 校验系统密钥明文的有界 UTF-8 形状。
    pub fn new(mut value: String) -> Result<Self, SystemSecretError> {
        if !valid_plaintext(&value) {
            value.zeroize();
            return Err(SystemSecretError::InvalidPlaintext);
        }
        Ok(Self { value })
    }

    fn expose(&self) -> &str {
        &self.value
    }
}

impl Drop for PlainSystemSecret {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

impl fmt::Debug for PlainSystemSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlainSystemSecret(<已脱敏>)")
    }
}

/// 已解密系统密钥；仅投递适配器可读取，释放时立即清零。
pub struct DecryptedSystemSecret {
    value: String,
}

impl DecryptedSystemSecret {
    /// 暴露明文给最终协议适配器；调用方不得克隆或记录。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.value
    }

    /// 将明文所有权转交给受控协议适配器，避免产生未清零的中间副本。
    #[must_use]
    pub fn into_zeroizing(mut self) -> Zeroizing<String> {
        Zeroizing::new(mem::take(&mut self.value))
    }
}

impl Drop for DecryptedSystemSecret {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

impl fmt::Debug for DecryptedSystemSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DecryptedSystemSecret(<已脱敏>)")
    }
}

/// 复用启动期凭据主密钥的系统密钥加解密器，并使用独立 AAD 域隔离。
#[derive(Clone)]
pub struct SystemSecretCipher {
    key_id: String,
    key: CredentialKey,
}

impl SystemSecretCipher {
    /// 从已校验启动配置构造系统密钥加解密器。
    pub fn new(settings: &CredentialEncryptionSettings) -> Result<Self, SystemSecretError> {
        let key_id = settings.key_id().ok_or(SystemSecretError::MissingKey)?;
        let encoded_key = settings.key().ok_or(SystemSecretError::MissingKey)?;
        if !is_valid_key_id(key_id) {
            return Err(SystemSecretError::InvalidKey);
        }
        let key = CredentialKey::from_encoded(encoded_key.expose())
            .map_err(|_| SystemSecretError::InvalidKey)?;
        Ok(Self {
            key_id: key_id.to_owned(),
            key,
        })
    }

    /// 从启动期主密钥派生企业 SSO 公开登录限流专用密钥，不暴露主密钥本身。
    #[must_use]
    pub fn organization_sso_public_login_rate_limit_key(&self) -> [u8; 32] {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(self.key.as_slice())
            .expect("固定长度的启动期主密钥可用作 HMAC 密钥");
        mac.update(ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_KEY_CONTEXT);
        mac.finalize().into_bytes().into()
    }

    /// 使用系统邮件专用 AAD 生成密文封套。
    pub fn encrypt(
        &self,
        kind: SystemSecretKind,
        secret: &PlainSystemSecret,
    ) -> Result<EncryptedCredentialEnvelope, SystemSecretError> {
        self.encrypt_with_aad(&system_secret_aad(kind), secret)
    }

    /// 使用 Provider key 绑定的独立 AAD 加密自定义 OAuth2 Client Secret。
    pub fn encrypt_custom_oauth2_client_secret(
        &self,
        provider_key: &CustomOAuth2ProviderKey,
        secret: &PlainSystemSecret,
    ) -> Result<EncryptedCredentialEnvelope, SystemSecretError> {
        self.encrypt_with_aad(&custom_oauth2_system_secret_aad(provider_key), secret)
    }

    fn encrypt_with_aad(
        &self,
        aad: &[u8],
        secret: &PlainSystemSecret,
    ) -> Result<EncryptedCredentialEnvelope, SystemSecretError> {
        let mut nonce = [0_u8; CREDENTIAL_ENVELOPE_NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| SystemSecretError::Entropy)?;
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| SystemSecretError::InvalidKey)?;
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: secret.expose().as_bytes(),
                    aad,
                },
            )
            .map_err(|_| SystemSecretError::Encrypt)?;
        EncryptedCredentialEnvelope::new(self.key_id.clone(), nonce, ciphertext)
            .map_err(|_| SystemSecretError::InvalidEnvelope)
    }

    /// 验证密钥标识和 AAD 后解密系统邮件密码。
    pub fn decrypt(
        &self,
        kind: SystemSecretKind,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedSystemSecret, SystemSecretError> {
        self.decrypt_with_aad(&system_secret_aad(kind), envelope)
    }

    /// 使用 Provider key 绑定的独立 AAD 解密自定义 OAuth2 Client Secret。
    pub fn decrypt_custom_oauth2_client_secret(
        &self,
        provider_key: &CustomOAuth2ProviderKey,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedSystemSecret, SystemSecretError> {
        self.decrypt_with_aad(&custom_oauth2_system_secret_aad(provider_key), envelope)
    }

    /// 使用企业边界和配置版本绑定的独立 AAD 加密 SSO Provider 密钥。
    pub fn encrypt_organization_sso_provider_secret(
        &self,
        organization_id: af_domain::OrganizationId,
        provider_key: af_domain::OrganizationSsoProviderKey,
        protocol: af_domain::OrganizationSsoProtocol,
        config_version: i64,
        secret: &PlainSystemSecret,
    ) -> Result<EncryptedCredentialEnvelope, SystemSecretError> {
        self.encrypt(
            SystemSecretKind::OrganizationSsoProviderSecret {
                organization_id,
                provider_key,
                protocol,
                config_version,
            },
            secret,
        )
    }

    /// 解密绑定当前企业和配置版本的 SSO Provider 密钥，仅向协议适配器暴露明文。
    pub fn decrypt_organization_sso_provider_secret(
        &self,
        organization_id: af_domain::OrganizationId,
        provider_key: af_domain::OrganizationSsoProviderKey,
        protocol: af_domain::OrganizationSsoProtocol,
        config_version: i64,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedSystemSecret, SystemSecretError> {
        self.decrypt(
            SystemSecretKind::OrganizationSsoProviderSecret {
                organization_id,
                provider_key,
                protocol,
                config_version,
            },
            envelope,
        )
    }

    /// 加密企业 OIDC 登录材料；事务 state 摘要、防止跨登录事务搬用的字段也参与 AAD。
    pub fn encrypt_organization_sso_login_material(
        &self,
        organization_id: af_domain::OrganizationId,
        provider_id: i64,
        protocol: af_domain::OrganizationSsoProtocol,
        config_version: i64,
        state_digest: &str,
        secret: &PlainSystemSecret,
    ) -> Result<EncryptedCredentialEnvelope, SystemSecretError> {
        self.encrypt_with_aad(
            &organization_sso_login_material_aad(
                organization_id,
                provider_id,
                protocol,
                config_version,
                state_digest,
            )?,
            secret,
        )
    }

    /// 解密企业 OIDC 登录材料；企业、Provider、协议、版本和 state 摘要任一不一致都会失败。
    pub fn decrypt_organization_sso_login_material(
        &self,
        organization_id: af_domain::OrganizationId,
        provider_id: i64,
        protocol: af_domain::OrganizationSsoProtocol,
        config_version: i64,
        state_digest: &str,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedSystemSecret, SystemSecretError> {
        self.decrypt_with_aad(
            &organization_sso_login_material_aad(
                organization_id,
                provider_id,
                protocol,
                config_version,
                state_digest,
            )?,
            envelope,
        )
    }

    /// 加密企业 SAML Request ID；事务 state 摘要和 Provider 配置版本共同参与 AAD。
    pub fn encrypt_organization_sso_saml_login_material(
        &self,
        organization_id: af_domain::OrganizationId,
        provider_id: i64,
        config_version: i64,
        state_digest: &str,
        secret: &PlainSystemSecret,
    ) -> Result<EncryptedCredentialEnvelope, SystemSecretError> {
        self.encrypt_organization_sso_login_material(
            organization_id,
            provider_id,
            af_domain::OrganizationSsoProtocol::Saml,
            config_version,
            state_digest,
            secret,
        )
    }

    /// 解密绑定当前 SAML 事务的 Request ID；任一边界不一致都会失败。
    pub fn decrypt_organization_sso_saml_login_material(
        &self,
        organization_id: af_domain::OrganizationId,
        provider_id: i64,
        config_version: i64,
        state_digest: &str,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedSystemSecret, SystemSecretError> {
        self.decrypt_organization_sso_login_material(
            organization_id,
            provider_id,
            af_domain::OrganizationSsoProtocol::Saml,
            config_version,
            state_digest,
            envelope,
        )
    }

    fn decrypt_with_aad(
        &self,
        aad: &[u8],
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedSystemSecret, SystemSecretError> {
        if envelope.key_id() != self.key_id {
            return Err(SystemSecretError::KeyMismatch);
        }
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| SystemSecretError::InvalidKey)?;
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(envelope.nonce()),
                Payload {
                    msg: envelope.ciphertext(),
                    aad,
                },
            )
            .map_err(|_| SystemSecretError::Decrypt)?;
        let value = String::from_utf8(plaintext).map_err(|error| {
            let mut plaintext = error.into_bytes();
            plaintext.zeroize();
            SystemSecretError::InvalidPlaintext
        })?;
        if !valid_plaintext(&value) {
            let mut value = value;
            value.zeroize();
            return Err(SystemSecretError::InvalidPlaintext);
        }
        Ok(DecryptedSystemSecret { value })
    }
}

impl fmt::Debug for SystemSecretCipher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SystemSecretCipher")
            .field("key_id", &"<已脱敏>")
            .finish_non_exhaustive()
    }
}

fn system_secret_aad(kind: SystemSecretKind) -> Vec<u8> {
    let record = match kind {
        SystemSecretKind::SmtpPassword => "email_settings:1".to_owned(),
        SystemSecretKind::ProxyPassword => "network_settings:1".to_owned(),
        SystemSecretKind::StripeSecretKey
        | SystemSecretKind::StripeWebhookSecret
        | SystemSecretKind::EpayMerchantKey => "payment_settings:1".to_owned(),
        SystemSecretKind::OAuthLoginGithubClientSecret => "oauth_login_providers:github".to_owned(),
        SystemSecretKind::OAuthLoginDiscordClientSecret => {
            "oauth_login_providers:discord".to_owned()
        }
        SystemSecretKind::OAuthLoginOidcClientSecret => "oauth_login_providers:oidc".to_owned(),
        SystemSecretKind::OAuthLoginLinuxDoClientSecret => {
            "oauth_login_providers:linuxdo".to_owned()
        }
        SystemSecretKind::OAuthLoginWechatClientSecret => "oauth_login_providers:wechat".to_owned(),
        SystemSecretKind::OAuthLoginTelegramClientSecret => {
            "oauth_login_providers:telegram".to_owned()
        }
        SystemSecretKind::OAuthLoginGoogleClientSecret => "oauth_login_providers:google".to_owned(),
        SystemSecretKind::OAuthLoginState => "oauth_login_state:v1".to_owned(),
        SystemSecretKind::TotpSecret(user_id) => format!("users:{}:totp", user_id.get()),
        SystemSecretKind::PasskeyRegistrationState(user_id) => {
            format!("passkey_registration_challenges:user:{}", user_id.get())
        }
        SystemSecretKind::PasskeyAuthenticationState(user_id) => {
            format!("passkey_authentication_challenges:user:{}", user_id.get())
        }
        SystemSecretKind::CredentialProxyPassword(proxy_id) => {
            format!("proxies:{}", proxy_id.get())
        }
        SystemSecretKind::OrganizationSsoProviderSecret {
            organization_id,
            provider_key,
            protocol,
            config_version,
        } => format!(
            "organizations:{}:sso:providers:{}:protocol={}:version={}",
            organization_id.get(),
            provider_key.persistence_key(),
            protocol.code(),
            config_version
        ),
    };
    format!(
        "anyflows:system-secret:v1:record={record}:kind={}",
        kind.as_str()
    )
    .into_bytes()
}

fn custom_oauth2_system_secret_aad(provider_key: &CustomOAuth2ProviderKey) -> Vec<u8> {
    format!(
        "anyflows:system-secret:v1:record=oauth_login_custom_providers:{}:kind=oauth_login_custom_client_secret",
        provider_key.as_str()
    )
    .into_bytes()
}

fn organization_sso_login_material_aad(
    organization_id: af_domain::OrganizationId,
    provider_id: i64,
    protocol: af_domain::OrganizationSsoProtocol,
    config_version: i64,
    state_digest: &str,
) -> Result<Vec<u8>, SystemSecretError> {
    if provider_id <= 0
        || config_version <= 0
        || state_digest.len() != 64
        || !state_digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(SystemSecretError::InvalidPlaintext);
    }
    Ok(format!(
        "anyflows:system-secret:v1:record=organizations:{}:sso:login-materials:provider={}:protocol={}:version={}:state={}:kind=organization_sso_login_material",
        organization_id.get(),
        provider_id,
        protocol.code(),
        config_version,
        state_digest,
    )
    .into_bytes())
}

fn valid_plaintext(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SYSTEM_SECRET_BYTES
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use serde_json::json;

    use super::*;

    #[test]
    fn round_trips_smtp_password_without_debug_leakage() {
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let secret = PlainSystemSecret::new("mail secret with spaces".to_owned()).unwrap();
        let envelope = cipher
            .encrypt(SystemSecretKind::SmtpPassword, &secret)
            .unwrap();
        let decrypted = cipher
            .decrypt(SystemSecretKind::SmtpPassword, &envelope)
            .unwrap();
        assert_eq!(decrypted.expose_secret(), "mail secret with spaces");
        let rendered = format!("{cipher:?}{secret:?}{envelope:?}{decrypted:?}");
        assert!(!rendered.contains("mail secret"));
    }

    #[test]
    fn rejects_wrong_key_and_invalid_plaintext() {
        assert!(PlainSystemSecret::new(String::new()).is_err());
        assert!(PlainSystemSecret::new("bad\nsecret".to_owned()).is_err());
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let secret = PlainSystemSecret::new("smtp-password".to_owned()).unwrap();
        let envelope = cipher
            .encrypt(SystemSecretKind::SmtpPassword, &secret)
            .unwrap();
        let other = SystemSecretCipher::new(&settings_with_key_id("other-key")).unwrap();
        assert_eq!(
            other
                .decrypt(SystemSecretKind::SmtpPassword, &envelope)
                .unwrap_err(),
            SystemSecretError::KeyMismatch
        );
        assert_eq!(
            cipher
                .decrypt(SystemSecretKind::ProxyPassword, &envelope)
                .unwrap_err(),
            SystemSecretError::Decrypt
        );
    }

    #[test]
    fn credential_proxy_password_is_bound_to_the_proxy_record() {
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let secret = PlainSystemSecret::new("proxy-password".to_owned()).unwrap();
        let first = af_domain::ProxyId::new(7).unwrap();
        let second = af_domain::ProxyId::new(8).unwrap();
        let envelope = cipher
            .encrypt(SystemSecretKind::CredentialProxyPassword(first), &secret)
            .unwrap();

        assert_eq!(
            cipher
                .decrypt(SystemSecretKind::CredentialProxyPassword(first), &envelope)
                .unwrap()
                .expose_secret(),
            "proxy-password"
        );
        assert_eq!(
            cipher
                .decrypt(SystemSecretKind::CredentialProxyPassword(second), &envelope)
                .unwrap_err(),
            SystemSecretError::Decrypt
        );
    }

    #[test]
    fn system_secrets_use_their_own_record_aad_domains() {
        assert_eq!(
            system_secret_aad(SystemSecretKind::SmtpPassword),
            b"anyflows:system-secret:v1:record=email_settings:1:kind=smtp_password"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::ProxyPassword),
            b"anyflows:system-secret:v1:record=network_settings:1:kind=proxy_password"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::StripeSecretKey),
            b"anyflows:system-secret:v1:record=payment_settings:1:kind=stripe_secret_key"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::StripeWebhookSecret),
            b"anyflows:system-secret:v1:record=payment_settings:1:kind=stripe_webhook_secret"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::EpayMerchantKey),
            b"anyflows:system-secret:v1:record=payment_settings:1:kind=epay_merchant_key"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::OAuthLoginGithubClientSecret),
            b"anyflows:system-secret:v1:record=oauth_login_providers:github:kind=oauth_login_github_client_secret"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::OAuthLoginDiscordClientSecret),
            b"anyflows:system-secret:v1:record=oauth_login_providers:discord:kind=oauth_login_discord_client_secret"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::OAuthLoginWechatClientSecret),
            b"anyflows:system-secret:v1:record=oauth_login_providers:wechat:kind=oauth_login_wechat_client_secret"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::OAuthLoginTelegramClientSecret),
            b"anyflows:system-secret:v1:record=oauth_login_providers:telegram:kind=oauth_login_telegram_client_secret"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::OAuthLoginGoogleClientSecret),
            b"anyflows:system-secret:v1:record=oauth_login_providers:google:kind=oauth_login_google_client_secret"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::OAuthLoginState),
            b"anyflows:system-secret:v1:record=oauth_login_state:v1:kind=oauth_login_state"
        );
        assert_eq!(
            system_secret_aad(SystemSecretKind::PasskeyRegistrationState(
                af_domain::UserId::new(7).unwrap()
            )),
            b"anyflows:system-secret:v1:record=passkey_registration_challenges:user:7:kind=passkey_registration_state"
        );
    }

    #[test]
    fn wechat_app_secret_cannot_be_moved_to_another_provider() {
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let secret = PlainSystemSecret::new("wechat-app-secret".to_owned()).unwrap();
        let envelope = cipher
            .encrypt(SystemSecretKind::OAuthLoginWechatClientSecret, &secret)
            .unwrap();

        assert_eq!(
            cipher
                .decrypt(SystemSecretKind::OAuthLoginWechatClientSecret, &envelope)
                .unwrap()
                .expose_secret(),
            "wechat-app-secret"
        );
        assert_eq!(
            cipher
                .decrypt(SystemSecretKind::OAuthLoginGithubClientSecret, &envelope)
                .unwrap_err(),
            SystemSecretError::Decrypt
        );
    }

    #[test]
    fn payment_secrets_cannot_be_moved_between_fields() {
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let secret = PlainSystemSecret::new("payment-secret".to_owned()).unwrap();
        let envelope = cipher
            .encrypt(SystemSecretKind::StripeSecretKey, &secret)
            .unwrap();

        for wrong_kind in [
            SystemSecretKind::StripeWebhookSecret,
            SystemSecretKind::EpayMerchantKey,
            SystemSecretKind::ProxyPassword,
            SystemSecretKind::SmtpPassword,
            SystemSecretKind::OAuthLoginGithubClientSecret,
            SystemSecretKind::OAuthLoginDiscordClientSecret,
            SystemSecretKind::OAuthLoginWechatClientSecret,
            SystemSecretKind::OAuthLoginTelegramClientSecret,
            SystemSecretKind::OAuthLoginGoogleClientSecret,
            SystemSecretKind::OAuthLoginState,
        ] {
            assert_eq!(
                cipher.decrypt(wrong_kind, &envelope).unwrap_err(),
                SystemSecretError::Decrypt
            );
        }
    }

    #[test]
    fn custom_oauth2_client_secret_is_bound_to_provider_key() {
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let secret = PlainSystemSecret::new("custom-client-secret".to_owned()).unwrap();
        let first = CustomOAuth2ProviderKey::new("custom_first".to_owned()).unwrap();
        let second = CustomOAuth2ProviderKey::new("custom_second".to_owned()).unwrap();
        let envelope = cipher
            .encrypt_custom_oauth2_client_secret(&first, &secret)
            .unwrap();

        assert_eq!(
            cipher
                .decrypt_custom_oauth2_client_secret(&first, &envelope)
                .unwrap()
                .expose_secret(),
            "custom-client-secret"
        );
        assert_eq!(
            cipher
                .decrypt_custom_oauth2_client_secret(&second, &envelope)
                .unwrap_err(),
            SystemSecretError::Decrypt
        );
        let rendered = format!("{cipher:?}{secret:?}{envelope:?}");
        assert!(!rendered.contains("custom-client-secret"));
    }

    #[test]
    fn organization_sso_secret_is_bound_to_org_provider_protocol_and_version() {
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let organization = af_domain::OrganizationId::new(7).unwrap();
        let other_organization = af_domain::OrganizationId::new(8).unwrap();
        let provider = af_domain::OrganizationSsoProviderKey::new([0x51; 16]).unwrap();
        let other_provider = af_domain::OrganizationSsoProviderKey::new([0x52; 16]).unwrap();
        let secret = PlainSystemSecret::new("organization-sso-secret".to_owned()).unwrap();
        let envelope = cipher
            .encrypt_organization_sso_provider_secret(
                organization,
                provider,
                af_domain::OrganizationSsoProtocol::Oidc,
                3,
                &secret,
            )
            .unwrap();

        assert_eq!(
            cipher
                .decrypt_organization_sso_provider_secret(
                    organization,
                    provider,
                    af_domain::OrganizationSsoProtocol::Oidc,
                    3,
                    &envelope,
                )
                .unwrap()
                .expose_secret(),
            "organization-sso-secret"
        );
        for wrong in [
            (
                other_organization,
                provider,
                af_domain::OrganizationSsoProtocol::Oidc,
                3,
            ),
            (
                organization,
                other_provider,
                af_domain::OrganizationSsoProtocol::Oidc,
                3,
            ),
            (
                organization,
                provider,
                af_domain::OrganizationSsoProtocol::Saml,
                3,
            ),
            (
                organization,
                provider,
                af_domain::OrganizationSsoProtocol::Oidc,
                4,
            ),
        ] {
            assert_eq!(
                cipher
                    .decrypt_organization_sso_provider_secret(
                        wrong.0, wrong.1, wrong.2, wrong.3, &envelope,
                    )
                    .unwrap_err(),
                SystemSecretError::Decrypt
            );
        }

        let rendered = format!("{cipher:?}{secret:?}{envelope:?}");
        assert!(!rendered.contains("organization-sso-secret"));
    }

    #[test]
    fn organization_sso_login_material_is_bound_to_one_login_transaction() {
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let organization = af_domain::OrganizationId::new(7).unwrap();
        let provider_id = 19_i64;
        let start = crate::OrganizationSsoLoginStart::new("a".repeat(64)).unwrap();
        let state_digest = start.state_digest().to_owned();
        let verifier = start.pkce_verifier().to_owned();
        let nonce = start.nonce().to_owned();
        let secret = start
            .into_oidc_material()
            .into_plain_system_secret()
            .unwrap();
        let envelope = cipher
            .encrypt_organization_sso_login_material(
                organization,
                provider_id,
                af_domain::OrganizationSsoProtocol::Oidc,
                3,
                &state_digest,
                &secret,
            )
            .unwrap();
        let decrypted = cipher
            .decrypt_organization_sso_login_material(
                organization,
                provider_id,
                af_domain::OrganizationSsoProtocol::Oidc,
                3,
                &state_digest,
                &envelope,
            )
            .unwrap();
        let restored =
            crate::OrganizationSsoOidcLoginMaterial::from_decrypted_system_secret(decrypted)
                .unwrap();
        assert_eq!(restored.pkce_verifier(), verifier);
        assert_eq!(restored.nonce(), nonce);
        assert!(!format!("{restored:?}").contains(&verifier));
        assert!(!format!("{restored:?}").contains(&nonce));
        assert!(
            cipher
                .decrypt_organization_sso_login_material(
                    af_domain::OrganizationId::new(8).unwrap(),
                    provider_id,
                    af_domain::OrganizationSsoProtocol::Oidc,
                    3,
                    &state_digest,
                    &envelope,
                )
                .is_err()
        );
        assert!(
            cipher
                .decrypt_organization_sso_login_material(
                    organization,
                    provider_id,
                    af_domain::OrganizationSsoProtocol::Saml,
                    3,
                    &state_digest,
                    &envelope,
                )
                .is_err()
        );
        assert!(
            cipher
                .decrypt_organization_sso_login_material(
                    organization,
                    provider_id,
                    af_domain::OrganizationSsoProtocol::Oidc,
                    4,
                    &state_digest,
                    &envelope,
                )
                .is_err()
        );
        assert!(
            cipher
                .decrypt_organization_sso_login_material(
                    organization,
                    provider_id,
                    af_domain::OrganizationSsoProtocol::Oidc,
                    3,
                    &"b".repeat(64),
                    &envelope,
                )
                .is_err()
        );
        assert!(
            cipher
                .decrypt_organization_sso_login_material(
                    organization,
                    provider_id + 1,
                    af_domain::OrganizationSsoProtocol::Oidc,
                    3,
                    &state_digest,
                    &envelope,
                )
                .is_err()
        );
    }

    #[test]
    fn organization_sso_saml_request_material_is_bound_to_one_login_transaction() {
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let organization = af_domain::OrganizationId::new(7).unwrap();
        let provider_id = 19_i64;
        let material = crate::OrganizationSsoSamlLoginMaterial::new().unwrap();
        let request_id = material.request_id().to_owned();
        let secret = material.into_plain_system_secret().unwrap();
        let state_digest = "a".repeat(64);
        let envelope = cipher
            .encrypt_organization_sso_saml_login_material(
                organization,
                provider_id,
                3,
                &state_digest,
                &secret,
            )
            .unwrap();
        let decrypted = cipher
            .decrypt_organization_sso_saml_login_material(
                organization,
                provider_id,
                3,
                &state_digest,
                &envelope,
            )
            .unwrap();
        let restored =
            crate::OrganizationSsoSamlLoginMaterial::from_decrypted_system_secret(decrypted)
                .unwrap();
        assert_eq!(restored.request_id(), request_id);
        assert!(!format!("{restored:?}").contains(&request_id));
        assert!(
            cipher
                .decrypt_organization_sso_saml_login_material(
                    af_domain::OrganizationId::new(8).unwrap(),
                    provider_id,
                    3,
                    &state_digest,
                    &envelope,
                )
                .is_err()
        );
        assert!(
            cipher
                .decrypt_organization_sso_login_material(
                    organization,
                    provider_id,
                    af_domain::OrganizationSsoProtocol::Oidc,
                    3,
                    &state_digest,
                    &envelope,
                )
                .is_err()
        );
    }

    fn settings() -> CredentialEncryptionSettings {
        settings_with_key_id("primary-key")
    }

    fn settings_with_key_id(key_id: &str) -> CredentialEncryptionSettings {
        serde_json::from_value(json!({
            "key_id": key_id,
            "key": URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES])
        }))
        .unwrap()
    }
}
