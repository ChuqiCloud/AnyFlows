use std::fmt;

use thiserror::Error;

const REDEMPTION_ID_BYTES: usize = 16;
const REDEMPTION_KEY_BYTES: usize = REDEMPTION_ID_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 兑换码业务标识构造错误；不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RedemptionIdentifierError {
    /// 全零标识无法形成有效幂等边界。
    #[error("兑换码业务标识不能全为零")]
    AllZero,
    /// 持久化键不是固定长度的小写十六进制编码。
    #[error("兑换码业务持久化键格式无效")]
    InvalidEncoding,
}

macro_rules! opaque_redemption_id {
    ($(#[$meta:meta])* $name:ident, $debug_name:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; REDEMPTION_ID_BYTES]);

        impl $name {
            /// 校验并构造非零的 128 位标识。
            pub const fn new(
                bytes: [u8; REDEMPTION_ID_BYTES],
            ) -> Result<Self, RedemptionIdentifierError> {
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] != 0 {
                        return Ok(Self(bytes));
                    }
                    index += 1;
                }
                Err(RedemptionIdentifierError::AllZero)
            }

            /// 从数据库使用的 32 位小写十六进制键恢复标识。
            pub fn from_persistence_key(
                value: &str,
            ) -> Result<Self, RedemptionIdentifierError> {
                if value.len() != REDEMPTION_KEY_BYTES {
                    return Err(RedemptionIdentifierError::InvalidEncoding);
                }
                let mut bytes = [0_u8; REDEMPTION_ID_BYTES];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let high = decode_hex(pair[0])
                        .ok_or(RedemptionIdentifierError::InvalidEncoding)?;
                    let low = decode_hex(pair[1])
                        .ok_or(RedemptionIdentifierError::InvalidEncoding)?;
                    bytes[index] = (high << 4) | low;
                }
                Self::new(bytes)
            }

            /// 返回持久化边界使用的固定长度小写十六进制键。
            #[must_use]
            pub fn persistence_key(self) -> String {
                let mut encoded = String::with_capacity(REDEMPTION_KEY_BYTES);
                for byte in self.0 {
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
                }
                encoded
            }

            /// 返回生成器和钱包事件适配边界使用的原始 128 位值。
            #[must_use]
            pub const fn bytes(self) -> [u8; REDEMPTION_ID_BYTES] {
                self.0
            }
        }

        impl TryFrom<[u8; REDEMPTION_ID_BYTES]> for $name {
            type Error = RedemptionIdentifierError;

            fn try_from(bytes: [u8; REDEMPTION_ID_BYTES]) -> Result<Self, Self::Error> {
                Self::new(bytes)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!($debug_name, "(<redacted>)"))
            }
        }
    };
}

opaque_redemption_id!(
    /// 一批同面额兑换码的稳定业务标识。
    RedemptionBatchId,
    "RedemptionBatchId"
);
opaque_redemption_id!(
    /// 单个兑换码的稳定业务标识，同时作为钱包账本事件标识。
    RedemptionCodeId,
    "RedemptionCodeId"
);

/// 兑换码批次的闭合启停状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum RedemptionBatchStatus {
    /// 批次允许尚未使用且未过期的兑换码到账。
    Active = 1,
    /// 批次已由管理动作整体失效。
    Disabled = 2,
}

impl RedemptionBatchStatus {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断批次当前是否允许兑换。
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 单个兑换码的闭合消费状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum RedemptionCodeStatus {
    /// 兑换码尚未被消费。
    Available = 1,
    /// 兑换码已经绑定唯一用户并完成钱包到账。
    Redeemed = 2,
}

impl RedemptionCodeStatus {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

/// 兑换码状态数值转换错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RedemptionStateCodeError {
    /// 数据库批次状态不属于闭合集合。
    #[error("兑换码批次状态无效")]
    InvalidBatchStatus,
    /// 数据库兑换码状态不属于闭合集合。
    #[error("兑换码状态无效")]
    InvalidCodeStatus,
}

impl TryFrom<i16> for RedemptionBatchStatus {
    type Error = RedemptionStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Active),
            2 => Ok(Self::Disabled),
            _ => Err(RedemptionStateCodeError::InvalidBatchStatus),
        }
    }
}

impl TryFrom<i16> for RedemptionCodeStatus {
    type Error = RedemptionStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Available),
            2 => Ok(Self::Redeemed),
            _ => Err(RedemptionStateCodeError::InvalidCodeStatus),
        }
    }
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
