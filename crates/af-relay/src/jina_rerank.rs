use std::fmt;

use af_adapter::{Bytes, Method, Operation, StatusCode};
use af_domain::{AfError, UpstreamError};
use af_protocol::{CanonicalRerankRequest, RerankUsage, rerank_v1};

use crate::error::{classify_upstream_status, map_adaptor_error, parse_retry_after};
use crate::{
    RelayAttemptReport, RelayCandidateRequest, RelayError, RelayRequest, RelayStateMachine,
};

/// 已完成关联校验和模型重绑定的 Jina Rerank 响应。
pub struct JinaRerankResponse {
    body: Bytes,
    usage: Option<RerankUsage>,
}

impl JinaRerankResponse {
    /// 组合已重新编码的公开响应正文与可选真实用量。
    #[must_use]
    pub const fn new(body: Bytes, usage: Option<RerankUsage>) -> Self {
        Self { body, usage }
    }

    /// 消费响应并返回 HTTP 正文与真实用量；上游缺失用量时保持为空。
    #[must_use]
    pub fn into_parts(self) -> (Bytes, Option<RerankUsage>) {
        (self.body, self.usage)
    }
}

impl fmt::Debug for JinaRerankResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JinaRerankResponse")
            .field("body_bytes", &self.body.len())
            .field("usage", &self.usage)
            .finish()
    }
}

/// 使用固定候选状态机执行 Jina 原生 Rerank 完整响应转发。
pub async fn relay_jina_rerank(
    machine: &RelayStateMachine,
    request: CanonicalRerankRequest,
) -> Result<JinaRerankResponse, AfError> {
    relay_jina_rerank_with_report(machine, request)
        .await
        .into_result()
}

/// 执行 Jina Rerank 转发，并保留脱敏候选尝试报告。
pub async fn relay_jina_rerank_with_report(
    machine: &RelayStateMachine,
    request: CanonicalRerankRequest,
) -> RelayJinaRerankOutcome {
    let outbound_body = match encode_request(&request) {
        Ok(body) => body,
        Err(error) => return RelayJinaRerankOutcome::failed(error),
    };
    let relay_request = match RelayRequest::new(
        request.model().to_owned(),
        Operation::Rerank,
        Method::POST,
        Some(outbound_body),
    ) {
        Ok(request) => request.with_response_body_limit(rerank_v1::MAX_BODY_BYTES),
        Err(error) => return RelayJinaRerankOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report, _permit) = match machine.execute_with_report(relay_request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayJinaRerankOutcome::new(Err(map_relay_error(error)), report);
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
                return RelayJinaRerankOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayJinaRerankOutcome::new(Err(error.into()), report);
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
            return RelayJinaRerankOutcome::new(Err(error.into()), report);
        }
    };
    let response = match normalize_response(&request, &body) {
        Ok(response) => response,
        Err(()) => return protocol_failure(report),
    };
    RelayJinaRerankOutcome::new(Ok(response), report)
}

/// 使用映射后的上游模型重建 Jina Rerank 候选请求。
///
/// 模型字段与候选路由目标在同一函数内生成，避免只改 URL 目标而把客户端模型继续发往上游。
pub fn build_jina_rerank_candidate_request(
    request: &CanonicalRerankRequest,
    upstream_model: String,
) -> Result<RelayCandidateRequest, AfError> {
    let mapped = CanonicalRerankRequest::new(
        upstream_model.clone(),
        request.query().to_owned(),
        request.documents().to_vec(),
        request.top_n(),
        request.return_documents(),
    )
    .map_err(|_| AfError::Internal)?;
    let body = encode_request(&mapped)?;
    RelayCandidateRequest::new(upstream_model, Some(body)).map_err(map_relay_error)
}

/// Jina Rerank 业务结果及其脱敏候选报告。
pub struct RelayJinaRerankOutcome {
    result: Result<JinaRerankResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayJinaRerankOutcome {
    fn new(result: Result<JinaRerankResponse, AfError>, report: RelayAttemptReport) -> Self {
        Self { result, report }
    }

    fn failed(error: AfError) -> Self {
        Self::new(Err(error), RelayAttemptReport::default())
    }

    /// 消费包装并返回业务结果。
    pub fn into_result(self) -> Result<JinaRerankResponse, AfError> {
        self.result
    }

    /// 消费包装并拆分业务结果与候选报告。
    pub fn into_parts(self) -> (Result<JinaRerankResponse, AfError>, RelayAttemptReport) {
        (self.result, self.report)
    }
}

impl fmt::Debug for RelayJinaRerankOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayJinaRerankOutcome")
            .field("result", &self.result)
            .field("report", &self.report)
            .finish()
    }
}

fn encode_request(request: &CanonicalRerankRequest) -> Result<Bytes, AfError> {
    rerank_v1::build_request(request)
        .map_err(|_| AfError::Internal)
        .and_then(|value| {
            serde_json::to_vec(&value)
                .map(Bytes::from)
                .map_err(|_| AfError::Internal)
        })
}

fn normalize_response(
    request: &CanonicalRerankRequest,
    body: &[u8],
) -> Result<JinaRerankResponse, ()> {
    let public = normalize_jina_canonical_response(request, body)?;
    let usage = public.usage();
    let value = rerank_v1::build_response(&public).map_err(|_| ())?;
    let body = serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| ())?;
    Ok(JinaRerankResponse::new(body, usage))
}

