use af_domain::{CredentialKind, MAX_MODEL_NAME_BYTES};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue, HttpClientConfig, HttpClientPool, Method};

use super::*;
use crate::{AdaptorError, AdaptorTarget, Credential, RelayContext, UpstreamRequest};

fn context() -> RelayContext {
    RelayContext::new(
        HttpClientPool::default()
            .get(&HttpClientConfig::default())
            .unwrap(),
    )
}

fn endpoint(value: &str) -> CustomEndpointTemplate {
    CustomEndpointTemplate::parse(value).unwrap()
}

fn target(
    model: &'static str,
    operation: Operation,
    response_mode: ResponseMode,
) -> AdaptorTarget<'static> {
    AdaptorTarget::new(model, operation, response_mode)
}

#[test]
fn endpoint_template_renders_encoded_model_and_static_query() {
    let template = endpoint("/v1/models/{model}:invoke?alt=sse&api-version=2026-01-01");
    let target = template
        .render(
            &context()
                .with_base_url("https://gateway.example/proxy/custom/")
                .unwrap(),
            "model/with?query#fragment",
        )
        .unwrap();
    assert_eq!(
        target,
        "https://gateway.example/proxy/custom/v1/models/model%2Fwith%3Fquery%23fragment:invoke?alt=sse&api-version=2026-01-01"
    );

    let query_model = endpoint("/v1/invoke?model={model}")
        .render(
            &context()
                .with_base_url("https://gateway.example/root")
                .unwrap(),
            "model/name + plus",
        )
        .unwrap();
    assert_eq!(
        query_model,
        "https://gateway.example/root/v1/invoke?model=model%2Fname+%2B+plus"
    );

    let debug = format!("{template:?}");
    assert!(debug.contains("path_segment_count: 3"));
    assert!(debug.contains("query_pair_count: 2"));
    assert!(!debug.contains("invoke"));
    assert!(!debug.contains("api-version"));
}

#[test]
fn endpoint_template_rejects_ambiguous_or_unbounded_inputs() {
    for invalid in [
        "",
        "v1/chat/completions",
        "//upstream.example/path",
        "https://upstream.example/path",
        "/v1/",
        "/v1//chat",
        "/v1/../chat",
        "/v1/%2e%2e/chat",
        "/v1/chat#fragment",
        "/v1\\chat",
        "/v1/{unknown}",
        "/v1/{model}/{model}",
        "/v1/chat?",
        "/v1/chat?flag",
        "/v1/chat?a=1&a=2",
        "/v1/chat?a=1?b=2",
    ] {
        assert_eq!(
            CustomEndpointTemplate::parse(invalid),
            Err(AdaptorError::InvalidCustomEndpoint),
            "未拒绝端点模板：{invalid}"
        );
    }

    let too_many_segments = format!("/{}", vec!["segment"; 33].join("/"));
    assert_eq!(
        CustomEndpointTemplate::parse(too_many_segments),
        Err(AdaptorError::InvalidCustomEndpoint)
    );
    let too_many_query_pairs = format!(
        "/v1/chat?{}",
        (0..=MAX_CUSTOM_ENDPOINT_QUERY_PAIRS)
            .map(|index| format!("key{index}=value"))
            .collect::<Vec<_>>()
            .join("&")
    );
    assert_eq!(
        CustomEndpointTemplate::parse(too_many_query_pairs),
        Err(AdaptorError::InvalidCustomEndpoint)
    );
}

#[test]
fn endpoint_render_requires_explicit_base_and_valid_model() {
    let template = endpoint("/v1/models/{model}:invoke");
    assert_eq!(
        template.render(&context(), "model"),
        Err(AdaptorError::InvalidBaseUrl)
    );
    for model in ["", " model", "model\nsecret"] {
        assert_eq!(
            template.render(
                &context().with_base_url("https://upstream.example").unwrap(),
                model,
            ),
            Err(AdaptorError::InvalidRequestTarget)
        );
    }
    assert_eq!(
        template.render(
            &context().with_base_url("https://upstream.example").unwrap(),
            &"m".repeat(MAX_MODEL_NAME_BYTES + 1),
        ),
        Err(AdaptorError::InvalidRequestTarget)
    );
}

