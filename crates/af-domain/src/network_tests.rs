use std::net::{IpAddr, Ipv4Addr};

use serde_json::json;

use crate::{IpCidr, IpCidrParseError, TrustedClientIp};

fn client_ip(value: &str) -> TrustedClientIp {
    TrustedClientIp::new(value.parse().unwrap())
}

#[test]
fn bare_ips_and_cidrs_match_only_their_normalized_networks() {
    let host = "192.0.2.9".parse::<IpCidr>().unwrap();
    assert!(host.contains(client_ip("192.0.2.9")));
    assert!(!host.contains(client_ip("192.0.2.10")));

    let ipv4 = "192.0.2.129/24".parse::<IpCidr>().unwrap();
    assert!(ipv4.contains(client_ip("192.0.2.1")));
    assert!(!ipv4.contains(client_ip("192.0.3.1")));

    let ipv6 = "2001:db8:1::1/48".parse::<IpCidr>().unwrap();
    assert!(ipv6.contains(client_ip("2001:db8:1::ffff")));
    assert!(!ipv6.contains(client_ip("2001:db8:2::1")));
    assert!(!ipv6.contains(client_ip("192.0.2.1")));
}

#[test]
fn ipv4_mapped_ipv6_is_normalized_consistently() {
    let mapped = TrustedClientIp::new("::ffff:192.0.2.9".parse().unwrap());
    assert_eq!(mapped.get(), IpAddr::V4(Ipv4Addr::new(192, 0, 2, 9)));
    assert!("192.0.2.0/24".parse::<IpCidr>().unwrap().contains(mapped));
    assert!(
        "::ffff:192.0.2.0/120"
            .parse::<IpCidr>()
            .unwrap()
            .contains(client_ip("192.0.2.7"))
    );

    let mut mapped_input = [0_u8; 17];
    let mut ipv4_input = [0_u8; 17];
    let mapped_length = mapped.write_fingerprint_input(&mut mapped_input);
    let ipv4_length = client_ip("192.0.2.9").write_fingerprint_input(&mut ipv4_input);
    assert_eq!(mapped_length, 5);
    assert_eq!(mapped_input[..mapped_length], ipv4_input[..ipv4_length]);
}

#[test]
fn invalid_or_ambiguous_networks_fail_without_echoing_input() {
    for value in [
        "",
        " 192.0.2.1",
        "192.0.2.1 ",
        "192.0.2.1/",
        "192.0.2.1/+24",
        "192.0.2.1/33",
        "2001:db8::1/129",
        "192.0.2.1/24/7",
        "fe80::1%3",
        "::ffff:192.0.2.1/95",
    ] {
        let error = value.parse::<IpCidr>().unwrap_err();
        let rendered = format!("{error:?}\n{error}");
        assert_eq!(error, IpCidrParseError);
        if !value.is_empty() {
            assert!(!rendered.contains(value));
        }
    }
}

#[test]
fn debug_is_redacted_and_serde_uses_canonical_cidr_text() {
    let cidr = "192.0.2.129/24".parse::<IpCidr>().unwrap();
    let client = client_ip("192.0.2.9");

    assert_eq!(format!("{cidr:?}"), "IpCidr(<redacted>)");
    assert_eq!(format!("{client:?}"), "TrustedClientIp(<redacted>)");
    assert_eq!(serde_json::to_value(cidr).unwrap(), json!("192.0.2.0/24"));
    assert_eq!(
        serde_json::from_value::<IpCidr>(json!("2001:db8::1/64")).unwrap(),
        "2001:db8::/64".parse().unwrap()
    );
}
