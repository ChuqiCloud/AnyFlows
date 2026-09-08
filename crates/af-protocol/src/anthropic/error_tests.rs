use af_domain::PublicErrorCode;
use serde_json::json;

use super::encode_error;

#[test]
fn encodes_closed_anthropic_error_types() {
    let cases = [
        (PublicErrorCode::InvalidRequest, "invalid_request_error"),
        (PublicErrorCode::InvalidApiKey, "authentication_error"),
        (PublicErrorCode::InsufficientQuota, "billing_error"),
        (PublicErrorCode::ModelNotFound, "not_found_error"),
        (PublicErrorCode::TaskNotFound, "not_found_error"),
        (
            PublicErrorCode::IdempotencyConflict,
            "invalid_request_error",
        ),
        (PublicErrorCode::RequestOutcomeUnknown, "overloaded_error"),
        (PublicErrorCode::UpstreamUnavailable, "overloaded_error"),
        (PublicErrorCode::RateLimited, "rate_limit_error"),
        (PublicErrorCode::InternalError, "api_error"),
    ];

    for (code, kind) in cases {
        assert_eq!(
            encode_error(code, "Public message."),
            json!({
                "type": "error",
                "error": {
                    "type": kind,
                    "message": "Public message."
                }
            })
        );
    }
}
