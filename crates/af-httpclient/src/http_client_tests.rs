use std::{
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::mpsc::{self, Sender},
    thread,
    time::{Duration, Instant},
};

use crate::{
    HeaderMap, HttpClientConfig, HttpClientPool, HttpResponse, HttpTimeouts, HttpTransportError,
    Method, PooledClient, ProxyConfig, RemoteDnsPolicy, StatusCode, TargetAddressPolicy,
    pool::DEFAULT_USER_AGENT,
};

const SERVER_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn cached_client_reuses_the_same_http_connection() {
    let listener = loopback_listener();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener, SERVER_TIMEOUT);
        for _ in 0..2 {
            let request = read_request_head(&mut stream);
            assert!(request.starts_with("GET /reuse HTTP/1.1"));
            assert!(request.to_ascii_lowercase().contains(&format!(
                "\r\nuser-agent: {}\r\n",
                DEFAULT_USER_AGENT.to_ascii_lowercase()
            )));
            write_response(
                &mut stream,
                "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n",
            );
        }
    });

    let pool = HttpClientPool::new(4).unwrap();
    let config = direct_config(Duration::from_secs(1), Duration::from_secs(2));
    let url = format!("http://{address}/reuse");
    for _ in 0..2 {
        let response = send_get(&pool.get(&config).unwrap(), &url).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
    server.join().unwrap();
    assert_eq!(pool.len().unwrap(), 1);
}

#[tokio::test]
async fn local_dns_exception_keeps_the_original_http_host() {
    let listener = loopback_listener();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener, SERVER_TIMEOUT);
        let request = read_request_head(&mut stream);
        assert!(request.starts_with("GET /resolved HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains(&format!("\r\nhost: localhost:{}\r\n", address.port()))
        );
        write_response(
            &mut stream,
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
    });

    let config = direct_config(Duration::from_secs(1), Duration::from_secs(2));
    let target = format!("http://localhost:{}/resolved", address.port());
    let response = send_get(&HttpClientPool::default().get(&config).unwrap(), &target)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    server.join().unwrap();
}

#[tokio::test]
async fn required_proxy_routes_without_touching_the_origin() {
    let origin = loopback_listener();
    let origin_address = origin.local_addr().unwrap();
    let origin_monitor = monitor_listener(origin, Duration::from_millis(400));

    let proxy = loopback_listener();
    let proxy_address = proxy.local_addr().unwrap();
    let proxy_server = thread::spawn(move || {
        let mut stream = accept_with_timeout(&proxy, SERVER_TIMEOUT);
        let request = read_request_head(&mut stream);
        write_response(
            &mut stream,
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        request
    });

    let proxy = ProxyConfig::parse(format!("http://user:secret@{proxy_address}")).unwrap();
    let config = loopback_config(
        proxy,
        test_timeouts(Duration::from_secs(1), Duration::from_secs(2)),
    );
    let client = HttpClientPool::default().get(&config).unwrap();
    let client_debug = format!("{client:?}");
    assert!(!client_debug.contains("user"));
    assert!(!client_debug.contains("secret"));
    assert!(!client_debug.contains(&proxy_address.to_string()));

    let target = format!("http://{origin_address}/proxied");
    let response = send_get(&client, &target).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let request = proxy_server.join().unwrap();
    assert!(request.starts_with(&format!("GET http://{origin_address}/proxied HTTP/1.1")));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("proxy-authorization: basic dxnlcjpzzwnyzxq=")
    );
    assert!(!origin_monitor.join().unwrap());
}

