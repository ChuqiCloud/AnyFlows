use std::fmt;

use af_adapter::{Bytes, Method, Operation, StatusCode};
use af_domain::{AfError, UpstreamError};
use af_protocol::{
    CanonicalResponsesCompactionRequest, ResponsesCompactionUsage, openai_responses_compact,
};

use crate::error::{classify_upstream_status, map_adaptor_error, parse_retry_after};
use crate::{RelayAttemptReport, RelayError, RelayRequest, RelayStateMachine};

/// 已完成协议校验并可直接返回下游的 Responses Compact 响应。
pub struct ResponsesCompactionResponse {
    body: Bytes,
    usage: ResponsesCompactionUsage,
}

impl ResponsesCompactionResponse {
    /// 组合已重编码的公开响应正文与完整压缩用量。
    #[must_use]
    pub const fn new(body: Bytes, usage: ResponsesCompactionUsage) -> Self {
        Self { body, usage }
    }

    /// 消费响应并返回 HTTP 正文和专用计费用量。
    #[must_use]
    pub fn into_parts(self) -> (Bytes, ResponsesCompactionUsage) {
        (self.body, self.usage)
    }
}

impl fmt::Debug for ResponsesCompactionResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponsesCompactionResponse")
            .field("body_bytes", &self.body.len())
            .field("usage", &self.usage)
            .finish()
    }
}

/// 使用固定候选状态机执行原生 OpenAI Responses Compact 转发。
pub async fn relay_openai_responses_compact(
    machine: &RelayStateMachine,
    request: CanonicalResponsesCompactionRequest,
) -> Result<ResponsesCompactionResponse, AfError> {
    relay_openai_responses_compact_with_report(machine, request)
        .await
        .into_result()
}

/// 执行 Responses Compact 转发并保留脱敏候选尝试报告。
pub async fn relay_openai_responses_compact_with_report(
    machine: &RelayStateMachine,
    request: CanonicalResponsesCompactionRequest,
) -> RelayOpenAiResponsesCompactOutcome {
    let outbound_body = match encode_request(&request) {
        Ok(body) => body,
        Err(error) => return RelayOpenAiResponsesCompactOutcome::failed(error),
    };
    let relay_request = match RelayRequest::new(
        request.model().to_owned(),
        Operation::ResponsesCompact,
        Method::POST,
        Some(outbound_body),
    ) {
        Ok(request) => request.with_response_body_limit(openai_responses_compact::MAX_BODY_BYTES),
        Err(error) => return RelayOpenAiResponsesCompactOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report, _permit) = match machine.execute_with_report(relay_request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayOpenAiResponsesCompactOutcome::new(Err(map_relay_error(error)), report);
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
                return RelayOpenAiResponsesCompactOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayOpenAiResponsesCompactOutcome::new(Err(error.into()), report);
    }

    let body = match response
        .into_body()
        .into_bytes_with_limit(openai_responses_compact::MAX_BODY_BYTES)
        .await
    {
        Ok(body) => body,
        Err(error) => {
            let error = map_adaptor_error(error);
            report.reject_success(error);
            return RelayOpenAiResponsesCompactOutcome::new(Err(error.into()), report);
        }
    };
    let upstream = match openai_responses_compact::parse_response(&body) {
        Ok(response) => response,
        Err(_) => return protocol_failure(report),
    };
    let usage = upstream.usage();
    let body = match encode_response(&upstream) {
        Ok(body) => body,
        Err(()) => return protocol_failure(report),
    };
    RelayOpenAiResponsesCompactOutcome::new(
        Ok(ResponsesCompactionResponse::new(body, usage)),
        report,
    )
}

/// OpenAI Responses Compact 业务结果及其脱敏候选报告。
pub struct RelayOpenAiResponsesCompactOutcome {
    result: Result<ResponsesCompactionResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayOpenAiResponsesCompactOutcome {
    fn new(
        result: Result<ResponsesCompactionResponse, AfError>,
        report: RelayAttemptReport,
    ) -> Self {
        Self { result, report }
    }

    fn failed(error: AfError) -> Self {
        Self::new(Err(error), RelayAttemptReport::default())
    }

    /// 消费包装并返回业务结果。
    pub fn into_result(self) -> Result<ResponsesCompactionResponse, AfError> {
        self.result
    }

    /// 消费包装并拆分业务结果与候选报告。
    pub fn into_parts(
        self,
    ) -> (
        Result<ResponsesCompactionResponse, AfError>,
        RelayAttemptReport,
    ) {
        (self.result, self.report)
    }
}

impl fmt::Debug for RelayOpenAiResponsesCompactOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayOpenAiResponsesCompactOutcome")
            .field("result", &self.result)
            .field("report", &self.report)
            .finish()
    }
}

fn encode_request(request: &CanonicalResponsesCompactionRequest) -> Result<Bytes, AfError> {
    openai_responses_compact::build_request(request)
        .map_err(|_| AfError::Internal)
        .and_then(|value| {
            serde_json::to_vec(&value)
                .map(Bytes::from)
                .map_err(|_| AfError::Internal)
        })
}

fn encode_response(
    response: &af_protocol::CanonicalResponsesCompactionResponse,
) -> Result<Bytes, ()> {
    let value = openai_responses_compact::build_response(response).map_err(|_| ())?;
    serde_json::to_vec(&value).map(Bytes::from).map_err(|_| ())
}

fn protocol_failure(mut report: RelayAttemptReport) -> RelayOpenAiResponsesCompactOutcome {
    report.reject_success(UpstreamError::ProtocolError);
    RelayOpenAiResponsesCompactOutcome::new(Err(UpstreamError::ProtocolError.into()), report)
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn request_encoder_preserves_the_complete_context_window() {
        let request = openai_responses_compact::parse_request(
            &serde_json::to_vec(&json!({
                "model": "gpt-test",
                "instructions": "keep the contract",
                "input": [
                    {
                        "type": "message",
                        "role": "user",
                        "content": [{"type": "input_text", "text": "hello"}]
                    },
                    {
                        "type": "compaction",
                        "id": "cmp_1",
                        "encrypted_content": "opaque-window"
                    }
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        let body = encode_request(&request).unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["input"][0]["type"], "message");
        assert_eq!(value["input"][1]["type"], "compaction");
        assert_eq!(value["instructions"], "keep the contract");
    }

    #[test]
    fn response_encoder_preserves_order_and_complete_usage() {
        let response = openai_responses_compact::parse_response(
            &serde_json::to_vec(&json!({
                "id": "resp_compact_1",
                "created_at": 1,
                "object": "response.compaction",
                "output": [
                    {
                        "type": "message",
                        "id": "msg_1",
                        "role": "user",
                        "status": "completed",
                        "content": [{"type": "input_text", "text": "hello"}]
                    },
                    {
                        "type": "compaction",
                        "id": "cmp_1",
                        "encrypted_content": "opaque-window"
                    }
                ],
                "usage": {
                    "input_tokens": 11,
                    "input_tokens_details": {
                        "cached_tokens": 3,
                        "cache_write_tokens": 2
                    },
                    "output_tokens": 7,
                    "output_tokens_details": {"reasoning_tokens": 5},
                    "total_tokens": 18
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let usage = response.usage();
        let body = encode_response(&response).unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["output"][0]["type"], "message");
        assert_eq!(value["output"][1]["type"], "compaction");
        assert_eq!(usage.cached_tokens().get(), 3);
        assert_eq!(usage.cache_write_tokens().get(), 2);
        assert_eq!(usage.reasoning_tokens().get(), 5);
    }
}