#[test]
fn custom_header_authentication_validates_name_and_single_placeholder() {
    let authentication =
        CustomHeaderAuthentication::new("x-provider-key", "Token {credential}:suffix").unwrap();
    let debug = format!("{authentication:?}");
    assert!(!debug.contains("x-provider-key"));
    assert!(!debug.contains("Token"));

    for result in [
        CustomHeaderAuthentication::new("", "{credential}"),
        CustomHeaderAuthentication::new(" host", "{credential}"),
        CustomHeaderAuthentication::new("host", "{credential}"),
        CustomHeaderAuthentication::new("content-type", "{credential}"),
        CustomHeaderAuthentication::new("accept", "{credential}"),
        CustomHeaderAuthentication::new("x-request-id", "{credential}"),
        CustomHeaderAuthentication::new("x-provider-key", "credential"),
        CustomHeaderAuthentication::new("x-provider-key", "{credential}:{credential}"),
        CustomHeaderAuthentication::new("x-provider-key", "{{credential}}"),
        CustomHeaderAuthentication::new("x-provider-key", " {credential}"),
        CustomHeaderAuthentication::new("x-provider-key", "{credential}\nsecret"),
    ] {
        assert_eq!(result, Err(AdaptorError::InvalidCustomAuthentication));
    }
}

#[test]
fn custom_authentication_replaces_polluted_headers_and_redacts_secret() {
    let credential = Credential::api_key("custom-secret").unwrap();
    let authentication = CustomAuthentication::Header(
        CustomHeaderAuthentication::new("x-provider-key", "Token {credential}").unwrap(),
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("authorization"),
        HeaderValue::from_static("Bearer stale"),
    );
    headers.insert(
        HeaderName::from_static("x-provider-key"),
        HeaderValue::from_static("override-secret"),
    );
    authentication.apply(&mut headers, &credential).unwrap();
    assert!(headers.get("authorization").is_none());
    assert_eq!(headers["x-provider-key"], "Token custom-secret");
    let debug = format!("{authentication:?}\n{headers:?}\n{credential:?}");
    assert!(!debug.contains("custom-secret"));
    assert!(!debug.contains("override-secret"));

    let complex = Credential::bedrock("access-key", "secret-key", None).unwrap();
    assert_eq!(
        CustomAuthentication::Bearer.apply(&mut HeaderMap::new(), &complex),
        Err(AdaptorError::UnsupportedCredential {
            kind: CredentialKind::Bedrock
        })
    );
}

#[test]
fn custom_adaptor_selects_protocol_operation_and_stream_endpoint() {
    let adaptor = CustomAdaptor::with_supported_models(
        Protocol::Gemini,
        endpoint("/v1/models/{model}:generateContent"),
        CustomStreamEndpoint::Separate(endpoint(
            "/v1/models/{model}:streamGenerateContent?alt=sse",
        )),
        CustomAuthentication::Bearer,
        ["gemini-custom"],
    );
    let context = context()
        .with_base_url("https://upstream.example/proxy")
        .unwrap();
    assert_eq!(adaptor.channel_type(), ChannelType::Custom);
    assert_eq!(adaptor.default_protocol(), Protocol::Gemini);
    assert_eq!(adaptor.default_base_url(), "");
    assert_eq!(adaptor.supported_models(), ["gemini-custom"]);
    assert_eq!(
        adaptor
            .build_url(
                &context,
                target("gemini-custom", Operation::Chat, ResponseMode::Full),
            )
            .unwrap(),
        "https://upstream.example/proxy/v1/models/gemini-custom:generateContent"
    );
    assert_eq!(
        adaptor
            .build_url(
                &context,
                target("gemini-custom", Operation::Chat, ResponseMode::Stream),
            )
            .unwrap(),
        "https://upstream.example/proxy/v1/models/gemini-custom:streamGenerateContent?alt=sse"
    );
    assert_eq!(
        adaptor
            .build_url(
                &context,
                target("gemini-custom", Operation::Responses, ResponseMode::Full,),
            )
            .unwrap_err(),
        AdaptorError::UnsupportedOperation {
            operation: Operation::Responses
        }
    );

    let responses = CustomAdaptor::new(
        Protocol::OpenAiResponses,
        endpoint("/v1/responses"),
        CustomStreamEndpoint::Same,
        CustomAuthentication::None,
    );
    assert!(
        responses
            .build_url(
                &context,
                target("gpt-custom", Operation::Responses, ResponseMode::Stream),
            )
            .is_ok()
    );
    let non_streaming = CustomAdaptor::new(
        Protocol::Anthropic,
        endpoint("/v1/messages"),
        CustomStreamEndpoint::Unsupported,
        CustomAuthentication::Bearer,
    );
    assert_eq!(
        non_streaming
            .build_url(
                &context,
                target("claude-custom", Operation::Chat, ResponseMode::Stream),
            )
            .unwrap_err(),
        AdaptorError::UnsupportedResponseMode
    );
}

