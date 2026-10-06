use af_admin::PresentedApiKey;
use http::{HeaderValue, Request, header::AUTHORIZATION};

use crate::{ApiKeyExtractionError, QueryApiKeyPolicy, extract_presented_api_key};

const TEST_KEY: &str = "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

fn request(uri: &str) -> Request<()> {
    Request::builder().uri(uri).body(()).unwrap()
}

fn assert_extracted(request: &mut Request<()>, policy: QueryApiKeyPolicy) {
    let key = extract_presented_api_key(request, policy).unwrap();
    assert_eq!(key.expose_secret(), TEST_KEY);
    assert_scrubbed(request);
}

fn assert_scrubbed(request: &Request<()>) {
    assert!(request.headers().get(AUTHORIZATION).is_none());
    assert!(request.headers().get("x-api-key").is_none());
    assert!(request.headers().get("x-goog-api-key").is_none());
    assert!(!request.uri().to_string().contains(TEST_KEY));
    assert!(!format!("{request:?}").contains(TEST_KEY));
}

#[test]
fn accepts_each_unique_header_carrier_and_scrubs_it() {
    let mut authorization = request("/v1/chat/completions");
    authorization.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("bEaReR   {TEST_KEY}")).unwrap(),
    );
    assert_extracted(&mut authorization, QueryApiKeyPolicy::Deny);

    let mut x_api_key = request("/v1/chat/completions");
    x_api_key
        .headers_mut()
        .insert("x-api-key", HeaderValue::from_static(TEST_KEY));
    assert_extracted(&mut x_api_key, QueryApiKeyPolicy::Deny);

    let mut x_goog_api_key = request("/v1/chat/completions");
    x_goog_api_key
        .headers_mut()
        .insert("x-goog-api-key", HeaderValue::from_static(TEST_KEY));
    assert_extracted(&mut x_goog_api_key, QueryApiKeyPolicy::Deny);
}

#[test]
fn query_carrier_requires_opt_in_and_preserves_other_raw_parameters() {
    assert_eq!(QueryApiKeyPolicy::default(), QueryApiKeyPolicy::Deny);
    let uri = format!("/v1/chat/completions?first=a%20b&k%65y={TEST_KEY}&last=%2Fraw");
    let mut allowed = request(&uri);
    assert_extracted(&mut allowed, QueryApiKeyPolicy::Allow);
    assert_eq!(
        allowed.uri(),
        "/v1/chat/completions?first=a%20b&last=%2Fraw"
    );
    let mut denied = request(&uri);
    let error = extract_presented_api_key(&mut denied, QueryApiKeyPolicy::Deny).unwrap_err();
    assert_eq!(error, ApiKeyExtractionError::QueryDenied);
    assert_eq!(denied.uri(), "/v1/chat/completions?first=a%20b&last=%2Fraw");
    assert_scrubbed(&denied);

    let absolute_uri = format!("https://gateway.example/v1/chat/completions?key={TEST_KEY}&x=1");
    let mut absolute = request(&absolute_uri);
    assert_extracted(&mut absolute, QueryApiKeyPolicy::Allow);
    assert_eq!(
        absolute.uri(),
        "https://gateway.example/v1/chat/completions?x=1"
    );

    let sparse_uri = format!("/v1/chat/completions?a=1&&key={TEST_KEY}&b=2&");
    let mut sparse = request(&sparse_uri);
    assert_extracted(&mut sparse, QueryApiKeyPolicy::Allow);
    assert_eq!(sparse.uri(), "/v1/chat/completions?a=1&&b=2&");

    let encoded_key = TEST_KEY.replace('-', "%2D");
    let mut encoded = request(&format!("/v1/chat/completions?key={encoded_key}&x=1"));
    let error = extract_presented_api_key(&mut encoded, QueryApiKeyPolicy::Allow).unwrap_err();
    assert_eq!(error, ApiKeyExtractionError::Malformed);
    assert_eq!(encoded.uri(), "/v1/chat/completions?x=1");
    assert!(!format!("{encoded:?}").contains(&encoded_key));
}

#[test]
fn duplicate_or_mixed_carriers_are_always_ambiguous() {
    let mut duplicate = request("/v1/chat/completions");
    duplicate.headers_mut().append(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {TEST_KEY}")).unwrap(),
    );
    duplicate.headers_mut().append(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {TEST_KEY}")).unwrap(),
    );
    let error = extract_presented_api_key(&mut duplicate, QueryApiKeyPolicy::Deny).unwrap_err();
    assert_eq!(error, ApiKeyExtractionError::Ambiguous);
    assert_scrubbed(&duplicate);

    let mut mixed = request(&format!("/v1/chat/completions?key={TEST_KEY}"));
    mixed
        .headers_mut()
        .insert("x-api-key", HeaderValue::from_static(TEST_KEY));
    let error = extract_presented_api_key(&mut mixed, QueryApiKeyPolicy::Allow).unwrap_err();
    assert_eq!(error, ApiKeyExtractionError::Ambiguous);
    assert_eq!(mixed.uri(), "/v1/chat/completions");
    assert_scrubbed(&mixed);

    let mut repeated_query = request(&format!(
        "/v1/chat/completions?key={TEST_KEY}&key={TEST_KEY}"
    ));
    let error =
        extract_presented_api_key(&mut repeated_query, QueryApiKeyPolicy::Allow).unwrap_err();
    assert_eq!(error, ApiKeyExtractionError::Ambiguous);
    assert_eq!(repeated_query.uri(), "/v1/chat/completions");
    assert_scrubbed(&repeated_query);
}

#[test]
fn malformed_or_missing_credentials_fail_without_retaining_secret_input() {
    for authorization in [
        TEST_KEY.to_owned(),
        format!("Basic {TEST_KEY}"),
        format!("Bearer {TEST_KEY} "),
    ] {
        let mut request = request("/v1/chat/completions");
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_str(&authorization).unwrap(),
        );
        let error = extract_presented_api_key(&mut request, QueryApiKeyPolicy::Deny).unwrap_err();
        assert_eq!(error, ApiKeyExtractionError::Malformed);
        assert_scrubbed(&request);
    }

    let mut invalid_bytes = request("/v1/chat/completions");
    invalid_bytes
        .headers_mut()
        .insert("x-api-key", HeaderValue::from_bytes(&[0xff]).unwrap());
    let error = extract_presented_api_key(&mut invalid_bytes, QueryApiKeyPolicy::Deny).unwrap_err();
    assert_eq!(error, ApiKeyExtractionError::Malformed);
    assert_scrubbed(&invalid_bytes);

    let mut missing = request("/v1/chat/completions?model=test");
    let error = extract_presented_api_key(&mut missing, QueryApiKeyPolicy::Deny).unwrap_err();
    assert_eq!(error, ApiKeyExtractionError::Missing);
    assert_eq!(missing.uri(), "/v1/chat/completions?model=test");
}

#[test]
fn extracted_types_and_errors_have_only_fixed_redacted_debug_output() {
    let key = PresentedApiKey::parse(TEST_KEY).unwrap();
    for rendered in [
        format!("{key:?}"),
        format!("{:?}", ApiKeyExtractionError::Missing),
        format!("{:?}", ApiKeyExtractionError::Malformed),
        format!("{:?}", ApiKeyExtractionError::Ambiguous),
        format!("{:?}", ApiKeyExtractionError::QueryDenied),
    ] {
        assert!(!rendered.contains(TEST_KEY));
    }
}
