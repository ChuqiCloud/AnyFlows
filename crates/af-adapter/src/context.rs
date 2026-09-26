use std::fmt;

use af_httpclient::PooledClient;
use url::Url;

use crate::{AdaptorError, AdaptorResult, MAX_UPSTREAM_REQUEST_TARGET_BYTES};

/// 渠道基础地址的最大 UTF-8 字节数，与持久化边界保持一致。
pub const MAX_UPSTREAM_BASE_URL_BYTES: usize = 2_048;
/// 请求标识的最大 ASCII 字节数。
pub const MAX_REQUEST_ID_BYTES: usize = 128;

/// 适配器在一次转发尝试中可读取的最小上下文投影。
///
/// 完整转发状态仍归 `af-relay` 所有；本类型定义在适配层是为了保持依赖方向
/// 单向。它只携带受控 HTTP Client、可选渠道地址覆盖和无业务权限含义的请求标识。
#[derive(Clone)]
pub struct RelayContext {
    http_client: PooledClient,
    base_url: Option<String>,
    request_id: Option<String>,
    oauth_provider: Option<String>,
    oauth_account_key: Option<String>,
}

impl RelayContext {
    /// 以受连接池管理的 HTTP Client 创建适配器上下文。
    #[must_use]
    pub const fn new(http_client: PooledClient) -> Self {
        Self {
            http_client,
            base_url: None,
            request_id: None,
            oauth_provider: None,
            oauth_account_key: None,
        }
    }

    /// 设置已校验的渠道基础地址；不得携带 userinfo、查询串或片段。
    pub fn with_base_url(mut self, value: impl AsRef<str>) -> AdaptorResult<Self> {
        self.base_url = Some(parse_base_url(value.as_ref())?.into());
        Ok(self)
    }

    /// 设置用于链路关联的请求标识。
    pub fn with_request_id(mut self, value: impl Into<String>) -> AdaptorResult<Self> {
        let value = value.into();
        if !is_valid_request_id(&value) {
            return Err(AdaptorError::InvalidRequestId);
        }
        self.request_id = Some(value);
        Ok(self)
    }

    /// 设置 OAuth 账号身份元数据；令牌本体仍只保留在 `Credential` 中。
    #[must_use]
    pub fn with_oauth_identity(
        mut self,
        provider: Option<&str>,
        account_key: Option<&str>,
    ) -> Self {
        self.oauth_provider = provider.map(str::to_owned);
        self.oauth_account_key = account_key.map(str::to_owned);
        self
    }

    /// 返回受连接池管理的 HTTP Client。
    #[must_use]
    pub const fn http_client(&self) -> &PooledClient {
        &self.http_client
    }

    /// 返回渠道配置提供的基础地址覆盖。
    #[must_use]
    pub fn base_url_override(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    /// 返回请求标识。
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// 返回 OAuth 厂商标识。
    #[must_use]
    pub fn oauth_provider(&self) -> Option<&str> {
        self.oauth_provider.as_deref()
    }

    /// 返回 OAuth 账号标识；不得记录或回传给客户端。
    #[must_use]
    pub fn oauth_account_key(&self) -> Option<&str> {
        self.oauth_account_key.as_deref()
    }

    /// 判断当前上下文是否声明为 Codex OAuth 请求。
    #[must_use]
    pub fn is_codex_oauth(&self) -> bool {
        self.oauth_provider
            .as_deref()
            .is_some_and(|provider| provider.eq_ignore_ascii_case("codex"))
    }

    /// 解析实际使用的基础地址；渠道覆盖优先于适配器默认值。
    pub fn resolve_base_url(&self, default: &str) -> AdaptorResult<Url> {
        parse_base_url(self.base_url.as_deref().unwrap_or(default))
    }

    /// 在基础地址后追加相对路径，并完整保留已有反向代理路径前缀。
    pub fn append_path(&self, default: &str, relative_path: &str) -> AdaptorResult<String> {
        self.append_path_from(self.resolve_base_url(default)?, relative_path)
    }

    /// 在指定的固定基础地址后追加相对路径，忽略渠道提供的地址覆盖。
    ///
    /// 用于协议自身定义了固定上游端点的特殊渠道，例如 Codex OAuth。
    pub fn append_path_from(&self, mut target: Url, relative_path: &str) -> AdaptorResult<String> {
        if !is_valid_relative_path(relative_path) {
            return Err(AdaptorError::InvalidRequestTarget);
        }
        {
            let mut segments = target
                .path_segments_mut()
                .map_err(|_| AdaptorError::InvalidBaseUrl)?;
            segments.pop_if_empty();
            for segment in relative_path.split('/') {
                segments.push(segment);
            }
        }
        let target = String::from(target);
        if target.len() > MAX_UPSTREAM_REQUEST_TARGET_BYTES {
            return Err(AdaptorError::InvalidRequestTarget);
        }
        Ok(target)
    }
}

impl fmt::Debug for RelayContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayContext")
            .field("http_client", &"<受控>")
            .field("base_url", &self.base_url.as_ref().map(|_| "<已脱敏>"))
            .field("request_id", &self.request_id.as_ref().map(|_| "<已脱敏>"))
            .field("oauth_provider", &self.oauth_provider)
            .field(
                "oauth_account_key",
                &self.oauth_account_key.as_ref().map(|_| "<已脱敏>"),
            )
            .finish()
    }
}

