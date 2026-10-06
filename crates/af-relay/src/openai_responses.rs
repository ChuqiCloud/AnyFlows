use std::{fmt, time::Duration};

use af_adapter::{Method, Operation, ResponseMode, StatusCode};
use af_domain::{AfError, UpstreamError};
use af_protocol::{
    CanonicalRequestEnvelope, CanonicalResponse, openai_responses,
    openai_responses::OpenAiResponsesStreamDecoder,
};

use crate::attempt_gate::permit_completion_hook;
use crate::error::{classify_upstream_status, map_adaptor_error, parse_retry_after};
use crate::openai_chat::unix_timestamp;
use crate::openai_chat_usage::OpenAiChatUsageEstimator;
use crate::openai_responses_request::{
    PreparedOpenAiResponsesRequest, prepare_openai_responses_request,
};
use crate::{
    ChatResponse, GenerationStream, OpenAiResponsesStream, RelayAttemptReport, RelayError,
    RelayRequest, RelayStateMachine,
};

const STREAM_FIRST_EVENT_TIMEOUT: Duration = Duration::from_secs(30);

/// 使用固定候选状态机执行原生 OpenAI Responses 转发。
pub async fn relay_openai_responses(
    machine: &RelayStateMachine,
    request: impl Into<CanonicalRequestEnvelope>,
    request_id: &str,
) -> Result<ChatResponse, AfError> {
    relay_openai_responses_with_report(machine, request, request_id)
        .await
        .into_result()
}

/// 执行 Responses 转发并保留脱敏候选尝试报告。
pub async fn relay_openai_responses_with_report(
    machine: &RelayStateMachine,
    request: impl Into<CanonicalRequestEnvelope>,
    request_id: &str,
) -> RelayOpenAiResponsesOutcome {
    let PreparedOpenAiResponsesRequest {
        canonical,
        body: outbound_body,
        client_model,
    } = match prepare_openai_responses_request(request.into()) {
        Ok(prepared) => prepared,
        Err(error) => return RelayOpenAiResponsesOutcome::failed(error),
    };
    let usage_estimator = OpenAiChatUsageEstimator::new(&canonical);
    let response_mode = if canonical.stream {
        ResponseMode::Stream
    } else {
        ResponseMode::Full
    };
    let request = match RelayRequest::new(
        canonical.model.clone(),
        Operation::Responses,
        Method::POST,
        Some(outbound_body),
    )
    .map(|request| request.with_response_mode(response_mode))
    {
        Ok(request) => request,
        Err(error) => return RelayOpenAiResponsesOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report, mut permit) = match machine.execute_with_report(request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayOpenAiResponsesOutcome::new(Err(map_relay_error(error)), report);
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
                return RelayOpenAiResponsesOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayOpenAiResponsesOutcome::new(Err(error.into()), report);
    }

    let public_response_id = format!("resp_{request_id}");
    if canonical.stream {
        let created_at = match unix_timestamp() {
            Ok(created_at) => created_at,
            Err(error) => return RelayOpenAiResponsesOutcome::new(Err(error), report),
        };
        let (stream, usage) = match OpenAiResponsesStream::new(
            response.into_body(),
            &canonical,
            &public_response_id,
            &client_model,
            created_at,
        ) {
            Ok(stream) => stream,
            Err(error) => return RelayOpenAiResponsesOutcome::new(Err(error), report),
        };
        return match stream.prefetch_first(STREAM_FIRST_EVENT_TIMEOUT).await {
            Ok(stream) => {
                let mut body: Box<dyn GenerationStream> = Box::new(stream);
                if let Some(permit) = permit.take() {
                    body = body.with_completion_hook(permit_completion_hook(permit));
                }
                RelayOpenAiResponsesOutcome::new(Ok(ChatResponse::Stream { body, usage }), report)
            }
            Err(error) => {
                report.reject_success(error);
                RelayOpenAiResponsesOutcome::new(Err(error.into()), report)
            }
        };
    }

    let body = match response.into_body().into_bytes().await {
        Ok(body) => body,
        Err(error) => {
            let error = map_adaptor_error(error);
            report.reject_success(error);
            return RelayOpenAiResponsesOutcome::new(Err(error.into()), report);
        }
    };
    let upstream_response = match parse_full_response_body(&body) {
        Ok(response) => response,
        Err(_) => {
            report.reject_success(UpstreamError::ProtocolError);
            return RelayOpenAiResponsesOutcome::new(
                Err(UpstreamError::ProtocolError.into()),
                report,
            );
        }
    };
    let usage = match usage_estimator.resolve_response(&upstream_response) {
        Ok(usage) => usage,
        Err(_) => {
            report.reject_success(UpstreamError::ProtocolError);
            return RelayOpenAiResponsesOutcome::new(
                Err(UpstreamError::ProtocolError.into()),
                report,
            );
        }
    };
    let body = match build_public_full_response(
        upstream_response,
        usage,
        public_response_id,
        client_model,
    ) {
        Ok(body) => body,
        Err(_) => {
            report.reject_success(UpstreamError::ProtocolError);
            return RelayOpenAiResponsesOutcome::new(
                Err(UpstreamError::ProtocolError.into()),
                report,
            );
        }
    };
    RelayOpenAiResponsesOutcome::new(
        Ok(ChatResponse::Full {
            body,
            usage: Ok(usage),
        }),
        report,
    )
}

