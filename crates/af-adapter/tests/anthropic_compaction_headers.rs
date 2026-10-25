use af_adapter::{
    Adaptor, AdaptorError, AnthropicAdaptor, Bytes, Credential, HeaderMap, HeaderName, HeaderValue,
    HttpClientConfig, HttpClientPool, MAX_UPSTREAM_REQUEST_HEADER_COUNT,
    MAX_UPSTREAM_REQUEST_HEADER_VALUE_BYTES, MAX_UPSTREAM_REQUEST_HEADERS_BYTES,
    MAX_UPSTREAM_RESPONSE_BODY_BYTES, Method, RelayContext, ResponseMode, UpstreamRequest,
};

const ON_DEMAND_BETA: &str = "compact-2026-09-04";
const THRESHOLD_BETA: &str = "compact-2026-01-12";

fn context() -> RelayContext {
    RelayContext::new(
        HttpClientPool::default()
            .get(&HttpClientConfig::default())
            .unwrap(),
    )
}

fn request(body: Bytes, headers: HeaderMap) -> UpstreamRequest {
    UpstreamRequest::new(
        Method::POST,
        "https://upstream.example/proxy/anthropic/v1/messages",
        headers,
        Some(body),
    )
    .unwrap()
    .with_response_mode(ResponseMode::Stream)
    .with_response_body_limit(MAX_UPSTREAM_RESPONSE_BODY_BYTES + 1)
    .unwrap()
}

#[tokio::test]
async fn on_demand_compaction_adds_its_beta_when_no_beta_header_is_supplied() {
    let body = Bytes::from_static(br#"{"compaction":{"type":"summarize"}}"#);
    let finalized = AnthropicAdaptor::new()
        .finalize_request(
            request(body.clone(), HeaderMap::new()),
            &Credential::api_key("api-key-canary").unwrap(),
            &context(),
        )
        .await
        .unwrap();
    assert_eq!(finalized.headers()["anthropic-beta"], ON_DEMAND_BETA);
    assert_eq!(finalized.body(), Some(&body));
}

#[tokio::test]
async fn on_demand_compaction_adds_beta_after_overrides_without_changing_request_bytes() {
    let body = Bytes::from_static(
        br#"{ "model": "claude-test", "max_tokens": 4096,
"messages": [{"role":"user","content":"summarize this history"}],
"compaction": {"type":"summarize","instructions":"Keep tool decisions"},
"vendor_extension": {"keep_exact_bytes": true} }"#,
    );
    let adaptor = AnthropicAdaptor::new();
    let context = context();
    for credential in [
        Credential::api_key("api-key-canary").unwrap(),
        Credential::oauth("oauth-canary").unwrap(),
    ] {
        let mut headers = HeaderMap::new();
        adaptor
            .setup_headers(&mut headers, &credential, &context)
            .unwrap();
        let mut overrides = HeaderMap::new();
        overrides.append(
            "anthropic-beta",
            HeaderValue::from_static("client-feature-2099-01-01, compact-2026-09-04"),
        );
        overrides.append(
            "anthropic-beta",
            HeaderValue::from_static(
                "compact-2026-09-04, context-management-2025-06-27,client-feature-2099-01-01",
            ),
        );
        let upstream = request(body.clone(), headers)
            .with_header_overrides(overrides)
            .unwrap();
        let finalized = adaptor
            .finalize_request(upstream, &credential, &context)
            .await
            .unwrap();
        assert_eq!(
            finalized.headers()["anthropic-beta"],
            "client-feature-2099-01-01,compact-2026-09-04,context-management-2025-06-27"
        );
        assert_eq!(
            finalized.headers().get_all("anthropic-beta").iter().count(),
            1
        );
        assert_eq!(finalized.body(), Some(&body));
        assert_eq!(finalized.method(), Method::POST);
        assert_eq!(
            finalized.target(),
            "https://upstream.example/proxy/anthropic/v1/messages"
        );
        assert_eq!(finalized.response_mode(), ResponseMode::Stream);
        assert_eq!(
            finalized.response_body_limit(),
            MAX_UPSTREAM_RESPONSE_BODY_BYTES + 1
        );
        let second = adaptor
            .finalize_request(finalized, &credential, &context)
            .await
            .unwrap();
        assert_eq!(second.body(), Some(&body));
        assert_eq!(
            second.headers()["anthropic-beta"],
            "client-feature-2099-01-01,compact-2026-09-04,context-management-2025-06-27"
        );
    }
}

