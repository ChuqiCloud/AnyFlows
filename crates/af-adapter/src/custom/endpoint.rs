use std::{collections::HashSet, fmt};

use af_domain::MAX_MODEL_NAME_BYTES;

use crate::{AdaptorError, AdaptorResult, MAX_UPSTREAM_REQUEST_TARGET_BYTES, RelayContext};

/// Custom 相对端点模板的最大字节数。
pub const MAX_CUSTOM_ENDPOINT_TEMPLATE_BYTES: usize = 2_048;
/// Custom 相对端点模板允许的最大路径段数。
pub const MAX_CUSTOM_ENDPOINT_PATH_SEGMENTS: usize = 32;
/// Custom 相对端点模板允许的最大静态查询参数数。
pub const MAX_CUSTOM_ENDPOINT_QUERY_PAIRS: usize = 16;
/// Custom 端点模板唯一支持的动态占位符。
pub const CUSTOM_MODEL_PLACEHOLDER: &str = "{model}";

/// 已验证的 Custom 相对端点模板。
///
/// 模板只保存路径与非敏感静态查询参数，不能携带主机、片段或凭据。模型占位符
/// 最多出现一次，渲染时始终作为单个路径段内容或查询值编码。
#[derive(Clone, Eq, PartialEq)]
pub struct CustomEndpointTemplate {
    path_segments: Vec<String>,
    query_pairs: Vec<(String, String)>,
    has_model_placeholder: bool,
}

impl CustomEndpointTemplate {
    /// 解析以 `/` 开头的相对端点模板。
    pub fn parse(value: impl Into<String>) -> AdaptorResult<Self> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_CUSTOM_ENDPOINT_TEMPLATE_BYTES
            || value.trim() != value
            || !value.is_ascii()
            || value.bytes().any(|byte| !byte.is_ascii_graphic())
            || value.contains(['#', '\\', '%'])
        {
            return Err(AdaptorError::InvalidCustomEndpoint);
        }

        let (path, query) = match value.split_once('?') {
            Some((path, query)) if !query.is_empty() && !query.contains('?') => (path, Some(query)),
            Some(_) => return Err(AdaptorError::InvalidCustomEndpoint),
            None => (value.as_str(), None),
        };
        if !path.starts_with('/') || path.starts_with("//") || path.ends_with('/') {
            return Err(AdaptorError::InvalidCustomEndpoint);
        }

        let path_segments = path[1..].split('/').map(str::to_owned).collect::<Vec<_>>();
        if path_segments.is_empty()
            || path_segments.len() > MAX_CUSTOM_ENDPOINT_PATH_SEGMENTS
            || path_segments
                .iter()
                .any(|segment| matches!(segment.as_str(), "" | "." | ".."))
        {
            return Err(AdaptorError::InvalidCustomEndpoint);
        }

        let mut placeholder_count = 0_usize;
        for segment in &path_segments {
            placeholder_count = placeholder_count
                .checked_add(validate_template_value(segment)?)
                .ok_or(AdaptorError::InvalidCustomEndpoint)?;
        }

        let mut query_pairs = Vec::new();
        let mut query_names = HashSet::new();
        if let Some(query) = query {
            for pair in query.split('&') {
                if query_pairs.len() == MAX_CUSTOM_ENDPOINT_QUERY_PAIRS {
                    return Err(AdaptorError::InvalidCustomEndpoint);
                }
                let Some((name, value)) = pair.split_once('=') else {
                    return Err(AdaptorError::InvalidCustomEndpoint);
                };
                if !is_valid_query_name(name) || !query_names.insert(name.to_owned()) {
                    return Err(AdaptorError::InvalidCustomEndpoint);
                }
                placeholder_count = placeholder_count
                    .checked_add(validate_template_value(value)?)
                    .ok_or(AdaptorError::InvalidCustomEndpoint)?;
                query_pairs.push((name.to_owned(), value.to_owned()));
            }
        }
        if placeholder_count > 1 {
            return Err(AdaptorError::InvalidCustomEndpoint);
        }

        Ok(Self {
            path_segments,
            query_pairs,
            has_model_placeholder: placeholder_count == 1,
        })
    }

    /// 在显式渠道基础地址后渲染端点，并完整保留反向代理路径前缀。
    pub(crate) fn render(&self, context: &RelayContext, model: &str) -> AdaptorResult<String> {
        if !is_valid_model(model) {
            return Err(AdaptorError::InvalidRequestTarget);
        }
        let base_url = context
            .base_url_override()
            .ok_or(AdaptorError::InvalidBaseUrl)?;
        let mut target = context.resolve_base_url(base_url)?;
        {
            let mut segments = target
                .path_segments_mut()
                .map_err(|_| AdaptorError::InvalidBaseUrl)?;
            segments.pop_if_empty();
            for segment in &self.path_segments {
                let rendered = render_template_value(segment, model);
                segments.push(&rendered);
            }
        }
        if !self.query_pairs.is_empty() {
            let mut query = target.query_pairs_mut();
            for (name, value) in &self.query_pairs {
                let rendered = render_template_value(value, model);
                query.append_pair(name, &rendered);
            }
        }
        let target = String::from(target);
        if target.len() > MAX_UPSTREAM_REQUEST_TARGET_BYTES {
            return Err(AdaptorError::InvalidRequestTarget);
        }
        Ok(target)
    }
}

impl fmt::Debug for CustomEndpointTemplate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomEndpointTemplate")
            .field("path_segment_count", &self.path_segments.len())
            .field("query_pair_count", &self.query_pairs.len())
            .field("has_model_placeholder", &self.has_model_placeholder)
            .finish()
    }
}

fn validate_template_value(value: &str) -> AdaptorResult<usize> {
    let placeholder_count = value.match_indices(CUSTOM_MODEL_PLACEHOLDER).count();
    let static_value = value.replace(CUSTOM_MODEL_PLACEHOLDER, "");
    if static_value.contains(['{', '}']) {
        return Err(AdaptorError::InvalidCustomEndpoint);
    }
    Ok(placeholder_count)
}

fn render_template_value(value: &str, model: &str) -> String {
    value.replace(CUSTOM_MODEL_PLACEHOLDER, model)
}

fn is_valid_query_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~'))
}

fn is_valid_model(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MODEL_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
