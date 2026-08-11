use std::{fmt, sync::Arc};

use af_domain::{ChannelType, Protocol};

use crate::{
    Adaptor, AdaptorError, AdaptorResult, AnthropicAdaptor, BedrockAdaptor, CohereAdaptor,
    CustomAdaptor, CustomAuthentication, CustomEndpointTemplate, CustomStreamEndpoint,
    GeminiAdaptor, JinaAdaptor, OpenAiAdaptor, VertexAdaptor, VideoTaskAdaptor, XaiVideoAdaptor,
};

/// OpenAI 适配器的强类型构造配置。
///
/// 模型名与列表容量由上层配置边界校验；本类型保留原始顺序并在构造时把所有权移交
/// 给渠道实例。基础地址和凭据分别由 [`crate::RelayContext`] 与 [`crate::Credential`]
/// 管理，不在工厂配置中重复保存。
#[derive(Clone, Eq, PartialEq)]
pub struct OpenAiAdaptorSettings {
    protocol: Protocol,
    supported_models: Vec<String>,
}

impl OpenAiAdaptorSettings {
    /// 使用已校验的模型清单创建 OpenAI 适配器配置；空清单表示无内建限制。
    #[must_use]
    pub fn new(supported_models: Vec<String>) -> Self {
        Self {
            protocol: Protocol::OpenAiChat,
            supported_models,
        }
    }

    /// 为 OpenAI Chat、Responses 或 Embeddings 原生端点创建配置。
    pub fn for_protocol(protocol: Protocol, supported_models: Vec<String>) -> AdaptorResult<Self> {
        if !matches!(
            protocol,
            Protocol::OpenAiChat
                | Protocol::OpenAiResponses
                | Protocol::OpenAiEmbeddings
                | Protocol::OpenAiImages
                | Protocol::OpenAiAudio
                | Protocol::OpenAiSpeech
        ) {
            return Err(AdaptorError::InvalidChannelSettings {
                channel_type: ChannelType::OpenAi,
            });
        }
        Ok(Self {
            protocol,
            supported_models,
        })
    }

    fn into_parts(self) -> (Protocol, Vec<String>) {
        (self.protocol, self.supported_models)
    }
}

impl fmt::Debug for OpenAiAdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiAdaptorSettings")
            .field("protocol", &self.protocol)
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

/// Gemini Developer API 适配器的强类型构造配置。
///
/// 模型名与列表容量由上层配置边界校验；API 版本、模型路径和认证方案由适配器依据
/// 官方契约闭合处理，基础地址仍由 [`crate::RelayContext`] 管理。
#[derive(Clone, Eq, PartialEq)]
pub struct GeminiAdaptorSettings {
    supported_models: Vec<String>,
}

impl GeminiAdaptorSettings {
    /// 使用已校验的模型清单创建 Gemini 适配器配置；空清单表示无内建限制。
    #[must_use]
    pub fn new(supported_models: Vec<String>) -> Self {
        Self { supported_models }
    }

    fn into_supported_models(self) -> Vec<String> {
        self.supported_models
    }
}

impl fmt::Debug for GeminiAdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeminiAdaptorSettings")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}

/// Jina 原生 Rerank 适配器的强类型构造配置。
///
/// 模型清单由上层配置边界校验；基础地址与凭据继续由每次请求的受控上下文提供。
#[derive(Clone, Eq, PartialEq)]
pub struct JinaAdaptorSettings {
    supported_models: Vec<String>,
}

impl JinaAdaptorSettings {
    /// 使用已校验的模型清单创建 Jina 配置；空清单表示无内建模型限制。
    #[must_use]
    pub fn new(supported_models: Vec<String>) -> Self {
        Self { supported_models }
    }

    fn into_supported_models(self) -> Vec<String> {
        self.supported_models
    }
}

impl fmt::Debug for JinaAdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JinaAdaptorSettings")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}

/// Cohere v2 原生 Rerank 适配器的强类型构造配置。
#[derive(Clone, Eq, PartialEq)]
pub struct CohereAdaptorSettings {
    supported_models: Vec<String>,
}

impl CohereAdaptorSettings {
    /// 使用已校验的模型清单创建 Cohere 配置；空清单表示无内建模型限制。
    #[must_use]
    pub fn new(supported_models: Vec<String>) -> Self {
        Self { supported_models }
    }

