use std::{fmt, sync::Arc};

use af_domain::ChannelType;

use crate::{Adaptor, AdaptorError, AdaptorResult, OpenAiAdaptor};

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

/// 按供应商闭合的适配器构造配置。
///
/// 新渠道只有在对应适配器和专用配置都落地后才能增加变体，禁止用不透明 Map 或
/// 空占位提前放行。
#[derive(Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum AdaptorSettings {
    /// OpenAI Chat 及 Bearer-compatible 渠道配置。
    OpenAi(OpenAiAdaptorSettings),
}

impl fmt::Debug for AdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenAi(settings) => formatter
                .debug_tuple("AdaptorSettings::OpenAi")
                .field(settings)
                .finish(),
        }
    }
}

/// 按强类型渠道及已校验配置创建独立适配器实例。
///
/// 当前只实现 OpenAI；其余已知渠道必须固定失败，不能静默复用 OpenAI 认证与协议。
pub fn get_adaptor(
    channel_type: ChannelType,
    settings: AdaptorSettings,
) -> AdaptorResult<Arc<dyn Adaptor>> {
    match channel_type {
        ChannelType::OpenAi => match settings {
            AdaptorSettings::OpenAi(settings) => Ok(Arc::new(
                OpenAiAdaptor::with_supported_models(settings.into_supported_models()),
            )),
        },
        channel_type @ (ChannelType::Anthropic
        | ChannelType::Gemini
        | ChannelType::Bedrock
        | ChannelType::Vertex
        | ChannelType::Custom) => Err(AdaptorError::UnsupportedChannel { channel_type }),
    }
}

#[cfg(test)]
mod tests {
    use af_domain::{Operation, Protocol};
    use af_httpclient::{HeaderMap, HttpClientConfig, HttpClientPool};

    use super::*;
    use crate::{Credential, RelayContext};

    fn settings(models: &[&str]) -> AdaptorSettings {
        AdaptorSettings::OpenAi(OpenAiAdaptorSettings::new(
            models.iter().map(|model| (*model).to_owned()).collect(),
        ))
    }

    #[test]
    fn factory_builds_configured_isolated_openai_instances() {
        let unrestricted = get_adaptor(ChannelType::OpenAi, settings(&[])).unwrap();
        assert!(unrestricted.supported_models().is_empty());

        let first_settings = settings(&["private-model-a", "private-model-b"]);
        let settings_debug = format!("{first_settings:?}");
        assert!(settings_debug.contains("supported_model_count: 2"));
        assert!(!settings_debug.contains("private-model-a"));
        assert!(!settings_debug.contains("private-model-b"));

        let first = get_adaptor(ChannelType::OpenAi, first_settings).unwrap();
        let second = get_adaptor(ChannelType::OpenAi, settings(&["private-model-c"])).unwrap();
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
    fn factory_rejects_every_unimplemented_channel_without_configuration_leaks() {
        for channel_type in ChannelType::ALL
            .iter()
            .copied()
            .filter(|channel_type| *channel_type != ChannelType::OpenAi)
        {
            let error = get_adaptor(channel_type, settings(&["private-model-canary"]))
                .err()
                .expect("未实现渠道必须拒绝");
            assert_eq!(error, AdaptorError::UnsupportedChannel { channel_type });
            let rendered = format!("{error:?}\n{error}");
            assert!(!rendered.contains("private-model-canary"));
        }
    }

    #[test]
    fn factory_product_preserves_openai_url_header_and_redaction_contract() {
        let adaptor =
            get_adaptor(ChannelType::OpenAi, settings(&["private-model-contract"])).unwrap();
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
