use std::fmt;

use thiserror::Error;

use crate::{OrganizationCreditCurrency, OrganizationCreditInvoiceKey, OrganizationId, Quota};

const REPAYMENT_KEY_BYTES: usize = 16;
const REPAYMENT_KEY_HEX_BYTES: usize = REPAYMENT_KEY_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 企业还款与核销标识构造错误；不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditRepaymentKeyError {
    /// 全零标识不能形成有效幂等边界。
    #[error("企业还款或核销标识不能全为零")]
    AllZero,
    /// 持久化键不是固定长度的小写十六进制编码。
    #[error("企业还款或核销标识格式无效")]
    InvalidEncoding,
}

macro_rules! repayment_key {
    ($(#[$meta:meta])* $name:ident, $debug_name:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; REPAYMENT_KEY_BYTES]);

        impl $name {
            /// 校验并构造非零的 128 位标识。
            pub const fn new(
                bytes: [u8; REPAYMENT_KEY_BYTES],
            ) -> Result<Self, OrganizationCreditRepaymentKeyError> {
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] != 0 {
                        return Ok(Self(bytes));
                    }
                    index += 1;
                }
                Err(OrganizationCreditRepaymentKeyError::AllZero)
            }

            /// 从数据库固定键恢复业务标识。
            pub fn from_persistence_key(
                value: &str,
            ) -> Result<Self, OrganizationCreditRepaymentKeyError> {
                if value.len() != REPAYMENT_KEY_HEX_BYTES {
                    return Err(OrganizationCreditRepaymentKeyError::InvalidEncoding);
                }
                let mut bytes = [0_u8; REPAYMENT_KEY_BYTES];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let high = decode_hex(pair[0])
                        .ok_or(OrganizationCreditRepaymentKeyError::InvalidEncoding)?;
                    let low = decode_hex(pair[1])
                        .ok_or(OrganizationCreditRepaymentKeyError::InvalidEncoding)?;
                    bytes[index] = (high << 4) | low;
                }
                Self::new(bytes)
            }

            /// 返回数据库与审计投影共用的固定长度小写十六进制键。
            #[must_use]
            pub fn persistence_key(self) -> String {
                let mut encoded = String::with_capacity(REPAYMENT_KEY_HEX_BYTES);
                for byte in self.0 {
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
                }
                encoded
            }
        }

        impl TryFrom<[u8; REPAYMENT_KEY_BYTES]> for $name {
            type Error = OrganizationCreditRepaymentKeyError;

            fn try_from(bytes: [u8; REPAYMENT_KEY_BYTES]) -> Result<Self, Self::Error> {
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

repayment_key!(
    /// 已确认企业还款事实的稳定公开关联标识。
    OrganizationCreditRepaymentKey,
    "OrganizationCreditRepaymentKey"
);
repayment_key!(
    /// 还款来源的稳定幂等摘要键，不保存银行流水或回执原文。
    OrganizationCreditRepaymentSourceKey,
    "OrganizationCreditRepaymentSourceKey"
);
repayment_key!(
    /// 单次还款核销分配的稳定幂等标识。
    OrganizationCreditRepaymentAllocationKey,
    "OrganizationCreditRepaymentAllocationKey"
);

/// 已确认企业还款来源的闭合集合。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationCreditRepaymentSourceKind {
    /// 已由财务确认到账的对公银行转账。
    BankTransfer = 1,
    /// 其他经过独立财务流程确认的线下结算事实。
    OtherConfirmed = 2,
}

impl OrganizationCreditRepaymentSourceKind {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

impl TryFrom<i16> for OrganizationCreditRepaymentSourceKind {
    type Error = OrganizationCreditRepaymentError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::BankTransfer),
            2 => Ok(Self::OtherConfirmed),
            _ => Err(OrganizationCreditRepaymentError::InvalidSourceKind),
        }
    }
}

/// 已确认还款或核销使用的正整数金额。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationCreditRepaymentAmount(Quota);

impl OrganizationCreditRepaymentAmount {
    /// 构造正整数金额；零金额不能形成还款或核销事实。
    pub const fn new(units: i64) -> Result<Self, OrganizationCreditRepaymentError> {
        match Quota::new(units) {
            Ok(quota) if units > 0 => Ok(Self(quota)),
            _ => Err(OrganizationCreditRepaymentError::InvalidAmount),
        }
    }

    /// 返回持久化使用的整数金额。
    #[must_use]
    pub const fn units(self) -> i64 {
        self.0.units()
    }
}

/// 企业还款与核销领域错误；错误中不保留金额、来源或主体原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditRepaymentError {
    /// 还款、来源、分配或账单标识无效。
    #[error("企业还款或核销标识无效")]
    InvalidKey,
    /// 还款来源不属于闭合集合。
    #[error("企业还款来源类型无效")]
    InvalidSourceKind,
    /// 还款与核销金额必须大于零。
    #[error("企业还款或核销金额必须大于零")]
    InvalidAmount,
    /// 到账或核销时间无效。
    #[error("企业还款或核销时间无效")]
    InvalidTime,
}

/// 已确认企业还款不可变快照；只记录来源摘要，不代表钱包入账。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationCreditRepaymentSnapshot {
    organization_id: OrganizationId,
    repayment_key: OrganizationCreditRepaymentKey,
    source_kind: OrganizationCreditRepaymentSourceKind,
    source_key: OrganizationCreditRepaymentSourceKey,
    currency: OrganizationCreditCurrency,
    amount: OrganizationCreditRepaymentAmount,
    received_at: i64,
}

