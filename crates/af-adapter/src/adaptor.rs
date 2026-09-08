use std::fmt;

use af_domain::{ChannelType, Operation, Protocol};
use af_httpclient::HeaderMap;
use async_trait::async_trait;

use crate::{
    AdaptorResult, Credential, RelayContext, ResponseMode, TransportDispatcher, UpstreamRequest,
    UpstreamResponse,
};

/// 适配器构造上游 URL 所需的最小请求目标投影。
///
/// Gemini 等协议会把模型和流式模式编码进 URL；该类型让适配器获得必要信息，同时
/// 保持请求正文、身份和业务状态仍由上层持有。Debug 固定隐藏模型名。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AdaptorTarget<'a> {
    model: &'a str,
    operation: Operation,
    response_mode: ResponseMode,
}

impl<'a> AdaptorTarget<'a> {
    /// 创建一次已由协议层和转发层校验的上游目标投影。
    #[must_use]
    pub const fn new(model: &'a str, operation: Operation, response_mode: ResponseMode) -> Self {
        Self {
            model,
            operation,
            response_mode,
        }
    }

    /// 返回已完成模型映射的上游模型名。
    #[must_use]
    pub const fn model(self) -> &'a str {
        self.model
    }

    /// 返回本次请求的规范化操作。
    #[must_use]
    pub const fn operation(self) -> Operation {
        self.operation
    }

    /// 返回上层选择的响应交付模式。
    #[must_use]
    pub const fn response_mode(self) -> ResponseMode {
        self.response_mode
    }
}

impl fmt::Debug for AdaptorTarget<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdaptorTarget")
            .field("model", &"<已脱敏>")
            .field("operation", &self.operation)
            .field("response_mode", &self.response_mode)
            .finish()
    }
}

/// 上游供应商的对象安全请求适配契约。
///
/// 协议 JSON 与 Canonical IR 的互转属于 `af-protocol`；本 trait 只负责上游地址和
/// 鉴权头。发送能力由 [`AdaptorSendExt`] 统一提供且不可覆盖，防止实现绕过受控
/// Client、响应模式、容量限制和错误脱敏边界。
#[async_trait]
pub trait Adaptor: Send + Sync {
    /// 返回适配器对应的稳定渠道种类。
    fn channel_type(&self) -> ChannelType;

    /// 返回该上游原生使用的协议。
    fn default_protocol(&self) -> Protocol;

    /// 返回未应用渠道覆盖时的基础地址。
    fn default_base_url(&self) -> &str;

    /// 返回适配器内建支持的模型名；配置驱动模型可返回空列表。
    fn supported_models(&self) -> Vec<String>;

    /// 根据转发上下文与请求目标构造完整上游 URL。
    fn build_url(&self, context: &RelayContext, target: AdaptorTarget<'_>)
    -> AdaptorResult<String>;

    /// 根据已准备的凭据设置认证头。
    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()>;

    /// 在 URL、正文和覆盖 Header 全部确定后最终化请求。
    ///
    /// 普通 Bearer/API Key 适配器保持原请求不变；SigV4 签名或 Service Account
    /// token 交换在此阶段使用受控上下文完成，避免后续 Header 覆盖认证结果。
    async fn finalize_request(
        &self,
        request: UpstreamRequest,
        _credential: &Credential,
        _context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        Ok(request)
    }
}

/// 为所有适配器统一提供不可覆盖的默认 HTTP 发送能力。
///
/// 生产 Relay 可显式装配 [`TransportDispatcher`] 选择受控特殊传输；直接调用本扩展
/// 始终保持 HTTP，避免适配器根据正文或供应商名称隐式改变传输语义。
#[async_trait]
pub trait AdaptorSendExt: Adaptor {
    /// 发起上游请求；默认完整收集响应，显式流模式才保留背压与取消传播。
    async fn send(
        &self,
        request: UpstreamRequest,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamResponse>;
}

#[async_trait]
impl<T> AdaptorSendExt for T
where
    T: Adaptor + ?Sized,
{
    async fn send(
        &self,
        request: UpstreamRequest,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamResponse> {
        TransportDispatcher::http().send(request, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptor_target_preserves_routing_fields_and_redacts_model_debug() {
        let target = AdaptorTarget::new(
            "private-model-canary",
            Operation::Chat,
            ResponseMode::Stream,
        );
        assert_eq!(target.model(), "private-model-canary");
        assert_eq!(target.operation(), Operation::Chat);
        assert_eq!(target.response_mode(), ResponseMode::Stream);
        let debug = format!("{target:?}");
        assert!(debug.contains("operation: Chat"));
        assert!(debug.contains("response_mode: Stream"));
        assert!(!debug.contains("private-model-canary"));
    }
}
