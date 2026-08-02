use std::{
    fmt,
    net::{IpAddr, Ipv6Addr},
    time::Duration,
};

use reqwest::Url;

#[cfg(test)]
use crate::address::is_test_loopback_exception;
use crate::{HttpClientError, TimeoutPhase};

/// TCP 建连与代理握手的默认超时。
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 单次读取停顿的默认超时，成功读取后重新计时。
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(300);
/// 从建连开始到响应体结束的默认总超时。
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(900);
/// 单个测试 Client 最多允许 32 个精确环回地址。
#[cfg(test)]
pub(crate) const MAX_TARGET_ADDRESS_EXCEPTIONS: usize = 32;

/// 代理端解析上游域名时的信任策略。
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum RemoteDnsPolicy {
    /// 默认拒绝本进程无法验证和固定的代理端 DNS。
    #[default]
    Deny,
    /// 将解析与私网阻断责任显式委托给受信出口代理。
    TrustProxy,
}

/// 上游目标域名的实际解析位置。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TargetDnsStrategy {
    /// 由本进程解析，校验后的同一地址集直接交给连接器。
    Local,
    /// 由 HTTP(S) 或 SOCKS5H 代理解析，本进程无法观察最终地址。
    Proxy,
}

/// 生产配置只允许普通公网地址；内部测试可注入精确环回地址。
#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) struct TargetAddressPolicy {
    exact_exceptions: Vec<IpAddr>,
}

impl TargetAddressPolicy {
    /// 创建不含任何非公网例外的默认策略。
    #[must_use]
    pub const fn public_only() -> Self {
        Self {
            exact_exceptions: Vec::new(),
        }
    }

    /// 仅供 crate 内回环测试注入精确地址，生产构建不存在该入口。
    #[cfg(test)]
    pub(crate) fn allow_exact(
        addresses: impl IntoIterator<Item = IpAddr>,
    ) -> Result<Self, HttpClientError> {
        let mut exact_exceptions: Vec<_> = addresses
            .into_iter()
            .take(MAX_TARGET_ADDRESS_EXCEPTIONS + 1)
            .collect();
        if exact_exceptions.len() > MAX_TARGET_ADDRESS_EXCEPTIONS {
            return Err(HttpClientError::TooManyTargetAddressExceptions);
        }
        if exact_exceptions
            .iter()
            .any(|address| !is_test_loopback_exception(*address))
        {
            return Err(HttpClientError::InvalidTargetAddressException);
        }
        exact_exceptions.sort_unstable();
        exact_exceptions.dedup();
        Ok(Self { exact_exceptions })
    }

    /// 返回已配置的精确地址例外数量，不暴露具体网络拓扑。
    #[must_use]
    pub const fn exception_count(&self) -> usize {
        self.exact_exceptions.len()
    }

    pub(crate) fn contains_exception(&self, address: &IpAddr) -> bool {
        self.exact_exceptions.binary_search(address).is_ok()
    }
}

impl Default for TargetAddressPolicy {
    fn default() -> Self {
        Self::public_only()
    }
}

impl fmt::Debug for TargetAddressPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TargetAddressPolicy")
            .field("mode", &"public-only")
            .field("exact_exception_count", &self.exception_count())
            .finish()
    }
}

/// 建连、逐次读取与整体请求的三层超时。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HttpTimeouts {
    connect: Duration,
    read: Duration,
    request: Duration,
}

impl HttpTimeouts {
    /// 创建三层超时；任一零值都会被拒绝。
    pub fn new(
        connect: Duration,
        read: Duration,
        request: Duration,
    ) -> Result<Self, HttpClientError> {
        validate_timeout(connect, TimeoutPhase::Connect)?;
        validate_timeout(read, TimeoutPhase::Read)?;
        validate_timeout(request, TimeoutPhase::Request)?;
        Ok(Self {
            connect,
            read,
            request,
        })
    }

    /// 返回建连阶段超时。
    #[must_use]
    pub const fn connect(self) -> Duration {
        self.connect
    }

    /// 返回单次读取停顿超时；该值不等同于首 token deadline。
    #[must_use]
    pub const fn read(self) -> Duration {
        self.read
    }

    /// 返回覆盖完整响应体生命周期的总超时。
    #[must_use]
    pub const fn request(self) -> Duration {
        self.request
    }
}