#[tokio::test]
async fn required_proxy_failure_never_falls_back_to_direct() {
    let origin = loopback_listener();
    let origin_address = origin.local_addr().unwrap();
    let origin_monitor = monitor_listener(origin, Duration::from_millis(400));

    let proxy = loopback_listener();
    let proxy_address = proxy.local_addr().unwrap();
    let proxy_server = thread::spawn(move || {
        let stream = accept_with_timeout(&proxy, SERVER_TIMEOUT);
        drop(stream);
    });

    let proxy =
        ProxyConfig::parse(format!("http://proxy-user:proxy-secret@{proxy_address}")).unwrap();
    let config = loopback_config(
        proxy,
        test_timeouts(Duration::from_millis(300), Duration::from_secs(1)),
    );
    let target = format!("http://{origin_address}/must-not-fallback?token=request-secret");
    let error = send_get(&HttpClientPool::default().get(&config).unwrap(), &target)
        .await
        .unwrap_err();
    assert_transport_error_redacted(
        &error,
        &[
            &target,
            "request-secret",
            "proxy-user",
            "proxy-secret",
            &proxy_address.to_string(),
        ],
    );

    proxy_server.join().unwrap();
    assert!(!origin_monitor.join().unwrap());
}

#[tokio::test]
async fn automatic_redirects_are_disabled() {
    let destination = loopback_listener();
    let destination_address = destination.local_addr().unwrap();
    let destination_monitor = monitor_listener(destination, Duration::from_millis(400));

    let redirect = loopback_listener();
    let redirect_address = redirect.local_addr().unwrap();
    let redirect_server = thread::spawn(move || {
        let mut stream = accept_with_timeout(&redirect, SERVER_TIMEOUT);
        read_request_head(&mut stream);
        write_response(
            &mut stream,
            &format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{destination_address}/final\r\nX-Response-Token: response-secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
        );
    });

    let config = direct_config(Duration::from_secs(1), Duration::from_secs(2));
    let target = format!("http://{redirect_address}/redirect?token=request-secret");
    let response = send_get(&HttpClientPool::default().get(&config).unwrap(), &target)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    let response_debug = format!("{response:?}");
    assert!(!response_debug.contains(&target));
    assert!(!response_debug.contains("request-secret"));
    assert!(!response_debug.contains("response-secret"));

    redirect_server.join().unwrap();
    assert!(!destination_monitor.join().unwrap());
}

#[tokio::test]
async fn read_timeout_stops_a_stalled_response_body() {
    let (address, release, server) = stalled_response_server();
    let config = direct_config(Duration::from_secs(1), Duration::from_secs(4));
    let target = format!("http://{address}/stalled?token=request-secret");
    let response = send_get(&HttpClientPool::default().get(&config).unwrap(), &target)
        .await
        .unwrap();
    let error = response.bytes().await.unwrap_err();
    assert_eq!(error, HttpTransportError::ReadTimeout);
    assert_transport_error_redacted(&error, &[&target, "request-secret"]);
    let _ = release.send(());
    server.join().unwrap();
}

#[tokio::test]
async fn streaming_body_errors_are_redacted() {
    let (address, release, server) = stalled_response_server();
    let config = direct_config(Duration::from_secs(1), Duration::from_secs(4));
    let target = format!("http://{address}/stream?key=stream-secret");
    let response = send_get(&HttpClientPool::default().get(&config).unwrap(), &target)
        .await
        .unwrap();
    let mut stream = response.into_bytes_stream();
    let error = stream.next_chunk().await.unwrap().unwrap_err();
    assert_eq!(error, HttpTransportError::ReadTimeout);
    assert_transport_error_redacted(&error, &[&target, "stream-secret"]);
    let _ = release.send(());
    server.join().unwrap();
}

#[tokio::test]
async fn request_timeout_caps_an_active_response_body() {
    let (address, stop, server) = active_response_server();

    let config = direct_config(Duration::from_secs(5), Duration::from_secs(2));
    let target = format!("http://{address}/overall-deadline?token=overall-secret");
    let response = send_get(&HttpClientPool::default().get(&config).unwrap(), &target)
        .await
        .unwrap();
    let error = response.bytes().await.unwrap_err();
    assert_eq!(error, HttpTransportError::RequestTimeout);
    assert_transport_error_redacted(&error, &[&target, "overall-secret"]);
    let _ = stop.send(());
    server.join().unwrap();
}

