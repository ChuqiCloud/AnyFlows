use std::fmt;

use thiserror::Error;

const ID_BYTES: usize = 16;
const KEY_BYTES: usize = ID_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 企业授权订单标识的构造错误；错误中不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPlanOrderIdentifierError {
    /// 全零标识不能形成稳定幂等边界。
    #[error("企业授权订单标识不能全为零")]
    AllZero,
    /// 持久化键不是固定长度的小写十六进制编码。
    #[error("企业授权订单标识格式无效")]
    InvalidEncoding,
}

macro_rules! opaque_id {
    ($(#[$meta:meta])* $name:ident, $debug_name:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; ID_BYTES]);

        impl $name {
            /// 构造非零 128 位标识。
            pub const fn new(
                bytes: [u8; ID_BYTES],
            ) -> Result<Self, OrganizationPlanOrderIdentifierError> {
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] != 0 {
                        return Ok(Self(bytes));
                    }
                    index += 1;
                }
                Err(OrganizationPlanOrderIdentifierError::AllZero)
            }

            /// 从数据库固定的小写十六进制键恢复标识。
            pub fn from_persistence_key(
                value: &str,
            ) -> Result<Self, OrganizationPlanOrderIdentifierError> {
                if value.len() != KEY_BYTES {
                    return Err(OrganizationPlanOrderIdentifierError::InvalidEncoding);
                }
                let mut bytes = [0_u8; ID_BYTES];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let high = decode_hex(pair[0])
                        .ok_or(OrganizationPlanOrderIdentifierError::InvalidEncoding)?;
                    let low = decode_hex(pair[1])
                        .ok_or(OrganizationPlanOrderIdentifierError::InvalidEncoding)?;
                    bytes[index] = (high << 4) | low;
                }
                Self::new(bytes)
            }

            /// 返回数据库和接口共用的固定长度小写键。
            #[must_use]
            pub fn persistence_key(self) -> String {
                let mut encoded = String::with_capacity(KEY_BYTES);
                for byte in self.0 {
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
                }
                encoded
            }

            /// 返回受控支付事件边界所需的原始标识字节。
            #[must_use]
            pub const fn bytes(self) -> [u8; ID_BYTES] {
                self.0
            }
        }

        impl TryFrom<[u8; ID_BYTES]> for $name {
            type Error = OrganizationPlanOrderIdentifierError;

            fn try_from(bytes: [u8; ID_BYTES]) -> Result<Self, Self::Error> {
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

opaque_id!(
    /// 企业授权订单的稳定公开业务标识。
    OrganizationPlanOrderId,
    "OrganizationPlanOrderId"
);
opaque_id!(
    /// 企业授权订单创建请求的幂等标识。
    OrganizationPlanOrderRequestId,
    "OrganizationPlanOrderRequestId"
);
opaque_id!(
    /// 企业授权订单支付事件的本地审计标识。
    OrganizationPlanPaymentEventId,
    "OrganizationPlanPaymentEventId"
);

/// 企业授权订单的闭合生命周期状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum OrganizationPlanOrderStatus {
    /// 本地订单已创建，尚未提交 Provider。
    Created = 1,
    /// Provider 订单已建立，等待支付结果。
    Pending = 2,
    /// 已确认支付成功；授权发放由后续切片完成。
    Paid = 3,
    /// Provider 明确报告支付失败。
    Failed = 4,
    /// 订单被本地或 Provider 取消。
    Canceled = 5,
    /// 订单已超过有效期。
    Expired = 6,
}

/// 企业授权订单的商业动作；动作一经建单不可修改。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum OrganizationPlanOrderKind {
    Initial = 1,
    Renewal = 2,
    Upgrade = 3,
    Downgrade = 4,
}

impl OrganizationPlanOrderKind {
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

impl TryFrom<i16> for OrganizationPlanOrderKind {
    type Error = OrganizationPlanOrderStateError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Initial),
            2 => Ok(Self::Renewal),
            3 => Ok(Self::Upgrade),
            4 => Ok(Self::Downgrade),
            _ => Err(OrganizationPlanOrderStateError::InvalidOrderKind),
        }
    }
}

impl OrganizationPlanOrderStatus {
    /// 返回数据库使用的稳定状态码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断订单是否仍可接收支付结果。
    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Created | Self::Pending)
    }
}

impl TryFrom<i16> for OrganizationPlanOrderStatus {
    type Error = OrganizationPlanOrderStateError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Created),
            2 => Ok(Self::Pending),
            3 => Ok(Self::Paid),
            4 => Ok(Self::Failed),
            5 => Ok(Self::Canceled),
            6 => Ok(Self::Expired),
            _ => Err(OrganizationPlanOrderStateError::InvalidOrderStatus),
        }
    }
}

/// 已验签企业支付事件的闭合类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum OrganizationPlanPaymentEventType {
    /// Provider 确认支付成功。
    Succeeded = 1,
    /// Provider 明确报告支付失败。
    Failed = 2,
    /// Provider 确认订单过期。
    Expired = 3,
}

impl OrganizationPlanPaymentEventType {
    /// 返回数据库使用的稳定类型码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 返回后续确认切片使用的目标订单状态。
    #[must_use]
    pub const fn target_status(self) -> OrganizationPlanOrderStatus {
        match self {
            Self::Succeeded => OrganizationPlanOrderStatus::Paid,
            Self::Failed => OrganizationPlanOrderStatus::Failed,
            Self::Expired => OrganizationPlanOrderStatus::Expired,
        }
    }
}

impl TryFrom<i16> for OrganizationPlanPaymentEventType {
    type Error = OrganizationPlanOrderStateError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Succeeded),
            2 => Ok(Self::Failed),
            3 => Ok(Self::Expired),
            _ => Err(OrganizationPlanOrderStateError::InvalidPaymentEventType),
        }
    }
}

/// 企业授权订单状态码转换错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPlanOrderStateError {
    /// 订单状态不属于闭合集合。
    #[error("企业授权订单状态无效")]
    InvalidOrderStatus,
    /// 支付事件类型不属于闭合集合。
    #[error("企业授权支付事件类型无效")]
    InvalidPaymentEventType,
    /// 订单商业动作不属于闭合集合。
    #[error("企业授权订单动作无效")]
    InvalidOrderKind,
}

const fn decode_hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 标识固定小写键且调试脱敏() {
        let id = OrganizationPlanOrderId::new([0x12; 16]).expect("标识有效");
        assert_eq!(
            OrganizationPlanOrderId::from_persistence_key(&id.persistence_key()),
            Ok(id)
        );
        assert_eq!(format!("{id:?}"), "OrganizationPlanOrderId(<redacted>)");
        assert!(OrganizationPlanOrderId::new([0; 16]).is_err());
    }

    #[test]
    fn 订单状态和支付事件使用稳定码() {
        assert_eq!(
            OrganizationPlanOrderStatus::try_from(1),
            Ok(OrganizationPlanOrderStatus::Created)
        );
        assert_eq!(
            OrganizationPlanPaymentEventType::Succeeded.target_status(),
            OrganizationPlanOrderStatus::Paid
        );
        assert!(OrganizationPlanOrderStatus::try_from(99).is_err());
    }
}