impl Default for HttpTimeouts {
    fn default() -> Self {
        Self {
            connect: DEFAULT_CONNECT_TIMEOUT,
            read: DEFAULT_READ_TIMEOUT,
            request: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

/// 直连或强制代理配置；强制代理故障时绝不回退直连。
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ProxyConfig {
    route: ProxyRoute,
}

#[derive(Clone, Eq, Hash, PartialEq)]
enum ProxyRoute {
    Direct,
    Required(Url),
}

impl ProxyConfig {
    /// 创建显式直连配置；构建 Client 时会忽略系统代理环境变量。
    #[must_use]
    pub const fn direct() -> Self {
        Self {
            route: ProxyRoute::Direct,
        }
    }

    /// 解析强制代理地址，支持 HTTP、HTTPS、SOCKS5 与 SOCKS5H。
    ///
    /// SOCKS5 在本机解析目标域名，SOCKS5H 由代理解析；二者安全语义不同。
    pub fn parse(input: impl AsRef<str>) -> Result<Self, HttpClientError> {
        let input = input.as_ref().trim();
        let (_, authority) = input
            .split_once("://")
            .ok_or(HttpClientError::InvalidProxyUrl)?;
        if authority.starts_with('/') {
            return Err(HttpClientError::InvalidProxyUrl);
        }
        let mut endpoint = Url::parse(input).map_err(|_| HttpClientError::InvalidProxyUrl)?;
        match endpoint.scheme() {
            "http" | "https" => {}
            "socks5" | "socks5h" => {
                if endpoint.port().is_none() {
                    endpoint
                        .set_port(Some(1080))
                        .map_err(|_| HttpClientError::InvalidProxyUrl)?;
                }
            }
            _ => return Err(HttpClientError::UnsupportedProxyScheme),
        }
        if endpoint.host_str().is_none()
            || !matches!(endpoint.path(), "" | "/")
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(HttpClientError::InvalidProxyUrl);
        }
        Ok(Self {
            route: ProxyRoute::Required(endpoint),
        })
    }

    /// 使用结构化字段组装代理地址，避免上层通过字符串拼接泄露认证信息。
    pub fn from_parts(
        scheme: &str,
        host: &str,
        port: u16,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<Self, HttpClientError> {
        if !matches!(scheme, "http" | "https" | "socks5" | "socks5h") {
            return Err(HttpClientError::UnsupportedProxyScheme);
        }
        // URL 标准不允许部分特殊协议跨类别切换，因此用已校验协议直接创建无密钥占位端点。
        let mut endpoint = Url::parse(&format!("{scheme}://proxy.invalid"))
            .map_err(|_| HttpClientError::InvalidProxyUrl)?;
        let bracketed_ipv6 = host.parse::<Ipv6Addr>().ok().map(|_| format!("[{host}]"));
        endpoint
            .set_host(Some(bracketed_ipv6.as_deref().unwrap_or(host)))
            .map_err(|_| HttpClientError::InvalidProxyUrl)?;
        endpoint
            .set_port(Some(port))
            .map_err(|_| HttpClientError::InvalidProxyUrl)?;
        match (username, password) {
            (Some(username), Some(password)) => {
                endpoint
                    .set_username(username)
                    .map_err(|_| HttpClientError::InvalidProxyUrl)?;
                endpoint
                    .set_password(Some(password))
                    .map_err(|_| HttpClientError::InvalidProxyUrl)?;
            }
            (None, None) => {}
            _ => return Err(HttpClientError::InvalidProxyUrl),
        }
        Self::parse(endpoint)
    }

    /// 返回是否为显式直连。
    #[must_use]
    pub const fn is_direct(&self) -> bool {
        matches!(self.route, ProxyRoute::Direct)
    }

    /// 返回代理协议；直连时返回 `None`。
    #[must_use]
    pub fn scheme(&self) -> Option<&str> {
        match &self.route {
            ProxyRoute::Direct => None,
            ProxyRoute::Required(endpoint) => Some(endpoint.scheme()),
        }
    }

    /// 返回上游目标域名由本机还是代理端解析。
    #[must_use]
    pub fn target_dns_strategy(&self) -> TargetDnsStrategy {
        match &self.route {
            ProxyRoute::Direct => TargetDnsStrategy::Local,
            ProxyRoute::Required(endpoint) if endpoint.scheme() == "socks5" => {
                TargetDnsStrategy::Local
            }
            ProxyRoute::Required(_) => TargetDnsStrategy::Proxy,
        }
    }

    pub(crate) fn required_endpoint(&self) -> Option<&Url> {
        match &self.route {
            ProxyRoute::Direct => None,
            ProxyRoute::Required(endpoint) => Some(endpoint),
        }
    }

    pub(crate) fn endpoint_host(&self) -> Option<&str> {
        self.required_endpoint().and_then(Url::host_str)
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self::direct()
    }
}

impl fmt::Debug for ProxyConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.route {
            ProxyRoute::Direct => formatter.write_str("Direct"),
            ProxyRoute::Required(endpoint) => formatter
                .debug_struct("RequiredProxy")
                .field("scheme", &endpoint.scheme())
                .field("endpoint", &"<redacted>")
                .finish(),
        }
    }
}

/// 决定 Client 复用身份的代理、超时与目标安全配置。
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct HttpClientConfig {
    proxy: ProxyConfig,
    timeouts: HttpTimeouts,
    target_address_policy: TargetAddressPolicy,
    remote_dns_policy: RemoteDnsPolicy,
}

impl HttpClientConfig {
    /// 创建默认只允许公网目标且拒绝代理端 DNS 的 Client 配置。
    #[must_use]
    pub const fn new(proxy: ProxyConfig, timeouts: HttpTimeouts) -> Self {
        Self {
            proxy,
            timeouts,
            target_address_policy: TargetAddressPolicy::public_only(),
            remote_dns_policy: RemoteDnsPolicy::Deny,
        }
    }

