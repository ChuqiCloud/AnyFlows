use af_adapter::Bytes;
use af_domain::AfError;
use af_protocol::{
    CanonicalRerankRequest, CanonicalRerankResponse, RerankResult, cohere_rerank_v2,
};

use crate::{RelayCandidateRequest, RelayError};

/// 使用映射后的模型构造 Cohere v2 候选请求。
///
/// Cohere v2 不接受 `return_documents`，且 documents 只能是字符串，因此即使模型没有
/// 映射也必须为每个 Cohere 候选显式覆盖正文。
pub fn build_cohere_rerank_candidate_request(
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
    let body = serde_json::to_vec(&cohere_rerank_v2::build_request(&mapped))
        .map(Bytes::from)
        .map_err(|_| AfError::Internal)?;
    RelayCandidateRequest::new(upstream_model, Some(body)).map_err(map_relay_error)
}

/// 解析 Cohere v2 响应并依据原请求安全回填公开文档。
pub(crate) fn normalize_cohere_response(
    request: &CanonicalRerankRequest,
    body: &[u8],
) -> Result<CanonicalRerankResponse, ()> {
    let upstream = cohere_rerank_v2::parse_response(body).map_err(|_| ())?;
    let results = upstream
        .results()
        .iter()
        .map(|result| {
            let index = usize::try_from(result.index()).map_err(|_| ())?;
            let document = if request.return_documents() {
                Some(request.documents().get(index).cloned().ok_or(())?)
            } else {
                None
            };
            RerankResult::new(result.index(), result.relevance_score(), document).map_err(|_| ())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let normalized = CanonicalRerankResponse::new(
        upstream.response_id().map(str::to_owned),
        None,
        results,
        upstream.usage(),
    )
    .map_err(|_| ())?;
    normalized.validate_for_request(request).map_err(|_| ())?;
    normalized
        .rebind_model(request.model().to_owned())
        .map_err(|_| ())
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
    use af_protocol::rerank_v1;

    fn request(return_documents: bool) -> CanonicalRerankRequest {
        rerank_v1::parse_request(
            format!(
                r#"{{"model":"public-rerank","query":"private-query","documents":["private-a",{{"text":"private-b"}}],"top_n":2,"return_documents":{return_documents}}}"#
            )
            .as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn candidate_request_uses_v2_shape_and_mapped_model() {
        let candidate = build_cohere_rerank_candidate_request(
            &request(true),
            "private-upstream-model".to_owned(),
        )
        .unwrap();
        let (model, body) = candidate.into_parts();
        assert_eq!(model, "private-upstream-model");
        let value: Value = serde_json::from_slice(&body.unwrap()).unwrap();
        assert_eq!(
            value["documents"],
            serde_json::json!(["private-a", "private-b"])
        );
        assert!(value.get("return_documents").is_none());
        assert_eq!(value["model"], "private-upstream-model");
    }

    #[test]
    fn official_response_rebinds_model_and_backfills_requested_documents() {
        let request = request(true);
        let response = normalize_cohere_response(
            &request,
            br#"{"results":[{"index":1,"relevance_score":0.9},{"index":0,"relevance_score":0.7}],"id":"private-id","meta":{"api_version":{"version":"2"},"billed_units":{"search_units":1}}}"#,
        )
        .unwrap();
        assert_eq!(response.model(), Some("public-rerank"));
        assert_eq!(
            response.results()[0].document().unwrap().text(),
            "private-b"
        );
        assert_eq!(response.usage().unwrap().search_units().unwrap().get(), 1);
    }
}
