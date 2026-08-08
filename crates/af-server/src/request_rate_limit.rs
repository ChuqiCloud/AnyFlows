use std::{future::Future, num::NonZeroU32, pin::Pin, sync::Arc, time::Duration};

use af_admin::{
    ApiKeyDigest, TokenAuthenticationError, TokenAuthenticationFuture, TokenAuthenticator,
};
use af_cache::{
    CacheError, RedisRequestRateLimitStore, RequestRateLimitOutcome, RequestRateLimitRule,
    RequestRateLimitSubject,
};
use af_domain::{GatewayPrincipal, UpstreamRetryAfter};

const REQUEST_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

type AdmissionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RequestRateLimitOutcome, CacheError>> + Send + 'a>>;

/// 生产请求 RPM 准入的最小存储端口；具体 Redis 协议留在缓存 crate。
pub(crate) trait RequestRateLimitStore: Send + Sync {
    fn admit<'a>(&'a self, rules: &'a [RequestRateLimitRule]) -> AdmissionFuture<'a>;
}

impl RequestRateLimitStore for RedisRequestRateLimitStore {
    fn admit<'a>(&'a self, rules: &'a [RequestRateLimitRule]) -> AdmissionFuture<'a> {
        Box::pin(RedisRequestRateLimitStore::admit(self, rules))
    }
}

/// 在数据库令牌认证之后执行用户/分组 RPM 的运行时装饰器。
pub(crate) struct RuntimeTokenAuthenticator {
    inner: Arc<dyn TokenAuthenticator>,
    store: Option<Arc<dyn RequestRateLimitStore>>,
}

impl RuntimeTokenAuthenticator {
    /// 绑定认证器与可选的权威限流存储；没有 Redis 时有限策略仍会失败关闭。
    pub(crate) fn new(
        inner: Arc<dyn TokenAuthenticator>,
        store: Option<Arc<dyn RequestRateLimitStore>>,
    ) -> Self {
        Self { inner, store }
    }
}

impl TokenAuthenticator for RuntimeTokenAuthenticator {
    fn authenticate<'a>(
        &'a self,
        digest: &'a ApiKeyDigest,
        client_ip: af_domain::TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a> {
        let store = self.store.clone();
        Box::pin(async move {
            let authentication = self.inner.authenticate(digest, client_ip).await?;
            let rules = build_rules(
                authentication.principal(),
                authentication.user_rpm_limit(),
                authentication.group_rpm_limit(),
            )
            .map_err(|_| {
                tracing::error!(
                    target: "af_server::request_rate_limit",
                    error_kind = "request_rate_limit_rule_invalid",
                    "认证快照中的 RPM 规则无效"
                );
                TokenAuthenticationError::RateLimitUnavailable
            })?;
            if rules.is_empty() {
                return Ok(authentication);
            }

            let Some(store) = store else {
                tracing::error!(
                    target: "af_server::request_rate_limit",
                    error_kind = "request_rate_limit_store_missing",
                    "有限 RPM 策略缺少权威限流存储"
                );
                return Err(TokenAuthenticationError::RateLimitUnavailable);
            };
            match store.admit(&rules).await {
                Ok(RequestRateLimitOutcome::Admitted) => Ok(authentication),
                Ok(RequestRateLimitOutcome::Limited(rejection)) => {
                    let Some(retry_after) = retry_after(rejection.retry_after()) else {
                        tracing::error!(
                            target: "af_server::request_rate_limit",
                            error_kind = "request_rate_limit_retry_after_invalid",
                            "限流存储返回了无法表达的重试时间"
                        );
                        return Err(TokenAuthenticationError::RateLimitUnavailable);
                    };
                    Err(TokenAuthenticationError::RateLimited { retry_after })
                }
                Err(_) => {
                    tracing::error!(
                        target: "af_server::request_rate_limit",
                        error_kind = "request_rate_limit_store_error",
                        "生产请求限流存储调用失败"
                    );
                    Err(TokenAuthenticationError::RateLimitUnavailable)
                }
            }
        })
    }
}

fn build_rules(
    principal: GatewayPrincipal,
    user_limit: Option<NonZeroU32>,
    group_limit: Option<NonZeroU32>,
) -> Result<Vec<RequestRateLimitRule>, CacheError> {
    let mut rules = Vec::with_capacity(2);
    if let Some(limit) = user_limit {
        rules.push(RequestRateLimitRule::new(
            RequestRateLimitSubject::User(principal.user_id()),
            limit,
            REQUEST_RATE_LIMIT_WINDOW,
        )?);
    }
    if let Some(limit) = group_limit {
        rules.push(RequestRateLimitRule::new(
            RequestRateLimitSubject::Group(principal.group_id()),
            limit,
            REQUEST_RATE_LIMIT_WINDOW,
        )?);
    }
    Ok(rules)
}

