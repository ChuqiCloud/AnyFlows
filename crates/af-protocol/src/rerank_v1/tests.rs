use super::*;
use crate::{CanonicalRerankResponseError, RerankDocument};

#[test]
fn parses_mixed_text_documents_and_rebuilds_stable_request() {
    let request = parse_request(
        br#"{
            "model":"jina-reranker-v2-base-multilingual",
            "query":"private query",
            "documents":["first private document",{"text":"second private document"}],
            "top_n":2
        }"#,
    )
    .unwrap();

    assert_eq!(request.operation(), af_domain::Operation::Rerank);
    assert_eq!(request.model(), "jina-reranker-v2-base-multilingual");
    assert_eq!(request.query(), "private query");
    assert_eq!(request.documents().len(), 2);
    assert_eq!(request.documents()[0].text(), "first private document");
    assert!(!request.documents()[0].is_text_object());
    assert!(request.documents()[1].is_text_object());
    assert_eq!(request.top_n().unwrap().get(), 2);
    assert!(!request.return_documents());

    let debug = format!("{request:?}");
    assert!(!debug.contains("private query"));
    assert!(!debug.contains("private document"));
    assert!(!debug.contains("jina-reranker"));

    let rebuilt = build_request(&request).unwrap();
    insta::assert_snapshot!(serde_json::to_string_pretty(&rebuilt).unwrap(), @r###"
    {
      "documents": [
        "first private document",
        {
          "text": "second private document"
        }
      ],
      "model": "jina-reranker-v2-base-multilingual",
      "query": "private query",
      "return_documents": false,
      "top_n": 2
    }
    "###);
    assert_eq!(
        parse_request(&serde_json::to_vec(&rebuilt).unwrap()).unwrap(),
        request
    );
}

#[test]
fn request_defaults_return_documents_to_false_and_validates_top_n() {
    let request =
        parse_request(br#"{"model":"rerank-model","query":"q","documents":["one","two"]}"#)
            .unwrap();
    assert!(!request.return_documents());
    assert_eq!(request.expected_result_count(), 2);

    for body in [
        br#"{"model":"rerank-model","query":"q","documents":["one"],"top_n":0}"#.as_slice(),
        br#"{"model":"rerank-model","query":"q","documents":["one"],"top_n":2}"#.as_slice(),
        br#"{"model":"rerank-model","query":" ","documents":["one"]}"#.as_slice(),
        br#"{"model":"rerank-model","query":"q","documents":[]}"#.as_slice(),
        br#"{"model":"rerank-model","query":"q","documents":[{"text":"one","title":"leak"}]}"#
            .as_slice(),
    ] {
        assert_eq!(
            parse_request(body),
            Err(ParseRerankRequestError::InvalidValue)
        );
    }
}

#[test]
fn request_rejects_duplicate_null_unknown_and_known_unsupported_fields() {
    assert_eq!(
        parse_request(br#"{"model":"one","model":"two","query":"q","documents":["doc"]}"#),
        Err(ParseRerankRequestError::DuplicateKey)
    );

    for body in [
        br#"{"model":"model","query":"q","documents":["doc"],"top_n":null}"#.as_slice(),
        br#"{"model":"model","query":"q","documents":["doc"],"user":"raw-user"}"#.as_slice(),
    ] {
        assert_eq!(
            parse_request(body),
            Err(ParseRerankRequestError::InvalidValue)
        );
    }

    for body in [
        br#"{"model":"model","query":"q","documents":["doc"],"rank_fields":["text"]}"#.as_slice(),
        br#"{"model":"model","query":"q","documents":["doc"],"max_tokens_per_doc":4096}"#
            .as_slice(),
        br#"{"model":"model","query":"q","documents":["doc"],"return_embeddings":true}"#.as_slice(),
    ] {
        assert_eq!(
            parse_request(body),
            Err(ParseRerankRequestError::UnsupportedFeature)
        );
    }
}

#[test]
fn parses_ranked_response_with_token_and_search_unit_facts() {
    let response = parse_response(
        br#"{
            "id":"rerank-private-id",
            "model":"upstream-private-model",
            "object":"list",
            "results":[
                {"index":1,"relevance_score":0.95,"document":{"text":"second"}},
                {"index":0,"relevance_score":0.75,"document":"first"}
            ],
            "usage":{"prompt_tokens":7,"total_tokens":7,"completion_tokens":0},
            "meta":{"billed_units":{"search_units":1}}
        }"#,
    )
    .unwrap();

    assert_eq!(response.operation(), af_domain::Operation::Rerank);
    assert_eq!(response.response_id(), Some("rerank-private-id"));
    assert_eq!(response.model(), Some("upstream-private-model"));
    assert_eq!(response.results()[0].index(), 1);
    assert_eq!(response.results()[0].relevance_score().get(), 0.95);
    assert_eq!(response.results()[0].document().unwrap().text(), "second");
    let usage = response.usage().unwrap();
    assert_eq!(usage.token_usage().unwrap().input_tokens().get(), 7);
    assert_eq!(usage.search_units().unwrap().get(), 1);

    let debug = format!("{response:?}");
    assert!(!debug.contains("rerank-private-id"));
    assert!(!debug.contains("upstream-private-model"));
    assert!(!debug.contains("second"));

    let rebuilt = build_response(&response).unwrap();
    insta::assert_snapshot!(serde_json::to_string_pretty(&rebuilt).unwrap(), @r###"
    {
      "id": "rerank-private-id",
      "meta": {
        "billed_units": {
          "search_units": 1
        }
      },
      "model": "upstream-private-model",
      "object": "list",
      "results": [
        {
          "document": {
            "text": "second"
          },
          "index": 1,
          "relevance_score": 0.95
        },
        {
          "document": "first",
          "index": 0,
          "relevance_score": 0.75
        }
      ],
      "usage": {
        "prompt_tokens": 7,
        "total_tokens": 7
      }
    }
    "###);
    assert_eq!(
        parse_response(&serde_json::to_vec(&rebuilt).unwrap()).unwrap(),
        response
    );
}

#[test]
fn response_preserves_missing_usage_without_fabricating_zero() {
    let response = parse_response(
        br#"{
            "results":[
                {"index":0,"relevance_score":1.0}
            ]
        }"#,
    )
    .unwrap();
    assert_eq!(response.usage(), None);
    let rebuilt = build_response(&response).unwrap();
    assert!(rebuilt.get("usage").is_none());
    assert!(rebuilt.get("meta").is_none());
}

#[test]
fn response_validates_request_indexes_documents_counts_and_usage() {
    let request = parse_request(
        br#"{
            "model":"model",
            "query":"query",
            "documents":["first",{"text":"second"}],
            "top_n":1,
            "return_documents":true
        }"#,
    )
    .unwrap();
    let valid = parse_response(
        br#"{
            "results":[{"index":1,"relevance_score":0.9,"document":"second"}],
            "usage":{"total_tokens":3}
        }"#,
    )
    .unwrap();
    valid.validate_for_request(&request).unwrap();

    let wrong_document = parse_response(
        br#"{
            "results":[{"index":1,"relevance_score":0.9,"document":"first"}]
        }"#,
    )
    .unwrap();
    assert_eq!(
        wrong_document.validate_for_request(&request),
        Err(CanonicalRerankResponseError::DocumentMismatch)
    );

    let wrong_count = parse_response(
        br#"{
            "results":[
                {"index":1,"relevance_score":0.9,"document":"second"},
                {"index":0,"relevance_score":0.8,"document":"first"}
            ]
        }"#,
    )
    .unwrap();
    assert_eq!(
        wrong_count.validate_for_request(&request),
        Err(CanonicalRerankResponseError::ResultCountMismatch)
    );

    let excessive_usage = parse_response(
        br#"{
            "results":[{"index":1,"relevance_score":0.9,"document":"second"}],
            "usage":{"total_tokens":999}
        }"#,
    )
    .unwrap();
    assert_eq!(
        excessive_usage.validate_for_request(&request),
        Err(CanonicalRerankResponseError::UsageExceedsInputBudget)
    );
}

