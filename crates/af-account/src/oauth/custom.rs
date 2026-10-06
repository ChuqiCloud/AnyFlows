use std::fmt;

use thiserror::Error;
use url::{Host, Url};

use super::authorization::validate_scope;

const MAX_SUBJECT_FIELD_BYTES: usize = 64;
const MAX_PROVIDER_KEY_BYTES: usize = 32;
const MAX_DISPLAY_NAME_BYTES: usize = 128;
const MAX_CLIENT_ID_BYTES: usize = 255;

/// 自定义 Provider 的稳定命名空间键；该键同时用于路由和密钥 AAD。
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct CustomOAuth2ProviderKey(String);

impl CustomOAuth2ProviderKey {
    /// 创建受限的 `custom_` 命名空间键，避免覆盖内置 Provider。
    pub fn new(value: String) -> Result<Self, CustomOAuth2ProviderKeyError> {
        let value = value.trim().to_owned();
        if value.len() <= "custom_".len()
            || value.len() > MAX_PROVIDER_KEY_BYTES
            || !value.starts_with("custom_")
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
            })
        {
            return Err(CustomOAuth2ProviderKeyError::Invalid);
        }
        Ok(Self(value))
    }

    /// 返回稳定键；调用方不得把用户输入拼接到端点路径或查询参数中。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CustomOAuth2ProviderKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CustomOAuth2ProviderKey")
            .field(&self.0)
            .finish()
    }
}

/// 自定义 Provider 的强类型静态配置；不包含 Client Secret 明文。
#[derive(Clone)]
pub struct CustomOAuth2ProviderConfig {
    provider_key: CustomOAuth2ProviderKey,
    display_name: String,
    client_id: String,
    endpoints: CustomOAuth2EndpointBundle,
    enabled: bool,
}

impl CustomOAuth2ProviderConfig {
    /// 校验管理配置；启用状态仍需由后续仓储结合密钥是否存在决定可用性。
    pub fn new(
        provider_key: String,
        display_name: String,
        client_id: String,
        endpoints: CustomOAuth2EndpointBundle,
        enabled: bool,
    ) -> Result<Self, CustomOAuth2ProviderConfigError> {
        let provider_key = CustomOAuth2ProviderKey::new(provider_key)
            .map_err(|_| CustomOAuth2ProviderConfigError::InvalidProviderKey)?;
        if display_name.is_empty()
            || display_name.len() > MAX_DISPLAY_NAME_BYTES
            || display_name.chars().any(char::is_control)
        {
            return Err(CustomOAuth2ProviderConfigError::InvalidDisplayName);
        }
        if client_id.is_empty()
            || client_id.len() > MAX_CLIENT_ID_BYTES
            || !client_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(CustomOAuth2ProviderConfigError::InvalidClientId);
        }
        Ok(Self {
            provider_key,
            display_name,
            client_id,
            endpoints,
            enabled,
        })
    }

    #[must_use]
    pub fn provider_key(&self) -> &CustomOAuth2ProviderKey {
        &self.provider_key
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    #[must_use]
    pub fn endpoints(&self) -> &CustomOAuth2EndpointBundle {
        &self.endpoints
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// 只有启用且已配置密钥时才允许被后续公开能力投影。
    #[must_use]
    pub const fn available(&self, secret_configured: bool) -> bool {
        self.enabled && secret_configured
    }
}

impl fmt::Debug for CustomOAuth2ProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomOAuth2ProviderConfig")
            .field("provider_key", &self.provider_key)
            .field("display_name", &"<已脱敏>")
            .field("client_id", &"<已脱敏>")
            .field("endpoints", &"<已脱敏>")
            .field("enabled", &self.enabled)
            .finish()
    }
}

/// 自定义 Provider key 校验错误；不回显用户输入。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CustomOAuth2ProviderKeyError {
    #[error("自定义 OAuth2 Provider key 无效")]
    Invalid,
}