#[tokio::test]
async fn custom_finalize_restores_auth_json_and_request_id_after_overrides() {
    let adaptor = CustomAdaptor::new(
        Protocol::OpenAiChat,
        endpoint("/v2/chat"),
        CustomStreamEndpoint::Same,
        CustomAuthentication::Header(
            CustomHeaderAuthentication::new("x-provider-key", "Key {credential}").unwrap(),
        ),
    );
    let context = context()
        .with_base_url("https://upstream.example")
        .unwrap()
        .with_request_id("request-custom-1")
        .unwrap();
    let credential = Credential::oauth("custom-oauth").unwrap();
    let mut headers = HeaderMap::new();
    adaptor
        .setup_headers(&mut headers, &credential, &context)
        .unwrap();
    headers.insert(
        HeaderName::from_static("authorization"),
        HeaderValue::from_static("Bearer malicious"),
    );
    headers.insert(
        HeaderName::from_static("x-provider-key"),
        HeaderValue::from_static("malicious-key"),
    );
    headers.insert(
        HeaderName::from_static("content-type"),
        HeaderValue::from_static("text/plain"),
    );
    headers.insert(
        HeaderName::from_static("x-request-id"),
        HeaderValue::from_static("malicious-request"),
    );
    let request = UpstreamRequest::new(
        Method::POST,
        "https://upstream.example/v2/chat",
        headers,
        None,
    )
    .unwrap()
    .with_response_mode(ResponseMode::Stream);
    let request = adaptor
        .finalize_request(request, &credential, &context)
        .await
        .unwrap();
    assert!(request.headers().get("authorization").is_none());
    assert_eq!(request.headers()["x-provider-key"], "Key custom-oauth");
    assert_eq!(request.headers()["content-type"], "application/json");
    assert_eq!(request.headers()["accept"], "application/json");
    assert_eq!(request.headers()["x-request-id"], "request-custom-1");
    assert_eq!(request.response_mode(), ResponseMode::Stream);

    let get = UpstreamRequest::new(
        Method::GET,
        "https://upstream.example/v2/chat",
        HeaderMap::new(),
        None,
    )
    .unwrap();
    assert_eq!(
        adaptor
            .finalize_request(get, &credential, &context)
            .await
            .unwrap_err(),
        AdaptorError::UnsupportedRequestMethod
    );
}

#[test]
fn custom_debug_redacts_templates_auth_and_models() {
    let adaptor = CustomAdaptor::with_supported_models(
        Protocol::OpenAiChat,
        endpoint("/private-route/{model}"),
        CustomStreamEndpoint::Same,
        CustomAuthentication::Header(
            CustomHeaderAuthentication::new("x-private-key", "Private {credential}").unwrap(),
        ),
        ["private-model"],
    );
    let debug = format!("{adaptor:?}");
    assert!(debug.contains("protocol: OpenAiChat"));
    assert!(debug.contains("supported_model_count: 1"));
    for private in ["private-route", "x-private-key", "Private", "private-model"] {
        assert!(!debug.contains(private));
    }
}
