use std::{fmt, time::Duration};

use reqwest::Url;

use crate::{HttpClientError, TimeoutPhase};

/// TCP 建连与代理握手的默认超时。
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 单次读取停顿的默认超时，成功读取后重新计时。
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(300);
/// 从建连开始到响应体结束的默认总超时。
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(900);

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

    pub(crate) fn required_endpoint(&self) -> Option<&Url> {
        match &self.route {
            ProxyRoute::Direct => None,
            ProxyRoute::Required(endpoint) => Some(endpoint),
        }
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

/// 决定 Client 复用身份的代理与超时配置。
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct HttpClientConfig {
    proxy: ProxyConfig,
    timeouts: HttpTimeouts,
}

impl HttpClientConfig {
    /// 创建 Client 配置；代理身份与全部超时都会参与缓存 key。
    #[must_use]
    pub const fn new(proxy: ProxyConfig, timeouts: HttpTimeouts) -> Self {
        Self { proxy, timeouts }
    }

    /// 返回直连或强制代理配置。
    #[must_use]
    pub const fn proxy(&self) -> &ProxyConfig {
        &self.proxy
    }

    /// 返回三层超时配置。
    #[must_use]
    pub const fn timeouts(&self) -> HttpTimeouts {
        self.timeouts
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
}
