use super::*;
use crate::CanonicalEmbeddingResponseError;

#[test]
fn parses_single_text_request_with_float_defaults() {
    let request =
        parse_request(br#"{"model":"text-embedding-3-small","input":"private input"}"#).unwrap();

    assert_eq!(request.operation(), af_domain::Operation::Embedding);
    assert_eq!(request.model(), "text-embedding-3-small");
    assert_eq!(
        request.input(),
        &EmbeddingInput::Text("private input".to_owned())
    );
    assert_eq!(request.dimensions(), None);
    let debug = format!("{request:?}");
    assert!(!debug.contains("private input"));
    assert!(!debug.contains("text-embedding-3-small"));
}

#[test]
fn parses_text_array_and_rebuilds_openai_request() {
    let request = parse_request(
        br#"{"model":"text-embedding-3-large","input":["first","second"],"encoding_format":"float","dimensions":1024}"#,
    )
    .unwrap();

    let rebuilt = build_request(&request).unwrap();
    assert_eq!(rebuilt["model"], "text-embedding-3-large");
    assert_eq!(rebuilt["input"], serde_json::json!(["first", "second"]));
    assert_eq!(rebuilt["encoding_format"], "float");
    assert_eq!(rebuilt["dimensions"], 1024);
    assert_eq!(
        parse_request(&serde_json::to_vec(&rebuilt).unwrap()).unwrap(),
        request
    );
}

#[test]
fn rejects_unsupported_or_ambiguous_input_shapes() {
    for body in [
        br#"{"model":"text-embedding-3-small","input":[1,2,3]}"#.as_slice(),
        br#"{"model":"text-embedding-3-small","input":[]}"#.as_slice(),
        br#"{"model":"text-embedding-3-small","input":"","encoding_format":"float"}"#.as_slice(),
        br#"{"model":"text-embedding-3-small","input":"x","encoding_format":"base64"}"#.as_slice(),
        br#"{"model":"text-embedding-3-small","input":"x","user":"raw-user"}"#.as_slice(),
        br#"{"model":"text-embedding-3-small","input":"x","dimensions":0}"#.as_slice(),
    ] {
        assert!(parse_request(body).is_err());
    }
}

#[test]
fn rejects_duplicate_request_keys_before_deserialization() {
    assert_eq!(
        parse_request(br#"{"model":"first","model":"second","input":"x"}"#),
        Err(ParseEmbeddingRequestError::DuplicateKey)
    );
}

#[test]
fn parses_sorts_and_rebuilds_embedding_response() {
    let response = parse_response(
        br#"{
            "object":"list",
            "data":[
                {"object":"embedding","index":1,"embedding":[0.25,0.75]},
                {"object":"embedding","index":0,"embedding":[1.0,0.0]}
            ],
            "model":"upstream-private-model",
            "usage":{"prompt_tokens":7,"total_tokens":7}
        }"#,
    )
    .unwrap();

    assert_eq!(response.operation(), af_domain::Operation::Embedding);
    assert_eq!(response.vectors()[0].index(), 0);
    assert_eq!(response.vectors()[1].index(), 1);
    assert_eq!(response.usage().input_tokens().get(), 7);
    assert_eq!(response.usage().output_tokens(), TokenCount::ZERO);
    assert!(!format!("{response:?}").contains("upstream-private-model"));

    let rebuilt = build_response(&response).unwrap();
    assert_eq!(rebuilt["object"], "list");
    assert_eq!(rebuilt["data"][0]["index"], 0);
    assert_eq!(
        parse_response(&serde_json::to_vec(&rebuilt).unwrap()).unwrap(),
        response
    );

    let client_response = response
        .rebind_model("client-visible-model".to_owned())
        .unwrap();
    assert_eq!(
        build_response(&client_response).unwrap()["model"],
        "client-visible-model"
    );
}

#[test]
fn response_requires_contiguous_indexes_consistent_usage_and_request_dimensions() {
    let invalid_indexes = br#"{
        "object":"list",
        "data":[{"object":"embedding","index":1,"embedding":[1.0]}],
        "model":"model",
        "usage":{"prompt_tokens":1,"total_tokens":1}
    }"#;
    assert_eq!(
        parse_response(invalid_indexes),
        Err(ParseEmbeddingResponseError::InvalidValue)
    );

    let invalid_usage = br#"{
        "object":"list",
        "data":[{"object":"embedding","index":0,"embedding":[1.0]}],
        "model":"model",
        "usage":{"prompt_tokens":1,"total_tokens":2}
    }"#;
    assert_eq!(
        parse_response(invalid_usage),
        Err(ParseEmbeddingResponseError::InvalidValue)
    );

    let request =
        parse_request(br#"{"model":"model","input":["one","two"],"dimensions":2}"#).unwrap();
    let response = parse_response(
        br#"{
            "object":"list",
            "data":[{"object":"embedding","index":0,"embedding":[1.0,2.0]}],
            "model":"model",
            "usage":{"prompt_tokens":1,"total_tokens":1}
        }"#,
    )
    .unwrap();
    assert_eq!(
        response.validate_for_request(&request),
        Err(CanonicalEmbeddingResponseError::InputCountMismatch)
    );

    let dimensions_response = parse_response(
        br#"{
            "object":"list",
            "data":[
                {"object":"embedding","index":0,"embedding":[1.0]},
                {"object":"embedding","index":1,"embedding":[2.0]}
            ],
            "model":"model",
            "usage":{"prompt_tokens":1,"total_tokens":1}
        }"#,
    )
    .unwrap();
    assert_eq!(
        dimensions_response.validate_for_request(&request),
        Err(CanonicalEmbeddingResponseError::DimensionsMismatch)
    );

    let request_without_dimensions =
        parse_request(br#"{"model":"model","input":["one","two"]}"#).unwrap();
    let mixed_dimensions_response = parse_response(
        br#"{
            "object":"list",
            "data":[
                {"object":"embedding","index":0,"embedding":[1.0]},
                {"object":"embedding","index":1,"embedding":[2.0,3.0]}
            ],
            "model":"model",
            "usage":{"prompt_tokens":1,"total_tokens":1}
        }"#,
    )
    .unwrap();
    assert_eq!(
        mixed_dimensions_response.validate_for_request(&request_without_dimensions),
        Err(CanonicalEmbeddingResponseError::DimensionsMismatch)
    );
}

#[test]
fn response_rejects_duplicate_keys_and_unknown_fields() {
    assert_eq!(
        parse_response(
            br#"{
                "object":"list",
                "data":[],
                "model":"one",
                "model":"two",
                "usage":{"prompt_tokens":0,"total_tokens":0}
            }"#,
        ),
        Err(ParseEmbeddingResponseError::DuplicateKey)
    );
    assert_eq!(
        parse_response(
            br#"{
                "object":"list",
                "data":[{"object":"embedding","index":0,"embedding":[1.0],"leak":true}],
                "model":"model",
                "usage":{"prompt_tokens":1,"total_tokens":1}
            }"#,
        ),
        Err(ParseEmbeddingResponseError::InvalidValue)
    );
}
