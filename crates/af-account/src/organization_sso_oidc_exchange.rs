//! 企业 OIDC token exchange 的受控配置与输入契约。
//!
//! 本模块只负责在协议适配器边界校验配置和一次性材料，不发起网络请求，
//! 不持久化授权码、PKCE verifier 或 Token，也不负责身份绑定。

use std::{fmt, net::IpAddr};

use thiserror::Error;
use url::Url;
use zeroize::Zeroizing;

use crate::ORGANIZATION_SSO_OIDC_CALLBACK_PATH;

const MAX_ENDPOINT_BYTES: usize = 2_048;
const MAX_CLIENT_ID_BYTES: usize = 255;
const MAX_CODE_BYTES: usize = 4_096;
const MAX_CODE_VERIFIER_BYTES: usize = 512;

/// OIDC Provider 版本绑定的 token exchange 配置快照。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoOidcTokenExchangeConfig {
    provider_version: i64,
    issuer: Url,
    token_endpoint: Url,
    client_id: String,
    audience: String,
    redirect_uri: Url,
}

impl OrganizationSsoOidcTokenExchangeConfig {
    /// 创建不可变配置；回调地址必须指向固定企业 OIDC 回调路径。
    pub fn new(
        provider_version: i64,
        issuer: String,
        token_endpoint: String,
        client_id: String,
        audience: String,
        redirect_uri: String,
    ) -> Result<Self, OrganizationSsoOidcTokenExchangeError> {
        if provider_version <= 0 {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
        }
        let issuer = parse_https_url(&issuer)?;
        let token_endpoint = parse_https_url(&token_endpoint)?;
        let redirect_uri = parse_https_url(&redirect_uri)?;
        if redirect_uri.path() != ORGANIZATION_SSO_OIDC_CALLBACK_PATH
            || redirect_uri.query().is_some()
            || redirect_uri.fragment().is_some()
        {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
        }
        if !valid_ascii(&client_id, MAX_CLIENT_ID_BYTES)
            || !valid_ascii(&audience, MAX_CLIENT_ID_BYTES)
        {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
        }
        Ok(Self {
            provider_version,
            issuer,
            token_endpoint,
            client_id,
            audience,
            redirect_uri,
        })
    }

    #[must_use]
    pub const fn provider_version(&self) -> i64 {
        self.provider_version
    }

    #[must_use]
    pub fn issuer(&self) -> &Url {
        &self.issuer
    }

    #[must_use]
    pub fn token_endpoint(&self) -> &Url {
        &self.token_endpoint
    }

    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    #[must_use]
    pub fn audience(&self) -> &str {
        &self.audience
    }

    #[must_use]
    pub fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }
}

impl fmt::Debug for OrganizationSsoOidcTokenExchangeConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoOidcTokenExchangeConfig")
            .field("provider_version", &self.provider_version)
            .field("issuer", &self.issuer)
            .field("token_endpoint", &self.token_endpoint)
            .field("client_id", &"<redacted>")
            .field("audience", &"<redacted>")
            .field("redirect_uri", &self.redirect_uri)
            .finish()
    }
}

/// 单次 token exchange 所需的授权码和 PKCE verifier。
#[derive(Eq, PartialEq)]
pub struct OrganizationSsoOidcTokenExchangeInput {
    code: Zeroizing<String>,
    code_verifier: Zeroizing<String>,
}

impl OrganizationSsoOidcTokenExchangeInput {
    /// 校验并接管只在本次交换中使用的敏感材料。
    pub fn new(
        code: String,
        code_verifier: String,
    ) -> Result<Self, OrganizationSsoOidcTokenExchangeError> {
        if !valid_ascii(&code, MAX_CODE_BYTES)
            || !valid_ascii(&code_verifier, MAX_CODE_VERIFIER_BYTES)
        {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidInput);
        }
        Ok(Self {
            code: Zeroizing::new(code),
            code_verifier: Zeroizing::new(code_verifier),
        })
    }

    #[must_use]
    pub fn code(&self) -> &str {
        self.code.as_str()
    }

    #[must_use]
    pub fn code_verifier(&self) -> &str {
        self.code_verifier.as_str()
    }
}

