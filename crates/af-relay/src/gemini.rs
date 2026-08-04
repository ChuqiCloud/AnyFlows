use std::{fmt, time::Duration};

use af_adapter::{Method, Operation, ResponseMode, StatusCode};
use af_domain::{AfError, Protocol, UpstreamError};
use af_protocol::{CanonicalRequestEnvelope, CanonicalResponse, Usage, gemini};

use crate::attempt_gate::permit_completion_hook;
use crate::chat_stream_encoder::ChatStreamEncoder;
use crate::error::{classify_upstream_status, map_adaptor_error, parse_retry_after};
use crate::gemini_request::{PreparedGeminiRequest, prepare_gemini_request};
use crate::openai_chat::unix_timestamp;
use crate::openai_chat_usage::OpenAiChatUsageEstimator;
use crate::{
    ChatResponse, GeminiGenerateContentStream, GenerationStream, RelayAttemptReport, RelayError,
    RelayRequest, RelayStateMachine,
};

const STREAM_FIRST_EVENT_TIMEOUT: Duration = Duration::from_secs(30);

/// 使用固定候选状态机执行原生 Gemini `generateContent` 转发。
pub async fn relay_gemini(
    machine: &RelayStateMachine,
    request: impl Into<CanonicalRequestEnvelope>,
    request_id: &str,
) -> Result<ChatResponse, AfError> {
    relay_gemini_with_report(machine, request, request_id)
        .await
        .into_result()
}

/// 执行 Gemini 转发并保留脱敏候选尝试报告。
pub async fn relay_gemini_with_report(
    machine: &RelayStateMachine,
    request: impl Into<CanonicalRequestEnvelope>,
    request_id: &str,
) -> RelayGeminiOutcome {
    let PreparedGeminiRequest {
        canonical,
        body: outbound_body,
        client_model,
    } = match prepare_gemini_request(request.into()) {
        Ok(prepared) => prepared,
        Err(error) => return RelayGeminiOutcome::failed(error),
    };
    let usage_estimator = OpenAiChatUsageEstimator::new(&canonical);
    let stream_encoder = if canonical.stream {
        let created_at = match unix_timestamp() {
            Ok(created_at) => created_at,
            Err(error) => return RelayGeminiOutcome::failed(error),
        };
        match ChatStreamEncoder::new(
            Protocol::Gemini,
            request_id,
            &client_model,
            created_at,
            &canonical,
            false,
        ) {
            Ok(encoder) => Some(encoder),
            Err(error) => return RelayGeminiOutcome::failed(error),
        }
    } else {
        None
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
        Err(error) => return RelayGeminiOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report, mut permit) = match machine.execute_with_report(request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayGeminiOutcome::new(Err(map_relay_error(error)), report);
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
                return RelayGeminiOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayGeminiOutcome::new(Err(error.into()), report);
    }

    if canonical.stream {
        let encoder = stream_encoder.expect("流式请求必须已经创建 Gemini 下游编码器");
        let (stream, usage) =
            GeminiGenerateContentStream::new(response.into_body(), encoder, usage_estimator);
        return match stream.prefetch_first(STREAM_FIRST_EVENT_TIMEOUT).await {
            Ok(stream) => {
                let mut body: Box<dyn GenerationStream> = Box::new(stream);
                if let Some(permit) = permit.take() {
                    body = body.with_completion_hook(permit_completion_hook(permit));
                }
                RelayGeminiOutcome::new(Ok(ChatResponse::Stream { body, usage }), report)
            }
            Err(error) => {
                report.reject_success(error);
                RelayGeminiOutcome::new(Err(error.into()), report)
            }
        };
    }

    let body = match response.into_body().into_bytes().await {
        Ok(body) => body,
        Err(error) => {
            let error = map_adaptor_error(error);
            report.reject_success(error);
            return RelayGeminiOutcome::new(Err(error.into()), report);
        }
    };
    let upstream_response = match gemini::parse_response(&body) {
        Ok(response) => response,
        Err(_) => {
            report.reject_success(UpstreamError::ProtocolError);
            return RelayGeminiOutcome::new(Err(UpstreamError::ProtocolError.into()), report);
        }
    };
    let usage = match usage_estimator.resolve_response(&upstream_response) {
        Ok(usage) => usage,
        Err(_) => {
            report.reject_success(UpstreamError::ProtocolError);
            return RelayGeminiOutcome::new(Err(UpstreamError::ProtocolError.into()), report);
        }
    };
    let body = match build_public_full_response(
        upstream_response,
        usage,
        format!("response-{request_id}"),
        client_model,
    ) {
        Ok(body) => body,
        Err(()) => {
            report.reject_success(UpstreamError::ProtocolError);
            return RelayGeminiOutcome::new(Err(UpstreamError::ProtocolError.into()), report);
        }
    };
    RelayGeminiOutcome::new(
        Ok(ChatResponse::Full {
            body,
            usage: Ok(usage),
        }),
        report,
    )
}

/// Gemini 业务结果及其脱敏候选报告。
pub struct RelayGeminiOutcome {
    result: Result<ChatResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayGeminiOutcome {
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

impl fmt::Debug for RelayGeminiOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayGeminiOutcome")
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
    let value = gemini::build_response(&public_response).map_err(|_| ())?;
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
            "responseId": "private-response",
            "modelVersion": "private-upstream-model",
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": [{"text": "answer"}]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 3,
                "candidatesTokenCount": 2,
                "totalTokenCount": 5
            }
        });
        let upstream = gemini::parse_response(&serde_json::to_vec(&upstream).unwrap()).unwrap();
        let usage = upstream.usage.unwrap();
        let body = build_public_full_response(
            upstream,
            usage,
            "response-public".to_owned(),
            "public-model".to_owned(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["responseId"], "response-public");
        assert_eq!(value["modelVersion"], "public-model");
        assert_eq!(
            value["candidates"][0]["content"]["parts"][0]["text"],
            "answer"
        );
        let rendered = String::from_utf8(body.to_vec()).unwrap();
        assert!(!rendered.contains("private-response"));
        assert!(!rendered.contains("private-upstream-model"));
    }
}
