use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::*;
use crate::{
    CanonicalImageGenerationResponseError, ImageBackground, ImageOutputFormat, ImageQuality,
    MAX_IMAGE_PROMPT_CHARS,
};

fn png_base64(canary: &[u8]) -> String {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(canary);
    STANDARD.encode(bytes)
}

fn request_with_prompt(prompt: &str) -> CanonicalImageGenerationRequest {
    parse_request(
        serde_json::json!({"model":"gpt-image-client","prompt":prompt})
            .to_string()
            .as_bytes(),
    )
    .unwrap()
}

#[test]
fn parses_minimal_request_and_redacts_external_values() {
    let request =
        parse_request(br#"{"model":"gpt-image-private","prompt":"private prompt canary"}"#)
            .unwrap();

    assert_eq!(request.operation(), af_domain::Operation::Image);
    assert_eq!(request.model(), "gpt-image-private");
    assert_eq!(request.prompt(), "private prompt canary");
    assert_eq!(request.options().count(), None);
    assert_eq!(request.options().effective_count().get(), 1);
    assert_eq!(
        request.options().effective_output_format(),
        ImageOutputFormat::Png
    );

    let debug = format!("{request:?}");
    assert!(!debug.contains("gpt-image-private"));
    assert!(!debug.contains("private prompt canary"));
}

#[test]
fn parses_full_request_and_rebuilds_only_explicit_fields() {
    let request = parse_request(
        br#"{
            "model":"gpt-image-2",
            "prompt":"draw a private scene",
            "background":"transparent",
            "moderation":"low",
            "n":2,
            "output_compression":75,
            "output_format":"webp",
            "quality":"high",
            "size":"2048x2048",
            "stream":false
        }"#,
    )
    .unwrap();

    let rebuilt = build_request(&request).unwrap();
    assert_eq!(rebuilt["n"], 2);
    assert_eq!(rebuilt["size"], "2048x2048");
    assert_eq!(rebuilt["quality"], "high");
    assert_eq!(rebuilt["background"], "transparent");
    assert_eq!(rebuilt["moderation"], "low");
    assert_eq!(rebuilt["output_format"], "webp");
    assert_eq!(rebuilt["output_compression"], 75);
    assert!(rebuilt.get("stream").is_none());
    assert_eq!(
        parse_request(&serde_json::to_vec(&rebuilt).unwrap()).unwrap(),
        request
    );
}

#[test]
fn rejects_unsupported_fields_invalid_ranges_and_parameter_conflicts() {
    for body in [
        br#"{"model":"m","prompt":"x","stream":true}"#.as_slice(),
        br#"{"model":"m","prompt":"x","partial_images":1}"#.as_slice(),
        br#"{"model":"m","prompt":"x","response_format":"b64_json"}"#.as_slice(),
        br#"{"model":"m","prompt":"x","style":"vivid"}"#.as_slice(),
        br#"{"model":"m","prompt":"x","user":"user-1"}"#.as_slice(),
        br#"{"model":"m","prompt":"x","unknown":true}"#.as_slice(),
        br#"{"model":"m","prompt":"x","n":0}"#.as_slice(),
        br#"{"model":"m","prompt":"x","n":11}"#.as_slice(),
        br#"{"model":"m","prompt":"x","size":"1024x1000"}"#.as_slice(),
        br#"{"model":"m","prompt":"x","output_compression":50}"#.as_slice(),
        br#"{"model":"m","prompt":"x","output_format":"jpeg","background":"transparent"}"#
            .as_slice(),
        br#"{"model":"m","prompt":null}"#.as_slice(),
    ] {
        assert!(
            parse_request(body).is_err(),
            "body={}",
            String::from_utf8_lossy(body)
        );
    }

    let oversized_prompt = "x".repeat(MAX_IMAGE_PROMPT_CHARS + 1);
    let body = serde_json::json!({"model":"m","prompt":oversized_prompt}).to_string();
    assert!(parse_request(body.as_bytes()).is_err());
}

