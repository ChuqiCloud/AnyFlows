use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

const MAX_IP_CIDR_TEXT_BYTES: usize = 64;

/// 已完成 TCP 对端与可信代理链校验的客户端 IP。
///
/// 该值不实现 `Display` 或序列化，`Debug` 也不会暴露地址，避免客户端 IP 被直接
/// 写入日志、响应或上游请求。IPv4-mapped IPv6 会统一归一为 IPv4。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct TrustedClientIp(IpAddr);

impl TrustedClientIp {
    /// 从已经过 HTTP 信任边界解析的地址构造客户端 IP。
    #[must_use]
    pub fn new(address: IpAddr) -> Self {
        Self(normalize_ip(address))
    }

    /// 返回仅供受控上游安全验证请求使用的规范 IP 地址。
    #[must_use]
    pub const fn as_ip(self) -> IpAddr {
        self.0
    }

    /// 返回白名单匹配所需的规范地址。
    #[must_use]
    pub(crate) const fn get(self) -> IpAddr {
        self.0
    }

    /// 将地址族和规范化地址写入固定缓冲区，仅供带密钥隐私指纹计算使用。
    ///
    /// 返回实际写入长度。调用方不得记录、持久化或向外发送该缓冲区。
    #[must_use]
    pub fn write_fingerprint_input(self, output: &mut [u8; 17]) -> usize {
        match self.0 {
            IpAddr::V4(address) => {
                output[0] = 4;
                output[1..5].copy_from_slice(&address.octets());
                5
            }
            IpAddr::V6(address) => {
                output[0] = 6;
                output[1..17].copy_from_slice(&address.octets());
                17
            }
        }
    }
}

impl fmt::Debug for TrustedClientIp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TrustedClientIp(<redacted>)")
    }
}

/// IP 或 CIDR 文本无效；不保留原始输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("IP/CIDR 格式无效")]
pub struct IpCidrParseError;

/// 规范化的 IPv4/IPv6 网络；裸 IP 等价于对应主机前缀。
///
/// 该类型只提供包含关系，不实现 `Display`，`Debug` 固定脱敏。序列化仅用于显式
/// 配置持久化，始终输出规范 CIDR 文本。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct IpCidr {
    network: IpAddr,
    prefix_len: u8,
}

impl IpCidr {
    /// 返回该规则是否覆盖对应地址族的全部地址。
    #[must_use]
    pub const fn is_universal(self) -> bool {
        self.prefix_len == 0
    }

    /// 判断可信客户端 IP 是否落在当前网络内。
    #[must_use]
    pub fn contains(self, client_ip: TrustedClientIp) -> bool {
        match (self.network, client_ip.get()) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                masked_v4(address, self.prefix_len) == network
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                masked_v6(address, self.prefix_len) == network
            }
            _ => false,
        }
    }

    fn canonical_text(self) -> String {
        format!("{}/{}", self.network, self.prefix_len)
    }
}

impl FromStr for IpCidr {
    type Err = IpCidrParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty()
            || value.len() > MAX_IP_CIDR_TEXT_BYTES
            || value.trim() != value
            || value.chars().any(char::is_control)
            || value.matches('/').count() > 1
        {
            return Err(IpCidrParseError);
        }

        let (address_text, prefix_text) = value
            .split_once('/')
            .map_or((value, None), |(address, prefix)| (address, Some(prefix)));
        let address = address_text
            .parse::<IpAddr>()
            .map_err(|_| IpCidrParseError)?;
        let supplied_prefix = prefix_text
            .map(|prefix| {
                if prefix.is_empty() || !prefix.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(IpCidrParseError);
                }
                prefix.parse::<u8>().map_err(|_| IpCidrParseError)
            })
            .transpose()?;

        let (network, prefix_len) = match address {
            IpAddr::V4(address) => {
                let prefix_len = supplied_prefix.unwrap_or(32);
                if prefix_len > 32 {
                    return Err(IpCidrParseError);
                }
                (IpAddr::V4(masked_v4(address, prefix_len)), prefix_len)
            }
            IpAddr::V6(address) => {
                let prefix_len = supplied_prefix.unwrap_or(128);
                if prefix_len > 128 {
                    return Err(IpCidrParseError);
                }
                if let Some(mapped) = address.to_ipv4_mapped() {
                    // IPv4 映射网络只有完整覆盖 96 位前缀时才能无歧义地折叠为 IPv4。
                    if prefix_len < 96 {
                        return Err(IpCidrParseError);
                    }
                    let prefix_len = prefix_len - 96;
                    (IpAddr::V4(masked_v4(mapped, prefix_len)), prefix_len)
                } else {
                    (IpAddr::V6(masked_v6(address, prefix_len)), prefix_len)
                }
            }
        };

        Ok(Self {
            network,
            prefix_len,
        })
    }
}

impl Serialize for IpCidr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.canonical_text())
    }
}

impl<'de> Deserialize<'de> for IpCidr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

impl fmt::Debug for IpCidr {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IpCidr(<redacted>)")
    }
}

fn normalize_ip(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(address) => address
            .to_ipv4_mapped()
            .map_or(IpAddr::V6(address), IpAddr::V4),
        address => address,
    }
}

fn masked_v4(address: Ipv4Addr, prefix_len: u8) -> Ipv4Addr {
    let mask = if prefix_len == 0 {
        0
    } else {
        u32::MAX << (32 - prefix_len)
    };
    Ipv4Addr::from(u32::from(address) & mask)
}

fn masked_v6(address: Ipv6Addr, prefix_len: u8) -> Ipv6Addr {
    let mask = if prefix_len == 0 {
        0
    } else {
        u128::MAX << (128 - prefix_len)
    };
    Ipv6Addr::from(u128::from(address) & mask)
}
