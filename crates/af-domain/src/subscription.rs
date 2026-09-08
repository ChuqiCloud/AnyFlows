use std::fmt;

use thiserror::Error;

const SUBSCRIPTION_ID_BYTES: usize = 16;
const SUBSCRIPTION_KEY_BYTES: usize = SUBSCRIPTION_ID_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 订阅业务标识构造错误；不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionIdentifierError {
    /// 全零标识无法形成有效幂等边界。
    #[error("订阅业务标识不能全为零")]
    AllZero,
    /// 持久化键不是固定长度的小写十六进制编码。
    #[error("订阅业务持久化键格式无效")]
    InvalidEncoding,
}

macro_rules! opaque_subscription_id {
    ($(#[$meta:meta])* $name:ident, $debug_name:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; SUBSCRIPTION_ID_BYTES]);

        impl $name {
            /// 校验并构造非零的 128 位标识。
            pub const fn new(
                bytes: [u8; SUBSCRIPTION_ID_BYTES],
            ) -> Result<Self, SubscriptionIdentifierError> {
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] != 0 {
                        return Ok(Self(bytes));
                    }
                    index += 1;
                }
                Err(SubscriptionIdentifierError::AllZero)
            }

            /// 从数据库使用的 32 位小写十六进制键恢复标识。
            pub fn from_persistence_key(
                value: &str,
            ) -> Result<Self, SubscriptionIdentifierError> {
                if value.len() != SUBSCRIPTION_KEY_BYTES {
                    return Err(SubscriptionIdentifierError::InvalidEncoding);
                }
                let mut bytes = [0_u8; SUBSCRIPTION_ID_BYTES];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let high = decode_hex(pair[0])
                        .ok_or(SubscriptionIdentifierError::InvalidEncoding)?;
                    let low = decode_hex(pair[1])
                        .ok_or(SubscriptionIdentifierError::InvalidEncoding)?;
                    bytes[index] = (high << 4) | low;
                }
                Self::new(bytes)
            }

            /// 返回持久化边界使用的固定长度小写十六进制键。
            #[must_use]
            pub fn persistence_key(self) -> String {
                let mut encoded = String::with_capacity(SUBSCRIPTION_KEY_BYTES);
                for byte in self.0 {
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
                }
                encoded
            }
        }

        impl TryFrom<[u8; SUBSCRIPTION_ID_BYTES]> for $name {
            type Error = SubscriptionIdentifierError;

            fn try_from(bytes: [u8; SUBSCRIPTION_ID_BYTES]) -> Result<Self, Self::Error> {
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

opaque_subscription_id!(
    /// 订阅计划的稳定业务标识。
    SubscriptionPlanId,
    "SubscriptionPlanId"
);
opaque_subscription_id!(
    /// 订阅购买订单的稳定业务标识。
    SubscriptionOrderId,
    "SubscriptionOrderId"
);
opaque_subscription_id!(
    /// 创建订阅购买订单时由调用方复用的幂等请求标识。
    SubscriptionOrderRequestId,
    "SubscriptionOrderRequestId"
);
opaque_subscription_id!(
    /// 用户订阅记录及其幂等绑定的稳定业务标识。
    UserSubscriptionId,
    "UserSubscriptionId"
);
opaque_subscription_id!(
    /// 订阅订单已验签支付事件的本地审计标识。
    SubscriptionPaymentEventId,
    "SubscriptionPaymentEventId"
);

/// 订阅购买订单的闭合状态集合。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum SubscriptionOrderStatus {
    /// 本地订单已创建，尚未向支付 Provider 发起支付。
    Created = 1,
    /// Provider 订单已建立，等待支付结果。
    Pending = 2,
    /// Provider 已确认支付成功。
    Paid = 3,
    /// Provider 已明确报告支付失败。
    Failed = 4,
    /// 本地或 Provider 已取消订单。
    Canceled = 5,
    /// 订单已超过有效期。
    Expired = 6,
}

impl SubscriptionOrderStatus {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断订单是否仍允许接收支付确认。
    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Created | Self::Pending)
    }
}

/// 订阅计划的闭合启停状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum SubscriptionPlanStatus {
    /// 计划允许创建新的用户订阅。
    Active = 1,
    /// 计划已停止新增绑定，但不改变历史订阅事实。
    Disabled = 2,
}

