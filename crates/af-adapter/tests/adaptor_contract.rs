use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    thread,
    time::Duration,
};

use af_adapter::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorSendExt, AdaptorTarget, AdaptorTransportError,
    AnthropicAdaptor, BedrockAdaptor, Bytes, ChannelType, Credential, CredentialKind,
    CustomAdaptor, CustomAuthentication, CustomEndpointTemplate, CustomHeaderAuthentication,
    CustomStreamEndpoint, GeminiAdaptor, HeaderMap, HeaderName, HeaderValue,
    MAX_UPSTREAM_RESPONSE_BODY_BYTES, Method, OpenAiAdaptor, Operation, Protocol, RelayContext,
    ResponseMode, StatusCode, UpstreamRequest,
};
use af_httpclient::{HttpClientConfig, HttpClientPool, ProxyConfig, RemoteDnsPolicy};

const SERVER_TIMEOUT: Duration = Duration::from_secs(5);

struct TestAdaptor;

impl Adaptor for TestAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::OpenAi
    }

    fn default_protocol(&self) -> Protocol {
        Protocol::OpenAiChat
    }

    fn default_base_url(&self) -> &str {
        "https://default.invalid"
    }

    fn supported_models(&self) -> Vec<String> {
        vec!["test-model".to_owned()]
    }

    fn build_url(
        &self,
        context: &RelayContext,
        target: AdaptorTarget<'_>,
    ) -> AdaptorResult<String> {
        if target.operation() != Operation::Chat {
            return Err(AdaptorError::UnsupportedOperation {
                operation: target.operation(),
            });
        }
        context.append_path(self.default_base_url(), "v1/chat/completions")
    }

    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        _context: &RelayContext,
    ) -> AdaptorResult<()> {
        if credential.kind() != CredentialKind::ApiKey {
            return Err(AdaptorError::UnsupportedCredential {
                kind: credential.kind(),
            });
        }
        let value = HeaderValue::from_str(&format!("Bearer {}", credential.expose_secret()))
            .map_err(|_| AdaptorError::InvalidHeader)?;
        headers.insert(HeaderName::from_static("authorization"), value);
        Ok(())
    }
}