/// 自定义 Provider 配置校验错误；不回显端点、Client ID 或显示名。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CustomOAuth2ProviderConfigError {
    #[error("自定义 OAuth2 Provider key 无效")]
    InvalidProviderKey,
    #[error("自定义 OAuth2 Provider 显示名无效")]
    InvalidDisplayName,
    #[error("自定义 OAuth2 Client ID 无效")]
    InvalidClientId,
}

/// 自定义 OAuth2 Provider 的静态端点配置；不执行网络请求，也不持有密钥。
#[derive(Clone)]
pub struct CustomOAuth2EndpointBundle {
    authorization_endpoint: Url,
    token_endpoint: Url,
    userinfo_endpoint: Url,
    scope: String,
    subject_field: String,
}

impl CustomOAuth2EndpointBundle {
    /// 校验 HTTPS 端点、同源约束、scope 和 UserInfo subject 字段。
    pub fn new(
        authorization_endpoint: String,
        token_endpoint: String,
        userinfo_endpoint: String,
        scope: String,
        subject_field: String,
    ) -> Result<Self, CustomOAuth2EndpointError> {
        let authorization_endpoint = parse_endpoint(
            &authorization_endpoint,
            CustomOAuth2EndpointError::InvalidAuthorizationEndpoint,
        )?;
        let token_endpoint = parse_endpoint(
            &token_endpoint,
            CustomOAuth2EndpointError::InvalidTokenEndpoint,
        )?;
        let userinfo_endpoint = parse_endpoint(
            &userinfo_endpoint,
            CustomOAuth2EndpointError::InvalidUserinfoEndpoint,
        )?;
        if !same_origin(&authorization_endpoint, &token_endpoint)
            || !same_origin(&authorization_endpoint, &userinfo_endpoint)
        {
            return Err(CustomOAuth2EndpointError::DifferentOrigins);
        }
        validate_scope(&scope).map_err(|_| CustomOAuth2EndpointError::InvalidScope)?;
        validate_subject_field(&subject_field)?;
        reject_reserved_authorization_query(&authorization_endpoint)?;

        Ok(Self {
            authorization_endpoint,
            token_endpoint,
            userinfo_endpoint,
            scope,
            subject_field,
        })
    }

    /// 返回已校验的授权端点；调用方只能追加固定 OAuth 参数。
    #[must_use]
    pub fn authorization_endpoint(&self) -> &Url {
        &self.authorization_endpoint
    }

    /// 返回已校验的令牌端点。
    #[must_use]
    pub fn token_endpoint(&self) -> &Url {
        &self.token_endpoint
    }

    /// 返回已校验的 UserInfo 端点。
    #[must_use]
    pub fn userinfo_endpoint(&self) -> &Url {
        &self.userinfo_endpoint
    }

    /// 返回固定 scope；不接受登录请求覆盖。
    #[must_use]
    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// 返回 UserInfo 中用于外部身份绑定的顶层字段名。
    #[must_use]
    pub fn subject_field(&self) -> &str {
        &self.subject_field
    }
}

impl fmt::Debug for CustomOAuth2EndpointBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomOAuth2EndpointBundle")
            .field("authorization_endpoint", &"<已脱敏>")
            .field("token_endpoint", &"<已脱敏>")
            .field("userinfo_endpoint", &"<已脱敏>")
            .field("scope", &"<已脱敏>")
            .field("subject_field", &"<已脱敏>")
            .finish()
    }
}

/// 自定义 OAuth2 端点配置错误；不回显 URL、scope 或字段内容。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CustomOAuth2EndpointError {
    #[error("自定义 OAuth2 授权端点无效")]
    InvalidAuthorizationEndpoint,
    #[error("自定义 OAuth2 令牌端点无效")]
    InvalidTokenEndpoint,
    #[error("自定义 OAuth2 UserInfo 端点无效")]
    InvalidUserinfoEndpoint,
    #[error("自定义 OAuth2 端点必须使用同一 origin")]
    DifferentOrigins,
    #[error("自定义 OAuth2 scope 无效")]
    InvalidScope,
    #[error("自定义 OAuth2 subject 字段无效")]
    InvalidSubjectField,
}

