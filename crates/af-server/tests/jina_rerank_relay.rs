use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::Duration,
};

use af_adapter::{Credential, JinaAdaptor, RelayContext};
use af_domain::{AfError, UpstreamError};
use af_httpclient::{HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy};
use af_protocol::{CanonicalRerankRequest, rerank_v1};
use af_relay::{
    RelayCandidate, RelayStateMachine, build_jina_rerank_candidate_request,
    relay_jina_rerank_with_report,
};
use serde_json::Value;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

fn request(return_documents: bool) -> CanonicalRerankRequest {
    rerank_v1::parse_request(
        format!(
            r#"{{"model":"public-rerank-model","query":"private-query-canary","documents":["private-document-a",{{"text":"private-document-b"}}],"top_n":2,"return_documents":{return_documents}}}"#
        )
        .as_bytes(),
    )
    .unwrap()
}

fn machine(address: SocketAddr, request: &CanonicalRerankRequest) -> RelayStateMachine {
    let client = HttpClientPool::default()
        .get(
            &HttpClientConfig::new(
                ProxyConfig::parse(format!("http://{address}")).unwrap(),
                HttpTimeouts::default(),
            )
            .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy),
        )
        .unwrap();
    let context = RelayContext::new(client)
        .with_base_url("http://upstream.example/proxy/jina")
        .unwrap()
        .with_request_id("request-jina-relay")
        .unwrap();
    let candidate = RelayCandidate::new(
        Arc::new(JinaAdaptor::with_supported_models([
            "private-upstream-model",
        ])),
        context,
        Credential::api_key("private-jina-key").unwrap(),
    )
    .with_request(
        build_jina_rerank_candidate_request(request, "private-upstream-model".to_owned()).unwrap(),
    );
    RelayStateMachine::new(vec![candidate]).unwrap()
}

fn spawn_proxy(
    status_line: &'static str,
    response_body: &'static [u8],
) -> (
    SocketAddr,
    std::sync::mpsc::Receiver<Vec<u8>>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
        sender.send(read_request(&mut stream)).unwrap();
        let response_head = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response_body.len()
        );
        stream.write_all(response_head.as_bytes()).unwrap();
        stream.write_all(response_body).unwrap();
        stream.flush().unwrap();
    });
    (address, receiver, server)
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::with_capacity(2_048);
    let mut buffer = [0_u8; 512];
    let header_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "客户端在请求头结束前关闭连接");
        request.extend_from_slice(&buffer[..read]);
        assert!(request.len() <= 64 * 1_024, "测试请求超过 64 KiB");
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
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
    request
}

#[tokio::test]
async fn relay_uses_native_endpoint_headers_mapped_body_and_public_model() {
    let request = request(true);
    let (address, captured, server) = spawn_proxy(
        "200 OK",
        br#"{"id":"private-response-id","model":"private-upstream-model","results":[{"index":1,"relevance_score":0.9,"document":{"text":"private-document-b"}},{"index":0,"relevance_score":0.7,"document":"private-document-a"}],"usage":{"total_tokens":7}}"#,
    );
    let outcome = relay_jina_rerank_with_report(&machine(address, &request), request).await;
    let (result, report) = outcome.into_parts();
    let (body, usage) = result.unwrap().into_parts();
    assert_eq!(report.successful_candidate_index(), Some(0));
    assert!(report.failures().is_empty());
    assert_eq!(
        usage.unwrap().token_usage().unwrap().input_tokens().get(),
        7
    );
    let public: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(public["model"], "public-rerank-model");

    let captured = captured.recv_timeout(TEST_TIMEOUT).unwrap();
    server.join().unwrap();
    let separator = captured
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap()
        + 4;
    let head = String::from_utf8_lossy(&captured[..separator]).to_ascii_lowercase();
    assert!(head.starts_with("post http://upstream.example/proxy/jina/v1/rerank http/1.1"));
    assert!(head.contains("authorization: bearer private-jina-key"));
    assert!(head.contains("x-request-id: request-jina-relay"));
    assert!(head.contains("content-type: application/json"));
    let upstream: Value = serde_json::from_slice(&captured[separator..]).unwrap();
    assert_eq!(upstream["model"], "private-upstream-model");
    assert!(!String::from_utf8_lossy(&captured[separator..]).contains("public-rerank-model"));
}

#[tokio::test]
async fn non_success_and_protocol_failures_are_recorded_without_body_leaks() {
    let rate_limited_request = request(false);
    let (address, _captured, server) = spawn_proxy(
        "429 Too Many Requests",
        br#"{"error":{"type":"rate_limit_error","message":"private-error-canary"}}"#,
    );
    let outcome = relay_jina_rerank_with_report(
        &machine(address, &rate_limited_request),
        rate_limited_request,
    )
    .await;
    let (result, report) = outcome.into_parts();
    server.join().unwrap();
    assert!(matches!(
        result.unwrap_err(),
        AfError::Upstream(UpstreamError::RateLimited { .. })
    ));
    assert_eq!(report.successful_candidate_index(), None);
    assert_eq!(report.failures().len(), 1);
    assert!(matches!(
        report.failures()[0].error(),
        UpstreamError::RateLimited { .. }
    ));
    assert!(!format!("{report:?}").contains("private-error-canary"));

    let invalid_response_request = request(true);
    let (address, _captured, server) = spawn_proxy(
        "200 OK",
        br#"{"results":[{"index":0,"relevance_score":0.9,"document":"wrong-document"},{"index":1,"relevance_score":0.7,"document":{"text":"private-document-b"}}]}"#,
    );
    let outcome = relay_jina_rerank_with_report(
        &machine(address, &invalid_response_request),
        invalid_response_request,
    )
    .await;
    let (result, report) = outcome.into_parts();
    server.join().unwrap();
    assert_eq!(
        result.unwrap_err(),
        AfError::Upstream(UpstreamError::ProtocolError)
    );
    assert_eq!(report.successful_candidate_index(), None);
    assert_eq!(report.failures().len(), 1);
    assert_eq!(report.failures()[0].error(), UpstreamError::ProtocolError);
    assert!(!format!("{report:?}").contains("wrong-document"));
}
