use std::fmt;

use thiserror::Error;

const PAYMENT_ID_BYTES: usize = 16;
const PAYMENT_KEY_BYTES: usize = PAYMENT_ID_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 充值业务标识构造错误；不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TopupIdentifierError {
    /// 全零标识无法形成有效幂等边界。
    #[error("充值业务标识不能全为零")]
    AllZero,
    /// 持久化键不是固定长度的小写十六进制编码。
    #[error("充值业务持久化键格式无效")]
    InvalidEncoding,
}

macro_rules! opaque_payment_id {
    ($(#[$meta:meta])* $name:ident, $debug_name:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; PAYMENT_ID_BYTES]);

        impl $name {
            /// 校验并构造非零的 128 位标识。
            pub const fn new(
                bytes: [u8; PAYMENT_ID_BYTES],
            ) -> Result<Self, TopupIdentifierError> {
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] != 0 {
                        return Ok(Self(bytes));
                    }
                    index += 1;
                }
                Err(TopupIdentifierError::AllZero)
            }

            /// 从数据库使用的 32 位小写十六进制键恢复标识。
            pub fn from_persistence_key(value: &str) -> Result<Self, TopupIdentifierError> {
                if value.len() != PAYMENT_KEY_BYTES {
                    return Err(TopupIdentifierError::InvalidEncoding);
                }
                let mut bytes = [0_u8; PAYMENT_ID_BYTES];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let high = decode_hex(pair[0]).ok_or(TopupIdentifierError::InvalidEncoding)?;
                    let low = decode_hex(pair[1]).ok_or(TopupIdentifierError::InvalidEncoding)?;
                    bytes[index] = (high << 4) | low;
                }
                Self::new(bytes)
            }

            /// 返回持久化边界使用的固定长度小写十六进制键。
            #[must_use]
            pub fn persistence_key(self) -> String {
                let mut encoded = String::with_capacity(PAYMENT_KEY_BYTES);
                for byte in self.0 {
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
                }
                encoded
            }

            /// 返回生成器和受控适配边界使用的原始 128 位值。
            #[must_use]
            pub const fn bytes(self) -> [u8; PAYMENT_ID_BYTES] {
                self.0
            }
        }

        impl TryFrom<[u8; PAYMENT_ID_BYTES]> for $name {
            type Error = TopupIdentifierError;

            fn try_from(bytes: [u8; PAYMENT_ID_BYTES]) -> Result<Self, Self::Error> {
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

opaque_payment_id!(
    /// 充值订单的稳定公开关联标识。
    TopupOrderId,
    "TopupOrderId"
);
opaque_payment_id!(
    /// 创建充值订单时由调用方复用的稳定幂等标识。
    TopupRequestId,
    "TopupRequestId"
);
opaque_payment_id!(
    /// 单次已验证支付事件的本地审计标识。
    TopupPaymentEventId,
    "TopupPaymentEventId"
);

/// 充值状态数值转换错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TopupStateCodeError {
    /// 数据库订单状态不属于闭合集合。
    #[error("充值订单状态无效")]
    InvalidOrderStatus,
    /// 数据库支付事件类型不属于闭合集合。
    #[error("充值支付事件类型无效")]
    InvalidEventType,
}

/// 充值订单的闭合生命周期状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum TopupOrderStatus {
    /// 本地订单已创建，尚未绑定 Provider 订单。
    Created = 1,
    /// Provider 订单已创建，等待支付结果。
    Pending = 2,
    /// 已确认支付并完成一次到账账本提交。
    Paid = 3,
    /// Provider 明确报告支付失败。
    Failed = 4,
    /// 本地或 Provider 已取消订单。
    Canceled = 5,
    /// Provider 明确报告订单过期。
    Expired = 6,
}

impl TopupOrderStatus {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断状态是否仍允许接收首个终态支付事件。
    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Created | Self::Pending)
    }
}

impl TryFrom<i16> for TopupOrderStatus {
    type Error = TopupStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Created),
            2 => Ok(Self::Pending),
            3 => Ok(Self::Paid),
            4 => Ok(Self::Failed),
            5 => Ok(Self::Canceled),
            6 => Ok(Self::Expired),
            _ => Err(TopupStateCodeError::InvalidOrderStatus),
        }
    }
}

/// 已验证支付 webhook 的闭合事件类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum TopupPaymentEventType {
    /// Provider 确认资金支付成功。
    Succeeded = 1,
    /// Provider 确认支付失败。
    Failed = 2,
    /// Provider 确认订单已过期。
    Expired = 3,
}

impl TopupPaymentEventType {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 返回本事件首次应用时对应的订单终态。
    #[must_use]
    pub const fn target_status(self) -> TopupOrderStatus {
        match self {
            Self::Succeeded => TopupOrderStatus::Paid,
            Self::Failed => TopupOrderStatus::Failed,
            Self::Expired => TopupOrderStatus::Expired,
        }
    }
}

impl TryFrom<i16> for TopupPaymentEventType {
    type Error = TopupStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Succeeded),
            2 => Ok(Self::Failed),
            3 => Ok(Self::Expired),
            _ => Err(TopupStateCodeError::InvalidEventType),
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