    /// 返回直连或强制代理配置。
    #[must_use]
    pub const fn proxy(&self) -> &ProxyConfig {
        &self.proxy
    }

    /// 替换代理路由，同时保留现有超时与目标安全策略。
    #[must_use]
    pub fn with_proxy(mut self, proxy: ProxyConfig) -> Self {
        self.proxy = proxy;
        self
    }

    /// 返回三层超时配置。
    #[must_use]
    pub const fn timeouts(&self) -> HttpTimeouts {
        self.timeouts
    }

    /// 替换三层超时，同时保留代理、DNS 与目标地址安全策略。
    #[must_use]
    pub const fn with_timeouts(mut self, timeouts: HttpTimeouts) -> Self {
        self.timeouts = timeouts;
        self
    }

    /// 仅供 crate 内回环测试设置精确地址，生产构建不存在该入口。
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_target_address_policy(mut self, policy: TargetAddressPolicy) -> Self {
        self.target_address_policy = policy;
        self
    }

    /// 设置代理端 DNS 策略；信任模式要求代理侧执行等价的私网阻断。
    #[must_use]
    pub const fn with_remote_dns_policy(mut self, policy: RemoteDnsPolicy) -> Self {
        self.remote_dns_policy = policy;
        self
    }

    pub(crate) const fn target_address_policy(&self) -> &TargetAddressPolicy {
        &self.target_address_policy
    }

    /// 返回代理端 DNS 策略。
    #[must_use]
    pub const fn remote_dns_policy(&self) -> RemoteDnsPolicy {
        self.remote_dns_policy
    }
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self::new(ProxyConfig::direct(), HttpTimeouts::default())
    }
}

impl fmt::Debug for HttpClientConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpClientConfig")
            .field("proxy", &self.proxy)
            .field("timeouts", &self.timeouts)
            .field("target_address_policy", &self.target_address_policy)
            .field("remote_dns_policy", &self.remote_dns_policy)
            .finish()
    }
}

