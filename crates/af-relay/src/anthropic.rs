use std::{fmt, time::Duration};

use af_adapter::{Method, Operation, ResponseMode, StatusCode};
use af_domain::{AfError, UpstreamError};
use af_protocol::{CanonicalRequestEnvelope, CanonicalResponse, Usage, anthropic};

use crate::anthropic_request::{PreparedAnthropicRequest, prepare_anthropic_request};
use crate::attempt_gate::permit_completion_hook;
use crate::error::{classify_upstream_status, map_adaptor_error, parse_retry_after};
use crate::{
    AnthropicMessagesStream, ChatResponse, GenerationStream, RelayAttemptReport, RelayError,
    RelayRequest, RelayStateMachine,
};

const STREAM_FIRST_EVENT_TIMEOUT: Duration = Duration::from_secs(30);

/// 使用固定候选状态机执行原生 Anthropic Messages 转发。
pub async fn relay_anthropic(
    machine: &RelayStateMachine,
    request: impl Into<CanonicalRequestEnvelope>,
    request_id: &str,
) -> Result<ChatResponse, AfError> {
    relay_anthropic_with_report(machine, request, request_id)
        .await
        .into_result()
}

/// 执行 Anthropic 转发并保留脱敏候选尝试报告。
pub async fn relay_anthropic_with_report(
    machine: &RelayStateMachine,
    request: impl Into<CanonicalRequestEnvelope>,
    request_id: &str,
) -> RelayAnthropicOutcome {
    let PreparedAnthropicRequest {
        canonical,
        body: outbound_body,
        client_model,
    } = match prepare_anthropic_request(request.into()) {
        Ok(prepared) => prepared,
        Err(error) => return RelayAnthropicOutcome::failed(error),
    };
    let response_mode = if canonical.stream {
        ResponseMode::Stream
    } else {
        ResponseMode::Full
    };
    let request = match RelayRequest::new(
        canonical.model.clone(),
        Operation::Chat,
        Method::POST,
        Some(outbound_body),
    )
    .map(|request| request.with_response_mode(response_mode))
    {
        Ok(request) => request,
        Err(error) => return RelayAnthropicOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report, mut permit) = match machine.execute_with_report(request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayAnthropicOutcome::new(Err(map_relay_error(error)), report);
        }
    };
    let status = response.status();
    if status != StatusCode::OK {
        let retry_after = parse_retry_after(response.headers());
        let body = match response.into_body().into_bytes().await {
            Ok(body) => body,
            Err(error) => {
                let error = map_adaptor_error(error);
                report.reject_success(error);
                return RelayAnthropicOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayAnthropicOutcome::new(Err(error.into()), report);
    }

    let public_response_id = format!("msg_{request_id}");
    if canonical.stream {
        let (stream, usage) =
            AnthropicMessagesStream::new(response.into_body(), public_response_id, client_model);
        return match stream.prefetch_first(STREAM_FIRST_EVENT_TIMEOUT).await {
            Ok(stream) => {
                let mut body: Box<dyn GenerationStream> = Box::new(stream);
                if let Some(permit) = permit.take() {
                    body = body.with_completion_hook(permit_completion_hook(permit));
                }
                RelayAnthropicOutcome::new(Ok(ChatResponse::Stream { body, usage }), report)
            }
            Err(error) => {
                report.reject_success(error);
                RelayAnthropicOutcome::new(Err(error.into()), report)
            }
        };
    }

    let body = match response.into_body().into_bytes().await {
        Ok(body) => body,
        Err(error) => {
            let error = map_adaptor_error(error);
            report.reject_success(error);
            return RelayAnthropicOutcome::new(Err(error.into()), report);
        }
    };
    let upstream_response = match anthropic::parse_response(&body) {
        Ok(response) => response,
        Err(_) => {
            report.reject_success(UpstreamError::ProtocolError);
            return RelayAnthropicOutcome::new(Err(UpstreamError::ProtocolError.into()), report);
        }
    };
    let Some(usage) = upstream_response.usage else {
        report.reject_success(UpstreamError::ProtocolError);
        return RelayAnthropicOutcome::new(Err(UpstreamError::ProtocolError.into()), report);
    };
    let body = match build_public_full_response(
        upstream_response,
        usage,
        public_response_id,
        client_model,
    ) {
        Ok(body) => body,
        Err(()) => {
            report.reject_success(UpstreamError::ProtocolError);
            return RelayAnthropicOutcome::new(Err(UpstreamError::ProtocolError.into()), report);
        }
    };
    RelayAnthropicOutcome::new(
        Ok(ChatResponse::Full {
            body,
            usage: Ok(usage),
        }),
        report,
    )
}

/// Anthropic 业务结果及其脱敏候选报告。
pub struct RelayAnthropicOutcome {
    result: Result<ChatResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayAnthropicOutcome {
    fn new(result: Result<ChatResponse, AfError>, report: RelayAttemptReport) -> Self {
        Self { result, report }
    }

    fn failed(error: AfError) -> Self {
        Self::new(Err(error), RelayAttemptReport::default())
    }

    /// 消费包装并返回业务结果。
    pub fn into_result(self) -> Result<ChatResponse, AfError> {
        self.result
    }

    /// 消费包装并拆分业务结果与候选报告。
    pub fn into_parts(self) -> (Result<ChatResponse, AfError>, RelayAttemptReport) {
        (self.result, self.report)
    }
}

impl fmt::Debug for RelayAnthropicOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayAnthropicOutcome")
            .field("result", &self.result)
            .field("report", &self.report)
            .finish()
    }
}

fn map_relay_error(error: RelayError) -> AfError {
    match error {
        RelayError::InvalidModel => AfError::InvalidRequest,
        RelayError::Request(_) | RelayError::Adaptor(_) | RelayError::AttemptGateFailed => {
            AfError::Internal
        }
        RelayError::ConcurrencyUnavailable => AfError::ConcurrencyLimited,
        RelayError::Upstream(error) => error.into(),
    }
}

fn build_public_full_response(
    upstream_response: CanonicalResponse,
    usage: Usage,
    public_response_id: String,
    client_model: String,
) -> Result<af_adapter::Bytes, ()> {
    // 对外响应只保留 Canonical 语义，清除上游 ID、模型和同源 raw。
    let public_response = CanonicalResponse::new(
        Operation::Chat,
        public_response_id,
        client_model,
        upstream_response.created_at,
        upstream_response.choices,
        Some(usage),
    );
    let value = anthropic::build_response(&public_response).map_err(|_| ())?;
    serde_json::to_vec(&value)
        .map(af_adapter::Bytes::from)
        .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn full_response_rebuilds_public_identity_without_upstream_raw() {
        let upstream = json!({
            "id": "msg_private_upstream",
            "type": "message",
            "role": "assistant",
            "model": "private-upstream-model",
            "content": [{"type": "text", "text": "answer"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 3,
                "output_tokens": 2,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0
            }
        });
        let upstream = anthropic::parse_response(&serde_json::to_vec(&upstream).unwrap()).unwrap();
        let usage = upstream.usage.unwrap();
        let body = build_public_full_response(
            upstream,
            usage,
            "msg_public".to_owned(),
            "public-model".to_owned(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["id"], "msg_public");
        assert_eq!(value["model"], "public-model");
        assert_eq!(value["content"][0]["text"], "answer");
        let rendered = String::from_utf8(body.to_vec()).unwrap();
        assert!(!rendered.contains("msg_private_upstream"));
        assert!(!rendered.contains("private-upstream-model"));
    }
}
