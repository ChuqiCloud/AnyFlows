use std::fmt;

use af_adapter::{Bytes, Method, Operation, StatusCode};
use af_domain::{AfError, UpstreamError};
use af_protocol::{CanonicalEmbeddingRequest, Usage, openai_embeddings};

use crate::error::{classify_upstream_status, map_adaptor_error, parse_retry_after};
use crate::{RelayAttemptReport, RelayError, RelayRequest, RelayStateMachine};

/// 已完成协议校验并可直接返回下游的 Embeddings 响应。
pub struct EmbeddingResponse {
    body: Bytes,
    usage: Usage,
}

impl EmbeddingResponse {
    /// 组合已重编码的公开响应正文与规范化输入用量。
    #[must_use]
    pub const fn new(body: Bytes, usage: Usage) -> Self {
        Self { body, usage }
    }

    /// 消费响应并返回 HTTP 正文和最终计费用量。
    #[must_use]
    pub fn into_parts(self) -> (Bytes, Usage) {
        (self.body, self.usage)
    }
}

impl fmt::Debug for EmbeddingResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddingResponse")
            .field("body_bytes", &self.body.len())
            .field("usage", &self.usage)
            .finish()
    }
}

/// 使用固定候选状态机执行原生 OpenAI Embeddings 转发。
pub async fn relay_openai_embeddings(
    machine: &RelayStateMachine,
    request: CanonicalEmbeddingRequest,
) -> Result<EmbeddingResponse, AfError> {
    relay_openai_embeddings_with_report(machine, request)
        .await
        .into_result()
}

/// 执行 Embeddings 转发并保留脱敏候选尝试报告。
pub async fn relay_openai_embeddings_with_report(
    machine: &RelayStateMachine,
    request: CanonicalEmbeddingRequest,
) -> RelayOpenAiEmbeddingsOutcome {
    let outbound_body = match encode_request(&request) {
        Ok(body) => body,
        Err(error) => return RelayOpenAiEmbeddingsOutcome::failed(error),
    };
    let relay_request = match RelayRequest::new(
        request.model().to_owned(),
        Operation::Embedding,
        Method::POST,
        Some(outbound_body),
    ) {
        Ok(request) => request,
        Err(error) => return RelayOpenAiEmbeddingsOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report, _permit) = match machine.execute_with_report(relay_request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayOpenAiEmbeddingsOutcome::new(Err(map_relay_error(error)), report);
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
                return RelayOpenAiEmbeddingsOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayOpenAiEmbeddingsOutcome::new(Err(error.into()), report);
    }

    let body = match response.into_body().into_bytes().await {
        Ok(body) => body,
        Err(error) => {
            let error = map_adaptor_error(error);
            report.reject_success(error);
            return RelayOpenAiEmbeddingsOutcome::new(Err(error.into()), report);
        }
    };
    let upstream = match openai_embeddings::parse_response(&body) {
        Ok(response) => response,
        Err(_) => return protocol_failure(report),
    };
    if upstream.validate_for_request(&request).is_err() {
        return protocol_failure(report);
    }
    let public = match upstream.rebind_model(request.model().to_owned()) {
        Ok(response) => response,
        Err(_) => return protocol_failure(report),
    };
    let usage = public.usage();
    let body = match encode_response(&public) {
        Ok(body) => body,
        Err(_) => return protocol_failure(report),
    };
    RelayOpenAiEmbeddingsOutcome::new(Ok(EmbeddingResponse::new(body, usage)), report)
}

/// OpenAI Embeddings 业务结果及其脱敏候选报告。
pub struct RelayOpenAiEmbeddingsOutcome {
    result: Result<EmbeddingResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayOpenAiEmbeddingsOutcome {
    fn new(result: Result<EmbeddingResponse, AfError>, report: RelayAttemptReport) -> Self {
        Self { result, report }
    }

    fn failed(error: AfError) -> Self {
        Self::new(Err(error), RelayAttemptReport::default())
    }

    /// 消费包装并返回业务结果。
    pub fn into_result(self) -> Result<EmbeddingResponse, AfError> {
        self.result
    }

    /// 消费包装并拆分业务结果与候选报告。
    pub fn into_parts(self) -> (Result<EmbeddingResponse, AfError>, RelayAttemptReport) {
        (self.result, self.report)
    }
}

impl fmt::Debug for RelayOpenAiEmbeddingsOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayOpenAiEmbeddingsOutcome")
            .field("result", &self.result)
            .field("report", &self.report)
            .finish()
    }
}

fn encode_request(request: &CanonicalEmbeddingRequest) -> Result<Bytes, AfError> {
    openai_embeddings::build_request(request)
        .map_err(|_| AfError::Internal)
        .and_then(|value| {
            serde_json::to_vec(&value)
                .map(Bytes::from)
                .map_err(|_| AfError::Internal)
        })
}

fn encode_response(response: &af_protocol::CanonicalEmbeddingResponse) -> Result<Bytes, ()> {
    let value = openai_embeddings::build_response(response).map_err(|_| ())?;
    serde_json::to_vec(&value).map(Bytes::from).map_err(|_| ())
}

fn protocol_failure(mut report: RelayAttemptReport) -> RelayOpenAiEmbeddingsOutcome {
    report.reject_success(UpstreamError::ProtocolError);
    RelayOpenAiEmbeddingsOutcome::new(Err(UpstreamError::ProtocolError.into()), report)
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
    use af_protocol::{
        CanonicalEmbeddingResponse, EmbeddingVector, TokenCount, UsageDetails, UsageSemantics,
        UsageSource,
    };
    use serde_json::Value;

    use super::*;

    #[test]
    fn public_response_never_contains_upstream_model() {
        let usage = Usage::new(
            TokenCount::new(1).unwrap(),
            TokenCount::ZERO,
            UsageDetails::new(
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
            ),
            UsageSource::Upstream,
            UsageSemantics::Inclusive,
        )
        .unwrap();
        let upstream = CanonicalEmbeddingResponse::new(
            "private-upstream-model".to_owned(),
            vec![EmbeddingVector::new(0, vec![0.1, 0.2]).unwrap()],
            usage,
        )
        .unwrap();
        let public = upstream.rebind_model("public-model".to_owned()).unwrap();
        let body = encode_response(&public).unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["model"], "public-model");
        assert!(
            !String::from_utf8(body.to_vec())
                .unwrap()
                .contains("private-upstream-model")
        );
    }
}