#[test]
fn rejects_duplicate_request_keys_before_deserialization() {
    assert_eq!(
        parse_request(br#"{"model":"first","model":"second","prompt":"x"}"#),
        Err(ParseImageGenerationRequestError::DuplicateKey)
    );
}

#[test]
fn parses_valid_base64_response_usage_and_round_trips() {
    let encoded = png_base64(b"private-image-canary");
    let body = serde_json::json!({
        "created": 1_723_000_000,
        "background": "opaque",
        "data": [{"b64_json": encoded}],
        "output_format": "png",
        "quality": "high",
        "size": "1024x1024",
        "usage": {
            "input_tokens": 2,
            "input_tokens_details": {"image_tokens": 0, "text_tokens": 2},
            "output_tokens": 5,
            "total_tokens": 7,
            "output_tokens_details": {"image_tokens": 5, "text_tokens": 0}
        }
    });
    let response = parse_response(&serde_json::to_vec(&body).unwrap()).unwrap();

    assert_eq!(response.operation(), af_domain::Operation::Image);
    assert_eq!(response.images().len(), 1);
    assert_eq!(response.output_format(), ImageOutputFormat::Png);
    assert_eq!(response.background(), Some(ImageBackground::Opaque));
    assert_eq!(response.quality(), Some(ImageQuality::High));
    assert_eq!(response.usage().unwrap().input_tokens().get(), 2);
    assert_eq!(response.usage().unwrap().output_tokens().get(), 5);
    assert!(!format!("{response:?}").contains("private-image-canary"));

    let request = request_with_prompt("hello");
    response.validate_for_request(&request).unwrap();

    let rebuilt = build_response(&response).unwrap();
    assert!(rebuilt["data"][0].get("url").is_none());
    assert_eq!(
        parse_response(&serde_json::to_vec(&rebuilt).unwrap()).unwrap(),
        response
    );
}

#[test]
fn preserves_missing_usage_instead_of_inventing_zero() {
    let body = serde_json::json!({
        "created": 1,
        "data": [{"b64_json": png_base64(b"image")}]
    });
    let response = parse_response(&serde_json::to_vec(&body).unwrap()).unwrap();
    assert_eq!(response.usage(), None);
    assert!(build_response(&response).unwrap().get("usage").is_none());
}

#[test]
fn response_rejects_url_revised_prompt_invalid_base64_and_format_mismatch() {
    for body in [
        serde_json::json!({"created":1,"data":[{"url":"https://example.com/image.png"}]}),
        serde_json::json!({"created":1,"data":[{"b64_json":png_base64(b"x"),"revised_prompt":"secret"}]}),
        serde_json::json!({"created":1,"data":[{"b64_json":"not-base64"}]}),
        serde_json::json!({"created":1,"data":[{"b64_json":STANDARD.encode(b"not-image")}]}),
        serde_json::json!({"created":1,"data":[{"b64_json":png_base64(b"x")}],"output_format":"jpeg"}),
    ] {
        assert!(parse_response(&serde_json::to_vec(&body).unwrap()).is_err());
    }
}

#[test]
fn response_rejects_invalid_or_inconsistent_usage() {
    for usage in [
        serde_json::json!({
            "input_tokens": -1,
            "input_tokens_details": {"image_tokens":0,"text_tokens":0},
            "output_tokens": 1,
            "total_tokens": 0
        }),
        serde_json::json!({
            "input_tokens": 2,
            "input_tokens_details": {"image_tokens":0,"text_tokens":1},
            "output_tokens": 1,
            "total_tokens": 3
        }),
        serde_json::json!({
            "input_tokens": 1,
            "input_tokens_details": {"image_tokens":0,"text_tokens":1},
            "output_tokens": 2,
            "total_tokens": 4
        }),
        serde_json::json!({
            "input_tokens": 1,
            "input_tokens_details": {"image_tokens":0,"text_tokens":1},
            "output_tokens": 2,
            "total_tokens": 3,
            "output_tokens_details": {"image_tokens":1,"text_tokens":0}
        }),
    ] {
        let body = serde_json::json!({
            "created":1,
            "data":[{"b64_json":png_base64(b"x")}],
            "usage":usage
        });
        assert!(parse_response(&serde_json::to_vec(&body).unwrap()).is_err());
    }
}

#[test]
fn request_link_validation_checks_count_metadata_and_input_usage() {
    let request = parse_request(
        br#"{"model":"m","prompt":"hello","n":2,"quality":"high","size":"1024x1024"}"#,
    )
    .unwrap();
    let one_image = serde_json::json!({
        "created":1,
        "data":[{"b64_json":png_base64(b"x")}]
    });
    let response = parse_response(&serde_json::to_vec(&one_image).unwrap()).unwrap();
    assert_eq!(
        response.validate_for_request(&request),
        Err(CanonicalImageGenerationResponseError::ImageCountMismatch)
    );

    let metadata_mismatch = serde_json::json!({
        "created":1,
        "data":[
            {"b64_json":png_base64(b"x")},
            {"b64_json":png_base64(b"y")}
        ],
        "quality":"low",
        "size":"1024x1024"
    });
    let response = parse_response(&serde_json::to_vec(&metadata_mismatch).unwrap()).unwrap();
    assert_eq!(
        response.validate_for_request(&request),
        Err(CanonicalImageGenerationResponseError::MetadataMismatch)
    );

    let image_input_usage = serde_json::json!({
        "created":1,
        "data":[{"b64_json":png_base64(b"x")}],
        "usage":{
            "input_tokens":2,
            "input_tokens_details":{"image_tokens":1,"text_tokens":1},
            "output_tokens":1,
            "total_tokens":3
        }
    });
    let response = parse_response(&serde_json::to_vec(&image_input_usage).unwrap()).unwrap();
    assert_eq!(
        response.validate_for_request(&request_with_prompt("hello")),
        Err(CanonicalImageGenerationResponseError::UnexpectedInputImageUsage)
    );

    let excessive_usage = serde_json::json!({
        "created":1,
        "data":[{"b64_json":png_base64(b"x")}],
        "usage":{
            "input_tokens":2,
            "input_tokens_details":{"image_tokens":0,"text_tokens":2},
            "output_tokens":1,
            "total_tokens":3
        }
    });
    let response = parse_response(&serde_json::to_vec(&excessive_usage).unwrap()).unwrap();
    assert_eq!(
        response.validate_for_request(&request_with_prompt("x")),
        Err(CanonicalImageGenerationResponseError::UsageExceedsPromptBudget)
    );
}

#[test]
fn response_rejects_duplicate_and_unknown_fields() {
    assert_eq!(
        parse_response(
            format!(
                "{{\"created\":1,\"created\":2,\"data\":[{{\"b64_json\":\"{}\"}}]}}",
                png_base64(b"x")
            )
            .as_bytes()
        ),
        Err(ParseImageGenerationResponseError::DuplicateKey)
    );
    let unknown = serde_json::json!({
        "created":1,
        "data":[{"b64_json":png_base64(b"x"),"secret":"leak"}]
    });
    assert_eq!(
        parse_response(&serde_json::to_vec(&unknown).unwrap()),
        Err(ParseImageGenerationResponseError::InvalidValue)
    );
}
