use serde_json::json;

use super::*;
use crate::{RerankDocument, RerankTopN};

fn request() -> CanonicalRerankRequest {
    CanonicalRerankRequest::new(
        "rerank-v4.0-pro".to_owned(),
        "capital".to_owned(),
        vec![
            RerankDocument::Text("Washington".to_owned()),
            RerankDocument::TextObject("Carson City".to_owned()),
        ],
        Some(RerankTopN::new(1).unwrap()),
        true,
    )
    .unwrap()
}

#[test]
fn builds_official_v2_request_without_gateway_only_fields() {
    assert_eq!(
        build_request(&request()),
        json!({
            "model": "rerank-v4.0-pro",
            "query": "capital",
            "documents": ["Washington", "Carson City"],
            "top_n": 1
        })
    );
}

#[test]
fn parses_official_response_and_preserves_search_units() {
    let response = parse_response(
        br#"{"results":[{"index":0,"relevance_score":0.999071}],"id":"private-response","meta":{"api_version":{"version":"2","is_experimental":false},"billed_units":{"search_units":1}}}"#,
    )
    .unwrap();
    assert_eq!(response.response_id(), Some("private-response"));
    let usage = response.usage().unwrap();
    assert_eq!(usage.token_usage(), None);
    assert_eq!(usage.search_units().unwrap().get(), 1);
}

#[test]
fn preserves_real_input_tokens_without_converting_search_units() {
    let response = parse_response(
        br#"{"results":[{"index":0,"relevance_score":0.9}],"meta":{"billed_units":{"search_units":1},"tokens":{"input_tokens":7,"output_tokens":0}}}"#,
    )
    .unwrap();
    let usage = response.usage().unwrap();
    assert_eq!(usage.token_usage().unwrap().input_tokens().get(), 7);
    assert_eq!(usage.search_units().unwrap().get(), 1);
}

#[test]
fn rejects_duplicate_unknown_fractional_and_unmodeled_usage() {
    for invalid in [
        br#"{"results":[{"index":0,"index":1,"relevance_score":0.9}]}"#.as_slice(),
        br#"{"results":[{"index":0,"relevance_score":0.9,"document":"x"}]}"#.as_slice(),
        br#"{"results":[{"index":0,"relevance_score":0.9}],"meta":{"billed_units":{"search_units":0.5}}}"#.as_slice(),
        br#"{"results":[{"index":0,"relevance_score":0.9}],"meta":{"tokens":{"input_tokens":7,"output_tokens":1}}}"#.as_slice(),
        br#"{"results":[{"index":0,"relevance_score":0.9}],"meta":{"warnings":["private warning"]}}"#.as_slice(),
    ] {
        assert!(parse_response(invalid).is_err());
    }
}
