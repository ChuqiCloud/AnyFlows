use af_admin::PresentedApiKey;
use http::{HeaderMap, HeaderName, HeaderValue, Request, Uri, header::AUTHORIZATION};
use thiserror::Error;
use url::form_urlencoded;

const X_API_KEY: HeaderName = HeaderName::from_static("x-api-key");
const X_GOOG_API_KEY: HeaderName = HeaderName::from_static("x-goog-api-key");

/// 是否接受会进入前置代理访问日志的 `?key=` 兼容传参。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum QueryApiKeyPolicy {
    /// 默认拒绝查询参数凭据；部署层无需承担查询串脱敏前置条件。
    #[default]
    Deny,
    /// 显式允许查询参数凭据；前置代理必须同步关闭或脱敏查询串日志。
    Allow,
}

/// API Key 传输边界错误；所有变体均不保存原始 Header、URI 或密钥。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ApiKeyExtractionError {
    /// 请求没有携带任何受支持的凭据。
    #[error("请求缺少 API Key")]
    Missing,
    /// 唯一凭据不符合 carrier 或 API Key 格式。
    #[error("API Key 传参格式无效")]
    Malformed,
    /// 请求重复或同时使用多个 carrier，无法确定唯一凭据。
    #[error("请求携带了冲突的 API Key")]
    Ambiguous,
    /// 请求使用了未显式启用的查询参数 carrier。
    #[error("查询参数 API Key 未启用")]
    QueryDenied,
}

/// 从唯一 carrier 提取规范 API Key，并清除请求中的全部凭据载体。
///
/// 本函数只完成传输归一与脱敏，不代表认证成功。调用方必须把返回值交给真实
/// TokenAuthRepository 完成状态、软删除、过期等校验后才能进入业务 handler。
pub fn extract_presented_api_key<B>(
    request: &mut Request<B>,
    query_policy: QueryApiKeyPolicy,
) -> Result<PresentedApiKey, ApiKeyExtractionError> {
    let (result, sanitized_path_and_query) = {
        let headers = request.headers();
        let authorization = header_values(headers, &AUTHORIZATION);
        let x_api_key = header_values(headers, &X_API_KEY);
        let x_goog_api_key = header_values(headers, &X_GOOG_API_KEY);
        let query = inspect_query(request.uri());
        let carrier_count =
            authorization.len() + x_api_key.len() + x_goog_api_key.len() + query.values.len();

        let result = match carrier_count {
            0 => Err(ApiKeyExtractionError::Missing),
            1 if !authorization.is_empty() => parse_bearer(authorization[0]),
            1 if !x_api_key.is_empty() => parse_header(x_api_key[0]),
            1 if !x_goog_api_key.is_empty() => parse_header(x_goog_api_key[0]),
            1 if query_policy == QueryApiKeyPolicy::Allow => parse_text(query.values[0]),
            1 => Err(ApiKeyExtractionError::QueryDenied),
            _ => Err(ApiKeyExtractionError::Ambiguous),
        };
        (result, query.sanitized_path_and_query)
    };

    scrub_and_return(request, sanitized_path_and_query, result)
}

fn header_values<'a>(headers: &'a HeaderMap, name: &HeaderName) -> Vec<&'a HeaderValue> {
    headers.get_all(name).iter().collect()
}

fn bearer_value(value: &HeaderValue) -> Result<&str, ApiKeyExtractionError> {
    let value = header_value(value)?;
    let Some((scheme, credential)) = value.split_once(' ') else {
        return Err(ApiKeyExtractionError::Malformed);
    };
    let credential = credential.trim_start_matches(' ');
    if !scheme.eq_ignore_ascii_case("bearer")
        || credential.is_empty()
        || credential.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(ApiKeyExtractionError::Malformed);
    }
    Ok(credential)
}

fn parse_bearer(value: &HeaderValue) -> Result<PresentedApiKey, ApiKeyExtractionError> {
    parse_text(bearer_value(value)?)
}

fn parse_header(value: &HeaderValue) -> Result<PresentedApiKey, ApiKeyExtractionError> {
    parse_text(header_value(value)?)
}

fn parse_text(value: &str) -> Result<PresentedApiKey, ApiKeyExtractionError> {
    PresentedApiKey::parse(value).map_err(|_| ApiKeyExtractionError::Malformed)
}

fn header_value(value: &HeaderValue) -> Result<&str, ApiKeyExtractionError> {
    value.to_str().map_err(|_| ApiKeyExtractionError::Malformed)
}

struct QueryInspection<'a> {
    values: Vec<&'a str>,
    sanitized_path_and_query: Option<String>,
}

fn inspect_query(uri: &Uri) -> QueryInspection<'_> {
    let Some(query) = uri.query() else {
        return QueryInspection {
            values: Vec::new(),
            sanitized_path_and_query: None,
        };
    };
    let mut values = Vec::new();
    let mut retained = Vec::new();
    for segment in query.split('&') {
        let is_key = form_urlencoded::parse(segment.as_bytes())
            .next()
            .is_some_and(|(name, _)| name == "key");
        if is_key {
            values.push(segment.split_once('=').map_or("", |(_, value)| value));
        } else {
            // 非凭据参数按原始字节保留，避免解码后重编码改变下游语义。
            retained.push(segment);
        }
    }
    let sanitized_path_and_query = if values.is_empty() {
        None
    } else if retained.is_empty() {
        Some(uri.path().to_owned())
    } else {
        Some(format!("{}?{}", uri.path(), retained.join("&")))
    };
    QueryInspection {
        values,
        sanitized_path_and_query,
    }
}

fn scrub_and_return<B>(
    request: &mut Request<B>,
    sanitized_path_and_query: Option<String>,
    result: Result<PresentedApiKey, ApiKeyExtractionError>,
) -> Result<PresentedApiKey, ApiKeyExtractionError> {
    request.headers_mut().remove(AUTHORIZATION);
    request.headers_mut().remove(&X_API_KEY);
    request.headers_mut().remove(&X_GOOG_API_KEY);
    if let Some(path_and_query) = sanitized_path_and_query {
        let mut parts = request.uri().clone().into_parts();
        parts.path_and_query = Some(
            path_and_query
                .parse()
                .expect("移除 key 查询片段后必须仍是合法 PathAndQuery"),
        );
        *request.uri_mut() = Uri::from_parts(parts).expect("原 URI 组件必须保持合法组合");
    }
    result
}
