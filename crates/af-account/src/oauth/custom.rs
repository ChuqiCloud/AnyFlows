use std::fmt;

use thiserror::Error;
use url::{Host, Url};

use super::authorization::validate_scope;

const MAX_SUBJECT_FIELD_BYTES: usize = 64;

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
            "https://login.example.test/oauth/authorize?tenant=secret-tenant",
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
}