impl fmt::Debug for OrganizationSsoOidcTokenExchangeInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoOidcTokenExchangeInput")
            .field("code", &"<redacted>")
            .field("code_verifier", &"<redacted>")
            .finish()
    }
}

/// 配置与交换输入的稳定拒绝分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoOidcTokenExchangeError {
    #[error("企业 OIDC token exchange 配置无效")]
    InvalidConfiguration,
    #[error("企业 OIDC token exchange 输入无效")]
    InvalidInput,
}

fn parse_https_url(value: &str) -> Result<Url, OrganizationSsoOidcTokenExchangeError> {
    if value.is_empty() || value.len() > MAX_ENDPOINT_BYTES || value.trim() != value {
        return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
    }
    let url = Url::parse(value)
        .map_err(|_| OrganizationSsoOidcTokenExchangeError::InvalidConfiguration)?;
    if url.as_str() != value
        || url.scheme() != "https"
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || blocked_endpoint_host(url.host_str().unwrap_or_default())
    {
        return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
    }
    Ok(url)
}

fn blocked_endpoint_host(host: &str) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    normalized == "localhost"
        || normalized.ends_with(".localhost")
        || normalized == "localhost.localdomain"
        || normalized.parse::<IpAddr>().is_ok()
}

fn valid_ascii(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && value.bytes().all(|byte| byte.is_ascii_graphic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> OrganizationSsoOidcTokenExchangeConfig {
        OrganizationSsoOidcTokenExchangeConfig::new(
            7,
            "https://idp.example.test/".to_owned(),
            "https://idp.example.test/oauth2/token".to_owned(),
            "client-id".to_owned(),
            "client-id".to_owned(),
            "https://app.example.test/api/auth/organization-sso/oidc/callback".to_owned(),
        )
        .unwrap()
    }

    #[test]
    fn config_binds_version_and_fixed_callback_path() {
        let value = config();
        assert_eq!(value.provider_version(), 7);
        assert_eq!(value.client_id(), "client-id");
        assert!(
            OrganizationSsoOidcTokenExchangeConfig::new(
                7,
                "https://idp.example.test".to_owned(),
                "https://idp.example.test/oauth2/token?unsafe=1".to_owned(),
                "client-id".to_owned(),
                "client-id".to_owned(),
                "https://app.example.test/api/auth/organization-sso/oidc/callback".to_owned(),
            )
            .is_err()
        );
        assert!(
            OrganizationSsoOidcTokenExchangeConfig::new(
                7,
                "https://idp.example.test".to_owned(),
                "https://127.0.0.1/oauth2/token".to_owned(),
                "client-id".to_owned(),
                "client-id".to_owned(),
                "https://app.example.test/api/auth/organization-sso/oidc/callback".to_owned(),
            )
            .is_err()
        );
        assert!(
            OrganizationSsoOidcTokenExchangeConfig::new(
                7,
                "https://idp.example.test".to_owned(),
                "https://idp.example.test/oauth2/token".to_owned(),
                "client-id".to_owned(),
                "client-id".to_owned(),
                "https://app.example.test/unsafe-callback".to_owned(),
            )
            .is_err()
        );
    }

    #[test]
    fn input_redacts_sensitive_material() {
        let input = OrganizationSsoOidcTokenExchangeInput::new(
            "authorization-code".to_owned(),
            "pkce-verifier".to_owned(),
        )
        .unwrap();
        assert_eq!(input.code(), "authorization-code");
        assert_eq!(input.code_verifier(), "pkce-verifier");
        let debug = format!("{input:?}");
        assert!(!debug.contains("authorization-code"));
        assert!(!debug.contains("pkce-verifier"));
    }
}