/// Redis 返回毫秒窗口，HTTP `Retry-After` 必须向上取整为至少一秒。
fn retry_after(duration: Duration) -> Option<UpstreamRetryAfter> {
    let subsecond = if duration.subsec_nanos() == 0 { 0 } else { 1 };
    let seconds = duration.as_secs().checked_add(subsecond)?;
    UpstreamRetryAfter::from_seconds(seconds)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use af_admin::{PresentedApiKey, TokenAuthentication};
    use af_domain::{GroupId, TokenId, TokenModelPolicy, TrustedClientIp, UserId};

    use super::*;

    #[test]
    fn retry_after_rounds_up_and_rejects_zero_or_overlong_values() {
        assert_eq!(
            retry_after(Duration::from_millis(1)).map(UpstreamRetryAfter::seconds),
            Some(1)
        );
        assert_eq!(
            retry_after(Duration::from_secs(60)).map(UpstreamRetryAfter::seconds),
            Some(60)
        );
        assert_eq!(retry_after(Duration::ZERO), None);
        assert_eq!(
            retry_after(Duration::from_secs(UpstreamRetryAfter::MAX_SECONDS + 1)),
            None
        );
    }

    #[test]
    fn rules_keep_user_and_group_subjects_separate() {
        let principal = principal();
        let rules = build_rules(principal, NonZeroU32::new(3), NonZeroU32::new(5)).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(
            rules[0].subject(),
            RequestRateLimitSubject::User(principal.user_id())
        );
        assert_eq!(
            rules[1].subject(),
            RequestRateLimitSubject::Group(principal.group_id())
        );
        assert_eq!(rules[0].window(), REQUEST_RATE_LIMIT_WINDOW);
    }

    #[test]
    fn no_limits_produce_no_rules() {
        assert!(build_rules(principal(), None, None).unwrap().is_empty());
    }

    #[tokio::test]
    async fn unlimited_authentication_does_not_require_redis() {
        let expected = authentication(None, None);
        let runtime =
            RuntimeTokenAuthenticator::new(Arc::new(FixedAuthenticator(expected.clone())), None);

        assert_eq!(authenticate(&runtime).await, Ok(expected));
    }

    #[tokio::test]
    async fn finite_policy_without_store_fails_closed() {
        let runtime = RuntimeTokenAuthenticator::new(
            Arc::new(FixedAuthenticator(authentication(NonZeroU32::new(1), None))),
            None,
        );

        assert_eq!(
            authenticate(&runtime).await,
            Err(TokenAuthenticationError::RateLimitUnavailable)
        );
    }

    #[tokio::test]
    async fn admitted_request_checks_both_subjects_in_one_batch() {
        let store = Arc::new(AdmittingStore::default());
        let runtime = RuntimeTokenAuthenticator::new(
            Arc::new(FixedAuthenticator(authentication(
                NonZeroU32::new(3),
                NonZeroU32::new(5),
            ))),
            Some(store.clone()),
        );

        assert!(authenticate(&runtime).await.is_ok());
        assert_eq!(store.calls.load(Ordering::Relaxed), 1);
        let batches = store.batches.lock().unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 2);
        assert_eq!(
            batches[0][0].subject(),
            RequestRateLimitSubject::User(principal().user_id())
        );
        assert_eq!(
            batches[0][1].subject(),
            RequestRateLimitSubject::Group(principal().group_id())
        );
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

    #[derive(Default)]
    struct AdmittingStore {
        calls: AtomicUsize,
        batches: Mutex<Vec<Vec<RequestRateLimitRule>>>,
    }

    impl RequestRateLimitStore for AdmittingStore {
        fn admit<'a>(&'a self, rules: &'a [RequestRateLimitRule]) -> AdmissionFuture<'a> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.batches.lock().unwrap().push(rules.to_vec());
            Box::pin(async { Ok(RequestRateLimitOutcome::Admitted) })
        }
    }

    fn authentication(
        user_limit: Option<NonZeroU32>,
        group_limit: Option<NonZeroU32>,
    ) -> TokenAuthentication {
        TokenAuthentication::new(principal(), TokenModelPolicy::unrestricted())
            .with_rpm_limits(user_limit, group_limit)
    }

    async fn authenticate(
        runtime: &RuntimeTokenAuthenticator,
    ) -> Result<TokenAuthentication, TokenAuthenticationError> {
        let presented =
            PresentedApiKey::parse("sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8").unwrap();
        let digest = presented.digest();
        drop(presented);
        runtime.authenticate(&digest, client_ip()).await
    }

    fn principal() -> GatewayPrincipal {
        GatewayPrincipal::new(
            TokenId::new(1).unwrap(),
            UserId::new(2).unwrap(),
            GroupId::new(3).unwrap(),
        )
    }

    fn client_ip() -> TrustedClientIp {
        TrustedClientIp::new("192.0.2.1".parse().unwrap())
    }
}