#[test]
fn response_rejects_duplicate_indexes_invalid_order_and_usage_semantics() {
    for body in [
        br#"{
            "results":[
                {"index":0,"relevance_score":0.9},
                {"index":0,"relevance_score":0.8}
            ]
        }"#
        .as_slice(),
        br#"{
            "results":[
                {"index":0,"relevance_score":0.8},
                {"index":1,"relevance_score":0.9}
            ]
        }"#
        .as_slice(),
        br#"{
            "results":[{"index":0,"relevance_score":0.9}],
            "usage":{"prompt_tokens":2,"total_tokens":3}
        }"#
        .as_slice(),
        br#"{
            "results":[{"index":0,"relevance_score":0.9}],
            "usage":{"total_tokens":3,"completion_tokens":1}
        }"#
        .as_slice(),
        br#"{
            "results":[{"index":0,"relevance_score":0.9}],
            "meta":{"billed_units":{"search_units":0}}
        }"#
        .as_slice(),
    ] {
        assert_eq!(
            parse_response(body),
            Err(ParseRerankResponseError::InvalidValue)
        );
    }
}

#[test]
fn response_rejects_duplicate_unknown_null_and_embedding_fields() {
    assert_eq!(
        parse_response(
            br#"{
                "results":[{"index":0,"index":1,"relevance_score":0.9}]
            }"#
        ),
        Err(ParseRerankResponseError::DuplicateKey)
    );

    for body in [
        br#"{"results":[{"index":0,"relevance_score":0.9,"document":null}]}"#.as_slice(),
        br#"{"results":[{"index":0,"relevance_score":0.9,"private":true}]}"#.as_slice(),
    ] {
        assert_eq!(
            parse_response(body),
            Err(ParseRerankResponseError::InvalidValue)
        );
    }

    assert_eq!(
        parse_response(
            br#"{
                "results":[{"index":0,"relevance_score":0.9,"embedding":[1.0]}]
            }"#
        ),
        Err(ParseRerankResponseError::UnsupportedFeature)
    );
}

#[test]
fn canonical_response_rejects_non_finite_scores_and_unrequested_documents() {
    assert!(RerankRelevanceScore::new(f64::NAN).is_err());
    assert!(RerankRelevanceScore::new(f64::INFINITY).is_err());

    let request = CanonicalRerankRequest::new(
        "model".to_owned(),
        "query".to_owned(),
        vec![RerankDocument::Text("document".to_owned())],
        None,
        false,
    )
    .unwrap();
    let result = RerankResult::new(
        0,
        RerankRelevanceScore::new(1.0).unwrap(),
        Some(RerankDocument::Text("document".to_owned())),
    )
    .unwrap();
    let response = CanonicalRerankResponse::new(None, None, vec![result], None).unwrap();
    assert_eq!(
        response.validate_for_request(&request),
        Err(CanonicalRerankResponseError::DocumentMismatch)
    );
}
