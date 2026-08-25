//! 企业 OIDC JWKS 的受控读取边界。
//!
//! 本模块只从已验证的 Provider 配置快照读取公钥集，复用受控 HTTP Client 的 DNS、
//! SSRF、重定向与超时策略；不接受回调参数提供的地址，也不持久化 JWKS 或 ID Token。

use std::fmt;

use af_httpclient::{HeaderMap, HeaderName, HeaderValue, Method, PooledClient};
use futures_util::StreamExt as _;
use jsonwebtoken::jwk::JwkSet;
use serde_json::Value;
use thiserror::Error;
use url::Url;

use crate::OrganizationSsoOidcTokenExchangeConfig;

const MAX_JWKS_RESPONSE_BYTES: usize = 128 * 1024;
const MAX_JWKS_KEYS: usize = 32;

/// 仅从版本绑定配置快照读取 JWKS 的受控客户端。
#[derive(Clone)]
pub struct OrganizationSsoOidcJwksClient {
    jwks_uri: Url,
}

impl OrganizationSsoOidcJwksClient {
    /// 从已通过 OIDC 配置校验的 token exchange 快照取得固定 JWKS 地址。
    #[must_use]
    pub fn from_token_exchange_config(config: &OrganizationSsoOidcTokenExchangeConfig) -> Self {
        Self {
            jwks_uri: config.jwks_uri().clone(),
        }
    }

    /// 读取并校验受限 JWKS；网络或响应异常均返回不含地址和正文的稳定分类。
    pub async fn fetch(
        &self,
        http_client: &PooledClient,
    ) -> Result<JwkSet, OrganizationSsoOidcJwksError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
        let response = http_client
            .execute(Method::GET, self.jwks_uri.as_str(), headers, None)
            .await
            .map_err(|_| OrganizationSsoOidcJwksError::Transport)?;
        let status = response.status();
        let body = collect_jwks_response(response).await?;
        if !status.is_success() {
            return Err(OrganizationSsoOidcJwksError::EndpointRejected {
                status: status.as_u16(),
            });
        }
        parse_jwks(&body)
    }
}

impl fmt::Debug for OrganizationSsoOidcJwksClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoOidcJwksClient")
            .field("jwks_uri", &"<redacted>")
            .finish()
    }
}

/// JWKS 读取的稳定错误分类，不包含 Provider 地址、响应正文或 ID Token。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoOidcJwksError {
    /// 受控 HTTP Client 无法安全完成请求或读取响应。
    #[error("企业 OIDC JWKS 读取失败")]
    Transport,
    /// Provider 返回了非成功 HTTP 状态。
    #[error("企业 OIDC JWKS endpoint 拒绝请求（HTTP {status}）")]
    EndpointRejected { status: u16 },
    /// Provider 的 JWKS 响应超过固定内存边界。
    #[error("企业 OIDC JWKS 响应超过长度上限")]
    ResponseTooLarge,
    /// JWKS 不是可接受的受限 JSON 公钥集合。
    #[error("企业 OIDC JWKS 响应无效")]
    InvalidResponse,
}