fn parse_full_response_body(body: &[u8]) -> Result<CanonicalResponse, ()> {
    if let Ok(response) = openai_responses::parse_response(body) {
        return Ok(response);
    }

    // Codex OAuth always returns Responses as SSE. Aggregate it for a client that
    // requested a non-streaming response while retaining the same strict decoder.
    let mut decoder = OpenAiResponsesStreamDecoder::new();
    decoder.push(body).map_err(|_| ())?;
    decoder.finish().map_err(|_| ())?;
    decoder.take_terminal_response().ok_or(())
}

/// OpenAI Responses 业务结果及其脱敏候选报告。
pub struct RelayOpenAiResponsesOutcome {
    result: Result<ChatResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayOpenAiResponsesOutcome {
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

impl fmt::Debug for RelayOpenAiResponsesOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayOpenAiResponsesOutcome")
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
    usage: af_protocol::Usage,
    public_response_id: String,
    client_model: String,
) -> Result<af_adapter::Bytes, ()> {
    // 对外响应只保留 Canonical 语义，避免暴露上游响应 ID、模型和同源扩展字段。
    let public_response = CanonicalResponse::new(
        Operation::Responses,
        public_response_id,
        client_model,
        upstream_response.created_at,
        upstream_response.choices,
        Some(usage),
    );
    let value = openai_responses::build_response(&public_response).map_err(|_| ())?;
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
            "id": "resp_private_upstream",
            "object": "response",
            "created_at": 1_700_000_000,
            "status": "completed",
            "error": null,
            "incomplete_details": null,
            "model": "private-upstream-model",
            "output": [{
                "id": "msg_private",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "answer",
                    "annotations": [],
                    "logprobs": []
                }]
            }],
            "usage": {
                "input_tokens": 3,
                "input_tokens_details": {"cache_write_tokens": 0, "cached_tokens": 0},
                "output_tokens": 2,
                "output_tokens_details": {"reasoning_tokens": 0},
                "total_tokens": 5
            },
            "service_tier": "priority"
        });
        let upstream =
            openai_responses::parse_response(&serde_json::to_vec(&upstream).unwrap()).unwrap();
        let usage = upstream.usage.unwrap();
        let body = build_public_full_response(
            upstream,
            usage,
            "resp_public".to_owned(),
            "public-model".to_owned(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["id"], "resp_public");
        assert_eq!(value["model"], "public-model");
        assert_eq!(value["output"][0]["content"][0]["text"], "answer");
        let rendered = String::from_utf8(body.to_vec()).unwrap();
        assert!(!rendered.contains("resp_private_upstream"));
        assert!(!rendered.contains("private-upstream-model"));
        assert!(value.get("service_tier").is_none());
    }
}
