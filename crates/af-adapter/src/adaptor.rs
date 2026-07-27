use af_domain::{ChannelType, Operation, Protocol};
use af_httpclient::{Body, HeaderMap};
use async_trait::async_trait;

use crate::{
    AdaptorResult, Credential, RelayContext, ResponseMode, UpstreamRequest, UpstreamResponse,
};

/// 上游供应商的对象安全请求适配契约。
///
/// 协议 JSON 与 Canonical IR 的互转属于 `af-protocol`；本 trait 只负责上游地址和
/// 鉴权头。发送能力由 [`AdaptorSendExt`] 统一提供且不可覆盖，防止实现绕过受控
/// Client、响应模式、容量限制和错误脱敏边界。
pub trait Adaptor: Send + Sync {
    /// 返回适配器对应的稳定渠道种类。
    fn channel_type(&self) -> ChannelType;

    /// 返回该上游原生使用的协议。
    fn default_protocol(&self) -> Protocol;

    /// 返回未应用渠道覆盖时的基础地址。
    fn default_base_url(&self) -> &str;

    /// 返回适配器内建支持的模型名；配置驱动模型可返回空列表。
    fn supported_models(&self) -> Vec<String>;

    /// 根据转发上下文与操作构造完整上游 URL。
    fn build_url(&self, context: &RelayContext, operation: Operation) -> AdaptorResult<String>;

    /// 根据已准备的凭据设置认证头。
    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()>;
}

/// 为所有适配器统一提供不可覆盖的受控 HTTP 发送能力。
///
/// WebSocket 等非 HTTP 传输必须在后续切片通过受限 transport hook 接入统一
/// dispatcher，不得在适配器实现中自建绕过超时与取消传播的发送路径。
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
        let (method, target, headers, body, response_mode) = request.into_parts();
        let response = context
            .http_client()
            .execute(method, &target, headers, body.map(Body::from))
            .await?;
        match response_mode {
            ResponseMode::Full => UpstreamResponse::from_http_full(response).await,
            ResponseMode::Stream => UpstreamResponse::from_http_stream(response),
        }
    }
}
