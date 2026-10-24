use std::time::Duration;

#[cfg(test)]
#[path = "conversation_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "conversation_matrix_contract.rs"]
mod matrix_contract;

#[cfg(test)]
#[path = "compaction_contract.rs"]
mod compaction_contract;

use af_adapter::{Bytes, Method, ResponseMode, StatusCode};
use af_domain::{AfError, Operation, Protocol, UpstreamError};
use af_protocol::{
    CanonicalRequest, CanonicalRequestEnvelope, CanonicalResponse, Usage, anthropic, gemini,
    openai_chat, openai_responses,
};

use crate::{
    ChatResponse, GenerationStream, OpenAiChatStream, RelayAttemptReport, RelayError, RelayRequest,
    RelayStateMachine,
    anthropic_request::prepare_anthropic_request,
    attempt_gate::permit_completion_hook,
    chat_stream_encoder::ChatStreamEncoder,
    conversation_decoder::ConversationDecoder,
    error::{classify_upstream_status, map_adaptor_error, parse_retry_after},
    gemini_request::prepare_gemini_request,
    openai_chat::unix_timestamp,
    openai_chat_request::prepare_openai_chat_request,
    openai_chat_usage::OpenAiChatUsageEstimator,
    openai_responses_request::prepare_openai_responses_request,
};

/// 在四种对话协议间转换，复用候选调度、并发许可、usage 和流结束回调。
/// 原生同协议转发仍使用各自入口，保留同源字段及专属能力。
pub async fn relay_conversation_with_report(
    machine: &RelayStateMachine,
    request: CanonicalRequestEnvelope,
    upstream_protocol: Protocol,
    response_protocol: Protocol,
    request_id: &str,
) -> (Result<ChatResponse, AfError>, RelayAttemptReport) {
    let anthropic_beta = request.anthropic_beta().map(str::to_owned);
    let client_model = request.requested_model().to_owned();
    let include_usage = request.canonical().stream_options.include_usage();
    let (canonical, body) = match prepare_request(request, upstream_protocol) {
        Ok(prepared) => prepared,
        Err(error) => return (Err(error), RelayAttemptReport::default()),
    };
    let created_at = match unix_timestamp() {
        Ok(value) => value,
        Err(error) => return (Err(error), RelayAttemptReport::default()),
    };
    let encoder = if canonical.stream {
        match ChatStreamEncoder::new(
            response_protocol,
            request_id,
            &client_model,
            created_at,
            &canonical,
            include_usage,
        ) {
            Ok(encoder) => Some(encoder.with_upstream_protocol(upstream_protocol)),
            Err(error) => return (Err(error), RelayAttemptReport::default()),
        }
    } else {
        None
    };
    let mode = if canonical.stream {
        ResponseMode::Stream
    } else {
        ResponseMode::Full
    };
    let outbound = match RelayRequest::new(
        canonical.model.clone(),
        canonical.operation,
        Method::POST,
        Some(body),
    )
    .and_then(|request| request.with_anthropic_beta(anthropic_beta.as_deref()))
    {
        Ok(request) => request.with_response_mode(mode),
        Err(_) => return (Err(AfError::Internal), RelayAttemptReport::default()),
    };
    let (response, mut report, permit) = match machine.execute_with_report(outbound).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            let error = match error {
                RelayError::Upstream(error) => error.into(),
                RelayError::ConcurrencyUnavailable => AfError::ConcurrencyLimited,
                _ => AfError::Internal,
            };
            return (Err(error), report);
        }
    };
    let status = response.status();
    let retry_after = parse_retry_after(response.headers());
    let estimator = OpenAiChatUsageEstimator::new(&canonical);
    let result = async {
        if status != StatusCode::OK {
            let body = response
                .into_body()
                .into_bytes()
                .await
                .map_err(map_adaptor_error)?;
            return Err(classify_upstream_status(status, retry_after, &body));
        }
        if let Some(encoder) = encoder {
            let decoder = ConversationDecoder::new(upstream_protocol)?;
            let (stream, usage) = OpenAiChatStream::new(response.into_body(), encoder, estimator);
            let stream = stream
                .with_decoder(decoder)
                .prefetch_first(Duration::from_secs(30))
                .await?;
            let mut body: Box<dyn GenerationStream> = Box::new(stream);
            if let Some(permit) = permit {
                body = body.with_completion_hook(permit_completion_hook(permit));
            }
            return Ok(ChatResponse::Stream { body, usage });
        }
        let is_sse = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| {
                h.split(';')
                    .next()
                    .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"))
            });
        let mut response = if is_sse {
            crate::collect_conversation_response(upstream_protocol, response.into_body()).await?
        } else {
            let body = response
                .into_body()
                .into_bytes()
                .await
                .map_err(map_adaptor_error)?;
            parse_response(upstream_protocol, &body)?
        };
        let usage = if upstream_protocol == Protocol::Anthropic {
            anthropic::billing_usage(&response).map_err(|_| UpstreamError::ProtocolError)?
        } else {
            None
        }
        .map_or_else(
            || {
                estimator
                    .resolve_response(&response)
                    .map_err(|_| UpstreamError::ProtocolError)
            },
            Ok,
        )?;
        crate::anthropic_chat_bridge::adapt_response(
            upstream_protocol,
            response_protocol,
            &mut response,
        )?;
        let body = build_response(
            response_protocol,
            response,
            usage,
            request_id,
            &client_model,
            created_at,
        )?;
        Ok(ChatResponse::Full {
            body,
            usage: Ok(usage),
        })
    }
    .await;
    match result {
        Ok(response) => (Ok(response), report),
        Err(error) => {
            report.reject_success(error);
            (Err(error.into()), report)
        }
    }
}

