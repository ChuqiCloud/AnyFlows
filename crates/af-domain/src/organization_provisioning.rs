use std::fmt;

use thiserror::Error;

use crate::{OrganizationId, OrganizationProvisioningRequestId, UserId};

const KEY_BYTES: usize = 16;
const KEY_HEX_BYTES: usize = KEY_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 企业开通申请名称最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_PROVISIONING_NAME_BYTES: usize = 128;
/// 企业开通申请规范标识最大 ASCII 字节数。
pub const MAX_ORGANIZATION_PROVISIONING_SLUG_BYTES: usize = 64;
/// 企业开通申请业务说明最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_PROVISIONING_REASON_BYTES: usize = 2_000;
/// 企业开通审批理由最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_PROVISIONING_DECISION_REASON_BYTES: usize = 512;

macro_rules! provisioning_key {
    ($(#[$meta:meta])* $name:ident, $debug_name:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; KEY_BYTES]);

        impl $name {
            /// 校验并构造非零的 128 位事实键。
            pub const fn new(bytes: [u8; KEY_BYTES]) -> Result<Self, OrganizationProvisioningError> {
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] != 0 {
                        return Ok(Self(bytes));
                    }
                    index += 1;
                }
                Err(OrganizationProvisioningError::InvalidFactKey)
            }

            /// 从数据库固定长度小写十六进制文本恢复事实键。
            pub fn from_persistence_key(
                value: &str,
            ) -> Result<Self, OrganizationProvisioningError> {
                if value.len() != KEY_HEX_BYTES {
                    return Err(OrganizationProvisioningError::InvalidFactKey);
                }
                let mut bytes = [0_u8; KEY_BYTES];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let high = decode_hex(pair[0])
                        .ok_or(OrganizationProvisioningError::InvalidFactKey)?;
                    let low = decode_hex(pair[1])
                        .ok_or(OrganizationProvisioningError::InvalidFactKey)?;
                    bytes[index] = (high << 4) | low;
                }
                Self::new(bytes)
            }

            /// 返回数据库使用的固定长度小写十六进制文本。
            #[must_use]
            pub fn persistence_key(self) -> String {
                encode_hex(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!($debug_name, "(<redacted>)"))
            }
        }
    };
}

provisioning_key!(
    /// 企业开通申请的稳定公开键。
    OrganizationProvisioningRequestKey,
    "OrganizationProvisioningRequestKey"
);
provisioning_key!(
    /// 当前用户范围的企业开通申请幂等键。
    OrganizationProvisioningIdempotencyKey,
    "OrganizationProvisioningIdempotencyKey"
);

/// 企业开通申请闭合状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationProvisioningStatus {
    /// 等待平台管理员审批。
    Pending = 1,
    /// 已批准且完成企业原子开通。
    Approved = 2,
    /// 已由平台管理员拒绝。
    Rejected = 3,
}

impl OrganizationProvisioningStatus {
    /// 返回持久化使用的稳定编码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

impl TryFrom<i16> for OrganizationProvisioningStatus {
    type Error = OrganizationProvisioningError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Pending),
            2 => Ok(Self::Approved),
            3 => Ok(Self::Rejected),
            _ => Err(OrganizationProvisioningError::InvalidStatus),
        }
    }
}

/// 经领域校验的企业开通申请创建事实。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationProvisioningRequest {
    request_key: OrganizationProvisioningRequestKey,
    idempotency_key: OrganizationProvisioningIdempotencyKey,
    applicant_user_id: UserId,
    organization_name: String,
    organization_slug: String,
    business_reason: String,
    created_at: u64,
}

