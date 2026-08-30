use std::fmt;

use thiserror::Error;

use crate::{OrganizationCreditCurrency, OrganizationCreditTermPeriod, OrganizationId, Quota};

const INVOICE_KEY_BYTES: usize = 16;
const INVOICE_KEY_HEX_BYTES: usize = INVOICE_KEY_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 企业账期账单业务标识构造错误；不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditInvoiceKeyError {
    /// 全零标识不能形成有效的幂等边界。
    #[error("企业账期账单标识不能全为零")]
    AllZero,
    /// 持久化键不是固定长度的小写十六进制编码。
    #[error("企业账期账单标识格式无效")]
    InvalidEncoding,
}

macro_rules! invoice_key {
    ($(#[$meta:meta])* $name:ident, $debug_name:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; INVOICE_KEY_BYTES]);

        impl $name {
            /// 校验并构造非零的 128 位标识。
            pub const fn new(
                bytes: [u8; INVOICE_KEY_BYTES],
            ) -> Result<Self, OrganizationCreditInvoiceKeyError> {
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] != 0 {
                        return Ok(Self(bytes));
                    }
                    index += 1;
                }
                Err(OrganizationCreditInvoiceKeyError::AllZero)
            }

            /// 从数据库固定键恢复业务标识。
            pub fn from_persistence_key(
                value: &str,
            ) -> Result<Self, OrganizationCreditInvoiceKeyError> {
                if value.len() != INVOICE_KEY_HEX_BYTES {
                    return Err(OrganizationCreditInvoiceKeyError::InvalidEncoding);
                }
                let mut bytes = [0_u8; INVOICE_KEY_BYTES];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let high = decode_hex(pair[0])
                        .ok_or(OrganizationCreditInvoiceKeyError::InvalidEncoding)?;
                    let low = decode_hex(pair[1])
                        .ok_or(OrganizationCreditInvoiceKeyError::InvalidEncoding)?;
                    bytes[index] = (high << 4) | low;
                }
                Self::new(bytes)
            }

            /// 返回数据库与审计投影共用的固定长度小写十六进制键。
            #[must_use]
            pub fn persistence_key(self) -> String {
                let mut encoded = String::with_capacity(INVOICE_KEY_HEX_BYTES);
                for byte in self.0 {
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
                }
                encoded
            }
        }

        impl TryFrom<[u8; INVOICE_KEY_BYTES]> for $name {
            type Error = OrganizationCreditInvoiceKeyError;

            fn try_from(bytes: [u8; INVOICE_KEY_BYTES]) -> Result<Self, Self::Error> {
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

invoice_key!(
    /// 企业账期账单的稳定公开关联标识。
    OrganizationCreditInvoiceKey,
    "OrganizationCreditInvoiceKey"
);
invoice_key!(
    /// 账单来源的稳定幂等标识，只允许存储摘要键而非来源正文。
    OrganizationCreditInvoiceSourceKey,
    "OrganizationCreditInvoiceSourceKey"
);

/// 企业账期账单来源的闭合集合。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationCreditInvoiceSourceKind {
    /// 已确认的企业用量事实。
    Usage = 1,
    /// 其他经财务确认的应收事实摘要。
    OtherReceivable = 2,
}

impl OrganizationCreditInvoiceSourceKind {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

impl TryFrom<i16> for OrganizationCreditInvoiceSourceKind {
    type Error = OrganizationCreditInvoiceError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Usage),
            2 => Ok(Self::OtherReceivable),
            _ => Err(OrganizationCreditInvoiceError::InvalidSourceKind),
        }
    }
}

/// 企业账期账单生命周期；Settled/Voided 为终态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationCreditInvoiceStatus {
    /// 已签发且尚未到期。
    Due = 1,
    /// 已超过到期时间但尚未核销。
    Overdue = 2,
    /// 已由后续还款核销流程确认结清。
    Settled = 3,
    /// 在未核销前作废，不能再恢复。
    Voided = 4,
}

impl OrganizationCreditInvoiceStatus {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 校验状态单向推进，避免普通调账伪造结清或重新打开账单。
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (
                Self::Due,
                Self::Due | Self::Overdue | Self::Settled | Self::Voided
            ) | (Self::Overdue, Self::Overdue | Self::Settled | Self::Voided)
                | (Self::Settled, Self::Settled)
                | (Self::Voided, Self::Voided)
        )
    }
}

