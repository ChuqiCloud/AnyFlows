use std::fmt;

use thiserror::Error;

/// 企业套餐名称允许的最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_PLAN_NAME_BYTES: usize = 80;
/// 企业套餐说明允许的最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_PLAN_DESCRIPTION_BYTES: usize = 240;
/// 企业套餐支付 Provider 标识允许的最大字节数。
pub const MAX_ORGANIZATION_PLAN_PROVIDER_BYTES: usize = 32;
/// 企业套餐价格使用的 ISO 4217 币种长度。
pub const ORGANIZATION_PLAN_CURRENCY_BYTES: usize = 3;
/// 企业套餐有效期的最大天数。
pub const MAX_ORGANIZATION_PLAN_VALIDITY_DAYS: i64 = 3_650;

const PLAN_ID_BYTES: usize = 16;
const PLAN_ID_HEX_BYTES: usize = PLAN_ID_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 企业套餐输入或持久化标识校验错误；不保留外部原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPlanError {
    /// 标识全零，不能形成稳定的幂等边界。
    #[error("企业套餐标识不能全为零")]
    AllZero,
    /// 持久化标识不是固定长度的小写十六进制文本。
    #[error("企业套餐标识格式无效")]
    InvalidEncoding,
    /// 套餐名称或说明超出边界。
    #[error("企业套餐文本无效")]
    InvalidText,
    /// 套餐容量不是正整数。
    #[error("企业套餐容量必须为正整数")]
    InvalidCapacity,
    /// 审计保留期超出服务端边界。
    #[error("企业套餐审计保留期无效")]
    InvalidAuditRetention,
    /// 有效期不是服务端允许的正整数天数。
    #[error("企业套餐有效期无效")]
    InvalidValidity,
    /// Provider、币种或金额不符合货币最小单位契约。
    #[error("企业套餐价格无效")]
    InvalidPrice,
}

/// 不暴露数据库顺序的非零企业套餐业务标识。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationPlanId([u8; PLAN_ID_BYTES]);

impl OrganizationPlanId {
    /// 校验并构造非零套餐标识。
    pub const fn new(bytes: [u8; PLAN_ID_BYTES]) -> Result<Self, OrganizationPlanError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(OrganizationPlanError::AllZero)
    }

    /// 从数据库固定的小写十六进制键恢复套餐标识。
    pub fn from_persistence_key(value: &str) -> Result<Self, OrganizationPlanError> {
        if value.len() != PLAN_ID_HEX_BYTES {
            return Err(OrganizationPlanError::InvalidEncoding);
        }
        let mut bytes = [0_u8; PLAN_ID_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex(pair[0]).ok_or(OrganizationPlanError::InvalidEncoding)?;
            let low = decode_hex(pair[1]).ok_or(OrganizationPlanError::InvalidEncoding)?;
            bytes[index] = (high << 4) | low;
        }
        Self::new(bytes)
    }

    /// 返回数据库和接口共用的固定长度小写键。
    #[must_use]
    pub fn persistence_key(self) -> String {
        let mut encoded = String::with_capacity(PLAN_ID_HEX_BYTES);
        for byte in self.0 {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        encoded
    }
}

impl fmt::Debug for OrganizationPlanId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationPlanId(<redacted>)")
    }
}

/// 企业套餐销售状态；停用只影响新订单，不删除历史价格事实。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum OrganizationPlanStatus {
    /// 套餐可出现在可售目录中。
    Active = 1,
    /// 套餐不再接受新订单，但历史引用仍可读取。
    Disabled = 2,
}

impl OrganizationPlanStatus {
    /// 返回数据库使用的稳定状态码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断套餐是否仍可售。
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

impl TryFrom<i16> for OrganizationPlanStatus {
    type Error = OrganizationPlanError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Active),
            2 => Ok(Self::Disabled),
            _ => Err(OrganizationPlanError::InvalidEncoding),
        }
    }
}

/// 企业套餐出售时固化的容量和高级能力。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationPlanFeatures {
    team_management_enabled: bool,
    sso_enabled: bool,
    custom_roles_enabled: bool,
}

impl OrganizationPlanFeatures {
    /// 构造能力集合；未实现的自定义角色能力由上层销售边界拒绝。
    #[must_use]
    pub const fn new(
        team_management_enabled: bool,
        sso_enabled: bool,
        custom_roles_enabled: bool,
    ) -> Self {
        Self {
            team_management_enabled,
            sso_enabled,
            custom_roles_enabled,
        }
    }