    fn into_supported_models(self) -> Vec<String> {
        self.supported_models
    }
}

impl fmt::Debug for CohereAdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CohereAdaptorSettings")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}

/// Amazon Bedrock Runtime 适配器的强类型构造配置。
///
/// 区域在构造阶段完成主机注入与公共分区校验；模型清单只保存上层已映射值，
/// SigV4 凭据仍由每次请求的 [`crate::Credential`] 提供。
#[derive(Clone, Eq, PartialEq)]
pub struct BedrockAdaptorSettings {
    region: String,
    supported_models: Vec<String>,
}

impl BedrockAdaptorSettings {
    /// 使用显式区域和模型清单创建 Bedrock 配置。
    pub fn new(region: impl Into<String>, supported_models: Vec<String>) -> AdaptorResult<Self> {
        let region = region.into();
        if !crate::bedrock::is_valid_aws_region(&region) {
            return Err(AdaptorError::InvalidAwsRegion);
        }
        Ok(Self {
            region,
            supported_models,
        })
    }

    fn into_parts(self) -> (String, Vec<String>) {
        (self.region, self.supported_models)
    }
}

impl fmt::Debug for BedrockAdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BedrockAdaptorSettings")
            .field("region", &"<已脱敏>")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}

/// Google Vertex AI Google Publisher/Gemini 适配器的强类型构造配置。
///
/// 项目和 location 在构造阶段完成资源路径及区域端点校验；Service Account 凭据
/// 仍由每次请求的 [`crate::Credential`] 提供。
#[derive(Clone, Eq, PartialEq)]
pub struct VertexAdaptorSettings {
    project_id: String,
    location: String,
    supported_models: Vec<String>,
}

impl VertexAdaptorSettings {
    /// 使用显式项目、location 和模型清单创建 Vertex 配置。
    pub fn new(
        project_id: impl Into<String>,
        location: impl Into<String>,
        supported_models: Vec<String>,
    ) -> AdaptorResult<Self> {
        let project_id = project_id.into();
        let location = location.into();
        VertexAdaptor::new(project_id.clone(), location.clone())?;
        Ok(Self {
            project_id,
            location,
            supported_models,
        })
    }

    fn into_parts(self) -> (String, String, Vec<String>) {
        (self.project_id, self.location, self.supported_models)
    }
}

impl fmt::Debug for VertexAdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VertexAdaptorSettings")
            .field("project_id", &"<已脱敏>")
            .field("location", &"<已脱敏>")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}

/// 配置驱动 Custom 适配器的强类型构造配置。
///
/// 原生协议决定 Canonical 编码器；端点只能是已验证相对模板，基础地址继续由
/// [`crate::RelayContext`] 提供。认证不接受任意 Map 或脚本。
#[derive(Clone, Eq, PartialEq)]
pub struct CustomAdaptorSettings {
    protocol: Protocol,
    endpoint: CustomEndpointTemplate,
    stream_endpoint: CustomStreamEndpoint,
    authentication: CustomAuthentication,
    supported_models: Vec<String>,
}

impl CustomAdaptorSettings {
    /// 使用显式协议、端点、流式策略、认证与模型清单创建 Custom 配置。
    #[must_use]
    pub fn new(
        protocol: Protocol,
        endpoint: CustomEndpointTemplate,
        stream_endpoint: CustomStreamEndpoint,
        authentication: CustomAuthentication,
        supported_models: Vec<String>,
    ) -> Self {
        Self {
            protocol,
            endpoint,
            stream_endpoint,
            authentication,
            supported_models,
        }
    }

    fn into_parts(
        self,
    ) -> (
        Protocol,
        CustomEndpointTemplate,
        CustomStreamEndpoint,
        CustomAuthentication,
        Vec<String>,
    ) {
        (
            self.protocol,
            self.endpoint,
            self.stream_endpoint,
            self.authentication,
            self.supported_models,
        )
    }
}

