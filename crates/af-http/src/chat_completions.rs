use std::sync::Arc;

use af_admin::{
    AdminChannelReader, AdminChannelWriter, AdminGroupReader, AdminGroupWriter,
    AdminModelPriceService, AdminModelReader, AdminModelSyncService, AdminModelWriter,
    AdminTokenReader, AdminTokenWriter, AdminUsageLogReader, AdminUserReader, AdminUserWriter,
    InitialSetup, SessionAuthenticator, TokenAuthentication,
};
use af_analytics::AdminDashboardReader;
use af_domain::{AfError, Protocol, TokenModelPolicy};
use af_protocol::{
    CanonicalRequestEnvelope, apply_reasoning_model_suffix, openai_chat::parse_request_envelope,
};
use af_relay::ChatResponse;
use af_telemetry::RequestId;
use axum::{
    body::Bytes,
    extract::{Extension, State},
    response::{IntoResponse, Response},
};
use http::{
    HeaderValue, StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE},
};

use crate::{
    ChatService, OpenAiHttpError,
    streaming::{StreamDeliveryPolicy, openai_chat_body},
};

/// HTTP 层持有的框架私有服务状态。
#[derive(Clone)]
pub(crate) struct HttpState {
    chat: Arc<dyn ChatService>,
    pub(crate) session_authenticator: Arc<dyn SessionAuthenticator>,
    pub(crate) initial_setup: Option<Arc<dyn InitialSetup>>,
    pub(crate) model_catalog_reader: Option<Arc<dyn af_admin::ModelCatalogReader>>,
    pub(crate) admin_channel_reader: Option<Arc<dyn AdminChannelReader>>,
    pub(crate) admin_channel_writer: Option<Arc<dyn AdminChannelWriter>>,
    pub(crate) admin_channel_probe: Option<Arc<dyn crate::AdminChannelProbe>>,
    pub(crate) admin_dashboard_reader: Option<Arc<dyn AdminDashboardReader>>,
    pub(crate) admin_group_reader: Arc<dyn AdminGroupReader>,
    pub(crate) admin_group_writer: Arc<dyn AdminGroupWriter>,
    pub(crate) admin_model_reader: Option<Arc<dyn AdminModelReader>>,
    pub(crate) admin_model_price_service: Option<Arc<dyn AdminModelPriceService>>,
    pub(crate) admin_model_writer: Option<Arc<dyn AdminModelWriter>>,
    pub(crate) admin_model_sync_service: Option<Arc<dyn AdminModelSyncService>>,
    pub(crate) admin_token_reader: Arc<dyn AdminTokenReader>,
    pub(crate) admin_token_writer: Arc<dyn AdminTokenWriter>,
    pub(crate) admin_usage_log_reader: Option<Arc<dyn AdminUsageLogReader>>,
    pub(crate) admin_user_reader: Arc<dyn AdminUserReader>,
    pub(crate) admin_user_writer: Arc<dyn AdminUserWriter>,
    stream_delivery: StreamDeliveryPolicy,
}

impl HttpState {
    pub(crate) fn new(
        chat: Arc<dyn ChatService>,
        session_authenticator: Arc<dyn SessionAuthenticator>,
        admin_group_reader: Arc<dyn AdminGroupReader>,
        admin_group_writer: Arc<dyn AdminGroupWriter>,
        admin_token_reader: Arc<dyn AdminTokenReader>,
        admin_token_writer: Arc<dyn AdminTokenWriter>,
        admin_user_reader: Arc<dyn AdminUserReader>,
        admin_user_writer: Arc<dyn AdminUserWriter>,
    ) -> Self {
        Self {
            chat,
            session_authenticator,
            initial_setup: None,
            model_catalog_reader: None,
            admin_channel_reader: None,
            admin_channel_writer: None,
            admin_channel_probe: None,
            admin_dashboard_reader: None,
            admin_group_reader,
            admin_group_writer,
            admin_model_reader: None,
            admin_model_price_service: None,
            admin_model_writer: None,
            admin_model_sync_service: None,
            admin_token_reader,
            admin_token_writer,
            admin_usage_log_reader: None,
            admin_user_reader,
            admin_user_writer,
            stream_delivery: StreamDeliveryPolicy::default(),
        }
    }

