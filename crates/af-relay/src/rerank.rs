use std::fmt;

use af_adapter::{Bytes, Method, Operation, StatusCode};
use af_domain::{AfError, Protocol, UpstreamError};
use af_protocol::{CanonicalRerankRequest, CanonicalRerankResponse, RerankUsage, rerank_v1};

use crate::cohere_rerank::normalize_cohere_response;
use crate::error::{classify_upstream_status, map_adaptor_error, parse_retry_after};
use crate::jina_rerank::normalize_jina_canonical_response;
use crate::{RelayAttemptReport, RelayError, RelayRequest, RelayStateMachine};

/// 已完成供应商协议校验、请求关联和模型重绑定的 Rerank 响应。
pub struct RerankResponse {
    body: Bytes,
    usage: Option<RerankUsage>,
}

impl RerankResponse {
    /// 组合已重新编码的公开响应正文与可选真实用量。
    #[must_use]
    pub const fn new(body: Bytes, usage: Option<RerankUsage>) -> Self {
        Self { body, usage }
    }

    /// 消费响应并返回 HTTP 正文与真实用量。
    #[must_use]
    pub fn into_parts(self) -> (Bytes, Option<RerankUsage>) {
        (self.body, self.usage)
    }
}

impl fmt::Debug for RerankResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RerankResponse")
            .field("body_bytes", &self.body.len())
            .field("usage", &self.usage)
            .finish()
    }
}

/// 执行混合 Jina/Cohere Rerank 候选，并保留脱敏尝试报告。
pub async fn relay_rerank_with_report(
    machine: &RelayStateMachine,
    request: CanonicalRerankRequest,
    candidate_protocols: &[Protocol],
) -> RelayRerankOutcome {
    let outbound_body = match encode_public_request(&request) {
        Ok(body) => body,
        Err(error) => return RelayRerankOutcome::failed(error),
    };
    let relay_request = match RelayRequest::new(
        request.model().to_owned(),
        Operation::Rerank,
        Method::POST,
        Some(outbound_body),
    ) {
        Ok(request) => request.with_response_body_limit(rerank_v1::MAX_BODY_BYTES),
        Err(error) => return RelayRerankOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report, _permit) = match machine.execute_with_report(relay_request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayRerankOutcome::new(Err(map_relay_error(error)), report);
        }
    };

    let protocol = match report
        .successful_candidate_index()
        .and_then(|index| candidate_protocols.get(index))
        .copied()
    {
        Some(protocol @ (Protocol::JinaRerank | Protocol::CohereRerank)) => protocol,
        _ => return protocol_failure(report),
    };
    let status = response.status();
    if status != StatusCode::OK {
        let retry_after = parse_retry_after(response.headers());
        let body = match response.into_body().into_bytes().await {
            Ok(body) => body,
            Err(error) => {
                let error = map_adaptor_error(error);
                report.reject_success(error);
                return RelayRerankOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayRerankOutcome::new(Err(error.into()), report);
    }

    let body = match response
        .into_body()
        .into_bytes_with_limit(rerank_v1::MAX_BODY_BYTES)
        .await
    {
        Ok(body) => body,
        Err(error) => {
            let error = map_adaptor_error(error);
            report.reject_success(error);
            return RelayRerankOutcome::new(Err(error.into()), report);
        }
    };
    let canonical = match protocol {
        Protocol::JinaRerank => normalize_jina_canonical_response(&request, &body),
        Protocol::CohereRerank => normalize_cohere_response(&request, &body),
        _ => unreachable!("候选协议已在前置分支闭合"),
    };
    let response = match canonical.and_then(encode_public_response) {
        Ok(response) => response,
        Err(()) => return protocol_failure(report),
    };
    RelayRerankOutcome::new(Ok(response), report)
}

/// Rerank 业务结果及其脱敏候选报告。
pub struct RelayRerankOutcome {
    result: Result<RerankResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayRerankOutcome {
    fn new(result: Result<RerankResponse, AfError>, report: RelayAttemptReport) -> Self {
        Self { result, report }
    }

    fn failed(error: AfError) -> Self {
        Self::new(Err(error), RelayAttemptReport::default())
    }

    /// 消费包装并拆分业务结果与候选报告。
    pub fn into_parts(self) -> (Result<RerankResponse, AfError>, RelayAttemptReport) {
        (self.result, self.report)
    }
}

impl fmt::Debug for RelayRerankOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayRerankOutcome")
            .field("result", &self.result)
            .field("report", &self.report)
            .finish()
    }
}

fn encode_public_request(request: &CanonicalRerankRequest) -> Result<Bytes, AfError> {
    rerank_v1::build_request(request)
        .map_err(|_| AfError::Internal)
        .and_then(|value| {
            serde_json::to_vec(&value)
                .map(Bytes::from)
                .map_err(|_| AfError::Internal)
        })
}

fn encode_public_response(response: CanonicalRerankResponse) -> Result<RerankResponse, ()> {
    let usage = response.usage();
    let value = rerank_v1::build_response(&response).map_err(|_| ())?;
    let body = serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| ())?;
    Ok(RerankResponse::new(body, usage))
}

fn protocol_failure(mut report: RelayAttemptReport) -> RelayRerankOutcome {
    report.reject_success(UpstreamError::ProtocolError);
    RelayRerankOutcome::new(Err(UpstreamError::ProtocolError.into()), report)
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