impl fmt::Debug for CustomAdaptorSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomAdaptorSettings")
            .field("protocol", &self.protocol)
            .field("endpoint", &self.endpoint)
            .field("stream_endpoint", &self.stream_endpoint)
            .field("authentication", &self.authentication)
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
    /// Gemini Developer API 官方及原生协议兼容渠道配置。
    Gemini(GeminiAdaptorSettings),
    /// Jina 原生 Rerank 渠道配置。
    Jina(JinaAdaptorSettings),
    /// Cohere v2 原生 Rerank 渠道配置。
    Cohere(CohereAdaptorSettings),
    /// Amazon Bedrock Runtime SigV4 渠道配置。
    Bedrock(BedrockAdaptorSettings),
    /// Google Vertex AI Google Publisher/Gemini 渠道配置。
    Vertex(VertexAdaptorSettings),
    /// 配置驱动的 Custom JSON API 渠道配置。
    Custom(CustomAdaptorSettings),
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
            Self::Gemini(settings) => formatter
                .debug_tuple("AdaptorSettings::Gemini")
                .field(settings)
                .finish(),
            Self::Jina(settings) => formatter
                .debug_tuple("AdaptorSettings::Jina")
                .field(settings)
                .finish(),
            Self::Cohere(settings) => formatter
                .debug_tuple("AdaptorSettings::Cohere")
                .field(settings)
                .finish(),
            Self::Bedrock(settings) => formatter
                .debug_tuple("AdaptorSettings::Bedrock")
                .field(settings)
                .finish(),
            Self::Vertex(settings) => formatter
                .debug_tuple("AdaptorSettings::Vertex")
                .field(settings)
                .finish(),
            Self::Custom(settings) => formatter
                .debug_tuple("AdaptorSettings::Custom")
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
        (ChannelType::OpenAi, AdaptorSettings::OpenAi(settings)) => {
            let (protocol, models) = settings.into_parts();
            Ok(Arc::new(OpenAiAdaptor::with_protocol_and_supported_models(
                protocol, models,
            )?))
        }
        (ChannelType::Anthropic, AdaptorSettings::Anthropic(settings)) => Ok(Arc::new(
            AnthropicAdaptor::with_supported_models(settings.into_supported_models()),
        )),
        (ChannelType::Gemini, AdaptorSettings::Gemini(settings)) => Ok(Arc::new(
            GeminiAdaptor::with_supported_models(settings.into_supported_models()),
        )),
        (ChannelType::Jina, AdaptorSettings::Jina(settings)) => Ok(Arc::new(
            JinaAdaptor::with_supported_models(settings.into_supported_models()),
        )),
        (ChannelType::Cohere, AdaptorSettings::Cohere(settings)) => Ok(Arc::new(
            CohereAdaptor::with_supported_models(settings.into_supported_models()),
        )),
        (ChannelType::Bedrock, AdaptorSettings::Bedrock(settings)) => {
            let (region, models) = settings.into_parts();
            Ok(Arc::new(BedrockAdaptor::with_supported_models(
                region, models,
            )?))
        }
        (ChannelType::Vertex, AdaptorSettings::Vertex(settings)) => {
            let (project_id, location, models) = settings.into_parts();
            Ok(Arc::new(VertexAdaptor::with_supported_models(
                project_id, location, models,
            )?))
        }
        (ChannelType::Custom, AdaptorSettings::Custom(settings)) => {
            let (protocol, endpoint, stream_endpoint, authentication, models) =
                settings.into_parts();
            Ok(Arc::new(CustomAdaptor::with_supported_models(
                protocol,
                endpoint,
                stream_endpoint,
                authentication,
                models,
            )))
        }
        (
            channel_type @ (ChannelType::OpenAi
            | ChannelType::Anthropic
            | ChannelType::Gemini
            | ChannelType::Bedrock
            | ChannelType::Vertex
            | ChannelType::Jina
            | ChannelType::Cohere
            | ChannelType::Xai
            | ChannelType::Custom),
            _,
        ) => Err(AdaptorError::InvalidChannelSettings { channel_type }),
    }
}

/// 按渠道和协议创建独立异步任务适配器。
///
/// 任务适配器与普通请求适配器使用不同工厂，避免异步提交/轮询被误当成一次性响应。
pub fn get_task_adaptor(
    channel_type: ChannelType,
    protocol: Protocol,
) -> AdaptorResult<Arc<dyn VideoTaskAdaptor>> {
    match (channel_type, protocol) {
        (ChannelType::Xai, Protocol::XaiVideo) => Ok(Arc::new(XaiVideoAdaptor::new())),
        (channel_type, _) => Err(AdaptorError::InvalidChannelSettings { channel_type }),
    }
}

