use std::fmt;

use af_httpclient::PooledClient;
use thiserror::Error;
use zeroize::Zeroize as _;

use super::{
    OAuthAuthorizationContext, OAuthAuthorizationError, OAuthAuthorizationGrant,
    OAuthAuthorizationRequest, OAuthClientAuthenticationMethod, OAuthTokenExchange,
    OAuthTokenExchangeError, OAuthTokenRequestEncoding, OAuthTokenSet, UpstreamOAuthProvider,
    authorization::validate_authorization_profile,
};

const CODEX_AUTHORIZATION_ENDPOINT: &str = "https://auth.openai.com/oauth/authorize";
const CODEX_TOKEN_ENDPOINT: &str = "https://auth.openai.com/oauth/token";
const CODEX_SCOPE: &str = "openid email profile offline_access";
const CODEX_AUTHORIZATION_PARAMETERS: &[(&str, &str)] = &[
    ("prompt", "login"),
    ("id_token_add_organizations", "true"),
    ("codex_cli_simplified_flow", "true"),
];

const GOOGLE_AUTHORIZATION_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_AUTHORIZATION_PARAMETERS: &[(&str, &str)] = &[
    ("access_type", "offline"),
    ("prompt", "consent"),
    ("include_granted_scopes", "true"),
];
const GEMINI_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";
const ANTIGRAVITY_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs";

/// 单个上游 OAuth provider 的不可变协议配置。
pub struct OAuthProviderProfile {
    provider: UpstreamOAuthProvider,
    client_id: String,
    authorization_endpoint: &'static str,
    scope: &'static str,
    authorization_parameters: &'static [(&'static str, &'static str)],
    requires_refresh_token: bool,
    requires_expiration: bool,
    token_exchange: OAuthTokenExchange,
}

impl OAuthProviderProfile {
    /// 使用代码内白名单端点、scope、附加参数与交换编码创建 provider profile。
    pub fn new(
        provider: UpstreamOAuthProvider,
        client_id: String,
        mut client_secret: Option<String>,
    ) -> Result<Self, OAuthProviderProfileError> {
        let specification = match ProviderSpecification::for_provider(provider) {
            Ok(specification) => specification,
            Err(error) => {
                if let Some(secret) = client_secret.as_mut() {
                    secret.zeroize();
                }
                return Err(error);
            }
        };
        let token_exchange = OAuthTokenExchange::new_with_request_encoding(
            provider,
            client_id.clone(),
            specification.token_endpoint.to_owned(),
            specification.authentication_method,
            client_secret,
            specification.request_encoding,
        )?;
        // token 交换器接管密钥后再验证授权侧，后续错误会由交换器 Drop 清零密钥。
        validate_authorization_profile(
            &client_id,
            specification.authorization_endpoint,
            specification.scope,
            specification.authorization_parameters,
        )?;
        Ok(Self {
            provider,
            client_id,
            authorization_endpoint: specification.authorization_endpoint,
            scope: specification.scope,
            authorization_parameters: specification.authorization_parameters,
            requires_refresh_token: specification.requires_refresh_token,
            requires_expiration: specification.requires_expiration,
            token_exchange,
        })
    }

    /// 返回该 profile 对应的稳定 provider。
    #[must_use]
    pub const fn provider(&self) -> UpstreamOAuthProvider {
        self.provider
    }

    /// 返回固化的客户端认证方式。
    #[must_use]
    pub const fn authentication_method(&self) -> OAuthClientAuthenticationMethod {
        self.token_exchange.authentication_method()
    }

    /// 返回固化的 token 请求编码。
    #[must_use]
    pub const fn token_request_encoding(&self) -> OAuthTokenRequestEncoding {
        self.token_exchange.request_encoding()
    }

    /// 为一次业务授权创建只含 profile 白名单参数的请求。
    pub fn authorization_request(
        &self,
        context: OAuthAuthorizationContext,
        redirect_uri: String,
    ) -> Result<OAuthAuthorizationRequest, OAuthAuthorizationError> {
        OAuthAuthorizationRequest::new_with_authorization_parameters(
            self.provider,
            context,
            self.client_id.clone(),
            self.authorization_endpoint.to_owned(),
            redirect_uri,
            self.scope.to_owned(),
            self.authorization_parameters,
        )
    }

