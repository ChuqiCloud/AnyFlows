use std::{future::Future, pin::Pin, sync::Arc};

use af_admin::{
    ApiKeyDigest, PlaygroundAuthenticationFuture, TokenAuthenticationError,
    TokenAuthenticationFuture, TokenAuthenticator,
};
use af_db::{
    TokenRequestAdmissionOutcome, TokenRequestAdmissionRepository,
    TokenRequestAdmissionRepositoryError,
};
use af_domain::{TrustedClientIp, UserId};

type AdmissionFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<TokenRequestAdmissionOutcome, TokenRequestAdmissionRepositoryError>,
            > + Send
            + 'a,
    >,
>;

/// 令牌累计请求数准入的最小存储端口，便于运行时装饰器与数据库实现解耦。
pub(crate) trait TokenRequestAdmissionStore: Send + Sync {
    /// 原子占用令牌的一次累计请求额度。
    fn admit<'a>(&'a self, token_id: af_domain::TokenId) -> AdmissionFuture<'a>;
}

impl TokenRequestAdmissionStore for TokenRequestAdmissionRepository {
    fn admit<'a>(&'a self, token_id: af_domain::TokenId) -> AdmissionFuture<'a> {
        Box::pin(TokenRequestAdmissionRepository::admit(self, token_id))
    }
}

/// 在数据库令牌鉴权之后、RPM 之前执行累计请求数准入。
pub(crate) struct TokenRequestAdmissionAuthenticator {
    inner: Arc<dyn TokenAuthenticator>,
    store: Arc<dyn TokenRequestAdmissionStore>,
}

impl TokenRequestAdmissionAuthenticator {
    /// 绑定已完成基础鉴权的认证器与数据库准入存储。
    pub(crate) fn new(
        inner: Arc<dyn TokenAuthenticator>,
        store: Arc<dyn TokenRequestAdmissionStore>,
    ) -> Self {
        Self { inner, store }
    }
}

impl TokenAuthenticator for TokenRequestAdmissionAuthenticator {
    fn authenticate<'a>(
        &'a self,
        digest: &'a ApiKeyDigest,
        client_ip: TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a> {
        let store = Arc::clone(&self.store);
        Box::pin(async move {
            let authentication = self.inner.authenticate(digest, client_ip).await?;
            match store.admit(authentication.principal().token_id()).await {
                Ok(TokenRequestAdmissionOutcome::Admitted) => Ok(authentication),
                Ok(TokenRequestAdmissionOutcome::LimitReached) => {
                    Err(TokenAuthenticationError::RequestLimitReached)
                }
                Ok(TokenRequestAdmissionOutcome::Rejected) => {
                    Err(TokenAuthenticationError::InvalidApiKey)
                }
                Err(_) => {
                    tracing::error!(
                        target: "af_server::token_request_admission",
                        error_kind = "token_request_admission_store_error",
                        "令牌累计请求数准入存储调用失败"
                    );
                    Err(TokenAuthenticationError::Internal)
                }
            }
        })
    }

