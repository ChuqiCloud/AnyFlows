use std::sync::Arc;

use af_admin::TokenAuthentication;
use af_domain::AfError;
use af_protocol::openai_chat::parse_request;
use af_relay::{OpenAiChatResponse, RelayService};
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
    OpenAiHttpError,
    streaming::{StreamDeliveryPolicy, openai_chat_body},
};

/// HTTP 层持有的框架私有服务状态。
#[derive(Clone)]
pub(crate) struct HttpState {
    relay: Arc<RelayService>,
    stream_delivery: StreamDeliveryPolicy,
}

impl HttpState {
    pub(crate) fn new(relay: Arc<RelayService>) -> Self {
        Self {
            relay,
            stream_delivery: StreamDeliveryPolicy::default(),
        }
    }
}

/// 解析受限正文、执行请求级令牌策略，再把 Canonical 请求交给 Relay。
pub(crate) async fn chat_completions(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    body: Bytes,
) -> Result<Response, OpenAiHttpError> {
    let canonical =
        parse_request(&body).map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
    if !authentication.model_policy().allows(&canonical.model) {
        return Err(OpenAiHttpError::from(AfError::ModelNotAllowed));
    }
    let principal = authentication.principal();
    let response = state
        .relay
        .chat_completions(&principal, canonical, request_id.as_str())
        .await
        .map_err(OpenAiHttpError::from)?;
    match response {
        OpenAiChatResponse::Full(body) => {
            Ok((StatusCode::OK, [(CONTENT_TYPE, "application/json")], body).into_response())
        }
        OpenAiChatResponse::Stream(stream) => {
            let mut response = openai_chat_body(stream, state.stream_delivery).into_response();
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