impl TryFrom<i16> for OrganizationCreditInvoiceStatus {
    type Error = OrganizationCreditInvoiceError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Due),
            2 => Ok(Self::Overdue),
            3 => Ok(Self::Settled),
            4 => Ok(Self::Voided),
            _ => Err(OrganizationCreditInvoiceError::InvalidStatus),
        }
    }
}

/// 企业账期账单金额；沿用非负整数额度，单位由币种最小货币单位约定。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationCreditInvoiceAmount(Quota);

impl OrganizationCreditInvoiceAmount {
    /// 构造非负金额，零金额保留给后续对账场景但不产生资金动作。
    pub const fn new(units: i64) -> Result<Self, OrganizationCreditInvoiceError> {
        match Quota::new(units) {
            Ok(quota) => Ok(Self(quota)),
            Err(_) => Err(OrganizationCreditInvoiceError::NegativeAmount),
        }
    }

    /// 返回持久化使用的整数金额。
    #[must_use]
    pub const fn units(self) -> i64 {
        self.0.units()
    }
}

/// 账期账单领域错误；错误中不保留来源、金额或主体原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditInvoiceError {
    /// 账单或来源标识无效。
    #[error("企业账期账单标识无效")]
    InvalidKey,
    /// 来源类型不属于闭合集合。
    #[error("企业账期账单来源类型无效")]
    InvalidSourceKind,
    /// 账单状态不属于闭合集合。
    #[error("企业账期账单状态无效")]
    InvalidStatus,
    /// 金额不能为负数。
    #[error("企业账期账单金额不能为负数")]
    NegativeAmount,
    /// 账期版本必须为正整数。
    #[error("企业账期账期版本必须大于零")]
    InvalidTermVersion,
    /// 签发、到期和状态时间不满足顺序约束。
    #[error("企业账期账单时间区间无效")]
    InvalidTimeRange,
    /// 账单版本必须为正整数。
    #[error("企业账期账单版本必须大于零")]
    InvalidVersion,
}

/// 企业账期账单不可变快照；不执行扣款、还款核销或催收。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationCreditInvoiceSnapshot {
    organization_id: OrganizationId,
    invoice_key: OrganizationCreditInvoiceKey,
    source_kind: OrganizationCreditInvoiceSourceKind,
    source_key: OrganizationCreditInvoiceSourceKey,
    currency: OrganizationCreditCurrency,
    amount: OrganizationCreditInvoiceAmount,
    term_version: i64,
    period: OrganizationCreditTermPeriod,
    issued_at: i64,
    due_at: i64,
    status: OrganizationCreditInvoiceStatus,
    status_changed_at: i64,
    version: i64,
}