    #[must_use]
    pub const fn team_management_enabled(self) -> bool {
        self.team_management_enabled
    }

    #[must_use]
    pub const fn sso_enabled(self) -> bool {
        self.sso_enabled
    }

    #[must_use]
    pub const fn custom_roles_enabled(self) -> bool {
        self.custom_roles_enabled
    }
}

/// 企业套餐的不可变商业能力快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationPlanSnapshot {
    features: OrganizationPlanFeatures,
    seat_limit: i64,
    team_limit: i64,
    organization_key_limit: i64,
    audit_retention_days: i64,
    validity_days: i64,
}

impl OrganizationPlanSnapshot {
    /// 构造并校验套餐容量、能力和有效期。
    #[allow(clippy::too_many_arguments, reason = "套餐快照字段必须完整固化")]
    pub fn new(
        features: OrganizationPlanFeatures,
        seat_limit: i64,
        team_limit: i64,
        organization_key_limit: i64,
        audit_retention_days: i64,
        validity_days: i64,
    ) -> Result<Self, OrganizationPlanError> {
        if seat_limit <= 0 || team_limit <= 0 || organization_key_limit <= 0 {
            return Err(OrganizationPlanError::InvalidCapacity);
        }
        if !(1..=3_650).contains(&audit_retention_days) {
            return Err(OrganizationPlanError::InvalidAuditRetention);
        }
        if !(1..=MAX_ORGANIZATION_PLAN_VALIDITY_DAYS).contains(&validity_days) {
            return Err(OrganizationPlanError::InvalidValidity);
        }
        Ok(Self {
            features,
            seat_limit,
            team_limit,
            organization_key_limit,
            audit_retention_days,
            validity_days,
        })
    }

    #[must_use]
    pub const fn features(self) -> OrganizationPlanFeatures {
        self.features
    }
    #[must_use]
    pub const fn seat_limit(self) -> i64 {
        self.seat_limit
    }
    #[must_use]
    pub const fn team_limit(self) -> i64 {
        self.team_limit
    }
    #[must_use]
    pub const fn organization_key_limit(self) -> i64 {
        self.organization_key_limit
    }
    #[must_use]
    pub const fn audit_retention_days(self) -> i64 {
        self.audit_retention_days
    }
    #[must_use]
    pub const fn validity_days(self) -> i64 {
        self.validity_days
    }
}

/// 企业套餐单次支付的不可变价格快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationPlanPrice {
    provider: String,
    currency: String,
    amount_minor: i64,
}

impl OrganizationPlanPrice {
    /// 构造使用货币最小单位表达的正价格。
    pub fn new(
        provider: String,
        currency: String,
        amount_minor: i64,
    ) -> Result<Self, OrganizationPlanError> {
        if !valid_ascii_token(&provider, MAX_ORGANIZATION_PLAN_PROVIDER_BYTES)
            || currency.len() != ORGANIZATION_PLAN_CURRENCY_BYTES
            || !currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || amount_minor <= 0
        {
            return Err(OrganizationPlanError::InvalidPrice);
        }
        Ok(Self {
            provider,
            currency,
            amount_minor,
        })
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i64 {
        self.amount_minor
    }
}

fn valid_ascii_token(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn decode_hex(value: u8) -> Option<u8> {
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
    fn 套餐快照拒绝零容量和超期有效期() {
        let features = OrganizationPlanFeatures::new(true, false, false);
        assert_eq!(
            OrganizationPlanSnapshot::new(features, 0, 2, 1, 90, 30),
            Err(OrganizationPlanError::InvalidCapacity)
        );
        assert_eq!(
            OrganizationPlanSnapshot::new(features, 1, 2, 1, 90, 3_651),
            Err(OrganizationPlanError::InvalidValidity)
        );
    }

    #[test]
    fn 价格快照使用货币最小单位并拒绝非法币种() {
        let price = OrganizationPlanPrice::new("stripe".to_owned(), "USD".to_owned(), 1_999)
            .expect("价格有效");
        assert_eq!(price.amount_minor(), 1_999);
        assert!(OrganizationPlanPrice::new("stripe".to_owned(), "usd".to_owned(), 1).is_err());
    }

    #[test]
    fn 套餐标识使用固定小写键且调试脱敏() {
        let id = OrganizationPlanId::new([0x12; 16]).expect("标识有效");
        assert_eq!(
            OrganizationPlanId::from_persistence_key(&id.persistence_key()),
            Ok(id)
        );
        assert_eq!(format!("{id:?}"), "OrganizationPlanId(<redacted>)");
    }
}