impl OrganizationCreditRepaymentSnapshot {
    /// 校验并构造已确认还款事实。
    pub fn new(
        organization_id: OrganizationId,
        repayment_key: OrganizationCreditRepaymentKey,
        source_kind: OrganizationCreditRepaymentSourceKind,
        source_key: OrganizationCreditRepaymentSourceKey,
        currency: OrganizationCreditCurrency,
        amount: OrganizationCreditRepaymentAmount,
        received_at: i64,
    ) -> Result<Self, OrganizationCreditRepaymentError> {
        if received_at < 0 {
            return Err(OrganizationCreditRepaymentError::InvalidTime);
        }
        Ok(Self {
            organization_id,
            repayment_key,
            source_kind,
            source_key,
            currency,
            amount,
            received_at,
        })
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn repayment_key(&self) -> OrganizationCreditRepaymentKey {
        self.repayment_key
    }
    #[must_use]
    pub const fn source_kind(&self) -> OrganizationCreditRepaymentSourceKind {
        self.source_kind
    }
    #[must_use]
    pub const fn source_key(&self) -> OrganizationCreditRepaymentSourceKey {
        self.source_key
    }
    #[must_use]
    pub fn currency(&self) -> &OrganizationCreditCurrency {
        &self.currency
    }
    #[must_use]
    pub const fn amount(&self) -> OrganizationCreditRepaymentAmount {
        self.amount
    }
    #[must_use]
    pub const fn received_at(&self) -> i64 {
        self.received_at
    }
}

/// 一笔还款对一张账单的不可变核销分配事实。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationCreditRepaymentAllocation {
    organization_id: OrganizationId,
    allocation_key: OrganizationCreditRepaymentAllocationKey,
    repayment_key: OrganizationCreditRepaymentKey,
    invoice_key: OrganizationCreditInvoiceKey,
    currency: OrganizationCreditCurrency,
    amount: OrganizationCreditRepaymentAmount,
    allocated_at: i64,
}

impl OrganizationCreditRepaymentAllocation {
    /// 校验并构造单次追加式核销分配。
    pub fn new(
        organization_id: OrganizationId,
        allocation_key: OrganizationCreditRepaymentAllocationKey,
        repayment_key: OrganizationCreditRepaymentKey,
        invoice_key: OrganizationCreditInvoiceKey,
        currency: OrganizationCreditCurrency,
        amount: OrganizationCreditRepaymentAmount,
        allocated_at: i64,
    ) -> Result<Self, OrganizationCreditRepaymentError> {
        if allocated_at < 0 {
            return Err(OrganizationCreditRepaymentError::InvalidTime);
        }
        Ok(Self {
            organization_id,
            allocation_key,
            repayment_key,
            invoice_key,
            currency,
            amount,
            allocated_at,
        })
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn allocation_key(&self) -> OrganizationCreditRepaymentAllocationKey {
        self.allocation_key
    }
    #[must_use]
    pub const fn repayment_key(&self) -> OrganizationCreditRepaymentKey {
        self.repayment_key
    }
    #[must_use]
    pub const fn invoice_key(&self) -> OrganizationCreditInvoiceKey {
        self.invoice_key
    }
    #[must_use]
    pub fn currency(&self) -> &OrganizationCreditCurrency {
        &self.currency
    }
    #[must_use]
    pub const fn amount(&self) -> OrganizationCreditRepaymentAmount {
        self.amount
    }
    #[must_use]
    pub const fn allocated_at(&self) -> i64 {
        self.allocated_at
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

    #[test]
    fn 还款与分配标识往返且调试脱敏() {
        let key = OrganizationCreditRepaymentKey::new([0xab; 16]).expect("还款标识有效");
        assert_eq!(
            OrganizationCreditRepaymentKey::from_persistence_key(&key.persistence_key()),
            Ok(key)
        );
        assert_eq!(
            format!("{key:?}"),
            "OrganizationCreditRepaymentKey(<redacted>)"
        );
        assert!(OrganizationCreditRepaymentSourceKey::new([0; 16]).is_err());
        assert!(OrganizationCreditRepaymentAllocationKey::from_persistence_key("bad").is_err());
    }

    #[test]
    fn 还款金额必须为正整数() {
        assert_eq!(
            OrganizationCreditRepaymentAmount::new(0),
            Err(OrganizationCreditRepaymentError::InvalidAmount)
        );
        assert_eq!(
            OrganizationCreditRepaymentAmount::new(-1),
            Err(OrganizationCreditRepaymentError::InvalidAmount)
        );
        assert_eq!(
            OrganizationCreditRepaymentAmount::new(1)
                .expect("金额有效")
                .units(),
            1
        );
    }

    #[test]
    fn 还款与分配快照拒绝负时间() {
        let organization_id = OrganizationId::new(1).expect("企业标识有效");
        let repayment_key = OrganizationCreditRepaymentKey::new([1; 16]).expect("还款标识有效");
        let currency = OrganizationCreditCurrency::new("CNY").expect("币种有效");
        let amount = OrganizationCreditRepaymentAmount::new(100).expect("金额有效");
        assert!(
            OrganizationCreditRepaymentSnapshot::new(
                organization_id,
                repayment_key,
                OrganizationCreditRepaymentSourceKind::BankTransfer,
                OrganizationCreditRepaymentSourceKey::new([2; 16]).expect("来源标识有效"),
                currency.clone(),
                amount,
                -1,
            )
            .is_err()
        );
        assert!(
            OrganizationCreditRepaymentAllocation::new(
                organization_id,
                OrganizationCreditRepaymentAllocationKey::new([3; 16]).expect("分配标识有效"),
                repayment_key,
                OrganizationCreditInvoiceKey::new([4; 16]).expect("账单标识有效"),
                currency,
                amount,
                -1,
            )
            .is_err()
        );
    }
}