#[cfg(test)]
mod tests {
    use af_domain::{Operation, Protocol};
    use af_httpclient::{HeaderMap, HttpClientConfig, HttpClientPool};

    use super::*;
    use crate::{AdaptorTarget, Credential, RelayContext, ResponseMode};

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

    fn gemini_settings(models: &[&str]) -> AdaptorSettings {
        AdaptorSettings::Gemini(GeminiAdaptorSettings::new(
            models.iter().map(|model| (*model).to_owned()).collect(),
        ))
    }

    fn jina_settings(models: &[&str]) -> AdaptorSettings {
        AdaptorSettings::Jina(JinaAdaptorSettings::new(
            models.iter().map(|model| (*model).to_owned()).collect(),
        ))
    }

    fn cohere_settings(models: &[&str]) -> AdaptorSettings {
        AdaptorSettings::Cohere(CohereAdaptorSettings::new(
            models.iter().map(|model| (*model).to_owned()).collect(),
        ))
    }

    fn bedrock_settings(region: &str, models: &[&str]) -> AdaptorSettings {
        AdaptorSettings::Bedrock(
            BedrockAdaptorSettings::new(
                region,
                models.iter().map(|model| (*model).to_owned()).collect(),
            )
            .unwrap(),
        )
    }

    fn vertex_settings(project_id: &str, location: &str, models: &[&str]) -> AdaptorSettings {
        AdaptorSettings::Vertex(
            VertexAdaptorSettings::new(
                project_id,
                location,
                models.iter().map(|model| (*model).to_owned()).collect(),
            )
            .unwrap(),
        )
    }