fn parse_base_url(value: &str) -> AdaptorResult<Url> {
    if value.is_empty() || value.len() > MAX_UPSTREAM_BASE_URL_BYTES || value.trim() != value {
        return Err(AdaptorError::InvalidBaseUrl);
    }
    let parsed = Url::parse(value).map_err(|_| AdaptorError::InvalidBaseUrl)?;
    let supported_scheme = matches!(parsed.scheme(), "http" | "https");
    let has_credentials = !parsed.username().is_empty() || parsed.password().is_some();
    if !supported_scheme
        || !parsed.has_host()
        || has_credentials
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AdaptorError::InvalidBaseUrl);
    }
    if parsed.as_str().len() > MAX_UPSTREAM_BASE_URL_BYTES {
        return Err(AdaptorError::InvalidBaseUrl);
    }
    Ok(parsed)
}

fn is_valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REQUEST_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn is_valid_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_UPSTREAM_REQUEST_TARGET_BYTES
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains(['?', '#', '\\'])
        && value
            .split('/')
            .all(|segment| !matches!(segment, "" | "." | ".."))
}

#[cfg(test)]
mod tests {
    use af_httpclient::{HttpClientConfig, HttpClientPool};

    use super::*;

    fn context() -> RelayContext {
        RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        )
    }

    #[test]
    fn context_validates_and_redacts_operational_values() {
        let context = context()
            .with_base_url("https://upstream.example/v1")
            .unwrap()
            .with_request_id("request-123")
            .unwrap();
        assert_eq!(
            context.resolve_base_url("https://default.invalid").unwrap(),
            Url::parse("https://upstream.example/v1").unwrap()
        );

        let debug = format!("{context:?}");
        assert!(!debug.contains("upstream.example"));
        assert!(!debug.contains("request-123"));
    }

    #[test]
    fn context_rejects_sensitive_or_ambiguous_base_urls() {
        for value in [
            "ftp://upstream.example",
            "https://user:secret@upstream.example",
            "https://upstream.example?key=secret",
            "https://upstream.example#fragment",
            " https://upstream.example",
        ] {
            assert_eq!(
                context().with_base_url(value).unwrap_err(),
                AdaptorError::InvalidBaseUrl
            );
        }
    }

    #[test]
    fn context_rejects_unsafe_request_ids() {
        for value in ["", "request id", "request\nsecret", "请求"] {
            assert_eq!(
                context().with_request_id(value).unwrap_err(),
                AdaptorError::InvalidRequestId
            );
        }
    }

    #[test]
    fn append_path_preserves_prefix_with_or_without_trailing_slash() {
        for base_url in [
            "https://upstream.example/proxy/openai",
            "https://upstream.example/proxy/openai/",
        ] {
            let target = context()
                .with_base_url(base_url)
                .unwrap()
                .append_path("https://default.invalid", "v1/chat/completions")
                .unwrap();
            assert_eq!(
                target,
                "https://upstream.example/proxy/openai/v1/chat/completions"
            );
        }
        assert_eq!(
            context()
                .append_path("https://default.example/prefix", "v1/models")
                .unwrap(),
            "https://default.example/prefix/v1/models"
        );
    }

    #[test]
    fn context_rechecks_normalized_url_length() {
        let expanded = format!("https://upstream.example/{}", "测".repeat(600));
        assert!(expanded.len() < MAX_UPSTREAM_BASE_URL_BYTES);
        assert_eq!(
            context().with_base_url(expanded).unwrap_err(),
            AdaptorError::InvalidBaseUrl
        );
    }
}
