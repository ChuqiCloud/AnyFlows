use af_domain::PublicErrorCode;
use serde_json::json;

use super::encode_error;

#[test]
fn encodes_closed_google_json_api_statuses() {
    let cases = [
        (PublicErrorCode::InvalidRequest, 400, "INVALID_ARGUMENT"),
        (PublicErrorCode::InvalidApiKey, 401, "UNAUTHENTICATED"),
        (
            PublicErrorCode::InsufficientQuota,
            429,
            "RESOURCE_EXHAUSTED",
        ),
        (PublicErrorCode::ModelNotFound, 404, "NOT_FOUND"),
        (PublicErrorCode::TaskNotFound, 404, "NOT_FOUND"),
        (PublicErrorCode::IdempotencyConflict, 409, "ALREADY_EXISTS"),
        (PublicErrorCode::RequestOutcomeUnknown, 503, "UNAVAILABLE"),
        (PublicErrorCode::UpstreamUnavailable, 503, "UNAVAILABLE"),
        (PublicErrorCode::RateLimited, 429, "RESOURCE_EXHAUSTED"),
        (PublicErrorCode::InternalError, 500, "INTERNAL"),
    ];
    assert_eq!(cases.len(), PublicErrorCode::ALL.len());

    for (code, status_code, status) in cases {
        assert_eq!(
            encode_error(code, "Public message."),
            json!({
                "error": {
                    "code": status_code,
                    "message": "Public message.",
                    "status": status,
                }
            })
        );
    }
}