#[tokio::test]
async fn request_timeout_terminates_the_stream_after_one_error() {
    let (address, stop, server) = active_response_server();
    let config = direct_config(Duration::from_secs(5), Duration::from_secs(2));
    let target = format!("http://{address}/stream-deadline?token=stream-deadline-secret");
    let response = send_get(&HttpClientPool::default().get(&config).unwrap(), &target)
        .await
        .unwrap();
    let mut stream = response.into_bytes_stream();

    let error = loop {
        match stream.next_chunk().await.unwrap() {
            Ok(_) => {}
            Err(error) => break error,
        }
    };
    assert_eq!(error, HttpTransportError::RequestTimeout);
    assert_transport_error_redacted(&error, &[&target, "stream-deadline-secret"]);
    assert!(stream.next_chunk().await.is_none());
    let _ = stop.send(());
    server.join().unwrap();
}

#[tokio::test]
async fn invalid_request_targets_are_rejected_without_echoing_input() {
    let config = direct_config(Duration::from_secs(1), Duration::from_secs(2));
    let client = HttpClientPool::default().get(&config).unwrap();
    for target in [
        "gemini://upstream.invalid/path?key=target-secret",
        "http://request-user:request-secret@upstream.invalid/path",
    ] {
        let error = send_get(&client, target).await.unwrap_err();
        assert_eq!(error, HttpTransportError::InvalidRequestTarget);
        assert_transport_error_redacted(
            &error,
            &[target, "target-secret", "request-user", "request-secret"],
        );
    }
}

#[tokio::test]
async fn connect_method_is_rejected_without_logging_its_query() {
    let config = direct_config(Duration::from_secs(1), Duration::from_secs(2));
    let target = "http://upstream.invalid/tunnel?key=connect-query-secret";
    let error = HttpClientPool::default()
        .get(&config)
        .unwrap()
        .execute(Method::CONNECT, target, HeaderMap::new(), None)
        .await
        .unwrap_err();
    assert_eq!(error, HttpTransportError::UnsupportedRequestMethod);
    assert_transport_error_redacted(&error, &[target, "connect-query-secret"]);
}

#[tokio::test]
async fn read_timeout_also_covers_waiting_for_response_headers() {
    let listener = loopback_listener();
    let address = listener.local_addr().unwrap();
    let (release, wait_for_release) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener, SERVER_TIMEOUT);
        read_request_head(&mut stream);
        let _ = wait_for_release.recv_timeout(SERVER_TIMEOUT);
    });

    let config = direct_config(Duration::from_secs(1), Duration::from_secs(4));
    let target = format!("http://{address}/headers?token=header-secret");
    let error = send_get(&HttpClientPool::default().get(&config).unwrap(), &target)
        .await
        .unwrap_err();
    assert_eq!(error, HttpTransportError::ReadTimeout);
    assert_transport_error_redacted(&error, &[&target, "header-secret"]);
    let _ = release.send(());
    server.join().unwrap();
}

#[tokio::test]
async fn socks5_and_socks5h_keep_dns_resolution_semantics_distinct() {
    for (scheme, expect_remote_dns) in [("socks5", false), ("socks5h", true)] {
        let listener = loopback_listener();
        let proxy_address = listener.local_addr().unwrap();
        let proxy_server = socks_proxy_server(listener);
        let proxy = ProxyConfig::parse(format!("{scheme}://{proxy_address}")).unwrap();
        let mut config = loopback_config(
            proxy,
            HttpTimeouts::new(
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
            )
            .unwrap(),
        );
        if expect_remote_dns {
            config = config.with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
        }
        let target = "http://localhost/socks-target?token=socks-secret";

        let response = send_get(&HttpClientPool::default().get(&config).unwrap(), target)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (address_type, resolved_target) = proxy_server.join().unwrap();
        if expect_remote_dns {
            assert_eq!(address_type, 3);
            assert_eq!(resolved_target, "localhost");
        } else {
            assert!(
                resolved_target
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<std::net::IpAddr>()
                    .is_ok(),
                "{scheme} 未在本机解析目标：ATYP={address_type}, target={resolved_target}"
            );
        }
    }
}

