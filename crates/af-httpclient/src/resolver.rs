use std::{collections::HashSet, error::Error, fmt, net::SocketAddr, sync::Arc};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use crate::{TargetAddressPolicy, address::is_target_address_allowed};

/// 单次 A/AAAA 解析最多接受 64 个地址，避免异常响应放大内存与连接回退。
pub(crate) const MAX_RESOLVED_TARGET_ADDRESSES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DnsFailureKind {
    Lookup,
    Empty,
    TooMany,
    Blocked,
}

#[derive(Debug)]
struct DnsFailure(DnsFailureKind);

impl fmt::Display for DnsFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.0 {
            DnsFailureKind::Lookup => "上游域名解析失败",
            DnsFailureKind::Empty => "上游域名未返回地址",
            DnsFailureKind::TooMany => "上游域名返回地址过多",
            DnsFailureKind::Blocked => "上游域名解析到受限地址",
        };
        formatter.write_str(message)
    }
}

impl Error for DnsFailure {}

#[derive(Clone)]
pub(crate) struct ValidatedResolver {
    inner: Arc<dyn Resolve>,
    target_policy: TargetAddressPolicy,
    exempt_proxy_host: Option<String>,
}

impl ValidatedResolver {
    /// 创建系统解析器；代理主机仅豁免上游地址分类，不豁免数量与空结果边界。
    pub(crate) fn system(
        target_policy: TargetAddressPolicy,
        exempt_proxy_host: Option<&str>,
    ) -> Self {
        Self::new(Arc::new(SystemResolver), target_policy, exempt_proxy_host)
    }

    pub(crate) fn new(
        inner: Arc<dyn Resolve>,
        target_policy: TargetAddressPolicy,
        exempt_proxy_host: Option<&str>,
    ) -> Self {
        Self {
            inner,
            target_policy,
            exempt_proxy_host: exempt_proxy_host.map(normalize_dns_name),
        }
    }
}

impl Resolve for ValidatedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let exempt =
            self.exempt_proxy_host.as_deref() == Some(normalize_dns_name(name.as_str()).as_str());
        let resolving = self.inner.resolve(name);
        let target_policy = self.target_policy.clone();
        Box::pin(async move {
            let resolved = resolving
                .await
                .map_err(|_| boxed_failure(DnsFailureKind::Lookup))?;
            let mut addresses: Vec<_> = resolved.take(MAX_RESOLVED_TARGET_ADDRESSES + 1).collect();
            if addresses.len() > MAX_RESOLVED_TARGET_ADDRESSES {
                return Err(boxed_failure(DnsFailureKind::TooMany));
            }
            let mut seen = HashSet::with_capacity(addresses.len());
            addresses.retain(|address| seen.insert(*address));
            if addresses.is_empty() {
                return Err(boxed_failure(DnsFailureKind::Empty));
            }

            // SOCKS5 只取迭代器首项，因此必须在返回前完整校验整个 A/AAAA 集合。
            if !exempt
                && addresses
                    .iter()
                    .any(|address| !is_socket_address_allowed(*address, &target_policy))
            {
                return Err(boxed_failure(DnsFailureKind::Blocked));
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

#[derive(Clone, Copy)]
struct SystemResolver;

impl Resolve for SystemResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host, 0))
                .await
                .map_err(|_| boxed_failure(DnsFailureKind::Lookup))?;
            Ok(Box::new(addresses) as Addrs)
        })
    }
}

pub(crate) fn normalize_dns_name(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}

