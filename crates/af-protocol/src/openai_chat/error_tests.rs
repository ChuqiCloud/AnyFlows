use af_domain::PublicErrorCode;
use serde_json::json;

use super::encode_error;

#[test]
fn error_encoder_covers_every_public_code_with_openai_shape() {
    let cases = [
        (PublicErrorCode::InvalidRequest, "invalid_request_error"),
        (PublicErrorCode::InvalidApiKey, "invalid_request_error"),
        (PublicErrorCode::InsufficientQuota, "insufficient_quota"),
        (PublicErrorCode::ModelNotFound, "invalid_request_error"),
        (PublicErrorCode::TaskNotFound, "invalid_request_error"),
        (
            PublicErrorCode::IdempotencyConflict,
            "invalid_request_error",
        ),
        (PublicErrorCode::RequestOutcomeUnknown, "server_error"),
        (PublicErrorCode::UpstreamUnavailable, "server_error"),
        (PublicErrorCode::RateLimited, "rate_limit_error"),
        (PublicErrorCode::InternalError, "server_error"),
    ];
    assert_eq!(cases.len(), PublicErrorCode::ALL.len());

    for (code, kind) in cases {
        let encoded = encode_error(code, "Public error message.");
        assert_eq!(
            encoded,
            json!({
                "error": {
                    "message": "Public error message.",
                    "type": kind,
                    "param": null,
                    "code": code.as_str(),
                }
            })
        );
        assert_eq!(encoded.as_object().unwrap().len(), 1);
        assert_eq!(encoded["error"].as_object().unwrap().len(), 4);
    }
}