#[tokio::test]
async fn connect_timeout_covers_a_stalled_socks_handshake() {
    let listener = loopback_listener();
    let proxy_address = listener.local_addr().unwrap();
    let (release, wait_for_release) = mpsc::channel();
    let proxy_server = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener, SERVER_TIMEOUT);
        read_socks_greeting(&mut stream);
        let _ = wait_for_release.recv_timeout(SERVER_TIMEOUT);
    });
    let proxy = ProxyConfig::parse(format!("socks5h://{proxy_address}")).unwrap();
    let config = loopback_config(
        proxy,
        HttpTimeouts::new(
            Duration::from_millis(500),
            Duration::from_secs(2),
            Duration::from_secs(4),
        )
        .unwrap(),
    )
    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    let target = "http://localhost/connect-timeout?token=connect-secret";

    let error = send_get(&HttpClientPool::default().get(&config).unwrap(), target)
        .await
        .unwrap_err();
    assert_eq!(error, HttpTransportError::ConnectTimeout);
    assert_transport_error_redacted(&error, &[target, "connect-secret"]);
    let _ = release.send(());
    proxy_server.join().unwrap();
}

#[tokio::test]
async fn public_only_policy_rejects_loopback_literals_before_connecting() {
    let listener = loopback_listener();
    let address = listener.local_addr().unwrap();
    let monitor = monitor_listener(listener, Duration::from_millis(300));
    let client = HttpClientPool::default()
        .get(&HttpClientConfig::new(
            ProxyConfig::direct(),
            test_timeouts(Duration::from_secs(1), Duration::from_secs(2)),
        ))
        .unwrap();

    let target = format!("http://{address}/blocked?token=target-secret");
    let error = send_get(&client, &target).await.unwrap_err();
    assert_eq!(error, HttpTransportError::TargetAddressBlocked);
    assert_transport_error_redacted(&error, &[&target, "target-secret"]);
    assert!(!monitor.join().unwrap());
}

#[tokio::test]
async fn local_dns_rejection_maps_to_a_stable_redacted_error() {
    let listener = loopback_listener();
    let port = listener.local_addr().unwrap().port();
    let monitor = monitor_listener(listener, Duration::from_millis(300));
    let client = HttpClientPool::default()
        .get(&HttpClientConfig::new(
            ProxyConfig::direct(),
            test_timeouts(Duration::from_secs(1), Duration::from_secs(2)),
        ))
        .unwrap();

    let target = format!("http://localhost:{port}/blocked?token=dns-secret");
    let error = send_get(&client, &target).await.unwrap_err();
    assert_eq!(error, HttpTransportError::TargetAddressBlocked);
    assert_transport_error_redacted(&error, &[&target, "localhost", "dns-secret"]);
    assert!(!monitor.join().unwrap());
}

#[tokio::test]
async fn nonstandard_loopback_literals_are_normalized_then_rejected() {
    let client = HttpClientPool::default()
        .get(&HttpClientConfig::new(
            ProxyConfig::direct(),
            test_timeouts(Duration::from_secs(1), Duration::from_secs(2)),
        ))
        .unwrap();

    for target in [
        "http://127.1/blocked",
        "http://2130706433/blocked",
        "http://0x7f000001/blocked",
        "http://0177.0.0.1/blocked",
    ] {
        assert_eq!(
            send_get(&client, target).await.unwrap_err(),
            HttpTransportError::TargetAddressBlocked,
            "未阻断 {target}"
        );
    }
}