    pub(crate) async fn exchange(
        &self,
        http_client: &PooledClient,
        grant: OAuthAuthorizationGrant,
    ) -> Result<OAuthTokenSet, OAuthTokenExchangeError> {
        let token_set = self.token_exchange.exchange(http_client, grant).await?;
        self.validate_initial_token_set(&token_set)?;
        Ok(token_set)
    }

    fn validate_initial_token_set(
        &self,
        token_set: &OAuthTokenSet,
    ) -> Result<(), OAuthTokenExchangeError> {
        if (self.requires_refresh_token && token_set.refresh_token().is_none())
            || (self.requires_expiration && token_set.expires_in().is_none())
        {
            return Err(OAuthTokenExchangeError::InvalidTokenResponse);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_token_endpoint_for_test(&mut self, endpoint: url::Url) {
        self.token_exchange.set_token_endpoint_for_test(endpoint);
    }

    #[cfg(test)]
    fn token_endpoint_for_test(&self) -> &url::Url {
        self.token_exchange.token_endpoint_for_test()
    }
}

impl fmt::Debug for OAuthProviderProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthProviderProfile")
            .field("provider", &self.provider)
            .field("client_id", &"<已脱敏>")
            .field("authorization_endpoint", &"<已脱敏>")
            .field("scope", &"<已脱敏>")
            .field(
                "authorization_parameter_count",
                &self.authorization_parameters.len(),
            )
            .field("token_exchange", &self.token_exchange)
            .finish()
    }
}