#[tokio::test]
async fn object_safe_adaptor_uses_managed_default_transport() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server =
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with(
                "POST http://upstream.example/proxy/openai/v1/chat/completions HTTP/1.1"
            ));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer loopback-secret")
            );
            assert!(request.ends_with(r#"{"model":"test-model"}"#));
            stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nresponse-ok",
            )
            .unwrap();
            stream.flush().unwrap();
        });

    let client = HttpClientPool::default()
        .get(&loopback_proxy_config(address))
        .unwrap();
    let context = RelayContext::new(client)
        .with_base_url("http://upstream.example/proxy/openai")
        .unwrap()
        .with_request_id("request-loopback")
        .unwrap();
    let adaptor: Box<dyn Adaptor> = Box::new(TestAdaptor);
    assert_eq!(adaptor.channel_type(), ChannelType::OpenAi);
    assert_eq!(adaptor.default_protocol(), Protocol::OpenAiChat);
    assert_eq!(adaptor.supported_models(), ["test-model"]);

    let mut headers = HeaderMap::new();
    adaptor
        .setup_headers(
            &mut headers,
            &Credential::api_key("loopback-secret").unwrap(),
            &context,
        )
        .unwrap();
    let target = adaptor
        .build_url(
            &context,
            AdaptorTarget::new("test-model", Operation::Chat, ResponseMode::Stream),
        )
        .unwrap();
    let request = UpstreamRequest::new(
        Method::POST,
        target,
        headers,
        Some(Bytes::from_static(br#"{"model":"test-model"}"#)),
    )
    .unwrap()
    .with_response_mode(ResponseMode::Stream);
    let response = adaptor.send(request, &context).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body();
    assert!(body.is_streaming());
    assert_eq!(
        body.into_bytes().await.unwrap(),
        Bytes::from_static(b"response-ok")
    );

    server.join().unwrap();
}

#[tokio::test]
async fn openai_adaptor_builds_and_sends_chat_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        let lowercase = request.to_ascii_lowercase();
        assert!(
            request.starts_with(
                "POST http://upstream.example/proxy/openai/v1/chat/completions HTTP/1.1"
            )
        );
        assert!(lowercase.contains("authorization: bearer openai-loopback-secret"));
        assert!(lowercase.contains("content-type: application/json"));
        assert!(lowercase.contains("accept: application/json"));
        assert!(lowercase.contains("x-request-id: request-openai-loopback"));
        assert!(request.ends_with(r#"{"model":"gpt-test","messages":[]}"#));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 14\r\nConnection: close\r\n\r\n{\"choices\":[]}",
            )
            .unwrap();
        stream.flush().unwrap();
    });

    let client = HttpClientPool::default()
        .get(&loopback_proxy_config(address))
        .unwrap();
    let context = RelayContext::new(client)
        .with_base_url("http://upstream.example/proxy/openai")
        .unwrap()
        .with_request_id("request-openai-loopback")
        .unwrap();
    let adaptor = OpenAiAdaptor::with_supported_models(["gpt-test"]);
    assert_eq!(adaptor.supported_models(), ["gpt-test"]);
    let mut headers = HeaderMap::new();
    adaptor
        .setup_headers(
            &mut headers,
            &Credential::api_key("openai-loopback-secret").unwrap(),
            &context,
        )
        .unwrap();
    let request = UpstreamRequest::new(
        Method::POST,
        adaptor
            .build_url(
                &context,
                AdaptorTarget::new("gpt-test", Operation::Chat, ResponseMode::Full),
            )
            .unwrap(),
        headers,
        Some(Bytes::from_static(br#"{"model":"gpt-test","messages":[]}"#)),
    )
    .unwrap();
    let response = adaptor.send(request, &context).await.unwrap();
    let body = response.into_body();
    assert!(!body.is_streaming());
    assert_eq!(
        body.into_bytes().await.unwrap(),
        Bytes::from_static(br#"{"choices":[]}"#)
    );

    server.join().unwrap();
}

#[tokio::test]
async fn anthropic_adaptor_builds_and_sends_messages_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        let lowercase = request.to_ascii_lowercase();
        assert!(
            request
                .starts_with("POST http://upstream.example/proxy/anthropic/v1/messages HTTP/1.1")
        );
        assert!(lowercase.contains("x-api-key: anthropic-loopback-secret"));
        assert!(!lowercase.contains("\r\nauthorization:"));
        assert!(lowercase.contains("anthropic-version: 2023-06-01"));
        assert!(lowercase.contains("content-type: application/json"));
        assert!(lowercase.contains("accept: application/json"));
        assert!(lowercase.contains("x-request-id: request-anthropic-loopback"));
        assert!(request.ends_with(r#"{"model":"claude-test","max_tokens":16,"messages":[]}"#));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 18\r\nConnection: close\r\n\r\n{\"type\":\"message\"}",
            )
            .unwrap();
        stream.flush().unwrap();
    });

    let client = HttpClientPool::default()
        .get(&loopback_proxy_config(address))
        .unwrap();
    let context = RelayContext::new(client)
        .with_base_url("http://upstream.example/proxy/anthropic")
        .unwrap()
        .with_request_id("request-anthropic-loopback")
        .unwrap();
    let adaptor = AnthropicAdaptor::with_supported_models(["claude-test"]);
    assert_eq!(adaptor.supported_models(), ["claude-test"]);
    let mut headers = HeaderMap::new();
    adaptor
        .setup_headers(
            &mut headers,
            &Credential::api_key("anthropic-loopback-secret").unwrap(),
            &context,
        )
        .unwrap();
    let request = UpstreamRequest::new(
        Method::POST,
        adaptor
            .build_url(
                &context,
                AdaptorTarget::new("claude-test", Operation::Chat, ResponseMode::Full),
            )
            .unwrap(),
        headers,
        Some(Bytes::from_static(
            br#"{"model":"claude-test","max_tokens":16,"messages":[]}"#,
        )),
    )
    .unwrap();
    let response = adaptor.send(request, &context).await.unwrap();
    let body = response.into_body();
    assert!(!body.is_streaming());
    assert_eq!(
        body.into_bytes().await.unwrap(),
        Bytes::from_static(br#"{"type":"message"}"#)
    );

    server.join().unwrap();
}

#[tokio::test]
async fn gemini_adaptor_builds_and_sends_stream_generate_content_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        let lowercase = request.to_ascii_lowercase();
        assert!(request.starts_with(
            "POST http://upstream.example/proxy/gemini/v1beta/models/gemini-test:streamGenerateContent?alt=sse HTTP/1.1"
        ));
        assert!(lowercase.contains("x-goog-api-key: gemini-loopback-secret"));
        assert!(!lowercase.contains("\r\nauthorization:"));
        assert!(lowercase.contains("content-type: application/json"));
        assert!(lowercase.contains("accept: application/json"));
        assert!(lowercase.contains("x-request-id: request-gemini-loopback"));
        assert!(request.ends_with(r#"{"contents":[]}"#));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"candidates\":[]}",
            )
            .unwrap();
        stream.flush().unwrap();
    });

    let client = HttpClientPool::default()
        .get(&loopback_proxy_config(address))
        .unwrap();
    let context = RelayContext::new(client)
        .with_base_url("http://upstream.example/proxy/gemini")
        .unwrap()
        .with_request_id("request-gemini-loopback")
        .unwrap();
    let adaptor = GeminiAdaptor::with_supported_models(["gemini-test"]);
    assert_eq!(adaptor.supported_models(), ["gemini-test"]);
    let mut headers = HeaderMap::new();
    adaptor
        .setup_headers(
            &mut headers,
            &Credential::api_key("gemini-loopback-secret").unwrap(),
            &context,
        )
        .unwrap();
    let request = UpstreamRequest::new(
        Method::POST,
        adaptor
            .build_url(
                &context,
                AdaptorTarget::new("gemini-test", Operation::Chat, ResponseMode::Stream),
            )
            .unwrap(),
        headers,
        Some(Bytes::from_static(br#"{"contents":[]}"#)),
    )
    .unwrap()
    .with_response_mode(ResponseMode::Stream);
    let response = adaptor.send(request, &context).await.unwrap();
    let body = response.into_body();
    assert!(body.is_streaming());
    assert_eq!(
        body.into_bytes().await.unwrap(),
        Bytes::from_static(br#"{"candidates":[]}"#)
    );

    server.join().unwrap();
}

#[tokio::test]
async fn custom_adaptor_rebuilds_header_auth_and_uses_managed_transport() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        let lowercase = request.to_ascii_lowercase();
        assert!(request.starts_with(
            "POST http://upstream.example/proxy/custom/v2/models/custom-model%2Fversion:invoke?mode=chat HTTP/1.1"
        ));
        assert!(lowercase.contains("x-provider-token: token custom-loopback-secret"));
        assert!(!lowercase.contains("malicious-override"));
        assert!(!lowercase.contains("\r\nauthorization:"));
        assert!(lowercase.contains("content-type: application/json"));
        assert!(lowercase.contains("accept: application/json"));
        assert!(lowercase.contains("x-request-id: request-custom-loopback"));
        assert!(request.ends_with(r#"{"model":"custom-model/version"}"#));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 14\r\nConnection: close\r\n\r\n{\"choices\":[]}",
            )
            .unwrap();
        stream.flush().unwrap();
    });

    let client = HttpClientPool::default()
        .get(&loopback_proxy_config(address))
        .unwrap();
    let context = RelayContext::new(client)
        .with_base_url("http://upstream.example/proxy/custom")
        .unwrap()
        .with_request_id("request-custom-loopback")
        .unwrap();
    let adaptor = CustomAdaptor::with_supported_models(
        Protocol::OpenAiChat,
        CustomEndpointTemplate::parse("/v2/models/{model}:invoke?mode=chat").unwrap(),
        CustomStreamEndpoint::Same,
        CustomAuthentication::Header(
            CustomHeaderAuthentication::new("x-provider-token", "Token {credential}").unwrap(),
        ),
        ["custom-model/version"],
    );
    let credential = Credential::api_key("custom-loopback-secret").unwrap();
    let mut headers = HeaderMap::new();
    adaptor
        .setup_headers(&mut headers, &credential, &context)
        .unwrap();
    headers.insert(
        HeaderName::from_static("x-provider-token"),
        HeaderValue::from_static("malicious-override"),
    );
    let request = UpstreamRequest::new(
        Method::POST,
        adaptor
            .build_url(
                &context,
                AdaptorTarget::new("custom-model/version", Operation::Chat, ResponseMode::Full),
            )
            .unwrap(),
        headers,
        Some(Bytes::from_static(br#"{"model":"custom-model/version"}"#)),
    )
    .unwrap();
    let request = adaptor
        .finalize_request(request, &credential, &context)
        .await
        .unwrap();
    let response = adaptor.send(request, &context).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.into_body().into_bytes().await.unwrap(),
        Bytes::from_static(br#"{"choices":[]}"#)
    );

    server.join().unwrap();
}

#[tokio::test]
async fn bedrock_adaptor_signs_and_sends_invoke_model_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        let lowercase = request.to_ascii_lowercase();
        assert!(request.starts_with(
            "POST http://upstream.example/proxy/bedrock/model/us.anthropic.claude-test-v1%3A0/invoke HTTP/1.1"
        ));
        assert!(
            lowercase.contains("authorization: aws4-hmac-sha256 credential=akidexample00000001/")
        );
        assert!(lowercase.contains("/us-east-1/bedrock/aws4_request"));
        assert!(lowercase.contains("x-amz-date:"));
        assert!(lowercase.contains("x-amz-security-token: bedrock-loopback-session"));
        assert!(lowercase.contains("content-type: application/json"));
        assert!(lowercase.contains("accept: application/json"));
        assert!(lowercase.contains("x-request-id: request-bedrock-loopback"));
        assert!(request.ends_with(r#"{"messages":[]}"#));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 18\r\nConnection: close\r\n\r\n{\"type\":\"message\"}",
            )
            .unwrap();
        stream.flush().unwrap();
    });

    let client = HttpClientPool::default()
        .get(&loopback_proxy_config(address))
        .unwrap();
    let context = RelayContext::new(client)
        .with_base_url("http://upstream.example/proxy/bedrock")
        .unwrap()
        .with_request_id("request-bedrock-loopback")
        .unwrap();
    let adaptor =
        BedrockAdaptor::with_supported_models("us-east-1", ["us.anthropic.claude-test-v1:0"])
            .unwrap();
    let credential = Credential::bedrock(
        "AKIDEXAMPLE00000001",
        "bedrock-loopback-secret",
        Some("bedrock-loopback-session".to_owned()),
    )
    .unwrap();
    let mut headers = HeaderMap::new();
    adaptor
        .setup_headers(&mut headers, &credential, &context)
        .unwrap();
    let request = UpstreamRequest::new(
        Method::POST,
        adaptor
            .build_url(
                &context,
                AdaptorTarget::new(
                    "us.anthropic.claude-test-v1:0",
                    Operation::Chat,
                    ResponseMode::Full,
                ),
            )
            .unwrap(),
        headers,
        Some(Bytes::from_static(br#"{"messages":[]}"#)),
    )
    .unwrap();
    let request = adaptor
        .finalize_request(request, &credential, &context)
        .await
        .unwrap();
    let response = adaptor.send(request, &context).await.unwrap();
    let body = response.into_body();
    assert!(!body.is_streaming());
    assert_eq!(
        body.into_bytes().await.unwrap(),
        Bytes::from_static(br#"{"type":"message"}"#)
    );

    server.join().unwrap();
}

#[tokio::test]
async fn default_transport_collects_non_streaming_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        assert!(request.starts_with("GET http://upstream.example/full HTTP/1.1"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nfull-ok")
            .unwrap();
        stream.flush().unwrap();
    });

    let client = HttpClientPool::default()
        .get(&loopback_proxy_config(address))
        .unwrap();
    let context = RelayContext::new(client);
    let request = UpstreamRequest::new(
        Method::GET,
        "http://upstream.example/full",
        HeaderMap::new(),
        None,
    )
    .unwrap();
    let response = TestAdaptor.send(request, &context).await.unwrap();
    let body = response.into_body();
    assert!(!body.is_streaming());
    assert_eq!(
        body.into_bytes().await.unwrap(),
        Bytes::from_static(b"full-ok")
    );

    server.join().unwrap();
}

#[tokio::test]
async fn default_transport_rejects_declared_oversized_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        assert!(request.starts_with("GET http://upstream.example/oversized HTTP/1.1"));
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_UPSTREAM_RESPONSE_BODY_BYTES + 1
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    let client = HttpClientPool::default()
        .get(&loopback_proxy_config(address))
        .unwrap();
    let context = RelayContext::new(client);
    let request = UpstreamRequest::new(
        Method::GET,
        "http://upstream.example/oversized",
        HeaderMap::new(),
        None,
    )
    .unwrap();
    assert_eq!(
        TestAdaptor.send(request, &context).await.unwrap_err(),
        AdaptorError::ResponseBodyTooLarge
    );

    server.join().unwrap();
}

#[test]
fn adaptor_rejects_wrong_credential_kind_without_exposing_secret() {
    let client = HttpClientPool::default()
        .get(&HttpClientConfig::default())
        .unwrap();
    let context = RelayContext::new(client);
    let credential = Credential::oauth("oauth-secret").unwrap();
    let error = TestAdaptor
        .setup_headers(&mut HeaderMap::new(), &credential, &context)
        .unwrap_err();

    assert_eq!(
        error,
        AdaptorError::UnsupportedCredential {
            kind: CredentialKind::Oauth,
        }
    );
    let rendered = format!("{error:?}\n{error}\n{credential:?}");
    assert!(!rendered.contains("oauth-secret"));
}

#[tokio::test]
async fn managed_transport_rejects_private_upstream_by_default() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let context = RelayContext::new(
        HttpClientPool::default()
            .get(&HttpClientConfig::default())
            .unwrap(),
    );
    let target = format!("http://{address}/blocked?token=adapter-secret");
    let request = UpstreamRequest::new(Method::GET, &target, HeaderMap::new(), None).unwrap();

    let error = TestAdaptor.send(request, &context).await.unwrap_err();
    assert_eq!(
        error,
        AdaptorError::Transport(AdaptorTransportError::TargetAddressBlocked)
    );
    let rendered = format!("{error:?}\n{error}");
    assert!(!rendered.contains(&target));
    assert!(!rendered.contains("adapter-secret"));
    assert!(matches!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    ));
}

fn loopback_proxy_config(address: SocketAddr) -> HttpClientConfig {
    HttpClientConfig::new(
        ProxyConfig::parse(format!("http://{address}")).unwrap(),
        af_httpclient::HttpTimeouts::default(),
    )
    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy)
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::with_capacity(1_024);
    let mut buffer = [0_u8; 512];
    let header_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "客户端在请求头结束前关闭连接");
        request.extend_from_slice(&buffer[..read]);
        assert!(request.len() <= 16 * 1_024, "测试请求头超过 16 KiB");
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while request.len() - header_end < content_length {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "客户端在请求体结束前关闭连接");
        request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).unwrap()
}