#[tokio::test]
async fn remote_dns_proxy_rejects_domains_until_explicitly_trusted() {
    let denied_proxy = loopback_listener();
    let denied_proxy_address = denied_proxy.local_addr().unwrap();
    let denied_monitor = monitor_listener(denied_proxy, Duration::from_millis(300));
    let denied_config = HttpClientConfig::new(
        ProxyConfig::parse(format!("http://{denied_proxy_address}")).unwrap(),
        test_timeouts(Duration::from_secs(1), Duration::from_secs(2)),
    );
    let denied = send_get(
        &HttpClientPool::default().get(&denied_config).unwrap(),
        "http://public.example/denied?token=remote-secret",
    )
    .await
    .unwrap_err();
    assert_eq!(denied, HttpTransportError::RemoteDnsDenied);
    assert_transport_error_redacted(&denied, &["public.example", "remote-secret"]);
    assert!(!denied_monitor.join().unwrap());

    let trusted_proxy = loopback_listener();
    let trusted_proxy_address = trusted_proxy.local_addr().unwrap();
    let proxy_server = thread::spawn(move || {
        let mut stream = accept_with_timeout(&trusted_proxy, SERVER_TIMEOUT);
        let request = read_request_head(&mut stream);
        write_response(
            &mut stream,
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        request
    });
    let trusted_config = HttpClientConfig::new(
        ProxyConfig::parse(format!("http://{trusted_proxy_address}")).unwrap(),
        test_timeouts(Duration::from_secs(1), Duration::from_secs(2)),
    )
    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    let response = send_get(
        &HttpClientPool::default().get(&trusted_config).unwrap(),
        "http://public.example/trusted",
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        proxy_server
            .join()
            .unwrap()
            .starts_with("GET http://public.example/trusted HTTP/1.1")
    );
}

#[tokio::test]
async fn every_remote_dns_proxy_scheme_denies_domains_by_default() {
    for scheme in ["http", "https", "socks5h"] {
        let config = HttpClientConfig::new(
            ProxyConfig::parse(format!("{scheme}://127.0.0.1:9")).unwrap(),
            test_timeouts(Duration::from_secs(1), Duration::from_secs(2)),
        );
        let error = send_get(
            &HttpClientPool::default().get(&config).unwrap(),
            "https://public.example/blocked",
        )
        .await
        .unwrap_err();
        assert_eq!(error, HttpTransportError::RemoteDnsDenied, "协议 {scheme}");
    }
}

#[tokio::test]
async fn socks5_local_dns_blocks_private_answer_before_touching_proxy() {
    let proxy = loopback_listener();
    let proxy_address = proxy.local_addr().unwrap();
    let monitor = monitor_listener(proxy, Duration::from_millis(300));
    let config = HttpClientConfig::new(
        ProxyConfig::parse(format!("socks5://{proxy_address}")).unwrap(),
        test_timeouts(Duration::from_secs(1), Duration::from_secs(2)),
    );

    let error = send_get(
        &HttpClientPool::default().get(&config).unwrap(),
        "http://localhost/private",
    )
    .await
    .unwrap_err();
    assert_eq!(error, HttpTransportError::TargetAddressBlocked);
    assert!(!monitor.join().unwrap());
}

#[tokio::test]
async fn socks5_proxy_host_exemption_cannot_be_reused_as_the_target() {
    let config = HttpClientConfig::new(
        ProxyConfig::parse("socks5://proxy.example:1080").unwrap(),
        test_timeouts(Duration::from_secs(1), Duration::from_secs(2)),
    );
    let client = HttpClientPool::default().get(&config).unwrap();
    let target = "http://PROXY.EXAMPLE./must-not-use-proxy-exemption";

    assert_eq!(
        send_get(&client, target).await.unwrap_err(),
        HttpTransportError::TargetAddressBlocked
    );
}

fn direct_config(read: Duration, request: Duration) -> HttpClientConfig {
    loopback_config(ProxyConfig::direct(), test_timeouts(read, request))
}

fn loopback_config(proxy: ProxyConfig, timeouts: HttpTimeouts) -> HttpClientConfig {
    HttpClientConfig::new(proxy, timeouts).with_target_address_policy(
        TargetAddressPolicy::allow_exact(["127.0.0.1".parse().unwrap(), "::1".parse().unwrap()])
            .unwrap(),
    )
}

fn test_timeouts(read: Duration, request: Duration) -> HttpTimeouts {
    HttpTimeouts::new(Duration::from_millis(500), read, request).unwrap()
}

async fn send_get(client: &PooledClient, target: &str) -> Result<HttpResponse, HttpTransportError> {
    client
        .execute(Method::GET, target, HeaderMap::new(), None)
        .await
}

fn loopback_listener() -> TcpListener {
    TcpListener::bind(("127.0.0.1", 0)).unwrap()
}

fn stalled_response_server() -> (SocketAddr, Sender<()>, thread::JoinHandle<()>) {
    let listener = loopback_listener();
    let address = listener.local_addr().unwrap();
    let (release, wait_for_release) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener, SERVER_TIMEOUT);
        read_request_head(&mut stream);
        write_response(
            &mut stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n",
        );
        let _ = wait_for_release.recv_timeout(SERVER_TIMEOUT);
    });
    (address, release, server)
}