/// provider profile 配置错误；不回显客户端标识、密钥、端点或 scope。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthProviderProfileError {
    /// 当前 provider 的端点或回调合约尚未形成可验证的闭合配置。
    #[error("OAuth provider 合约尚未启用：{provider}")]
    ProviderContractUnavailable { provider: UpstreamOAuthProvider },
    /// 授权端点、客户端标识、scope 或白名单参数违反闭合约束。
    #[error("OAuth provider 授权配置无效")]
    Authorization(#[from] OAuthAuthorizationError),
    /// token endpoint、客户端认证或请求编码配置无效。
    #[error("OAuth provider token 配置无效")]
    TokenExchange(#[from] OAuthTokenExchangeError),
}

struct ProviderSpecification {
    authorization_endpoint: &'static str,
    token_endpoint: &'static str,
    scope: &'static str,
    authorization_parameters: &'static [(&'static str, &'static str)],
    authentication_method: OAuthClientAuthenticationMethod,
    request_encoding: OAuthTokenRequestEncoding,
    requires_refresh_token: bool,
    requires_expiration: bool,
}

impl ProviderSpecification {
    const fn for_provider(
        provider: UpstreamOAuthProvider,
    ) -> Result<Self, OAuthProviderProfileError> {
        match provider {
            // Claude Code 的公开参考对 token endpoint 与 redirect 合约存在冲突，
            // 在取得可验证契约前只保留 state + JSON 底层能力，不猜测激活 profile。
            UpstreamOAuthProvider::ClaudeCode => {
                Err(OAuthProviderProfileError::ProviderContractUnavailable { provider })
            }
            UpstreamOAuthProvider::Codex => Ok(Self {
                authorization_endpoint: CODEX_AUTHORIZATION_ENDPOINT,
                token_endpoint: CODEX_TOKEN_ENDPOINT,
                scope: CODEX_SCOPE,
                authorization_parameters: CODEX_AUTHORIZATION_PARAMETERS,
                authentication_method: OAuthClientAuthenticationMethod::Public,
                request_encoding: OAuthTokenRequestEncoding::Form,
                requires_refresh_token: true,
                requires_expiration: true,
            }),
            UpstreamOAuthProvider::Gemini => Ok(Self {
                authorization_endpoint: GOOGLE_AUTHORIZATION_ENDPOINT,
                token_endpoint: GOOGLE_TOKEN_ENDPOINT,
                scope: GEMINI_SCOPE,
                authorization_parameters: GOOGLE_AUTHORIZATION_PARAMETERS,
                authentication_method: OAuthClientAuthenticationMethod::ClientSecretPost,
                request_encoding: OAuthTokenRequestEncoding::Form,
                requires_refresh_token: true,
                requires_expiration: true,
            }),
            UpstreamOAuthProvider::Antigravity => Ok(Self {
                authorization_endpoint: GOOGLE_AUTHORIZATION_ENDPOINT,
                token_endpoint: GOOGLE_TOKEN_ENDPOINT,
                scope: ANTIGRAVITY_SCOPE,
                authorization_parameters: GOOGLE_AUTHORIZATION_PARAMETERS,
                authentication_method: OAuthClientAuthenticationMethod::ClientSecretPost,
                request_encoding: OAuthTokenRequestEncoding::Form,
                requires_refresh_token: true,
                requires_expiration: true,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use af_domain::{ChannelId, CredentialId, UserId};

    use super::*;
    use crate::oauth::OAuthAuthorizationSessionStore;

    const REDIRECT_URI: &str = "http://127.0.0.1:43123/oauth/callback";
    const EXPECTED_CODEX_PARAMETERS: &[(&str, &str)] = &[
        ("prompt", "login"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
    ];
    const EXPECTED_GOOGLE_PARAMETERS: &[(&str, &str)] = &[
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("include_granted_scopes", "true"),
    ];

    #[test]
    fn provider_profiles_emit_only_their_closed_authorization_contracts() {
        for expectation in expectations() {
            let profile = profile(expectation.provider);
            assert_eq!(profile.provider(), expectation.provider);
            assert_eq!(
                profile.authentication_method(),
                expectation.authentication_method
            );
            assert_eq!(profile.token_request_encoding(), expectation.encoding);

            let request = profile
                .authorization_request(context(), REDIRECT_URI.to_owned())
                .unwrap();
            let start = OAuthAuthorizationSessionStore::with_defaults()
                .begin(&request)
                .unwrap();
            let url = start.authorization_url();
            assert_eq!(url.host_str(), Some(expectation.host));
            assert_eq!(url.path(), expectation.path);
            assert_eq!(
                profile.token_endpoint_for_test().host_str(),
                Some(expectation.token_host)
            );
            assert_eq!(
                profile.token_endpoint_for_test().path(),
                expectation.token_path
            );
            let pairs = url.query_pairs().into_owned().collect::<Vec<_>>();
            let values = pairs.iter().cloned().collect::<HashMap<_, _>>();
            assert_eq!(pairs.len(), values.len(), "授权参数不得重复");
            assert_eq!(
                values.get("client_id").map(String::as_str),
                Some("client-marker")
            );
            assert_eq!(
                values.get("redirect_uri").map(String::as_str),
                Some(REDIRECT_URI)
            );
            assert_eq!(
                values.get("scope").map(String::as_str),
                Some(expectation.scope)
            );
            assert_eq!(
                values.get("response_type").map(String::as_str),
                Some("code")
            );
            assert_eq!(
                values.get("code_challenge_method").map(String::as_str),
                Some("S256")
            );
            assert!(values.get("state").is_some_and(|value| value.len() == 43));
            assert!(
                values
                    .get("code_challenge")
                    .is_some_and(|value| value.len() == 43)
            );
            assert!(!values.contains_key("client_secret"));
            for (name, expected) in expectation.additional_parameters {
                assert_eq!(
                    values.get(*name).map(String::as_str),
                    Some(*expected),
                    "provider 白名单参数不匹配"
                );
            }
            assert_eq!(
                values.len(),
                7 + expectation.additional_parameters.len(),
                "授权 URL 不得出现 profile 之外的参数"
            );

            let debug = format!("{profile:?}{request:?}");
            for private in [
                "client-marker",
                expectation.host,
                expectation.scope,
                "client-secret-marker",
            ] {
                assert!(!debug.contains(private));
            }
        }
    }

    #[test]
    fn profile_client_authentication_requirements_are_closed() {
        assert!(
            OAuthProviderProfile::new(
                UpstreamOAuthProvider::Codex,
                "client".to_owned(),
                Some("unexpected-secret".to_owned()),
            )
            .is_err()
        );
        for provider in [
            UpstreamOAuthProvider::Gemini,
            UpstreamOAuthProvider::Antigravity,
        ] {
            assert!(OAuthProviderProfile::new(provider, "client".to_owned(), None).is_err());
        }
        assert_eq!(
            OAuthProviderProfile::new(
                UpstreamOAuthProvider::ClaudeCode,
                "client".to_owned(),
                None,
            )
            .unwrap_err(),
            OAuthProviderProfileError::ProviderContractUnavailable {
                provider: UpstreamOAuthProvider::ClaudeCode
            }
        );
    }

    #[test]
    fn initial_offline_connections_require_refresh_token_and_expiration() {
        for provider in [
            UpstreamOAuthProvider::Codex,
            UpstreamOAuthProvider::Gemini,
            UpstreamOAuthProvider::Antigravity,
        ] {
            let profile = profile(provider);
            let missing_refresh = OAuthTokenSet::for_test(
                provider,
                context(),
                "access-token-marker".to_owned(),
                None,
                Some(Duration::from_secs(3_600)),
                None,
            );
            assert_eq!(
                profile
                    .validate_initial_token_set(&missing_refresh)
                    .unwrap_err(),
                OAuthTokenExchangeError::InvalidTokenResponse
            );
            let missing_expiration = OAuthTokenSet::for_test(
                provider,
                context(),
                "access-token-marker".to_owned(),
                Some("refresh-token-marker".to_owned()),
                None,
                None,
            );
            assert_eq!(
                profile
                    .validate_initial_token_set(&missing_expiration)
                    .unwrap_err(),
                OAuthTokenExchangeError::InvalidTokenResponse
            );
            let complete = OAuthTokenSet::for_test(
                provider,
                context(),
                "access-token-marker".to_owned(),
                Some("refresh-token-marker".to_owned()),
                Some(Duration::from_secs(3_600)),
                None,
            );
            profile.validate_initial_token_set(&complete).unwrap();
        }
    }

    fn profile(provider: UpstreamOAuthProvider) -> OAuthProviderProfile {
        let secret = matches!(
            provider,
            UpstreamOAuthProvider::Gemini | UpstreamOAuthProvider::Antigravity
        )
        .then(|| "client-secret-marker".to_owned());
        OAuthProviderProfile::new(provider, "client-marker".to_owned(), secret).unwrap()
    }

    fn context() -> OAuthAuthorizationContext {
        OAuthAuthorizationContext::new(
            UserId::new(11).unwrap(),
            Some(ChannelId::new(22).unwrap()),
            Some(CredentialId::new(33).unwrap()),
        )
        .unwrap()
    }

    struct ProfileExpectation {
        provider: UpstreamOAuthProvider,
        host: &'static str,
        path: &'static str,
        token_host: &'static str,
        token_path: &'static str,
        scope: &'static str,
        additional_parameters: &'static [(&'static str, &'static str)],
        authentication_method: OAuthClientAuthenticationMethod,
        encoding: OAuthTokenRequestEncoding,
    }

    fn expectations() -> [ProfileExpectation; 3] {
        [
            ProfileExpectation {
                provider: UpstreamOAuthProvider::Codex,
                host: "auth.openai.com",
                path: "/oauth/authorize",
                token_host: "auth.openai.com",
                token_path: "/oauth/token",
                scope: "openid email profile offline_access",
                additional_parameters: EXPECTED_CODEX_PARAMETERS,
                authentication_method: OAuthClientAuthenticationMethod::Public,
                encoding: OAuthTokenRequestEncoding::Form,
            },
            ProfileExpectation {
                provider: UpstreamOAuthProvider::Gemini,
                host: "accounts.google.com",
                path: "/o/oauth2/v2/auth",
                token_host: "oauth2.googleapis.com",
                token_path: "/token",
                scope: "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile",
                additional_parameters: EXPECTED_GOOGLE_PARAMETERS,
                authentication_method: OAuthClientAuthenticationMethod::ClientSecretPost,
                encoding: OAuthTokenRequestEncoding::Form,
            },
            ProfileExpectation {
                provider: UpstreamOAuthProvider::Antigravity,
                host: "accounts.google.com",
                path: "/o/oauth2/v2/auth",
                token_host: "oauth2.googleapis.com",
                token_path: "/token",
                scope: "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs",
                additional_parameters: EXPECTED_GOOGLE_PARAMETERS,
                authentication_method: OAuthClientAuthenticationMethod::ClientSecretPost,
                encoding: OAuthTokenRequestEncoding::Form,
            },
        ]
    }
}
