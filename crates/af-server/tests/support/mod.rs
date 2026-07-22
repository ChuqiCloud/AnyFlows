use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream as StdTcpStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

pub const IO_TIMEOUT: Duration = Duration::from_secs(15);
pub const UPSTREAM_BASE_URL: &str = "http://upstream.example/proxy";
pub const UPSTREAM_MODEL: &str = "test-model";
pub const UPSTREAM_KEY: &str = "configured-upstream-key";
pub const CLIENT_KEY: &str = "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
pub const TOKEN_ID: i64 = 8_100_001;
pub const USER_ID: i64 = 8_100_002;
pub const GROUP_ID: i64 = 8_100_003;

#[derive(Debug)]
pub struct CapturedRequest {
    pub head: String,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub struct RawResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// 通过原始 TCP 请求测试服务，避免客户端自动改写协议边界。
pub async fn send_request(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: &[u8],
    headers: &[(&str, &str)],
) -> RawResponse {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut stream = loop {
        match TcpStream::connect(address).await {
            Ok(stream) => break stream,
            Err(error) => {
                assert!(Instant::now() < deadline, "等待测试服务监听超时: {error}");
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
    };
    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n");
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-length"))
    {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("Connection: close\r\n");
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();

    let mut bytes = Vec::new();
    timeout(IO_TIMEOUT, stream.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    parse_response(bytes)
}

fn parse_response(bytes: Vec<u8>) -> RawResponse {
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .unwrap();
    let head = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    let mut lines = head.lines();
    let status = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    RawResponse {
        status,
        headers,
        body: bytes[header_end..].to_vec(),
    }
}

/// 启动只处理一次请求的本地代理，并返回完整的上游请求快照。
pub fn spawn_proxy(
    status_line: &str,
    body: &[u8],
    headers: &[(&str, &str)],
) -> (
    SocketAddr,
    mpsc::Receiver<CapturedRequest>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let status_line = status_line.to_owned();
    let body = body.to_vec();
    let headers = headers
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect::<Vec<_>>();
    let (captured_tx, captured_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + IO_TIMEOUT;
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "本地测试代理等待连接超时");
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("本地测试代理接受连接失败: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        let captured = read_proxy_request(&mut stream);
        captured_tx.send(captured).unwrap();

        let mut response = format!(
            "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            response.push_str(&name);
            response.push_str(": ");
            response.push_str(&value);
            response.push_str("\r\n");
        }
        response.push_str("\r\n");
        stream.write_all(response.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        stream.flush().unwrap();
    });
    (address, captured_rx, handle)
}

/// 启动分两段发送的 SSE 上游；第二段只在测试显式放行后写出。
pub fn spawn_streaming_proxy(
    first: &[u8],
    rest: &[u8],
) -> (
    SocketAddr,
    mpsc::Receiver<CapturedRequest>,
    mpsc::Sender<()>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let first = first.to_vec();
    let rest = rest.to_vec();
    let (captured_tx, captured_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        captured_tx.send(read_proxy_request(&mut stream)).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        stream.write_all(&first).unwrap();
        stream.flush().unwrap();
        release_rx.recv_timeout(IO_TIMEOUT).unwrap();
        stream.write_all(&rest).unwrap();
        stream.flush().unwrap();
    });
    (address, captured_rx, release_tx, handle)
}

/// 启动在首段 SSE 后等待连接关闭的上游，用于验证客户端断连传播。
pub fn spawn_abort_observing_proxy(
    first: &[u8],
) -> (
    SocketAddr,
    mpsc::Receiver<CapturedRequest>,
    mpsc::Receiver<()>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let first = first.to_vec();
    let (captured_tx, captured_rx) = mpsc::channel();
    let (aborted_tx, aborted_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        captured_tx.send(read_proxy_request(&mut stream)).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n")
            .unwrap();
        stream.write_all(&first).unwrap();
        stream.flush().unwrap();

        let mut byte = [0_u8; 1];
        assert_eq!(stream.read(&mut byte).unwrap(), 0, "上游连接未随客户端断开");
        aborted_tx.send(()).unwrap();
    });
    (address, captured_rx, aborted_rx, handle)
}

fn read_proxy_request(stream: &mut StdTcpStream) -> CapturedRequest {
    let mut request = Vec::with_capacity(1_024);
    let mut buffer = [0_u8; 512];
    let header_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "代理请求头尚未结束时连接已关闭");
        request.extend_from_slice(&buffer[..read]);
        assert!(request.len() <= 64 * 1024, "代理测试请求头超过限制");
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let head = String::from_utf8(request[..header_end].to_vec()).unwrap();
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while request.len() < header_end + content_length {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "代理请求体尚未结束时连接已关闭");
        request.extend_from_slice(&buffer[..read]);
    }
    CapturedRequest {
        head,
        body: request[header_end..header_end + content_length].to_vec(),
    }
}
