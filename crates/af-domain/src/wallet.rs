use std::fmt;

use thiserror::Error;

const WALLET_EVENT_ID_BYTES: usize = 16;
const WALLET_EVENT_KEY_BYTES: usize = WALLET_EVENT_ID_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
const SYSTEM_OPENING_NAMESPACE: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];

/// 钱包账本事件标识构造错误；不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WalletEventIdError {
    /// 全零标识无法提供有效的调用方幂等边界。
    #[error("钱包事件标识不能全为零")]
    AllZero,
    /// 持久化键不是固定长度的小写十六进制编码。
    #[error("钱包事件持久化键格式无效")]
    InvalidEncoding,
}

/// 单次钱包余额变更的稳定幂等标识。
///
/// 调用方应为每次业务事实生成一次随机 128 位标识，并让所有不确定重试复用同一值。
/// 该类型不实现 `Display` 或 Serde，避免事件标识被无意写入通用日志。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WalletEventId([u8; WALLET_EVENT_ID_BYTES]);

impl WalletEventId {
    /// 校验并构造非零的 128 位钱包事件标识。
    pub const fn new(bytes: [u8; WALLET_EVENT_ID_BYTES]) -> Result<Self, WalletEventIdError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(WalletEventIdError::AllZero)
    }

    /// 从数据库使用的 32 位小写十六进制键恢复标识。
    pub fn from_persistence_key(value: &str) -> Result<Self, WalletEventIdError> {
        if value.len() != WALLET_EVENT_KEY_BYTES {
            return Err(WalletEventIdError::InvalidEncoding);
        }

        let mut bytes = [0_u8; WALLET_EVENT_ID_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex(pair[0]).ok_or(WalletEventIdError::InvalidEncoding)?;
            let low = decode_hex(pair[1]).ok_or(WalletEventIdError::InvalidEncoding)?;
            bytes[index] = (high << 4) | low;
        }
        Self::new(bytes)
    }

    /// 返回持久化和已鉴权审计响应使用的固定长度小写十六进制键。
    #[must_use]
    pub fn persistence_key(self) -> String {
        let mut encoded = String::with_capacity(WALLET_EVENT_KEY_BYTES);
        for byte in self.0 {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        encoded
    }

    /// 判断事件是否占用系统 opening 基线的保留命名空间。
    #[must_use]
    pub const fn is_system_opening(self) -> bool {
        let mut index = 0;
        while index < SYSTEM_OPENING_NAMESPACE.len() {
            if self.0[index] != SYSTEM_OPENING_NAMESPACE[index] {
                return false;
            }
            index += 1;
        }
        true
    }

    /// 返回生成器或协议适配边界使用的原始 128 位值。
    #[must_use]
    pub const fn bytes(self) -> [u8; WALLET_EVENT_ID_BYTES] {
        self.0
    }
}

impl TryFrom<[u8; WALLET_EVENT_ID_BYTES]> for WalletEventId {
    type Error = WalletEventIdError;

    fn try_from(bytes: [u8; WALLET_EVENT_ID_BYTES]) -> Result<Self, Self::Error> {
        Self::new(bytes)
    }
}

impl fmt::Debug for WalletEventId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WalletEventId(<redacted>)")
    }
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