async fn collect_jwks_response(
    response: af_httpclient::HttpResponse,
) -> Result<Vec<u8>, OrganizationSsoOidcJwksError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_JWKS_RESPONSE_BYTES as u64)
    {
        return Err(OrganizationSsoOidcJwksError::ResponseTooLarge);
    }
    let mut stream = response.into_bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| OrganizationSsoOidcJwksError::Transport)?;
        let next_len = body
            .len()
            .checked_add(chunk.len())
            .ok_or(OrganizationSsoOidcJwksError::ResponseTooLarge)?;
        if next_len > MAX_JWKS_RESPONSE_BYTES {
            return Err(OrganizationSsoOidcJwksError::ResponseTooLarge);
        }
        body.try_reserve(chunk.len())
            .map_err(|_| OrganizationSsoOidcJwksError::ResponseTooLarge)?;
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn parse_jwks(body: &[u8]) -> Result<JwkSet, OrganizationSsoOidcJwksError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| OrganizationSsoOidcJwksError::InvalidResponse)?;
    let object = value
        .as_object()
        .ok_or(OrganizationSsoOidcJwksError::InvalidResponse)?;
    if object.len() != 1 || !object.contains_key("keys") {
        return Err(OrganizationSsoOidcJwksError::InvalidResponse);
    }
    let keys = object
        .get("keys")
        .and_then(Value::as_array)
        .ok_or(OrganizationSsoOidcJwksError::InvalidResponse)?;
    if keys.is_empty() || keys.len() > MAX_JWKS_KEYS {
        return Err(OrganizationSsoOidcJwksError::InvalidResponse);
    }
    serde_json::from_value(value).map_err(|_| OrganizationSsoOidcJwksError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        net::{SocketAddr, TcpListener, TcpStream},
        sync::mpsc,
        thread::{self, JoinHandle},
        time::Duration,
    };

    use af_httpclient::{
        HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy,
    };
    use serde_json::json;

    use super::*;

    #[test]
    fn parser_accepts_bounded_jwk_set() {
        let jwks = parse_jwks(
            br#"{"keys":[{"kty":"RSA","kid":"directory-key","use":"sig","alg":"RS256","n":"sXchj82oHfV3b6WRv9fMfEHz-8WW3B9NQHfqOp8WoVECsK1IE09ohPI71geV8spp1oWzMDPJR2Oo3z7jKQQsMw","e":"AQAB"}]}"#,
        )
        .expect("受限 JWKS 应被接受");
        assert_eq!(jwks.keys.len(), 1);
    }

    #[test]
    fn parser_rejects_extra_empty_and_oversized_key_sets() {
        let too_many = json!({
            "keys": (0..=MAX_JWKS_KEYS).map(|_| json!({"kty": "RSA"})).collect::<Vec<_>>()
        });
        for value in [
            json!({"keys": []}),
            json!({"keys": [], "unexpected": true}),
            too_many,
        ] {
            assert_eq!(
                parse_jwks(&serde_json::to_vec(&value).expect("测试 JSON 序列化成功")),
                Err(OrganizationSsoOidcJwksError::InvalidResponse)
            );
        }
    }

    #[test]
    fn client_debug_redacts_provider_endpoint() {
        let config = OrganizationSsoOidcTokenExchangeConfig::new(
            7,
            "https://idp.example.test/".to_owned(),
            "https://idp.example.test/oauth2/token".to_owned(),
            "https://idp.example.test/oauth2/jwks".to_owned(),
            "client-id".to_owned(),
            "client-id".to_owned(),
            "https://app.example.test/api/auth/organization-sso/oidc/callback".to_owned(),
        )
        .expect("测试配置有效");
        let client = OrganizationSsoOidcJwksClient::from_token_exchange_config(&config);
        assert!(!format!("{client:?}").contains("idp.example.test"));
    }

    #[tokio::test]
    async fn fetch_uses_controlled_client_and_rejects_endpoint_failures() {
        let (sender, receiver) = mpsc::channel();
        let (address, server) = spawn_json_server(
            200,
            br#"{"keys":[{"kty":"RSA","kid":"directory-key","use":"sig","alg":"RS256","n":"sXchj82oHfV3b6WRv9fMfEHz-8WW3B9NQHfqOp8WoVECsK1IE09ohPI71geV8spp1oWzMDPJR2Oo3z7jKQQsMw","e":"AQAB"}]}"#,
            move |request| sender.send(request.to_owned()).expect("发送请求记录成功"),
        );
        let client = test_client("http://jwks.example.test/keys");
        let jwks = client
            .fetch(&proxied_client(address))
            .await
            .expect("受控 JWKS 请求成功");
        server.join().expect("JWKS 测试服务退出成功");
        let request = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("收到受控 JWKS 请求");
        assert!(request.starts_with("GET http://jwks.example.test/keys HTTP/1.1\r\n"));
        assert!(header_value(&request, "accept").is_some_and(|value| value == "application/json"));
        assert_eq!(jwks.keys.len(), 1);

        let (address, server) = spawn_json_server(503, br#"{}"#, |_| {});
        let error = test_client("http://jwks.example.test/keys")
            .fetch(&proxied_client(address))
            .await
            .expect_err("非成功状态必须拒绝");
        server.join().expect("失败 JWKS 测试服务退出成功");
        assert_eq!(
            error,
            OrganizationSsoOidcJwksError::EndpointRejected { status: 503 }
        );
    }

    #[tokio::test]
    async fn fetch_rejects_declared_oversized_response() {
        let (address, server) = spawn_raw_server(
            |stream| {
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            MAX_JWKS_RESPONSE_BYTES + 1
                        )
                        .as_bytes(),
                    )
                    .expect("写入超长响应头成功");
            },
            |_| {},
        );
        let error = test_client("http://jwks.example.test/keys")
            .fetch(&proxied_client(address))
            .await
            .expect_err("超长响应必须拒绝");
        server.join().expect("超长 JWKS 测试服务退出成功");
        assert_eq!(error, OrganizationSsoOidcJwksError::ResponseTooLarge);
    }

    fn test_client(jwks_uri: &str) -> OrganizationSsoOidcJwksClient {
        OrganizationSsoOidcJwksClient {
            jwks_uri: Url::parse(jwks_uri).expect("测试 JWKS 地址有效"),
        }
    }

    fn proxied_client(address: SocketAddr) -> PooledClient {
        let config = HttpClientConfig::new(
            ProxyConfig::parse(format!("http://{address}")).expect("测试代理地址有效"),
            HttpTimeouts::default(),
        )
        .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
        HttpClientPool::default()
            .get(&config)
            .expect("受控测试 Client 创建成功")
    }

    fn spawn_json_server<F>(
        status: u16,
        body: &'static [u8],
        inspect: F,
    ) -> (SocketAddr, JoinHandle<()>)
    where
        F: FnOnce(&str) + Send + 'static,
    {
        spawn_raw_server(
            move |stream| {
                let reason = if status == 200 {
                    "OK"
                } else {
                    "Service Unavailable"
                };
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .expect("写入 JSON 响应头成功");
                stream.write_all(body).expect("写入 JSON 响应体成功");
            },
            inspect,
        )
    }

    fn spawn_raw_server<R, F>(respond: R, inspect: F) -> (SocketAddr, JoinHandle<()>)
    where
        R: FnOnce(&mut TcpStream) + Send + 'static,
        F: FnOnce(&str) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定测试服务成功");
        let address = listener.local_addr().expect("读取测试服务地址成功");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("接收受控 Client 请求成功");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("设置请求读取超时成功");
            let request = read_request(&mut stream);
            inspect(&request);
            respond(&mut stream);
        });
        (address, handle)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4_096];
        loop {
            let count = stream.read(&mut buffer).expect("读取测试请求成功");
            assert!(count > 0, "请求在响应头完成前结束");
            bytes.extend_from_slice(&buffer[..count]);
            if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                return String::from_utf8(bytes).expect("测试请求必须是 UTF-8");
            }
        }
    }

    fn header_value<'a>(request: &'a str, expected_name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case(expected_name)
                    .then(|| value.trim())
            })
        })
    }
}