impl SubscriptionPlanStatus {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断计划是否允许创建新订阅。
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 单个用户订阅的闭合生命周期状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum UserSubscriptionStatus {
    /// 当前时间进入窗口后允许消费订阅额度。
    Active = 1,
    /// 订阅被临时暂停，保留当前窗口和已用额度。
    Suspended = 2,
    /// 订阅被明确取消，不再进入后续周期。
    Canceled = 3,
    /// 订阅已到达不可继续推进的终止窗口。
    Expired = 4,
}

impl UserSubscriptionStatus {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断当前状态是否允许迁移到目标状态。
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Active, Self::Suspended)
                | (Self::Suspended, Self::Active)
                | (Self::Active | Self::Suspended, Self::Canceled)
                | (Self::Canceled, Self::Expired)
        )
    }
}

/// 订阅额度时间窗的闭合日历周期。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum SubscriptionCycle {
    /// 按 UTC 日历日推进。
    Daily = 1,
    /// 按 UTC 日历周推进。
    Weekly = 2,
    /// 按 UTC 日历月推进。
    Monthly = 3,
    /// 按 UTC 日历年推进。
    Yearly = 4,
}

impl SubscriptionCycle {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

/// 订阅状态数值转换错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionStateCodeError {
    /// 数据库计划状态不属于闭合集合。
    #[error("订阅计划状态无效")]
    InvalidPlanStatus,
    /// 数据库用户订阅状态不属于闭合集合。
    #[error("用户订阅状态无效")]
    InvalidSubscriptionStatus,
    /// 数据库周期不属于闭合集合。
    #[error("订阅周期无效")]
    InvalidCycle,
    /// 订阅购买订单状态不属于闭合集合。
    #[error("订阅购买订单状态无效")]
    InvalidOrderStatus,
    /// 订阅支付事件类型不属于闭合集合。
    #[error("订阅支付事件类型无效")]
    InvalidPaymentEventType,
}

impl TryFrom<i16> for SubscriptionPlanStatus {
    type Error = SubscriptionStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Active),
            2 => Ok(Self::Disabled),
            _ => Err(SubscriptionStateCodeError::InvalidPlanStatus),
        }
    }
}

impl TryFrom<i16> for UserSubscriptionStatus {
    type Error = SubscriptionStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Active),
            2 => Ok(Self::Suspended),
            3 => Ok(Self::Canceled),
            4 => Ok(Self::Expired),
            _ => Err(SubscriptionStateCodeError::InvalidSubscriptionStatus),
        }
    }
}

impl TryFrom<i16> for SubscriptionCycle {
    type Error = SubscriptionStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Daily),
            2 => Ok(Self::Weekly),
            3 => Ok(Self::Monthly),
            4 => Ok(Self::Yearly),
            _ => Err(SubscriptionStateCodeError::InvalidCycle),
        }
    }
}

impl TryFrom<i16> for SubscriptionOrderStatus {
    type Error = SubscriptionStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Created),
            2 => Ok(Self::Pending),
            3 => Ok(Self::Paid),
            4 => Ok(Self::Failed),
            5 => Ok(Self::Canceled),
            6 => Ok(Self::Expired),
            _ => Err(SubscriptionStateCodeError::InvalidOrderStatus),
        }
    }
}

/// 已验签订阅支付 webhook 的闭合事件类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum SubscriptionPaymentEventType {
    /// Provider 确认资金支付成功。
    Succeeded = 1,
    /// Provider 明确报告支付失败。
    Failed = 2,
    /// Provider 确认订单已过期。
    Expired = 3,
}

impl SubscriptionPaymentEventType {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 返回本事件首次应用时对应的订单终态。
    #[must_use]
    pub const fn target_status(self) -> SubscriptionOrderStatus {
        match self {
            Self::Succeeded => SubscriptionOrderStatus::Paid,
            Self::Failed => SubscriptionOrderStatus::Failed,
            Self::Expired => SubscriptionOrderStatus::Expired,
        }
    }
}

impl TryFrom<i16> for SubscriptionPaymentEventType {
    type Error = SubscriptionStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Succeeded),
            2 => Ok(Self::Failed),
            3 => Ok(Self::Expired),
            _ => Err(SubscriptionStateCodeError::InvalidPaymentEventType),
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