    /// 为同源首次安装路由注入一次性 setup 服务。
    pub(crate) fn with_initial_setup(mut self, setup: Arc<dyn InitialSetup>) -> Self {
        self.initial_setup = Some(setup);
        self
    }

    /// 为游客与登录用户模型目录路由注入运行时只读服务。
    pub(crate) fn with_model_catalog_reader(
        mut self,
        reader: Arc<dyn af_admin::ModelCatalogReader>,
    ) -> Self {
        self.model_catalog_reader = Some(reader);
        self
    }

    /// 为用量日志管理路由注入真实只读服务。
    pub(crate) fn with_admin_usage_log_reader(
        mut self,
        reader: Arc<dyn AdminUsageLogReader>,
    ) -> Self {
        self.admin_usage_log_reader = Some(reader);
        self
    }

    /// 为模型商品元数据管理路由注入真实只读服务。
    pub(crate) fn with_admin_model_reader(
        mut self,
        reader: Option<Arc<dyn AdminModelReader>>,
    ) -> Self {
        self.admin_model_reader = reader;
        self
    }

    /// 为正式模型价格、公开参考价和原子应用路由注入真实服务。
    pub(crate) fn with_admin_model_price_service(
        mut self,
        service: Option<Arc<dyn AdminModelPriceService>>,
    ) -> Self {
        self.admin_model_price_service = service;
        self
    }

    /// 为模型商品元数据管理路由注入真实写入服务。
    pub(crate) fn with_admin_model_writer(
        mut self,
        writer: Option<Arc<dyn AdminModelWriter>>,
    ) -> Self {
        self.admin_model_writer = writer;
        self
    }

    /// 为模型缺失检测、同步预览与原子应用路由注入真实服务。
    pub(crate) fn with_admin_model_sync_service(
        mut self,
        service: Option<Arc<dyn AdminModelSyncService>>,
    ) -> Self {
        self.admin_model_sync_service = service;
        self
    }

    /// 为管理看板路由注入真实聚合读取服务。
    pub(crate) fn with_admin_dashboard_reader(
        mut self,
        reader: Arc<dyn AdminDashboardReader>,
    ) -> Self {
        self.admin_dashboard_reader = Some(reader);
        self
    }

    /// 为包含渠道管理路由的组合根注入真实只读服务。
    pub(crate) fn with_admin_channel_reader(mut self, reader: Arc<dyn AdminChannelReader>) -> Self {
        self.admin_channel_reader = Some(reader);
        self
    }

    /// 为包含渠道管理写路由的组合根注入真实写入服务。
    pub(crate) fn with_admin_channel_writer(mut self, writer: Arc<dyn AdminChannelWriter>) -> Self {
        self.admin_channel_writer = Some(writer);
        self
    }

    /// 为渠道管理路由注入可选的真实测活服务。
    pub(crate) fn with_admin_channel_probe(
        mut self,
        probe: Option<Arc<dyn crate::AdminChannelProbe>>,
    ) -> Self {
        self.admin_channel_probe = probe;
        self
    }

    /// 返回协议入口共享的 Chat 执行端口。
    pub(crate) fn chat_service(&self) -> &Arc<dyn ChatService> {
        &self.chat
    }

    /// 返回统一的下游 SSE 交付策略。
    pub(crate) const fn stream_delivery(&self) -> StreamDeliveryPolicy {
        self.stream_delivery
    }
}