fn active_response_server() -> (SocketAddr, Sender<()>, thread::JoinHandle<()>) {
    let listener = loopback_listener();
    let address = listener.local_addr().unwrap();
    let (stop, wait_for_stop) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener, SERVER_TIMEOUT);
        read_request_head(&mut stream);
        write_response(
            &mut stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n",
        );
        for _ in 0..100 {
            if wait_for_stop.try_recv().is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
            if stream.write_all(b"x").is_err() {
                break;
            }
            let _ = stream.flush();
        }
    });
    (address, stop, server)
}

fn socks_proxy_server(listener: TcpListener) -> thread::JoinHandle<(u8, String)> {
    thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener, SERVER_TIMEOUT);
        read_socks_greeting(&mut stream);
        stream.write_all(&[5, 0]).unwrap();
        stream.flush().unwrap();

        let target = read_socks_connect_target(&mut stream);
        stream.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).unwrap();
        stream.flush().unwrap();
        let request = read_request_head(&mut stream);
        assert!(request.starts_with("GET /socks-target?token=socks-secret HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("\r\nhost: localhost\r\n")
        );
        write_response(
            &mut stream,
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        target
    })
}

fn read_socks_greeting(stream: &mut TcpStream) {
    let mut greeting = [0_u8; 2];
    stream.read_exact(&mut greeting).unwrap();
    assert_eq!(greeting[0], 5);
    let mut methods = vec![0_u8; usize::from(greeting[1])];
    stream.read_exact(&mut methods).unwrap();
    assert!(methods.contains(&0));
}

fn read_socks_connect_target(stream: &mut TcpStream) -> (u8, String) {
    let mut request = [0_u8; 4];
    stream.read_exact(&mut request).unwrap();
    assert_eq!(&request[..3], &[5, 1, 0]);
    let address_type = request[3];
    let target = match address_type {
        1 => {
            let mut address = [0_u8; 4];
            stream.read_exact(&mut address).unwrap();
            std::net::Ipv4Addr::from(address).to_string()
        }
        3 => {
            let mut length = [0_u8; 1];
            stream.read_exact(&mut length).unwrap();
            let mut domain = vec![0_u8; usize::from(length[0])];
            stream.read_exact(&mut domain).unwrap();
            String::from_utf8(domain).unwrap()
        }
        4 => {
            let mut address = [0_u8; 16];
            stream.read_exact(&mut address).unwrap();
            std::net::Ipv6Addr::from(address).to_string()
        }
        other => panic!("SOCKS5 地址类型无效：{other}"),
    };
    let mut port = [0_u8; 2];
    stream.read_exact(&mut port).unwrap();
    (address_type, target)
}

fn accept_with_timeout(listener: &TcpListener, timeout: Duration) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(timeout)).unwrap();
                stream.set_write_timeout(Some(timeout)).unwrap();
                return stream;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("测试服务未能接受连接：{error}"),
        }
    }
}

fn monitor_listener(listener: TcpListener, timeout: Duration) -> thread::JoinHandle<bool> {
    thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + timeout;
        loop {
            match listener.accept() {
                Ok((_stream, _)) => return true,
                Err(error)
                    if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => return false,
                Err(error) => panic!("测试监听失败：{error}"),
            }
        }
    })
}

fn read_request_head(stream: &mut TcpStream) -> String {
    let mut request = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 512];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "客户端在请求头结束前关闭连接");
        request.extend_from_slice(&buffer[..read]);
        assert!(request.len() <= 16 * 1024, "测试请求头超过 16 KiB");
    }
    String::from_utf8(request).unwrap()
}

fn write_response(stream: &mut TcpStream, response: &str) {
    stream.write_all(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}

fn assert_transport_error_redacted(error: &HttpTransportError, secrets: &[&str]) {
    let rendered = format!("{error}\n{error:?}");
    for secret in secrets {
        assert!(
            !rendered.contains(secret),
            "传输错误泄露了敏感文本：{secret}"
        );
    }
    assert!(std::error::Error::source(error).is_none());
}
