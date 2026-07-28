use std::sync::Arc;

use af_admin::{
    AdminChannelReader, AdminChannelWriter, AdminGroupReader, AdminGroupWriter, AdminTokenReader,
    AdminTokenWriter, AdminUsageLogReader, AdminUserReader, AdminUserWriter, InitialSetup,
    SessionAuthenticator, TokenAuthentication,
};
use af_analytics::AdminDashboardReader;
use af_domain::AfError;
use af_protocol::openai_chat::parse_request_envelope;
use af_relay::OpenAiChatResponse;
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
    pub(crate) admin_channel_reader: Option<Arc<dyn AdminChannelReader>>,
    pub(crate) admin_channel_writer: Option<Arc<dyn AdminChannelWriter>>,
    pub(crate) admin_channel_probe: Option<Arc<dyn crate::AdminChannelProbe>>,
    pub(crate) admin_dashboard_reader: Option<Arc<dyn AdminDashboardReader>>,
    pub(crate) admin_group_reader: Arc<dyn AdminGroupReader>,
    pub(crate) admin_group_writer: Arc<dyn AdminGroupWriter>,
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
            admin_channel_reader: None,
            admin_channel_writer: None,
            admin_channel_probe: None,
            admin_dashboard_reader: None,
            admin_group_reader,
            admin_group_writer,
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

    /// 为用量日志管理路由注入真实只读服务。
    pub(crate) fn with_admin_usage_log_reader(
        mut self,
        reader: Arc<dyn AdminUsageLogReader>,
    ) -> Self {
        self.admin_usage_log_reader = Some(reader);
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
}

/// 解析受限正文、执行请求级令牌策略，再把不可变请求信封交给 Relay。
pub(crate) async fn chat_completions(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    body: Bytes,
) -> Result<Response, OpenAiHttpError> {
    let request =
        parse_request_envelope(body).map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
    if !authentication
        .model_policy()
        .allows(&request.canonical().model)
    {
        return Err(OpenAiHttpError::from(AfError::ModelNotAllowed));
    }
    let principal = authentication.principal();
    let response = state
        .chat
        .chat_completions(&principal, request, request_id.as_str())
        .await
        .map_err(OpenAiHttpError::from)?;
    match response {
        OpenAiChatResponse::Full { body, usage: _ } => {
            Ok((StatusCode::OK, [(CONTENT_TYPE, "application/json")], body).into_response())
        }
        OpenAiChatResponse::Stream { body, usage: _ } => {
            let mut response = openai_chat_body(body, state.stream_delivery).into_response();
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
            Ok(response)
        }
    }
}
