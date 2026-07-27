use std::{fmt, net::SocketAddr, sync::Arc};

use af_config::{ClientIpSource, MAX_TRUSTED_PROXY_CIDRS, ServerConfig};
use af_domain::{IpCidr, TrustedClientIp};
use axum::{extract::ConnectInfo, http::Request};
use http::HeaderMap;

const MAX_X_FORWARDED_FOR_BYTES: usize = 2_048;
const MAX_X_FORWARDED_FOR_HOPS: usize = 32;
const X_FORWARDED_FOR: &str = "x-forwarded-for";
const CLIENT_IP_HEADERS: [&str; 4] = [
    X_FORWARDED_FOR,
    "forwarded",
    "x-real-ip",
    "cf-connecting-ip",
];

/// 可信客户端 IP 无法从已配置的传输边界确定。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClientIpResolutionError {
    /// 服务入口没有注入 TCP 对端，表示监听接线不完整。
    MissingPeer,
    /// 可信代理提供的 XFF 链缺失、畸形或没有不可信客户端边界。
    InvalidProxyChain,
}

/// 根据静态服务拓扑解析可信客户端 IP，并脱敏所有中间地址载体。
#[derive(Clone)]
pub(crate) struct ClientIpResolver {
    source: ClientIpSource,
    proxy_config_valid: bool,
    trusted_proxy_cidrs: Arc<[IpCidr]>,
}

impl ClientIpResolver {
    /// 从已完成启动校验的服务配置建立解析器。
    pub(crate) fn from_config(config: &ServerConfig) -> Self {
        let trusted_proxy_cidrs = config.trusted_proxy_cidrs();
        let valid_proxy_config = config.client_ip_source() == ClientIpSource::XForwardedFor
            && !trusted_proxy_cidrs.is_empty()
            && trusted_proxy_cidrs.len() <= MAX_TRUSTED_PROXY_CIDRS
            && !trusted_proxy_cidrs
                .iter()
                .any(|network| network.is_universal());
        Self {
            source: config.client_ip_source(),
            proxy_config_valid: valid_proxy_config,
            trusted_proxy_cidrs: if valid_proxy_config {
                trusted_proxy_cidrs.into()
            } else {
                Arc::default()
            },
        }
    }

    /// 解析并移除 TCP 对端扩展与所有客户端地址提示头。
    pub(crate) fn resolve<B>(
        &self,
        request: &mut Request<B>,
    ) -> Result<TrustedClientIp, ClientIpResolutionError> {
        let peer = request
            .extensions_mut()
            .remove::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(address)| TrustedClientIp::new(address.ip()));
        let result = peer
            .ok_or(ClientIpResolutionError::MissingPeer)
            .and_then(|peer| self.resolve_with_peer(peer, request.headers()));

        for header in CLIENT_IP_HEADERS {
            request.headers_mut().remove(header);
        }
        result
    }

    fn resolve_with_peer(
        &self,
        peer: TrustedClientIp,
        headers: &HeaderMap,
    ) -> Result<TrustedClientIp, ClientIpResolutionError> {
        if self.source == ClientIpSource::Peer {
            return Ok(peer);
        }
        if !self.proxy_config_valid {
            return Err(ClientIpResolutionError::InvalidProxyChain);
        }
        if !self.is_trusted_proxy(peer) {
            return Ok(peer);
        }

        let mut header_values = headers.get_all(X_FORWARDED_FOR).iter();
        let value = header_values
            .next()
            .ok_or(ClientIpResolutionError::InvalidProxyChain)?;
        if header_values.next().is_some() || value.as_bytes().len() > MAX_X_FORWARDED_FOR_BYTES {
            return Err(ClientIpResolutionError::InvalidProxyChain);
        }
        let value = value
            .to_str()
            .map_err(|_| ClientIpResolutionError::InvalidProxyChain)?;
        let mut hops = Vec::new();
        for hop in value.split(',') {
            if hops.len() == MAX_X_FORWARDED_FOR_HOPS {
                return Err(ClientIpResolutionError::InvalidProxyChain);
            }
            let hop = hop.trim_matches([' ', '\t']);
            if hop.is_empty() {
                return Err(ClientIpResolutionError::InvalidProxyChain);
            }
            let hop = hop
                .parse()
                .map(TrustedClientIp::new)
                .map_err(|_| ClientIpResolutionError::InvalidProxyChain)?;
            hops.push(hop);
        }

        let mut current = peer;
        for hop in hops.into_iter().rev() {
            if !self.is_trusted_proxy(current) {
                return Ok(current);
            }
            current = hop;
        }

        if self.is_trusted_proxy(current) {
            Err(ClientIpResolutionError::InvalidProxyChain)
        } else {
            Ok(current)
        }
    }

    fn is_trusted_proxy(&self, address: TrustedClientIp) -> bool {
        self.trusted_proxy_cidrs
            .iter()
            .any(|network| network.contains(address))
    }
}

impl fmt::Debug for ClientIpResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClientIpResolver(<redacted>)")
    }
}
