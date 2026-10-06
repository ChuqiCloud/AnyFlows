use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::TargetAddressPolicy;

// 普通模型上游不需要 IANA 特殊用途地址；即使部分协议段可全局路由也统一拒绝。
const BLOCKED_IPV4_NETWORKS: &[(u32, u8)] = &[
    (0x0000_0000, 8),  // 当前网络
    (0x0a00_0000, 8),  // RFC 1918 私网
    (0x6440_0000, 10), // 共享地址空间
    (0x7f00_0000, 8),  // 环回地址
    (0xa9fe_0000, 16), // 链路本地与云元数据
    (0xac10_0000, 12), // RFC 1918 私网
    (0xc000_0000, 24), // IETF 协议分配
    (0xc000_0200, 24), // TEST-NET-1
    (0xc01f_c400, 24), // AS112 IPv4 服务
    (0xc034_c100, 24), // 自动组播隧道
    (0xc058_6300, 24), // 6to4 中继任播
    (0xc0a8_0000, 16), // RFC 1918 私网
    (0xc0af_3000, 24), // AS112 直接委派
    (0xc612_0000, 15), // 基准测试
    (0xc633_6400, 24), // TEST-NET-2
    (0xcb00_7100, 24), // TEST-NET-3
    (0xe000_0000, 4),  // 组播
    (0xf000_0000, 4),  // 保留地址与受限广播
];

const GLOBAL_IPV6_UNICAST: (u128, u8) = (0x2000_0000_0000_0000_0000_0000_0000_0000, 3);
const BLOCKED_IPV6_NETWORKS: &[(u128, u8)] = &[
    // IETF 协议分配，含 Teredo、基准测试、AMT、ORCHID 等特殊用途段。
    (0x2001_0000_0000_0000_0000_0000_0000_0000, 23),
    (0x2001_0db8_0000_0000_0000_0000_0000_0000, 32), // 文档段
    (0x2002_0000_0000_0000_0000_0000_0000_0000, 16), // 6to4
    (0x2620_004f_8000_0000_0000_0000_0000_0000, 48), // AS112 IPv6 服务
    (0x3ffe_0000_0000_0000_0000_0000_0000_0000, 16), // 已废弃 6bone
    (0x3fff_0000_0000_0000_0000_0000_0000_0000, 20), // 文档段
];

pub(crate) fn is_target_address_allowed(address: IpAddr, policy: &TargetAddressPolicy) -> bool {
    is_ordinary_public_address(address) || policy.contains_exception(&address)
}

#[cfg(test)]
pub(crate) fn is_test_loopback_exception(address: IpAddr) -> bool {
    address.is_loopback()
}

fn is_ordinary_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_ordinary_public_ipv4(address),
        IpAddr::V6(address) => is_ordinary_public_ipv6(address),
    }
}

fn is_ordinary_public_ipv4(address: Ipv4Addr) -> bool {
    let bits = u32::from(address);
    !BLOCKED_IPV4_NETWORKS
        .iter()
        .any(|&(network, prefix)| ipv4_in_network(bits, network, prefix))
}

fn is_ordinary_public_ipv6(address: Ipv6Addr) -> bool {
    let bits = u128::from(address);
    let (global_network, global_prefix) = GLOBAL_IPV6_UNICAST;
    ipv6_in_network(bits, global_network, global_prefix)
        && !BLOCKED_IPV6_NETWORKS
            .iter()
            .any(|&(network, prefix)| ipv6_in_network(bits, network, prefix))
}

fn ipv4_in_network(address: u32, network: u32, prefix: u8) -> bool {
    let mask = u32::MAX << (u32::BITS - u32::from(prefix));
    address & mask == network & mask
}

fn ipv6_in_network(address: u128, network: u128, prefix: u8) -> bool {
    let mask = u128::MAX << (u128::BITS - u32::from(prefix));
    address & mask == network & mask
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public_only_allows(value: &str) -> bool {
        is_target_address_allowed(value.parse().unwrap(), &TargetAddressPolicy::public_only())
    }

    #[test]
    fn blocks_ipv4_special_purpose_and_metadata_ranges() {
        for address in [
            "0.0.0.0",
            "0.255.255.255",
            "10.0.0.1",
            "100.64.0.1",
            "100.100.100.200",
            "127.0.0.1",
            "169.254.169.254",
            "172.31.255.255",
            "192.0.0.9",
            "192.0.2.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "255.255.255.255",
        ] {
            assert!(!public_only_allows(address), "意外放行 {address}");
        }
    }

    #[test]
    fn blocks_ipv6_special_purpose_and_transition_ranges() {
        for address in [
            "::",
            "::1",
            "::ffff:127.0.0.1",
            "64:ff9b::7f00:1",
            "64:ff9b:1::7f00:1",
            "100::1",
            "100:0:0:1::1",
            "2001::1",
            "2001:db8::1",
            "2002:7f00:1::1",
            "3ffe::1",
            "3fff::1",
            "5f00::1",
            "fc00::1",
            "fd00:ec2::254",
            "fe80::1",
            "fec0::1",
            "ff02::1",
        ] {
            assert!(!public_only_allows(address), "意外放行 {address}");
        }
    }

    #[test]
    fn allows_ordinary_public_unicast_addresses() {
        for address in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "2001:4860:4860::8888",
            "2606:4700:4700::1111",
        ] {
            assert!(public_only_allows(address), "意外阻断 {address}");
        }
    }

    #[test]
    fn exact_exceptions_do_not_expand_to_adjacent_addresses() {
        let allowed: IpAddr = "127.0.0.1".parse().unwrap();
        let policy = TargetAddressPolicy::allow_exact([allowed]).unwrap();
        assert!(is_target_address_allowed(allowed, &policy));
        assert!(!is_target_address_allowed(
            "127.0.0.2".parse().unwrap(),
            &policy
        ));
        assert!(!is_target_address_allowed(
            "::ffff:127.0.0.1".parse().unwrap(),
            &policy
        ));
    }

    #[test]
    fn cidr_boundaries_do_not_block_adjacent_public_ipv4() {
        assert!(public_only_allows("100.63.255.255"));
        assert!(public_only_allows("100.128.0.0"));
        assert!(public_only_allows("172.15.255.255"));
        assert!(public_only_allows("172.32.0.0"));
        assert!(public_only_allows("223.255.255.255"));
    }
}