fn parse_endpoint(
    value: &str,
    invalid: CustomOAuth2EndpointError,
) -> Result<Url, CustomOAuth2EndpointError> {
    let url = Url::parse(value).map_err(|_| invalid)?;
    let Some(host) = url.host() else {
        return Err(invalid);
    };
    let Host::Domain(host) = host else {
        return Err(invalid);
    };
    if url.scheme() != "https"
        || host.is_empty()
        || !host.is_ascii()
        || host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
        || url.username() != ""
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
        || url.path().is_empty()
    {
        return Err(invalid);
    }
    Ok(url)
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left
            .host_str()
            .zip(right.host_str())
            .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        && left.port_or_known_default() == right.port_or_known_default()
}

fn validate_subject_field(value: &str) -> Result<(), CustomOAuth2EndpointError> {
    if value.is_empty()
        || value.len() > MAX_SUBJECT_FIELD_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(CustomOAuth2EndpointError::InvalidSubjectField);
    }
    Ok(())
}

fn reject_reserved_authorization_query(endpoint: &Url) -> Result<(), CustomOAuth2EndpointError> {
    const RESERVED: [&str; 7] = [
        "response_type",
        "client_id",
        "redirect_uri",
        "scope",
        "state",
        "code_challenge",
        "code_challenge_method",
    ];
    if endpoint
        .query_pairs()
        .any(|(name, _)| RESERVED.contains(&name.as_ref()))
    {
        return Err(CustomOAuth2EndpointError::InvalidAuthorizationEndpoint);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTHORIZATION: &str = "https://login.example.test/oauth/authorize";
    const TOKEN: &str = "https://login.example.test/oauth/token";
    const USERINFO: &str = "https://login.example.test/oauth/userinfo";
    const SCOPE: &str = "openid profile";

    fn bundle(
        authorization_endpoint: &str,
        token_endpoint: &str,
        userinfo_endpoint: &str,
        scope: &str,
        subject_field: &str,
    ) -> Result<CustomOAuth2EndpointBundle, CustomOAuth2EndpointError> {
        CustomOAuth2EndpointBundle::new(
            authorization_endpoint.to_owned(),
            token_endpoint.to_owned(),
            userinfo_endpoint.to_owned(),
            scope.to_owned(),
            subject_field.to_owned(),
        )
    }

    #[test]
    fn accepts_same_origin_https_endpoints_and_exposes_only_fixed_values() {
        let bundle = bundle(AUTHORIZATION, TOKEN, USERINFO, SCOPE, "sub").unwrap();
        assert_eq!(bundle.authorization_endpoint().as_str(), AUTHORIZATION);
        assert_eq!(bundle.token_endpoint().as_str(), TOKEN);
        assert_eq!(bundle.userinfo_endpoint().as_str(), USERINFO);
        assert_eq!(bundle.scope(), SCOPE);
        assert_eq!(bundle.subject_field(), "sub");
    }

    #[test]
    fn rejects_non_https_local_and_ip_literal_endpoints() {
        for endpoint in [
            "http://login.example.test/oauth/authorize",
            "https://localhost/oauth/authorize",
            "https://login.localhost/oauth/authorize",
            "https://127.0.0.1/oauth/authorize",
            "https://[::1]/oauth/authorize",
            "https://user:pass@login.example.test/oauth/authorize",
            "https://login.example.test/oauth/authorize#fragment",
            "https://login.example.test/oauth/authorize?tenant=one",
        ] {
            assert_eq!(
                bundle(endpoint, TOKEN, USERINFO, SCOPE, "sub").unwrap_err(),
                CustomOAuth2EndpointError::InvalidAuthorizationEndpoint,
                "意外放行 {endpoint}"
            );
        }
    }

    #[test]
    fn rejects_cross_origin_endpoints_and_reserved_authorization_query() {
        assert_eq!(
            bundle(
                AUTHORIZATION,
                "https://token.example.test/oauth/token",
                USERINFO,
                SCOPE,
                "sub"
            )
            .unwrap_err(),
            CustomOAuth2EndpointError::DifferentOrigins
        );
        assert_eq!(
            bundle(
                "https://login.example.test/oauth/authorize?state=attacker",
                TOKEN,
                USERINFO,
                SCOPE,
                "sub"
            )
            .unwrap_err(),
            CustomOAuth2EndpointError::InvalidAuthorizationEndpoint
        );
    }

    #[test]
    fn rejects_invalid_scope_and_subject_field() {
        assert_eq!(
            bundle(AUTHORIZATION, TOKEN, USERINFO, "openid  profile", "sub").unwrap_err(),
            CustomOAuth2EndpointError::InvalidScope
        );
        for field in ["", "sub.field", "sub field", "sub/field", "é"] {
            assert_eq!(
                bundle(AUTHORIZATION, TOKEN, USERINFO, SCOPE, field).unwrap_err(),
                CustomOAuth2EndpointError::InvalidSubjectField,
                "意外放行 {field:?}"
            );
        }
    }

    #[test]
    fn debug_output_does_not_expose_configuration() {
        let bundle = bundle(
            "https://login.example.test/oauth/authorize/secret-tenant",
            TOKEN,
            USERINFO,
            "openid profile secret-scope",
            "secret-subject-field",
        )
        .unwrap();
        let debug = format!("{bundle:?}");
        for private in [
            "login.example.test",
            "secret-tenant",
            "secret-scope",
            "secret-subject-field",
        ] {
            assert!(!debug.contains(private), "Debug 泄露了 {private}");
        }
    }

    #[test]
    fn provider_key_is_limited_to_custom_namespace() {
        assert_eq!(
            CustomOAuth2ProviderKey::new("github".to_owned()).unwrap_err(),
            CustomOAuth2ProviderKeyError::Invalid
        );
        assert_eq!(
            CustomOAuth2ProviderKey::new("custom_GitHub".to_owned()).unwrap_err(),
            CustomOAuth2ProviderKeyError::Invalid
        );
        let key = CustomOAuth2ProviderKey::new("custom_github-main".to_owned()).unwrap();
        assert_eq!(key.as_str(), "custom_github-main");
    }

    #[test]
    fn provider_config_requires_safe_display_name_and_client_id() {
        let endpoints = bundle(AUTHORIZATION, TOKEN, USERINFO, SCOPE, "sub").unwrap();
        let config = CustomOAuth2ProviderConfig::new(
            "custom_github".to_owned(),
            "企业登录".to_owned(),
            "client-id_1".to_owned(),
            endpoints.clone(),
            false,
        )
        .unwrap();
        assert_eq!(config.provider_key().as_str(), "custom_github");
        assert_eq!(config.display_name(), "企业登录");
        assert!(!config.available(true));
        assert!(!config.available(false));

        assert_eq!(
            CustomOAuth2ProviderConfig::new(
                "custom_github".to_owned(),
                "name\n".to_owned(),
                "client-id".to_owned(),
                endpoints.clone(),
                true,
            )
            .unwrap_err(),
            CustomOAuth2ProviderConfigError::InvalidDisplayName
        );
        assert_eq!(
            CustomOAuth2ProviderConfig::new(
                "custom_github".to_owned(),
                "name".to_owned(),
                "client/id".to_owned(),
                endpoints,
                true,
            )
            .unwrap_err(),
            CustomOAuth2ProviderConfigError::InvalidClientId
        );
    }

    #[test]
    fn provider_config_debug_does_not_expose_endpoint_or_client_id() {
        let endpoints = bundle(
            "https://login.example.test/tenant-secret/authorize",
            TOKEN,
            USERINFO,
            "openid profile",
            "sub",
        )
        .unwrap();
        let config = CustomOAuth2ProviderConfig::new(
            "custom_secret".to_owned(),
            "Secret Provider".to_owned(),
            "client-secret-id".to_owned(),
            endpoints,
            true,
        )
        .unwrap();
        let debug = format!("{config:?}");
        for private in ["login.example.test", "tenant-secret", "client-secret-id"] {
            assert!(!debug.contains(private), "Debug 泄露了 {private}");
        }
    }
}