/// 解析 Jina 响应并完成请求关联和客户端模型重绑定。
pub(crate) fn normalize_jina_canonical_response(
    request: &CanonicalRerankRequest,
    body: &[u8],
) -> Result<af_protocol::CanonicalRerankResponse, ()> {
    let upstream = rerank_v1::parse_response(body).map_err(|_| ())?;
    upstream.validate_for_request(request).map_err(|_| ())?;
    upstream
        .rebind_model(request.model().to_owned())
        .map_err(|_| ())
}

fn protocol_failure(mut report: RelayAttemptReport) -> RelayJinaRerankOutcome {
    report.reject_success(UpstreamError::ProtocolError);
    RelayJinaRerankOutcome::new(Err(UpstreamError::ProtocolError.into()), report)
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
    use serde_json::Value;

    use super::*;

    fn request(return_documents: bool) -> CanonicalRerankRequest {
        rerank_v1::parse_request(
            format!(
                r#"{{"model":"public-rerank-model","query":"private-query-canary","documents":["private-document-a",{{"text":"private-document-b"}}],"top_n":2,"return_documents":{return_documents}}}"#
            )
            .as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn candidate_request_rebuilds_body_with_the_mapped_model() {
        let request = request(true);
        let candidate =
            build_jina_rerank_candidate_request(&request, "private-upstream-model".to_owned())
                .unwrap();
        let (model, body) = candidate.into_parts();
        assert_eq!(model, "private-upstream-model");

        let body = body.unwrap();
        let mapped = rerank_v1::parse_request(&body).unwrap();
        assert_eq!(mapped.model(), "private-upstream-model");
        assert_eq!(mapped.query(), request.query());
        assert_eq!(mapped.documents(), request.documents());
        assert_eq!(mapped.top_n(), request.top_n());
        assert_eq!(mapped.return_documents(), request.return_documents());
        assert!(
            !String::from_utf8(body.to_vec())
                .unwrap()
                .contains("public-rerank-model")
        );
    }

    #[test]
    fn response_is_validated_rebound_and_preserves_jina_token_usage() {
        let request = request(true);
        let response = normalize_response(
            &request,
            br#"{"id":"private-response-id","model":"private-upstream-model","results":[{"index":1,"relevance_score":0.9,"document":{"text":"private-document-b"}},{"index":0,"relevance_score":0.7,"document":"private-document-a"}],"usage":{"prompt_tokens":7,"total_tokens":7}}"#,
        )
        .unwrap();
        let (body, usage) = response.into_parts();
        assert_eq!(
            usage.unwrap().token_usage().unwrap().input_tokens().get(),
            7
        );
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["model"], "public-rerank-model");
        let rendered = String::from_utf8(body.to_vec()).unwrap();
        assert!(!rendered.contains("private-upstream-model"));
        assert!(rendered.contains("private-response-id"));
    }

    #[test]
    fn missing_usage_remains_absent_after_normalization() {
        let request = request(false);
        let response = normalize_response(
            &request,
            br#"{"results":[{"index":0,"relevance_score":0.9},{"index":1,"relevance_score":0.7}]}"#,
        )
        .unwrap();
        let (body, usage) = response.into_parts();
        assert_eq!(usage, None);
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert!(value.get("usage").is_none());
        assert!(value.get("meta").is_none());
        assert_eq!(value["model"], "public-rerank-model");
    }

    #[test]
    fn response_association_order_and_usage_fail_closed() {
        let request = request(true);
        for invalid in [
            br#"{"results":[{"index":0,"relevance_score":0.7,"document":"private-document-a"},{"index":0,"relevance_score":0.6,"document":"private-document-a"}]}"#.as_slice(),
            br#"{"results":[{"index":0,"relevance_score":0.7,"document":"private-document-a"},{"index":1,"relevance_score":0.9,"document":{"text":"private-document-b"}}]}"#.as_slice(),
            br#"{"results":[{"index":0,"relevance_score":0.9,"document":"wrong-document"},{"index":1,"relevance_score":0.7,"document":{"text":"private-document-b"}}]}"#.as_slice(),
            br#"{"results":[{"index":0,"relevance_score":0.9,"document":"private-document-a"},{"index":1,"relevance_score":0.7,"document":{"text":"private-document-b"}}],"usage":{"total_tokens":999999}}"#.as_slice(),
        ] {
            assert!(normalize_response(&request, invalid).is_err());
        }
    }

    #[test]
    fn debug_never_exposes_models_query_documents_or_response_body() {
        let request = request(false);
        let candidate =
            build_jina_rerank_candidate_request(&request, "private-upstream-model".to_owned())
                .unwrap();
        let response = normalize_response(
            &request,
            br#"{"results":[{"index":0,"relevance_score":0.9},{"index":1,"relevance_score":0.7}]}"#,
        )
        .unwrap();
        let rendered = format!("{request:?}\n{candidate:?}\n{response:?}");
        for private in [
            "public-rerank-model",
            "private-upstream-model",
            "private-query-canary",
            "private-document-a",
            "private-document-b",
        ] {
            assert!(!rendered.contains(private));
        }
    }
}
