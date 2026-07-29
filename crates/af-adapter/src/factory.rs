use std::{fmt, sync::Arc};

use af_domain::ChannelType;

use crate::{Adaptor, AdaptorError, AdaptorResult, AnthropicAdaptor, OpenAiAdaptor};

/// OpenAI 适配器的强类型构造配置。
///
/// 模型名与列表容量由上层配置边界校验；本类型保留原始顺序并在构造时把所有权移交
/// 给渠道实例。基础地址和凭据分别由 [`crate::RelayContext`] 与 [`crate::Credential`]
/// 管理，不在工厂配置中重复保存。
#[derive(Clone, Eq, PartialEq)]
pub struct OpenAiAdaptorSettings {
    supported_models: Vec<String>,
}

impl OpenAiAdaptorSettings {
    /// 使用已校验的模型清单创建 OpenAI 适配器配置；空清单表示无内建限制。
    #[must_use]
    pub fn new(supported_models: Vec<String>) -> Self {
        Self { supported_models }
    }

    fn into_supported_models(self) -> Vec<String> {
        self.supported_models
    }
}

impl fmt::Debug for OpenAiAdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiAdaptorSettings")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}

/// Anthropic Messages 适配器的强类型构造配置。
///
/// 模型名与列表容量由上层配置边界校验；API 版本和认证方案由适配器依据官方
/// Credential 契约闭合处理，基础地址仍由 [`crate::RelayContext`] 管理。
#[derive(Clone, Eq, PartialEq)]
pub struct AnthropicAdaptorSettings {
    supported_models: Vec<String>,
}

impl AnthropicAdaptorSettings {
    /// 使用已校验的模型清单创建 Anthropic 适配器配置；空清单表示无内建限制。
    #[must_use]
    pub fn new(supported_models: Vec<String>) -> Self {
        Self { supported_models }
    }

    fn into_supported_models(self) -> Vec<String> {
        self.supported_models
    }
}

impl fmt::Debug for AnthropicAdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicAdaptorSettings")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}

/// 按供应商闭合的适配器构造配置。
///
/// 新渠道只有在对应适配器和专用配置都落地后才能增加变体，禁止用不透明 Map 或
/// 空占位提前放行。
#[derive(Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum AdaptorSettings {
    /// OpenAI Chat 及 Bearer-compatible 渠道配置。
    OpenAi(OpenAiAdaptorSettings),
    /// Anthropic Messages 官方及标准兼容渠道配置。
    Anthropic(AnthropicAdaptorSettings),
}

impl fmt::Debug for AdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenAi(settings) => formatter
                .debug_tuple("AdaptorSettings::OpenAi")
                .field(settings)
                .finish(),
            Self::Anthropic(settings) => formatter
                .debug_tuple("AdaptorSettings::Anthropic")
                .field(settings)
                .finish(),
        }
    }
}

/// 按强类型渠道及已校验配置创建独立适配器实例。
///
/// 已实现渠道必须收到同种强类型配置；其余渠道固定失败，不能静默复用其他供应商
/// 的认证、协议或模型清单。
pub fn get_adaptor(
    channel_type: ChannelType,
    settings: AdaptorSettings,
) -> AdaptorResult<Arc<dyn Adaptor>> {
    match (channel_type, settings) {
        (ChannelType::OpenAi, AdaptorSettings::OpenAi(settings)) => Ok(Arc::new(
            OpenAiAdaptor::with_supported_models(settings.into_supported_models()),
        )),
        (ChannelType::Anthropic, AdaptorSettings::Anthropic(settings)) => Ok(Arc::new(
            AnthropicAdaptor::with_supported_models(settings.into_supported_models()),
        )),
        (
            channel_type @ (ChannelType::OpenAi | ChannelType::Anthropic),
            AdaptorSettings::OpenAi(_) | AdaptorSettings::Anthropic(_),
        ) => Err(AdaptorError::InvalidChannelSettings { channel_type }),
        (
            channel_type @ (ChannelType::Gemini
            | ChannelType::Bedrock
            | ChannelType::Vertex
            | ChannelType::Custom),
            _,
        ) => Err(AdaptorError::UnsupportedChannel { channel_type }),
    }
}

#[cfg(test)]
mod tests {
    use af_domain::{Operation, Protocol};
    use af_httpclient::{HeaderMap, HttpClientConfig, HttpClientPool};

    use super::*;
    use crate::{Credential, RelayContext};

    fn openai_settings(models: &[&str]) -> AdaptorSettings {
        AdaptorSettings::OpenAi(OpenAiAdaptorSettings::new(
            models.iter().map(|model| (*model).to_owned()).collect(),
        ))
    }

    fn anthropic_settings(models: &[&str]) -> AdaptorSettings {
        AdaptorSettings::Anthropic(AnthropicAdaptorSettings::new(
            models.iter().map(|model| (*model).to_owned()).collect(),
        ))
    }