#[tokio::test]
async fn threshold_compaction_uses_its_own_beta_and_keeps_explicit_context_management_beta() {
    let body = Bytes::from_static(
        br#"{"model":"claude-test","context_management":{"edits":[
{"type":"clear_thinking_20251015","keep":"all"},
{"type":"compact_20260112","trigger":{"type":"input_tokens","value":150000},"pause_after_compaction":true}
]}}"#,
    );
    let adaptor = AnthropicAdaptor::new();
    let credential = Credential::api_key("api-key-canary").unwrap();
    let context = context();
    for client_beta in [None, Some("context-management-2025-06-27")] {
        let mut headers = HeaderMap::new();
        if let Some(beta) = client_beta {
            headers.insert("anthropic-beta", HeaderValue::from_static(beta));
        }
        let finalized = adaptor
            .finalize_request(request(body.clone(), headers), &credential, &context)
            .await
            .unwrap();
        let expected = client_beta.map_or_else(
            || THRESHOLD_BETA.to_owned(),
            |beta| format!("{beta},{THRESHOLD_BETA}"),
        );
        assert_eq!(finalized.headers()["anthropic-beta"], expected.as_str());
        assert_eq!(finalized.body(), Some(&body));
        assert!(!expected.contains(ON_DEMAND_BETA));
        if client_beta.is_none() {
            assert!(!expected.contains("context-management-2025-06-27"));
        }
    }
}

#[tokio::test]
async fn unrelated_context_edits_and_replayed_blocks_do_not_guess_a_compaction_beta() {
    let adaptor = AnthropicAdaptor::new();
    let credential = Credential::api_key("api-key-canary").unwrap();
    let context = context();
    for body in [
        br#"{"context_management":{"edits":[{"type":"clear_thinking_20251015","keep":"all"}]}}"#.as_slice(),
        br#"{"messages":[{"role":"assistant","content":[{"type":"compaction","content":"summary"}]}]}"#.as_slice(),
        br#"{"model":"claude-test","messages":[]}"#.as_slice(),
    ] {
        for client_beta in [None, Some("remote-provider-beta-2099-01-01")] {
            let mut headers = HeaderMap::new();
            if let Some(beta) = client_beta {
                headers.append("anthropic-beta", HeaderValue::from_static(beta));
                headers.append("anthropic-beta", HeaderValue::from_static(beta));
            }
            let body = Bytes::copy_from_slice(body);
            let finalized = adaptor
                .finalize_request(request(body.clone(), headers), &credential, &context)
                .await
                .unwrap();
            assert_eq!(finalized.body(), Some(&body));
            assert_eq!(
                finalized.headers().get("anthropic-beta").map(|beta| beta.as_bytes()),
                client_beta.map(str::as_bytes)
            );
        }
    }
}

#[tokio::test]
async fn on_demand_compaction_cannot_be_combined_with_context_management() {
    let adaptor = AnthropicAdaptor::new();
    let credential = Credential::api_key("api-key-canary").unwrap();
    let context = context();
    for body in [
        br#"{"compaction":{"type":"summarize"},"context_management":{"edits":[{"type":"compact_20260112"}]}}"#.as_slice(),
        br#"{"compaction":{"type":"summarize"},"context_management":{"edits":[{"type":"clear_thinking_20251015"}]}}"#.as_slice(),
    ] {
        let error = adaptor
            .finalize_request(
                request(Bytes::copy_from_slice(body), HeaderMap::new()),
                &credential,
                &context,
            )
            .await
            .unwrap_err();
        assert_eq!(error, AdaptorError::InvalidTaskRequest);
        assert!(!error.to_string().contains("summarize"));
    }
}

#[tokio::test]
async fn beta_negotiation_rechecks_header_value_count_and_total_budgets() {
    let adaptor = AnthropicAdaptor::new();
    let credential = Credential::api_key("api-key-canary").unwrap();
    let context = context();
    let body = Bytes::from_static(br#"{"compaction":{"type":"summarize"}}"#);

    let mut value_overflow = HeaderMap::new();
    value_overflow.insert(
        "anthropic-beta",
        HeaderValue::from_bytes(&vec![b'x'; MAX_UPSTREAM_REQUEST_HEADER_VALUE_BYTES]).unwrap(),
    );

    let mut count_overflow = HeaderMap::new();
    for index in 0..MAX_UPSTREAM_REQUEST_HEADER_COUNT {
        count_overflow.insert(
            HeaderName::from_bytes(format!("x-test-{index}").as_bytes()).unwrap(),
            HeaderValue::from_static("x"),
        );
    }

    let mut total_overflow = HeaderMap::new();
    let mut remaining = MAX_UPSTREAM_REQUEST_HEADERS_BYTES;
    for index in 0..16 {
        let name = format!("x-padding-{index}");
        let size = MAX_UPSTREAM_REQUEST_HEADER_VALUE_BYTES.min(remaining - name.len());
        total_overflow.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_bytes(&vec![b'x'; size]).unwrap(),
        );
        remaining -= name.len() + size;
    }
    assert_eq!(remaining, 0);

    for headers in [value_overflow, count_overflow, total_overflow] {
        let error = adaptor
            .finalize_request(request(body.clone(), headers), &credential, &context)
            .await
            .unwrap_err();
        assert_eq!(error, AdaptorError::InvalidHeader);
    }
}