    fn authenticate_playground<'a>(
        &'a self,
        user_id: UserId,
    ) -> PlaygroundAuthenticationFuture<'a> {
        // 内部试炼场主体固定没有累计请求数上限，避免把会话调用误记为外部 Key 准入。
        self.inner.authenticate_playground(user_id)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use af_admin::{PresentedApiKey, TokenAuthentication};
    use af_domain::{GatewayPrincipal, GroupId, TokenId, TokenModelPolicy, UserId};

    use super::*;

    const TEST_KEY: &str = "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

    #[tokio::test]
    async fn admitted_request_returns_original_authentication() {
        let expected = authentication();
        let store = Arc::new(FixedStore::new(Ok(TokenRequestAdmissionOutcome::Admitted)));
        let runtime = TokenRequestAdmissionAuthenticator::new(
            Arc::new(FixedAuthenticator(expected.clone())),
            store.clone(),
        );

        assert_eq!(authenticate(&runtime).await, Ok(expected));
        assert_eq!(store.calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn limit_reached_is_a_closed_authentication_error() {
        let store = Arc::new(FixedStore::new(Ok(
            TokenRequestAdmissionOutcome::LimitReached,
        )));
        let runtime = TokenRequestAdmissionAuthenticator::new(
            Arc::new(FixedAuthenticator(authentication())),
            store,
        );

        assert_eq!(
            authenticate(&runtime).await,
            Err(TokenAuthenticationError::RequestLimitReached)
        );
    }

    #[tokio::test]
    async fn rejected_state_maps_to_invalid_key_and_store_failure_to_internal() {
        for (result, expected) in [
            (
                Ok(TokenRequestAdmissionOutcome::Rejected),
                TokenAuthenticationError::InvalidApiKey,
            ),
            (
                Err(TokenRequestAdmissionRepositoryError::Query),
                TokenAuthenticationError::Internal,
            ),
        ] {
            let runtime = TokenRequestAdmissionAuthenticator::new(
                Arc::new(FixedAuthenticator(authentication())),
                Arc::new(FixedStore::new(result)),
            );
            assert_eq!(authenticate(&runtime).await, Err(expected));
        }
    }

    #[tokio::test]
    async fn inner_authentication_failure_does_not_touch_admission_store() {
        let store = Arc::new(FixedStore::new(Ok(TokenRequestAdmissionOutcome::Admitted)));
        let runtime =
            TokenRequestAdmissionAuthenticator::new(Arc::new(FailingAuthenticator), store.clone());

        assert_eq!(
            authenticate(&runtime).await,
            Err(TokenAuthenticationError::InvalidApiKey)
        );
        assert_eq!(store.calls.load(Ordering::Relaxed), 0);
    }

    struct FixedAuthenticator(TokenAuthentication);

    impl TokenAuthenticator for FixedAuthenticator {
        fn authenticate<'a>(
            &'a self,
            _digest: &'a ApiKeyDigest,
            _client_ip: TrustedClientIp,
        ) -> TokenAuthenticationFuture<'a> {
            let authentication = self.0.clone();
            Box::pin(async move { Ok(authentication) })
        }
    }

    struct FailingAuthenticator;

    impl TokenAuthenticator for FailingAuthenticator {
        fn authenticate<'a>(
            &'a self,
            _digest: &'a ApiKeyDigest,
            _client_ip: TrustedClientIp,
        ) -> TokenAuthenticationFuture<'a> {
            Box::pin(async { Err(TokenAuthenticationError::InvalidApiKey) })
        }
    }

    struct FixedStore {
        result: Result<TokenRequestAdmissionOutcome, TokenRequestAdmissionRepositoryError>,
        calls: AtomicUsize,
    }

    impl FixedStore {
        fn new(
            result: Result<TokenRequestAdmissionOutcome, TokenRequestAdmissionRepositoryError>,
        ) -> Self {
            Self {
                result,
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl TokenRequestAdmissionStore for FixedStore {
        fn admit<'a>(&'a self, _token_id: TokenId) -> AdmissionFuture<'a> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let result = self.result;
            Box::pin(async move { result })
        }
    }

    async fn authenticate(
        runtime: &TokenRequestAdmissionAuthenticator,
    ) -> Result<TokenAuthentication, TokenAuthenticationError> {
        let presented = PresentedApiKey::parse(TEST_KEY).unwrap();
        let digest = presented.digest();
        drop(presented);
        runtime
            .authenticate(&digest, TrustedClientIp::new("192.0.2.1".parse().unwrap()))
            .await
    }

    fn authentication() -> TokenAuthentication {
        TokenAuthentication::new(
            GatewayPrincipal::new(
                TokenId::new(1).unwrap(),
                UserId::new(2).unwrap(),
                GroupId::new(3).unwrap(),
            ),
            TokenModelPolicy::unrestricted(),
        )
    }
}