    fn custom_settings(protocol: Protocol, models: &[&str]) -> AdaptorSettings {
        AdaptorSettings::Custom(CustomAdaptorSettings::new(
            protocol,
            CustomEndpointTemplate::parse("/private/{model}:invoke").unwrap(),
            CustomStreamEndpoint::Same,
            CustomAuthentication::Header(
                crate::CustomHeaderAuthentication::new("x-private-key", "Token {credential}")
                    .unwrap(),
            ),
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
    fn factory_builds_configured_isolated_gemini_instances() {
        let unrestricted = get_adaptor(ChannelType::Gemini, gemini_settings(&[])).unwrap();
        assert!(unrestricted.supported_models().is_empty());

        let configured_settings = gemini_settings(&["private-gemini-a", "private-gemini-b"]);
        let settings_debug = format!("{configured_settings:?}");
        assert!(settings_debug.contains("supported_model_count: 2"));
        assert!(!settings_debug.contains("private-gemini-a"));
        assert!(!settings_debug.contains("private-gemini-b"));

        let configured = get_adaptor(ChannelType::Gemini, configured_settings).unwrap();
        assert!(!Arc::ptr_eq(&unrestricted, &configured));
        assert_eq!(configured.channel_type(), ChannelType::Gemini);
        assert_eq!(configured.default_protocol(), Protocol::Gemini);
        assert_eq!(
            configured.default_base_url(),
            GeminiAdaptor::DEFAULT_BASE_URL
        );
        assert_eq!(
            configured.supported_models(),
            ["private-gemini-a", "private-gemini-b"]
        );
    }

    #[test]
    fn factory_builds_configured_isolated_jina_instances() {
        let unrestricted = get_adaptor(ChannelType::Jina, jina_settings(&[])).unwrap();
        assert!(unrestricted.supported_models().is_empty());

        let configured_settings = jina_settings(&["private-reranker-a", "private-reranker-b"]);
        let settings_debug = format!("{configured_settings:?}");
        assert!(settings_debug.contains("supported_model_count: 2"));
        assert!(!settings_debug.contains("private-reranker-a"));
        assert!(!settings_debug.contains("private-reranker-b"));

        let configured = get_adaptor(ChannelType::Jina, configured_settings).unwrap();
        assert!(!Arc::ptr_eq(&unrestricted, &configured));
        assert_eq!(configured.channel_type(), ChannelType::Jina);
        assert_eq!(configured.default_protocol(), Protocol::JinaRerank);
        assert_eq!(configured.default_base_url(), JinaAdaptor::DEFAULT_BASE_URL);
        assert_eq!(
            configured.supported_models(),
            ["private-reranker-a", "private-reranker-b"]
        );
    }

    #[test]
    fn factory_builds_configured_isolated_cohere_instances() {
        let unrestricted = get_adaptor(ChannelType::Cohere, cohere_settings(&[])).unwrap();
        assert!(unrestricted.supported_models().is_empty());

        let settings = cohere_settings(&["private-reranker-a", "private-reranker-b"]);
        let settings_debug = format!("{settings:?}");
        assert!(settings_debug.contains("supported_model_count: 2"));
        assert!(!settings_debug.contains("private-reranker-a"));

        let configured = get_adaptor(ChannelType::Cohere, settings).unwrap();
        assert_eq!(configured.channel_type(), ChannelType::Cohere);
        assert_eq!(configured.default_protocol(), Protocol::CohereRerank);
        assert_eq!(
            configured.default_base_url(),
            CohereAdaptor::DEFAULT_BASE_URL
        );
        assert_eq!(
            configured.supported_models(),
            ["private-reranker-a", "private-reranker-b"]
        );
    }

    #[test]
    fn factory_builds_configured_isolated_bedrock_instances() {
        let unrestricted =
            get_adaptor(ChannelType::Bedrock, bedrock_settings("us-east-1", &[])).unwrap();
        assert!(unrestricted.supported_models().is_empty());

        let settings = bedrock_settings(
            "eu-west-1",
            &["eu.anthropic.private-a-v1:0", "eu.anthropic.private-b-v1:0"],
        );
        let settings_debug = format!("{settings:?}");
        assert!(settings_debug.contains("supported_model_count: 2"));
        assert!(!settings_debug.contains("eu-west-1"));
        assert!(!settings_debug.contains("private-a"));

        let configured = get_adaptor(ChannelType::Bedrock, settings).unwrap();
        assert!(!Arc::ptr_eq(&unrestricted, &configured));
        assert_eq!(configured.channel_type(), ChannelType::Bedrock);
        assert_eq!(configured.default_protocol(), Protocol::Anthropic);
        assert_eq!(
            configured.default_base_url(),
            "https://bedrock-runtime.eu-west-1.amazonaws.com"
        );
        assert_eq!(
            configured.supported_models(),
            ["eu.anthropic.private-a-v1:0", "eu.anthropic.private-b-v1:0"]
        );
    }

    #[test]
    fn factory_builds_configured_isolated_vertex_instances() {
        let unrestricted = get_adaptor(
            ChannelType::Vertex,
            vertex_settings("vertex-project", "global", &[]),
        )
        .unwrap();
        assert!(unrestricted.supported_models().is_empty());

        let settings = vertex_settings(
            "vertex-project",
            "us-central1",
            &["private-gemini-a", "private-gemini-b"],
        );
        let settings_debug = format!("{settings:?}");
        assert!(settings_debug.contains("supported_model_count: 2"));
        for private in [
            "vertex-project",
            "us-central1",
            "private-gemini-a",
            "private-gemini-b",
        ] {
            assert!(!settings_debug.contains(private));
        }

        let configured = get_adaptor(ChannelType::Vertex, settings).unwrap();
        assert!(!Arc::ptr_eq(&unrestricted, &configured));
        assert_eq!(configured.channel_type(), ChannelType::Vertex);
        assert_eq!(configured.default_protocol(), Protocol::Gemini);
        assert_eq!(
            configured.default_base_url(),
            "https://us-central1-aiplatform.googleapis.com"
        );
        assert_eq!(
            configured.supported_models(),
            ["private-gemini-a", "private-gemini-b"]
        );
    }

    #[test]
    fn factory_builds_configured_isolated_custom_instances() {
        let settings = custom_settings(
            Protocol::OpenAiChat,
            &["private-custom-a", "private-custom-b"],
        );
        let settings_debug = format!("{settings:?}");
        assert!(settings_debug.contains("protocol: OpenAiChat"));
        assert!(settings_debug.contains("supported_model_count: 2"));
        for private in [
            "private/{model}",
            "x-private-key",
            "Token",
            "private-custom-a",
            "private-custom-b",
        ] {
            assert!(!settings_debug.contains(private));
        }

        let configured = get_adaptor(ChannelType::Custom, settings).unwrap();
        assert_eq!(configured.channel_type(), ChannelType::Custom);
        assert_eq!(configured.default_protocol(), Protocol::OpenAiChat);
        assert_eq!(configured.default_base_url(), "");
        assert_eq!(
            configured.supported_models(),
            ["private-custom-a", "private-custom-b"]
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
                gemini_settings(&["private-gemini-model"]),
            ),
            (
                ChannelType::Gemini,
                openai_settings(&["private-openai-model"]),
            ),
            (
                ChannelType::Bedrock,
                gemini_settings(&["private-gemini-model-for-bedrock"]),
            ),
            (
                ChannelType::Jina,
                openai_settings(&["private-openai-model-for-jina"]),
            ),
            (
                ChannelType::Cohere,
                jina_settings(&["private-jina-model-for-cohere"]),
            ),
            (
                ChannelType::Xai,
                openai_settings(&["private-openai-model-for-xai"]),
            ),
            (
                ChannelType::Vertex,
                bedrock_settings("us-east-1", &["private-bedrock-model-for-vertex"]),
            ),
            (
                ChannelType::Custom,
                openai_settings(&["private-openai-model-for-custom"]),
            ),
            (
                ChannelType::OpenAi,
                custom_settings(Protocol::OpenAiChat, &["private-custom-model-for-openai"]),
            ),
        ];
        for (channel_type, settings) in cases {
            let error = get_adaptor(channel_type, settings)
                .err()
                .expect("错配配置必须拒绝");
            assert_eq!(error, AdaptorError::InvalidChannelSettings { channel_type });
            let rendered = format!("{error:?}\n{error}");
            assert!(!rendered.contains("private-anthropic-model"));
            assert!(!rendered.contains("private-gemini-model"));
            assert!(!rendered.contains("private-openai-model"));
            assert!(!rendered.contains("private-bedrock-model-for-vertex"));
            assert!(!rendered.contains("private-openai-model-for-jina"));
            assert!(!rendered.contains("private-jina-model-for-cohere"));
            assert!(!rendered.contains("private-openai-model-for-xai"));
            assert!(!rendered.contains("private-openai-model-for-custom"));
            assert!(!rendered.contains("private-custom-model-for-openai"));
        }
    }

    #[test]
    fn factory_channel_enum_has_no_unimplemented_placeholders() {
        assert_eq!(ChannelType::ALL.len(), 9);
        assert!(ChannelType::ALL.contains(&ChannelType::Jina));
        assert!(ChannelType::ALL.contains(&ChannelType::Cohere));
        assert!(ChannelType::ALL.contains(&ChannelType::Xai));
        assert!(ChannelType::ALL.contains(&ChannelType::Custom));
    }

    #[test]
    fn task_factory_only_accepts_the_native_xai_video_pair() {
        assert!(get_task_adaptor(ChannelType::Xai, Protocol::XaiVideo).is_ok());
        for (channel_type, protocol) in [
            (ChannelType::Xai, Protocol::OpenAiImages),
            (ChannelType::OpenAi, Protocol::XaiVideo),
        ] {
            assert_eq!(
                get_task_adaptor(channel_type, protocol)
                    .err()
                    .expect("错配任务协议必须拒绝"),
                AdaptorError::InvalidChannelSettings { channel_type }
            );
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
            adaptor
                .build_url(
                    &context,
                    AdaptorTarget::new(
                        "private-model-contract",
                        Operation::Chat,
                        ResponseMode::Full,
                    ),
                )
                .unwrap(),
            "https://private-upstream.example/proxy/v1/chat/completions"
        );
        assert_eq!(headers["authorization"], "Bearer private-credential");
        assert_eq!(headers["x-request-id"], "private-request-id");
        let error = adaptor
            .build_url(
                &context,
                AdaptorTarget::new(
                    "private-model-contract",
                    Operation::Responses,
                    ResponseMode::Full,
                ),
            )
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