    #[test]
    fn factory_builds_configured_isolated_openai_instances() {
        let unrestricted = get_adaptor(ChannelType::OpenAi, openai_settings(&[])).unwrap();
        assert!(unrestricted.supported_models().is_empty());

        let first_settings = openai_settings(&["private-model-a", "private-model-b"]);
        let settings_debug = format!("{first_settings:?}");
        assert!(settings_debug.contains("supported_model_count: 2"));
        assert!(!settings_debug.contains("private-model-a"));
        assert!(!settings_debug.contains("private-model-b"));

        let first = get_adaptor(ChannelType::OpenAi, first_settings).unwrap();
        let second =
            get_adaptor(ChannelType::OpenAi, openai_settings(&["private-model-c"])).unwrap();
        assert!(!Arc::ptr_eq(&unrestricted, &first));
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(first.channel_type(), ChannelType::OpenAi);
        assert_eq!(first.default_protocol(), Protocol::OpenAiChat);
        assert_eq!(first.default_base_url(), OpenAiAdaptor::DEFAULT_BASE_URL);
        assert_eq!(
            first.supported_models(),
            ["private-model-a", "private-model-b"]
        );
        assert_eq!(second.supported_models(), ["private-model-c"]);

        let mut detached_models = first.supported_models();
        detached_models.clear();
        assert_eq!(
            first.supported_models(),
            ["private-model-a", "private-model-b"]
        );
    }

    #[test]
    fn factory_builds_configured_isolated_anthropic_instances() {
        let unrestricted = get_adaptor(ChannelType::Anthropic, anthropic_settings(&[])).unwrap();
        assert!(unrestricted.supported_models().is_empty());

        let configured_settings = anthropic_settings(&["private-claude-a", "private-claude-b"]);
        let settings_debug = format!("{configured_settings:?}");
        assert!(settings_debug.contains("supported_model_count: 2"));
        assert!(!settings_debug.contains("private-claude-a"));
        assert!(!settings_debug.contains("private-claude-b"));

        let configured = get_adaptor(ChannelType::Anthropic, configured_settings).unwrap();
        assert!(!Arc::ptr_eq(&unrestricted, &configured));
        assert_eq!(configured.channel_type(), ChannelType::Anthropic);
        assert_eq!(configured.default_protocol(), Protocol::Anthropic);
        assert_eq!(
            configured.default_base_url(),
            AnthropicAdaptor::DEFAULT_BASE_URL
        );
        assert_eq!(
            configured.supported_models(),
            ["private-claude-a", "private-claude-b"]
        );
    }

    #[test]
    fn factory_rejects_mismatched_known_channel_settings_without_leaks() {
        let cases = [
            (
                ChannelType::OpenAi,
                anthropic_settings(&["private-anthropic-model"]),
            ),
            (
                ChannelType::Anthropic,
                openai_settings(&["private-openai-model"]),
            ),
        ];
        for (channel_type, settings) in cases {
            let error = get_adaptor(channel_type, settings)
                .err()
                .expect("错配配置必须拒绝");
            assert_eq!(error, AdaptorError::InvalidChannelSettings { channel_type });
            let rendered = format!("{error:?}\n{error}");
            assert!(!rendered.contains("private-anthropic-model"));
            assert!(!rendered.contains("private-openai-model"));
        }
    }

    #[test]
    fn factory_rejects_every_unimplemented_channel_without_configuration_leaks() {
        for channel_type in ChannelType::ALL.iter().copied().filter(|channel_type| {
            !matches!(channel_type, ChannelType::OpenAi | ChannelType::Anthropic)
        }) {
            let error = get_adaptor(channel_type, openai_settings(&["private-model-canary"]))
                .err()
                .expect("未实现渠道必须拒绝");
            assert_eq!(error, AdaptorError::UnsupportedChannel { channel_type });
            let rendered = format!("{error:?}\n{error}");
            assert!(!rendered.contains("private-model-canary"));
        }
    }

    #[test]
    fn factory_product_preserves_openai_url_header_and_redaction_contract() {
        let adaptor = get_adaptor(
            ChannelType::OpenAi,
            openai_settings(&["private-model-contract"]),
        )
        .unwrap();
        let context = RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        )
        .with_base_url("https://private-upstream.example/proxy")
        .unwrap()
        .with_request_id("private-request-id")
        .unwrap();
        let credential = Credential::api_key("private-credential").unwrap();
        let mut headers = HeaderMap::new();
        adaptor
            .setup_headers(&mut headers, &credential, &context)
            .unwrap();

        assert_eq!(
            adaptor.build_url(&context, Operation::Chat).unwrap(),
            "https://private-upstream.example/proxy/v1/chat/completions"
        );
        assert_eq!(headers["authorization"], "Bearer private-credential");
        assert_eq!(headers["x-request-id"], "private-request-id");
        let error = adaptor
            .build_url(&context, Operation::Responses)
            .unwrap_err();
        let rendered = format!("{context:?}\n{credential:?}\n{error:?}\n{error}");
        for private in [
            "private-upstream.example",
            "private-request-id",
            "private-credential",
            "private-model-contract",
        ] {
            assert!(!rendered.contains(private));
        }

        // HeaderMap 保留可观察的 request-id，但 sensitive 认证头仍必须脱敏。
        let headers_debug = format!("{headers:?}");
        assert!(headers_debug.contains("private-request-id"));
        assert!(!headers_debug.contains("private-credential"));
        assert!(!headers_debug.contains("private-model-contract"));
    }
}
