use super::{OAuthEndpointErrorCode, OAuthTokenExchangeError};

/// OAuth 刷新失败允许影响凭据运行状态的闭合分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthRefreshFailureKind {
    /// 上游或网络暂时不可用，凭据进入有限冷却后允许重试。
    Transient,
    /// token endpoint 明确返回 `invalid_grant`，凭据应自动停用。
    Revoked,
}

/// 仅依据结构化上游事实分类；本地配置与请求构造错误不得惩罚凭据。
pub(super) const fn classify_refresh_failure(
    error: OAuthTokenExchangeError,
) -> Option<OAuthRefreshFailureKind> {
    match error {
        OAuthTokenExchangeError::EndpointRejected {
            code: Some(OAuthEndpointErrorCode::InvalidGrant),
            ..
        } => Some(OAuthRefreshFailureKind::Revoked),
        OAuthTokenExchangeError::Transport
        | OAuthTokenExchangeError::EndpointRejected { .. }
        | OAuthTokenExchangeError::ResponseTooLarge
        | OAuthTokenExchangeError::MalformedResponse
        | OAuthTokenExchangeError::InvalidTokenResponse => Some(OAuthRefreshFailureKind::Transient),
        OAuthTokenExchangeError::InvalidClientId
        | OAuthTokenExchangeError::InvalidTokenEndpoint
        | OAuthTokenExchangeError::InvalidClientAuthentication
        | OAuthTokenExchangeError::InvalidRefreshRequest
        | OAuthTokenExchangeError::ProviderMismatch
        | OAuthTokenExchangeError::RequestTooLarge
        | OAuthTokenExchangeError::RequestEncoding => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_structured_invalid_grant_is_permanent() {
        assert_eq!(
            classify_refresh_failure(OAuthTokenExchangeError::EndpointRejected {
                status: 400,
                code: Some(OAuthEndpointErrorCode::InvalidGrant),
            }),
            Some(OAuthRefreshFailureKind::Revoked)
        );
        for error in [
            OAuthTokenExchangeError::EndpointRejected {
                status: 401,
                code: None,
            },
            OAuthTokenExchangeError::EndpointRejected {
                status: 403,
                code: Some(OAuthEndpointErrorCode::Other),
            },
            OAuthTokenExchangeError::EndpointRejected {
                status: 429,
                code: Some(OAuthEndpointErrorCode::InvalidClient),
            },
            OAuthTokenExchangeError::EndpointRejected {
                status: 503,
                code: None,
            },
            OAuthTokenExchangeError::Transport,
            OAuthTokenExchangeError::ResponseTooLarge,
            OAuthTokenExchangeError::MalformedResponse,
            OAuthTokenExchangeError::InvalidTokenResponse,
        ] {
            assert_eq!(
                classify_refresh_failure(error),
                Some(OAuthRefreshFailureKind::Transient),
                "{error:?} 必须保留恢复窗口"
            );
        }
    }

    #[test]
    fn local_configuration_and_invariant_errors_do_not_penalize_credentials() {
        for error in [
            OAuthTokenExchangeError::InvalidClientId,
            OAuthTokenExchangeError::InvalidTokenEndpoint,
            OAuthTokenExchangeError::InvalidClientAuthentication,
            OAuthTokenExchangeError::InvalidRefreshRequest,
            OAuthTokenExchangeError::ProviderMismatch,
            OAuthTokenExchangeError::RequestTooLarge,
            OAuthTokenExchangeError::RequestEncoding,
        ] {
            assert_eq!(
                classify_refresh_failure(error),
                None,
                "{error:?} 不得改变凭据状态"
            );
        }
    }
}