impl OrganizationProvisioningRequest {
    /// 校验并构造当前用户提交的开通申请；申请人必须来自受信鉴权上下文。
    pub fn new(
        request_key: OrganizationProvisioningRequestKey,
        idempotency_key: OrganizationProvisioningIdempotencyKey,
        applicant_user_id: UserId,
        organization_name: String,
        organization_slug: String,
        business_reason: String,
        created_at: u64,
    ) -> Result<Self, OrganizationProvisioningError> {
        if !valid_text(&organization_name, MAX_ORGANIZATION_PROVISIONING_NAME_BYTES) {
            return Err(OrganizationProvisioningError::InvalidOrganizationName);
        }
        if !valid_slug(&organization_slug) {
            return Err(OrganizationProvisioningError::InvalidOrganizationSlug);
        }
        if !valid_text(&business_reason, MAX_ORGANIZATION_PROVISIONING_REASON_BYTES) {
            return Err(OrganizationProvisioningError::InvalidBusinessReason);
        }
        if created_at > i64::MAX as u64 {
            return Err(OrganizationProvisioningError::InvalidTimestamp);
        }
        Ok(Self {
            request_key,
            idempotency_key,
            applicant_user_id,
            organization_name,
            organization_slug,
            business_reason,
            created_at,
        })
    }

    #[must_use]
    pub const fn request_key(&self) -> OrganizationProvisioningRequestKey {
        self.request_key
    }

    #[must_use]
    pub const fn idempotency_key(&self) -> OrganizationProvisioningIdempotencyKey {
        self.idempotency_key
    }

    #[must_use]
    pub const fn applicant_user_id(&self) -> UserId {
        self.applicant_user_id
    }

    #[must_use]
    pub fn organization_name(&self) -> &str {
        &self.organization_name
    }

    #[must_use]
    pub fn organization_slug(&self) -> &str {
        &self.organization_slug
    }

    #[must_use]
    pub fn business_reason(&self) -> &str {
        &self.business_reason
    }

    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
}

impl fmt::Debug for OrganizationProvisioningRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationProvisioningRequest(<redacted>)")
    }
}

/// 已持久化的企业开通申请安全投影。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationProvisioningRecord {
    id: OrganizationProvisioningRequestId,
    request: OrganizationProvisioningRequest,
    status: OrganizationProvisioningStatus,
    version: u64,
    reviewer_user_id: Option<UserId>,
    decision_reason: Option<String>,
    organization_id: Option<OrganizationId>,
    updated_at: u64,
    decided_at: Option<u64>,
}

impl OrganizationProvisioningRecord {
    /// 从数据库恢复申请快照，并复验状态、版本和审批字段组合。
    #[allow(clippy::too_many_arguments)]
    pub fn from_persistence(
        id: OrganizationProvisioningRequestId,
        request: OrganizationProvisioningRequest,
        status: OrganizationProvisioningStatus,
        version: u64,
        reviewer_user_id: Option<UserId>,
        decision_reason: Option<String>,
        organization_id: Option<OrganizationId>,
        updated_at: u64,
        decided_at: Option<u64>,
    ) -> Result<Self, OrganizationProvisioningError> {
        if version == 0 || updated_at < request.created_at() {
            return Err(OrganizationProvisioningError::InvalidState);
        }
        if decision_reason.as_ref().is_some_and(|reason| {
            !valid_text(reason, MAX_ORGANIZATION_PROVISIONING_DECISION_REASON_BYTES)
        }) {
            return Err(OrganizationProvisioningError::InvalidDecisionReason);
        }
        if decided_at.is_some_and(|value| value < request.created_at() || value > updated_at) {
            return Err(OrganizationProvisioningError::InvalidState);
        }
        let valid_shape = match status {
            OrganizationProvisioningStatus::Pending => {
                reviewer_user_id.is_none()
                    && decision_reason.is_none()
                    && organization_id.is_none()
                    && decided_at.is_none()
            }
            OrganizationProvisioningStatus::Approved => {
                reviewer_user_id.is_some() && organization_id.is_some() && decided_at.is_some()
            }
            OrganizationProvisioningStatus::Rejected => {
                reviewer_user_id.is_some()
                    && decision_reason.is_some()
                    && organization_id.is_none()
                    && decided_at.is_some()
            }
        };
        if !valid_shape {
            return Err(OrganizationProvisioningError::InvalidState);
        }
        Ok(Self {
            id,
            request,
            status,
            version,
            reviewer_user_id,
            decision_reason,
            organization_id,
            updated_at,
            decided_at,
        })
    }