pub(crate) fn dns_failure_kind(error: &reqwest::Error) -> Option<DnsFailureKind> {
    let mut current: Option<&(dyn Error + 'static)> = Some(error);
    while let Some(error) = current {
        if let Some(failure) = error.downcast_ref::<DnsFailure>() {
            return Some(failure.0);
        }
        current = error.source();
    }
    None
}

fn is_socket_address_allowed(address: SocketAddr, policy: &TargetAddressPolicy) -> bool {
    if let SocketAddr::V6(address) = address
        && (address.flowinfo() != 0 || address.scope_id() != 0)
    {
        return false;
    }
    is_target_address_allowed(address.ip(), policy)
}

fn boxed_failure(kind: DnsFailureKind) -> Box<dyn Error + Send + Sync> {
    Box::new(DnsFailure(kind))
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use super::*;

    #[derive(Clone)]
    struct SequenceResolver {
        answers: Arc<Mutex<VecDeque<Vec<SocketAddr>>>>,
    }

    impl SequenceResolver {
        fn new(answers: Vec<Vec<SocketAddr>>) -> Self {
            Self {
                answers: Arc::new(Mutex::new(answers.into())),
            }
        }
    }

    impl Resolve for SequenceResolver {
        fn resolve(&self, _name: Name) -> Resolving {
            let answer = self.answers.lock().unwrap().pop_front().unwrap();
            Box::pin(async move { Ok(Box::new(answer.into_iter()) as Addrs) })
        }
    }

    async fn resolve_once(
        resolver: &ValidatedResolver,
        host: &str,
    ) -> Result<Vec<SocketAddr>, Box<dyn Error + Send + Sync>> {
        resolver
            .resolve(host.parse().unwrap())
            .await
            .map(Iterator::collect)
    }

    fn address(value: &str) -> SocketAddr {
        value.parse().unwrap()
    }

    #[tokio::test]
    async fn returns_the_same_fully_validated_address_snapshot() {
        let expected = vec![address("[2606:4700:4700::1111]:0"), address("8.8.8.8:0")];
        let inner = SequenceResolver::new(vec![vec![expected[0], expected[1], expected[0]]]);
        let resolver =
            ValidatedResolver::new(Arc::new(inner), TargetAddressPolicy::public_only(), None);
        assert_eq!(
            resolve_once(&resolver, "upstream.example").await.unwrap(),
            expected
        );
    }

    #[tokio::test]
    async fn mixed_public_and_private_answers_fail_closed() {
        let inner = SequenceResolver::new(vec![vec![address("8.8.8.8:0"), address("127.0.0.1:0")]]);
        let resolver =
            ValidatedResolver::new(Arc::new(inner), TargetAddressPolicy::public_only(), None);
        let error = resolve_once(&resolver, "mixed.example").await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<DnsFailure>().map(|failure| failure.0),
            Some(DnsFailureKind::Blocked)
        );
        let rendered = format!("{error:?}\n{error}");
        assert!(!rendered.contains("mixed.example"));
        assert!(!rendered.contains("127.0.0.1"));
    }

    #[tokio::test]
    async fn every_new_resolution_rechecks_rebinding_answers() {
        let inner = SequenceResolver::new(vec![
            vec![address("8.8.8.8:0")],
            vec![address("127.0.0.1:0")],
        ]);
        let resolver =
            ValidatedResolver::new(Arc::new(inner), TargetAddressPolicy::public_only(), None);
        assert!(resolve_once(&resolver, "rebind.example").await.is_ok());
        assert!(resolve_once(&resolver, "rebind.example").await.is_err());
    }

    #[tokio::test]
    async fn exact_exceptions_remain_narrow_and_proxy_host_is_separate() {
        let loopback: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let policy = TargetAddressPolicy::allow_exact([loopback]).unwrap();
        let inner = SequenceResolver::new(vec![
            vec![address("127.0.0.1:0")],
            vec![address("127.0.0.2:0")],
            vec![address("127.0.0.2:0")],
        ]);
        let resolver = ValidatedResolver::new(Arc::new(inner), policy, Some("proxy.local"));

        assert!(resolve_once(&resolver, "allowed.example").await.is_ok());
        assert!(resolve_once(&resolver, "blocked.example").await.is_err());
        assert!(resolve_once(&resolver, "PROXY.LOCAL.").await.is_ok());
    }

    #[tokio::test]
    async fn rejects_empty_and_excessive_answers() {
        let too_many = (0..=MAX_RESOLVED_TARGET_ADDRESSES)
            .map(|index| SocketAddr::from(([8, 8, 8, 8], u16::try_from(index).unwrap())))
            .collect();
        let inner = SequenceResolver::new(vec![Vec::new(), too_many]);
        let resolver =
            ValidatedResolver::new(Arc::new(inner), TargetAddressPolicy::public_only(), None);

        for expected in [DnsFailureKind::Empty, DnsFailureKind::TooMany] {
            let error = resolve_once(&resolver, "budget.example").await.unwrap_err();
            assert_eq!(
                error.downcast_ref::<DnsFailure>().map(|failure| failure.0),
                Some(expected)
            );
        }
    }
}