fn validate_timeout(timeout: Duration, phase: TimeoutPhase) -> Result<(), HttpClientError> {
    if timeout.is_zero() {
        return Err(HttpClientError::InvalidTimeout { phase });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_each_zero_timeout_independently() {
        for (connect, read, request, phase) in [
            (
                Duration::ZERO,
                Duration::from_secs(1),
                Duration::from_secs(1),
                TimeoutPhase::Connect,
            ),
            (
                Duration::from_secs(1),
                Duration::ZERO,
                Duration::from_secs(1),
                TimeoutPhase::Read,
            ),
            (
                Duration::from_secs(1),
                Duration::from_secs(1),
                Duration::ZERO,
                TimeoutPhase::Request,
            ),
        ] {
            assert_eq!(
                HttpTimeouts::new(connect, read, request),
                Err(HttpClientError::InvalidTimeout { phase })
            );
        }
    }

    #[test]
    fn accepts_supported_proxy_schemes_and_normalizes_socks_port() {
        for scheme in ["http", "https", "socks5", "socks5h"] {
            let proxy = ProxyConfig::parse(format!("{scheme}://proxy.example")).unwrap();
            assert_eq!(proxy.scheme(), Some(scheme));
            assert!(!proxy.is_direct());
            if scheme.starts_with("socks") {
                assert_eq!(proxy.required_endpoint().unwrap().port(), Some(1080));
            }
        }
    }

    #[test]
    fn builds_proxy_from_structured_parts_without_leaking_credentials() {
        let proxy = ProxyConfig::from_parts(
            "socks5h",
            "proxy.example",
            1080,
            Some("proxy-user"),
            Some("proxy-secret"),
        )
        .unwrap();
        assert_eq!(proxy.scheme(), Some("socks5h"));
        let rendered = format!("{proxy:?}");
        assert!(!rendered.contains("proxy-user"));
        assert!(!rendered.contains("proxy-secret"));

        let ipv6 = ProxyConfig::from_parts("http", "2001:db8::1", 8080, None, None).unwrap();
        assert!(matches!(
            ipv6.required_endpoint().unwrap().host(),
            Some(url::Host::Ipv6(_))
        ));
    }

    #[test]
    fn rejects_unsupported_or_ambiguous_proxy_urls() {
        assert_eq!(
            ProxyConfig::parse("ftp://proxy.example").unwrap_err(),
            HttpClientError::UnsupportedProxyScheme
        );
        for input in [
            "not a url",
            "http:///missing-host",
            "http://proxy.example/path",
            "http://proxy.example?token=secret",
            "http://proxy.example#fragment",
        ] {
            assert_eq!(
                ProxyConfig::parse(input).unwrap_err(),
                HttpClientError::InvalidProxyUrl
            );
        }
    }

    #[test]
    fn normalizes_equivalent_urls_but_keeps_credentials_distinct() {
        let first = ProxyConfig::parse("HTTP://proxy.EXAMPLE:80").unwrap();
        let second = ProxyConfig::parse("http://proxy.example/").unwrap();
        assert_eq!(first, second);

        let alice = ProxyConfig::parse("http://alice:one@proxy.example").unwrap();
        let bob = ProxyConfig::parse("http://bob:two@proxy.example").unwrap();
        assert_ne!(alice, bob);
    }

    #[test]
    fn debug_output_redacts_proxy_endpoint_and_credentials() {
        let proxy = ProxyConfig::parse("http://user:secret@proxy.example:8080").unwrap();
        let config = HttpClientConfig::new(proxy.clone(), HttpTimeouts::default());
        for debug in [format!("{proxy:?}"), format!("{config:?}")] {
            assert!(!debug.contains("user"));
            assert!(!debug.contains("secret"));
            assert!(!debug.contains("proxy.example"));
            assert!(debug.contains("redacted"));
        }
    }

    #[test]
    fn target_policies_are_strict_by_default_and_normalize_exceptions() {
        let first = "127.0.0.1".parse().unwrap();
        let second = "::1".parse().unwrap();
        let policy = TargetAddressPolicy::allow_exact([second, first, first]).unwrap();
        assert_eq!(policy.exception_count(), 2);

        let config = HttpClientConfig::default()
            .with_target_address_policy(policy)
            .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
        assert_eq!(config.target_address_policy().exception_count(), 2);
        assert_eq!(config.remote_dns_policy(), RemoteDnsPolicy::TrustProxy);

        let debug = format!("{config:?}");
        assert!(!debug.contains("127.0.0.1"));
        assert!(!debug.contains("::1"));
    }

    #[test]
    fn rejects_excessive_target_address_exceptions() {
        let addresses = (0..=MAX_TARGET_ADDRESS_EXCEPTIONS).map(|index| {
            IpAddr::V4(std::net::Ipv4Addr::new(
                10,
                0,
                0,
                u8::try_from(index + 1).unwrap(),
            ))
        });
        assert_eq!(
            TargetAddressPolicy::allow_exact(addresses).unwrap_err(),
            HttpClientError::TooManyTargetAddressExceptions
        );
    }

    #[test]
    fn target_address_exceptions_are_limited_to_loopback_tests() {
        for address in [
            "0.0.0.0",
            "10.0.0.1",
            "100.100.100.200",
            "169.254.169.254",
            "224.0.0.1",
            "::",
            "fd00:ec2::254",
            "fe80::1",
            "ff02::1",
        ] {
            assert_eq!(
                TargetAddressPolicy::allow_exact([address.parse().unwrap()]).unwrap_err(),
                HttpClientError::InvalidTargetAddressException,
                "意外接受 {address}"
            );
        }
    }

    #[test]
    fn proxy_schemes_expose_target_dns_strategy() {
        assert_eq!(
            ProxyConfig::direct().target_dns_strategy(),
            TargetDnsStrategy::Local
        );
        for (scheme, strategy) in [
            ("socks5", TargetDnsStrategy::Local),
            ("socks5h", TargetDnsStrategy::Proxy),
            ("http", TargetDnsStrategy::Proxy),
            ("https", TargetDnsStrategy::Proxy),
        ] {
            assert_eq!(
                ProxyConfig::parse(format!("{scheme}://proxy.example"))
                    .unwrap()
                    .target_dns_strategy(),
                strategy
            );
        }
    }
}