    /// 判断持久化事实是否与一次创建重试完全一致。
    #[must_use]
    pub fn matches_create(&self, request: &OrganizationProvisioningRequest) -> bool {
        self.status == OrganizationProvisioningStatus::Pending && self.request == *request
    }

    #[must_use]
    pub const fn id(&self) -> OrganizationProvisioningRequestId {
        self.id
    }

    #[must_use]
    pub const fn request_key(&self) -> OrganizationProvisioningRequestKey {
        self.request.request_key()
    }

    #[must_use]
    pub const fn idempotency_key(&self) -> OrganizationProvisioningIdempotencyKey {
        self.request.idempotency_key()
    }

    #[must_use]
    pub const fn applicant_user_id(&self) -> UserId {
        self.request.applicant_user_id()
    }

    #[must_use]
    pub fn organization_name(&self) -> &str {
        self.request.organization_name()
    }

    #[must_use]
    pub fn organization_slug(&self) -> &str {
        self.request.organization_slug()
    }

    #[must_use]
    pub fn business_reason(&self) -> &str {
        self.request.business_reason()
    }

    #[must_use]
    pub const fn status(&self) -> OrganizationProvisioningStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub const fn reviewer_user_id(&self) -> Option<UserId> {
        self.reviewer_user_id
    }

    #[must_use]
    pub fn decision_reason(&self) -> Option<&str> {
        self.decision_reason.as_deref()
    }

    #[must_use]
    pub const fn organization_id(&self) -> Option<OrganizationId> {
        self.organization_id
    }

    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.request.created_at()
    }

    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    #[must_use]
    pub const fn decided_at(&self) -> Option<u64> {
        self.decided_at
    }
}

impl fmt::Debug for OrganizationProvisioningRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationProvisioningRecord")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 企业开通申请创建结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationProvisioningCreateOutcome {
    Created(OrganizationProvisioningRecord),
    Existing(OrganizationProvisioningRecord),
}

/// 企业开通申请领域错误；不携带企业资料或业务说明原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationProvisioningError {
    #[error("企业开通申请事实键无效")]
    InvalidFactKey,
    #[error("企业名称无效")]
    InvalidOrganizationName,
    #[error("企业规范标识无效")]
    InvalidOrganizationSlug,
    #[error("企业开通业务说明无效")]
    InvalidBusinessReason,
    #[error("企业开通审批理由无效")]
    InvalidDecisionReason,
    #[error("企业开通申请状态无效")]
    InvalidStatus,
    #[error("企业开通申请时间无效")]
    InvalidTimestamp,
    #[error("企业开通申请状态组合无效")]
    InvalidState,
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ORGANIZATION_PROVISIONING_SLUG_BYTES
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
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
    fn request_rejects_non_canonical_input_and_redacts_debug() {
        let user_id = UserId::new(1).unwrap();
        let key = OrganizationProvisioningRequestKey::new([1; KEY_BYTES]).unwrap();
        let idempotency = OrganizationProvisioningIdempotencyKey::new([2; KEY_BYTES]).unwrap();
        assert!(
            OrganizationProvisioningRequest::new(
                key,
                idempotency,
                user_id,
                " Acme".to_owned(),
                "Acme".to_owned(),
                "reason".to_owned(),
                1,
            )
            .is_err()
        );
        let request = OrganizationProvisioningRequest::new(
            key,
            idempotency,
            user_id,
            "Acme".to_owned(),
            "acme-ai".to_owned(),
            "private business reason".to_owned(),
            1,
        )
        .unwrap();
        let rendered = format!("{request:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("private business reason"));
    }

    #[test]
    fn persistence_keys_require_lowercase_non_zero_hex() {
        let key = OrganizationProvisioningRequestKey::new([0xa5; KEY_BYTES]).unwrap();
        assert_eq!(
            OrganizationProvisioningRequestKey::from_persistence_key(&key.persistence_key()),
            Ok(key)
        );
        assert!(
            OrganizationProvisioningRequestKey::from_persistence_key(
                &key.persistence_key().to_ascii_uppercase()
            )
            .is_err()
        );
        assert!(
            OrganizationProvisioningRequestKey::from_persistence_key(&"0".repeat(KEY_HEX_BYTES))
                .is_err()
        );
    }
}
