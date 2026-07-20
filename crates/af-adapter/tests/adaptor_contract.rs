use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

use af_adapter::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorSendExt, Bytes, ChannelType, Credential,
    CredentialKind, HeaderMap, HeaderName, HeaderValue, MAX_UPSTREAM_RESPONSE_BODY_BYTES, Method,
    OpenAiAdaptor, Operation, Protocol, RelayContext, ResponseMode, StatusCode, UpstreamRequest,
};
use af_httpclient::{HttpClientConfig, HttpClientPool};

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

    fn build_url(&self, context: &RelayContext, operation: Operation) -> AdaptorResult<String> {
        if operation != Operation::Chat {
            return Err(AdaptorError::UnsupportedOperation { operation });
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
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        assert!(request.starts_with("POST /proxy/openai/v1/chat/completions HTTP/1.1"));
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
        .get(&HttpClientConfig::default())
        .unwrap();
    let context = RelayContext::new(client)
        .with_base_url(format!("http://{address}/proxy/openai"))
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
    let target = adaptor.build_url(&context, Operation::Chat).unwrap();
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
        assert!(request.starts_with("POST /proxy/openai/v1/chat/completions HTTP/1.1"));
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
        .get(&HttpClientConfig::default())
        .unwrap();
    let context = RelayContext::new(client)
        .with_base_url(format!("http://{address}/proxy/openai"))
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
        adaptor.build_url(&context, Operation::Chat).unwrap(),
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
async fn default_transport_collects_non_streaming_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        assert!(request.starts_with("GET /full HTTP/1.1"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nfull-ok")
            .unwrap();
        stream.flush().unwrap();
    });

    let client = HttpClientPool::default()
        .get(&HttpClientConfig::default())
        .unwrap();
    let context = RelayContext::new(client);
    let request = UpstreamRequest::new(
        Method::GET,
        format!("http://{address}/full"),
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
        assert!(request.starts_with("GET /oversized HTTP/1.1"));
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_UPSTREAM_RESPONSE_BODY_BYTES + 1
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    let client = HttpClientPool::default()
        .get(&HttpClientConfig::default())
        .unwrap();
    let context = RelayContext::new(client);
    let request = UpstreamRequest::new(
        Method::GET,
        format!("http://{address}/oversized"),
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