pub(crate) fn prepare_request(
    request: CanonicalRequestEnvelope,
    protocol: Protocol,
) -> Result<(CanonicalRequest, Bytes), AfError> {
    // 候选选择与实际执行必须使用同一份目标转换计划。
    let (request, _) = request
        .plan_conversation(protocol)
        .ok_or(AfError::InvalidRequest)?
        .into_parts();
    match protocol {
        Protocol::OpenAiChat => prepare_openai_chat_request(request).map(|r| (r.canonical, r.body)),
        Protocol::OpenAiResponses => {
            prepare_openai_responses_request(request).map(|r| (r.canonical, r.body))
        }
        Protocol::Anthropic => prepare_anthropic_request(request).map(|r| (r.canonical, r.body)),
        Protocol::Gemini => prepare_gemini_request(request).map(|r| (r.canonical, r.body)),
        _ => Err(AfError::InvalidRequest),
    }
}

pub(crate) fn parse_response(
    protocol: Protocol,
    body: &[u8],
) -> Result<CanonicalResponse, UpstreamError> {
    match protocol {
        Protocol::OpenAiChat => {
            openai_chat::parse_response(body).map_err(|_| UpstreamError::ProtocolError)
        }
        Protocol::OpenAiResponses => crate::openai_responses::parse_full_response_body(body)
            .map_err(|_| UpstreamError::ProtocolError),
        Protocol::Anthropic => {
            anthropic::parse_response(body).map_err(|_| UpstreamError::ProtocolError)
        }
        Protocol::Gemini => gemini::parse_response(body).map_err(|_| UpstreamError::ProtocolError),
        _ => Err(UpstreamError::ProtocolError),
    }
}

pub(crate) fn build_response(
    protocol: Protocol,
    mut response: CanonicalResponse,
    usage: Usage,
    request_id: &str,
    model: &str,
    created_at: i64,
) -> Result<Bytes, UpstreamError> {
    let (operation, id) = match protocol {
        Protocol::OpenAiChat => (Operation::Chat, format!("chatcmpl-{request_id}")),
        Protocol::OpenAiResponses => (Operation::Responses, format!("resp_{request_id}")),
        Protocol::Anthropic => (Operation::Chat, format!("msg_{request_id}")),
        Protocol::Gemini => (Operation::Chat, format!("response-{request_id}")),
        _ => return Err(UpstreamError::ProtocolError),
    };
    // 同源 Anthropic 用量分为顶层 wire 快照和额外压缩迭代；公开响应保留前者。
    // 跨协议适配已经清除来源 raw，此时使用完成内部结算后的归一用量。
    let native_anthropic = protocol == Protocol::Anthropic
        && response
            .raw_passthrough()
            .is_some_and(|raw| raw.fields_for_protocol(Protocol::Anthropic).is_ok());
    response.operation = operation;
    response.id = id;
    response.model = model.to_owned();
    response.created_at = Some(response.created_at.unwrap_or(created_at));
    if !native_anthropic {
        response = CanonicalResponse::new(
            response.operation,
            response.id,
            response.model,
            response.created_at,
            response.choices,
            Some(crate::anthropic_chat_bridge::adapt_usage(protocol, usage)?),
        );
    }
    let value = match protocol {
        Protocol::OpenAiChat => {
            openai_chat::build_response(&response).map_err(|_| UpstreamError::ProtocolError)?
        }
        Protocol::OpenAiResponses => {
            openai_responses::build_response(&response).map_err(|_| UpstreamError::ProtocolError)?
        }
        Protocol::Anthropic => {
            anthropic::build_response(&response).map_err(|_| UpstreamError::ProtocolError)?
        }
        Protocol::Gemini => {
            gemini::build_response(&response).map_err(|_| UpstreamError::ProtocolError)?
        }
        _ => return Err(UpstreamError::ProtocolError),
    };
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| UpstreamError::ProtocolError)
}
