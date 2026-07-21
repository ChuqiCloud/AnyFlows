use std::net::SocketAddr;

use af_config::{ClientIpSource, ServerConfig};
use af_domain::{IpCidr, TrustedClientIp};
use axum::extract::ConnectInfo;
use http::{HeaderValue, Request};

use crate::client_ip::{ClientIpResolutionError, ClientIpResolver};

fn resolver(source: ClientIpSource, networks: &[&str]) -> ClientIpResolver {
    let config = ServerConfig::default().with_client_ip_source(
        source,
        networks
            .iter()
            .map(|network| network.parse::<IpCidr>().unwrap()),
    );
    ClientIpResolver::from_config(&config)
}

fn request(peer: Option<&str>) -> Request<()> {
    let mut request = Request::new(());
    if let Some(peer) = peer {
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(peer.parse().unwrap(), 43123)));
    }
    request
}

fn assert_ip(actual: TrustedClientIp, expected: &str) {
    assert_eq!(actual, TrustedClientIp::new(expected.parse().unwrap()));
}

#[test]
fn peer_source_ignores_and_removes_all_forwarding_headers() {
    let resolver = resolver(ClientIpSource::Peer, &[]);
    let mut request = request(Some("192.0.2.10"));
    for (name, value) in [
        ("x-forwarded-for", "198.51.100.8"),
        ("forwarded", "for=198.51.100.8"),
        ("x-real-ip", "198.51.100.8"),
        ("cf-connecting-ip", "198.51.100.8"),
    ] {
        request
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }

    assert_ip(resolver.resolve(&mut request).unwrap(), "192.0.2.10");
    assert!(
        request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .is_none()
    );
    for name in [
        "x-forwarded-for",
        "forwarded",
        "x-real-ip",
        "cf-connecting-ip",
    ] {
        assert!(request.headers().get(name).is_none());
    }
}

#[test]
fn untrusted_peer_never_parses_forwarded_input() {
    let resolver = resolver(ClientIpSource::XForwardedFor, &["10.0.0.0/8"]);
    let mut request = request(Some("192.0.2.10"));
    request
        .headers_mut()
        .insert("x-forwarded-for", HeaderValue::from_static("not-an-ip,"));

    assert_ip(resolver.resolve(&mut request).unwrap(), "192.0.2.10");
    assert!(request.headers().get("x-forwarded-for").is_none());
}

#[test]
fn trusted_proxy_chain_stops_at_the_first_untrusted_hop_from_the_right() {
    let resolver = resolver(ClientIpSource::XForwardedFor, &["10.0.0.0/8"]);
    let mut request = request(Some("10.0.0.3"));
    request.headers_mut().insert(
        "x-forwarded-for",
        HeaderValue::from_static("198.51.100.99, 203.0.113.7, 10.0.0.2"),
    );

    assert_ip(resolver.resolve(&mut request).unwrap(), "203.0.113.7");
}

#[test]
fn trusted_proxy_chain_accepts_32_hops_and_rejects_33() {
    let resolver = resolver(ClientIpSource::XForwardedFor, &["10.0.0.0/8"]);
    let accepted = std::iter::once("198.51.100.99")
        .chain(std::iter::repeat_n("10.0.0.2", 31))
        .collect::<Vec<_>>()
        .join(", ");
    let mut accepted_request = request(Some("10.0.0.1"));
    accepted_request
        .headers_mut()
        .insert("x-forwarded-for", HeaderValue::from_str(&accepted).unwrap());
    assert_ip(
        resolver.resolve(&mut accepted_request).unwrap(),
        "198.51.100.99",
    );

    let rejected = std::iter::once("198.51.100.99")
        .chain(std::iter::repeat_n("10.0.0.2", 32))
        .collect::<Vec<_>>()
        .join(", ");
    let mut rejected_request = request(Some("10.0.0.1"));
    rejected_request
        .headers_mut()
        .insert("x-forwarded-for", HeaderValue::from_str(&rejected).unwrap());
    assert_eq!(
        resolver.resolve(&mut rejected_request),
        Err(ClientIpResolutionError::InvalidProxyChain)
    );
}

#[test]
fn mapped_tcp_peer_matches_ipv4_trusted_proxy_network() {
    let resolver = resolver(ClientIpSource::XForwardedFor, &["127.0.0.1/32"]);
    let mut request = request(Some("::ffff:127.0.0.1"));
    request
        .headers_mut()
        .insert("x-forwarded-for", HeaderValue::from_static("2001:db8::9"));

    assert_ip(resolver.resolve(&mut request).unwrap(), "2001:db8::9");
}

#[test]
fn missing_peer_and_invalid_trusted_proxy_chains_fail_closed() {
    let peer_resolver = resolver(ClientIpSource::Peer, &[]);
    let mut missing = request(None);
    missing
        .headers_mut()
        .insert("x-forwarded-for", HeaderValue::from_static("198.51.100.7"));
    assert_eq!(
        peer_resolver.resolve(&mut missing),
        Err(ClientIpResolutionError::MissingPeer)
    );
    assert!(missing.headers().get("x-forwarded-for").is_none());

    let resolver = resolver(ClientIpSource::XForwardedFor, &["10.0.0.0/8"]);
    let invalid_values = [
        "",
        "198.51.100.7,",
        "198.51.100.7:443",
        "unknown",
        "invalid, 203.0.113.7, 10.0.0.2",
        "10.0.0.2",
        &"203.0.113.7,".repeat(33),
        &"1".repeat(2_049),
    ];
    for value in invalid_values {
        let mut request = request(Some("10.0.0.1"));
        request
            .headers_mut()
            .insert("x-forwarded-for", HeaderValue::from_str(value).unwrap());
        assert_eq!(
            resolver.resolve(&mut request),
            Err(ClientIpResolutionError::InvalidProxyChain)
        );
    }

    let mut duplicate = request(Some("10.0.0.1"));
    duplicate
        .headers_mut()
        .append("x-forwarded-for", HeaderValue::from_static("203.0.113.7"));
    duplicate
        .headers_mut()
        .append("x-forwarded-for", HeaderValue::from_static("203.0.113.8"));
    assert_eq!(
        resolver.resolve(&mut duplicate),
        Err(ClientIpResolutionError::InvalidProxyChain)
    );

    let mut non_ascii = request(Some("10.0.0.1"));
    non_ascii
        .headers_mut()
        .insert("x-forwarded-for", HeaderValue::from_bytes(&[0xff]).unwrap());
    assert_eq!(
        resolver.resolve(&mut non_ascii),
        Err(ClientIpResolutionError::InvalidProxyChain)
    );
}

#[test]
fn resolver_debug_never_exposes_proxy_networks() {
    let resolver = resolver(ClientIpSource::XForwardedFor, &["10.23.45.0/24"]);
    let debug = format!("{resolver:?}");
    assert_eq!(debug, "ClientIpResolver(<redacted>)");
    assert!(!debug.contains("10.23.45"));
}

#[test]
fn resolver_rejects_unsafe_config_when_startup_validation_is_bypassed() {
    let resolver = resolver(ClientIpSource::XForwardedFor, &["0.0.0.0/0"]);
    let mut request = request(Some("192.0.2.10"));
    request
        .headers_mut()
        .insert("x-forwarded-for", HeaderValue::from_static("198.51.100.7"));

    assert_eq!(
        resolver.resolve(&mut request),
        Err(ClientIpResolutionError::InvalidProxyChain)
    );
}