impl OrganizationCreditInvoiceSnapshot {
    /// 校验并构造账单事实，来源只以稳定摘要键关联。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        invoice_key: OrganizationCreditInvoiceKey,
        source_kind: OrganizationCreditInvoiceSourceKind,
        source_key: OrganizationCreditInvoiceSourceKey,
        currency: OrganizationCreditCurrency,
        amount: OrganizationCreditInvoiceAmount,
        term_version: i64,
        period: OrganizationCreditTermPeriod,
        issued_at: i64,
        due_at: i64,
        status: OrganizationCreditInvoiceStatus,
        status_changed_at: i64,
        version: i64,
    ) -> Result<Self, OrganizationCreditInvoiceError> {
        if term_version <= 0 {
            return Err(OrganizationCreditInvoiceError::InvalidTermVersion);
        }
        if issued_at < 0 || due_at <= issued_at || status_changed_at < issued_at {
            return Err(OrganizationCreditInvoiceError::InvalidTimeRange);
        }
        if version <= 0 {
            return Err(OrganizationCreditInvoiceError::InvalidVersion);
        }
        Ok(Self {
            organization_id,
            invoice_key,
            source_kind,
            source_key,
            currency,
            amount,
            term_version,
            period,
            issued_at,
            due_at,
            status,
            status_changed_at,
            version,
        })
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn invoice_key(&self) -> OrganizationCreditInvoiceKey {
        self.invoice_key
    }
    #[must_use]
    pub const fn source_kind(&self) -> OrganizationCreditInvoiceSourceKind {
        self.source_kind
    }
    #[must_use]
    pub const fn source_key(&self) -> OrganizationCreditInvoiceSourceKey {
        self.source_key
    }
    #[must_use]
    pub fn currency(&self) -> &OrganizationCreditCurrency {
        &self.currency
    }
    #[must_use]
    pub const fn amount(&self) -> OrganizationCreditInvoiceAmount {
        self.amount
    }
    #[must_use]
    pub const fn term_version(&self) -> i64 {
        self.term_version
    }
    #[must_use]
    pub const fn period(&self) -> OrganizationCreditTermPeriod {
        self.period
    }
    #[must_use]
    pub const fn issued_at(&self) -> i64 {
        self.issued_at
    }
    #[must_use]
    pub const fn due_at(&self) -> i64 {
        self.due_at
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationCreditInvoiceStatus {
        self.status
    }
    #[must_use]
    pub const fn status_changed_at(&self) -> i64 {
        self.status_changed_at
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoice(status: OrganizationCreditInvoiceStatus) -> OrganizationCreditInvoiceSnapshot {
        OrganizationCreditInvoiceSnapshot::new(
            OrganizationId::new(1).expect("企业标识有效"),
            OrganizationCreditInvoiceKey::new([1; 16]).expect("账单标识有效"),
            OrganizationCreditInvoiceSourceKind::Usage,
            OrganizationCreditInvoiceSourceKey::new([2; 16]).expect("来源标识有效"),
            OrganizationCreditCurrency::new("CNY").expect("币种有效"),
            OrganizationCreditInvoiceAmount::new(100).expect("金额有效"),
            3,
            OrganizationCreditTermPeriod::net_days(30).expect("周期有效"),
            100,
            200,
            status,
            100,
            1,
        )
        .expect("账单有效")
    }

    #[test]
    fn 账单标识往返且调试脱敏() {
        let key = OrganizationCreditInvoiceKey::new([0xab; 16]).expect("标识有效");
        assert_eq!(
            OrganizationCreditInvoiceKey::from_persistence_key(&key.persistence_key()),
            Ok(key)
        );
        assert_eq!(
            format!("{key:?}"),
            "OrganizationCreditInvoiceKey(<redacted>)"
        );
        assert!(OrganizationCreditInvoiceKey::new([0; 16]).is_err());
        assert!(OrganizationCreditInvoiceSourceKey::from_persistence_key("bad").is_err());
    }

    #[test]
    fn 账单状态仅允许到期和终态推进() {
        assert!(
            OrganizationCreditInvoiceStatus::Due
                .can_transition_to(OrganizationCreditInvoiceStatus::Overdue)
        );
        assert!(
            OrganizationCreditInvoiceStatus::Overdue
                .can_transition_to(OrganizationCreditInvoiceStatus::Settled)
        );
        assert!(
            !OrganizationCreditInvoiceStatus::Settled
                .can_transition_to(OrganizationCreditInvoiceStatus::Due)
        );
        assert_eq!(
            invoice(OrganizationCreditInvoiceStatus::Due).term_version(),
            3
        );
    }

    #[test]
    fn 账单拒绝负金额和非法时间() {
        assert_eq!(
            OrganizationCreditInvoiceAmount::new(-1),
            Err(OrganizationCreditInvoiceError::NegativeAmount)
        );
        assert!(
            OrganizationCreditInvoiceSnapshot::new(
                OrganizationId::new(1).expect("企业标识有效"),
                OrganizationCreditInvoiceKey::new([1; 16]).expect("账单标识有效"),
                OrganizationCreditInvoiceSourceKind::OtherReceivable,
                OrganizationCreditInvoiceSourceKey::new([2; 16]).expect("来源标识有效"),
                OrganizationCreditCurrency::new("CNY").expect("币种有效"),
                OrganizationCreditInvoiceAmount::new(1).expect("金额有效"),
                1,
                OrganizationCreditTermPeriod::EndOfMonth,
                200,
                200,
                OrganizationCreditInvoiceStatus::Due,
                200,
                1,
            )
            .is_err()
        );
    }
}