/// 解析受限正文、执行请求级令牌策略，再把不可变请求信封交给 Relay。
pub(crate) async fn chat_completions(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    body: Bytes,
) -> Result<Response, OpenAiHttpError> {
    let request = prepare_chat_request(body, authentication.model_policy())?;
    let principal = authentication.principal();
    let response = state
        .chat
        .chat_completions(
            &principal,
            request,
            Protocol::OpenAiChat,
            request_id.as_str(),
        )
        .await
        .map_err(OpenAiHttpError::from)?;
    Ok(render_chat_response(response, state.stream_delivery))
}

/// 把已按客户端协议编码的 Relay 结果转换为统一 HTTP 完整或 SSE 响应。
pub(crate) fn render_chat_response(
    response: ChatResponse,
    stream_delivery: StreamDeliveryPolicy,
) -> Response {
    match response {
        ChatResponse::Full { body, usage: _ } => {
            (StatusCode::OK, [(CONTENT_TYPE, "application/json")], body).into_response()
        }
        ChatResponse::Stream { body, usage: _ } => {
            let mut response = openai_chat_body(body, stream_delivery).into_response();
            response.headers_mut().insert(
                CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream; charset=utf-8"),
            );
            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_static("no-cache, no-transform"),
            );
            response.headers_mut().insert(
                http::HeaderName::from_static("x-accel-buffering"),
                HeaderValue::from_static("no"),
            );
            // 不设置 Content-Length 或 Content-Encoding，交由 Hyper 按流式帧发送。
            *response.status_mut() = StatusCode::OK;
            response
        }
    }
}

/// 在令牌策略、计费和调度前完成协议解析与模型后缀归一。
fn prepare_chat_request(
    body: Bytes,
    model_policy: &TokenModelPolicy,
) -> Result<CanonicalRequestEnvelope, OpenAiHttpError> {
    let request =
        parse_request_envelope(body).map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
    prepare_canonical_request(request, model_policy).map_err(OpenAiHttpError::from)
}

/// 在所有 Chat 协议入口共享模型后缀和令牌白名单语义。
pub(crate) fn prepare_canonical_request(
    request: CanonicalRequestEnvelope,
    model_policy: &TokenModelPolicy,
) -> Result<CanonicalRequestEnvelope, AfError> {
    let request = apply_reasoning_model_suffix(request).map_err(|_| AfError::InvalidRequest)?;
    if !model_policy.allows(&request.canonical().model) {
        return Err(AfError::ModelNotAllowed);
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use af_protocol::ReasoningEffort;

    use super::*;

    fn restricted(models: &[&str]) -> TokenModelPolicy {
        TokenModelPolicy::try_from_allowlist(
            models.iter().map(|model| (*model).to_owned()).collect(),
        )
        .unwrap()
    }

    #[test]
    fn suffix_uses_base_model_for_token_policy_and_downstream_request() {
        let body = Bytes::from_static(
            br#"{"model":"gpt-5-high","messages":[{"role":"user","content":"hello"}]}"#,
        );
        let request = prepare_chat_request(body, &restricted(&["gpt-5"])).unwrap();

        assert_eq!(request.canonical().model, "gpt-5");
        assert_eq!(request.requested_model(), "gpt-5-high");
        assert_eq!(
            request.canonical().reasoning.unwrap().effort(),
            Some(ReasoningEffort::High)
        );
    }

    #[test]
    fn suffix_alias_does_not_bypass_base_model_allowlist() {
        let body = Bytes::from_static(
            br#"{"model":"gpt-5-high","messages":[{"role":"user","content":"hello"}]}"#,
        );
        assert!(prepare_chat_request(body, &restricted(&["gpt-5-high"])).is_err());
    }

    #[test]
    fn explicit_reasoning_conflict_is_rejected_before_dispatch() {
        let body = Bytes::from_static(
            br#"{"model":"gpt-5-high","messages":[{"role":"user","content":"hello"}],"reasoning_effort":"low"}"#,
        );
        assert!(prepare_chat_request(body, &TokenModelPolicy::unrestricted()).is_err());
    }
}
