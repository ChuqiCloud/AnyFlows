use std::fmt;

use thiserror::Error;

const RESERVATION_ID_BYTES: usize = 16;
const RESERVATION_KEY_BYTES: usize = RESERVATION_ID_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 计费预留幂等标识构造错误；不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BillingReservationIdError {
    /// 全零标识无法提供有效的调用方幂等边界。
    #[error("计费预留标识不能全为零")]
    AllZero,
    /// 持久化键不是固定长度的小写十六进制编码。
    #[error("计费预留持久化键格式无效")]
    InvalidEncoding,
}

/// 单次计费预留的稳定幂等标识。
///
/// 调用方应为每个下游请求生成一次随机 128 位标识，并让所有重试复用同一值。
/// 该类型不实现 `Display` 或 Serde，避免被直接写入日志和协议响应。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BillingReservationId([u8; RESERVATION_ID_BYTES]);

impl BillingReservationId {
    /// 校验并构造非零的 128 位计费预留标识。
    pub const fn new(bytes: [u8; RESERVATION_ID_BYTES]) -> Result<Self, BillingReservationIdError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(BillingReservationIdError::AllZero)
    }

    /// 从数据库使用的 32 位小写十六进制键恢复标识。
    pub fn from_persistence_key(value: &str) -> Result<Self, BillingReservationIdError> {
        if value.len() != RESERVATION_KEY_BYTES {
            return Err(BillingReservationIdError::InvalidEncoding);
        }

        let mut bytes = [0_u8; RESERVATION_ID_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex(pair[0]).ok_or(BillingReservationIdError::InvalidEncoding)?;
            let low = decode_hex(pair[1]).ok_or(BillingReservationIdError::InvalidEncoding)?;
            bytes[index] = (high << 4) | low;
        }
        Self::new(bytes)
    }

    /// 返回持久化边界使用的固定长度小写十六进制键。
    #[must_use]
    pub fn persistence_key(self) -> String {
        let mut encoded = String::with_capacity(RESERVATION_KEY_BYTES);
        for byte in self.0 {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        encoded
    }

    /// 返回生成器或协议适配边界使用的原始 128 位值。
    #[must_use]
    pub const fn bytes(self) -> [u8; RESERVATION_ID_BYTES] {
        self.0
    }
}

impl TryFrom<[u8; RESERVATION_ID_BYTES]> for BillingReservationId {
    type Error = BillingReservationIdError;

    fn try_from(bytes: [u8; RESERVATION_ID_BYTES]) -> Result<Self, Self::Error> {
        Self::new(bytes)
    }
}

impl fmt::Debug for BillingReservationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BillingReservationId(<redacted>)")
    }
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
