use std::fmt;

use thiserror::Error;

use crate::{OrganizationApprovalTemplateId, OrganizationId, UserId};

const KEY_BYTES: usize = 16;
const KEY_HEX_BYTES: usize = KEY_BYTES * 2;
const DIGEST_BYTES: usize = 32;
const DIGEST_HEX_BYTES: usize = DIGEST_BYTES * 2;
const MAX_TEMPLATE_KEY_BYTES: usize = 64;
const MAX_RESOURCE_SUMMARY_BYTES: usize = 1024;
const MAX_DECISION_REASON_BYTES: usize = 512;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 企业审批需要保护的固定资源类型。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationApprovalResourceType {
    Member = 1,
    Budget = 2,
    ServiceAccount = 3,
    ContractPrice = 4,
    Funds = 5,
}

impl OrganizationApprovalResourceType {
    /// 返回持久化使用的稳定编码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

impl TryFrom<i16> for OrganizationApprovalResourceType {
    type Error = OrganizationApprovalError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Member),
            2 => Ok(Self::Budget),
            3 => Ok(Self::ServiceAccount),
            4 => Ok(Self::ContractPrice),
            5 => Ok(Self::Funds),
            _ => Err(OrganizationApprovalError::InvalidResourceType),
        }
    }
}

/// 审批模板版本的稳定公开键。
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationApprovalTemplateKey(String);

impl OrganizationApprovalTemplateKey {
    /// 校验并构造模板键；键只用于稳定寻址，不承载模板正文。
    pub fn new(value: impl Into<String>) -> Result<Self, OrganizationApprovalError> {
        let value = value.into();
        if !valid_text(&value, MAX_TEMPLATE_KEY_BYTES) {
            return Err(OrganizationApprovalError::InvalidTemplateKey);
        }
        Ok(Self(value))
    }

    /// 返回持久化使用的模板键。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OrganizationApprovalTemplateKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationApprovalTemplateKey(<redacted>)")
    }
}

/// 模板定义的不可变摘要；不把审批规则正文写入本切片的公开事实。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct OrganizationApprovalTemplateDigest([u8; DIGEST_BYTES]);

impl OrganizationApprovalTemplateDigest {
    /// 校验并构造非零摘要。
    pub const fn new(bytes: [u8; DIGEST_BYTES]) -> Result<Self, OrganizationApprovalError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(OrganizationApprovalError::InvalidTemplateDigest)
    }

    /// 从数据库固定长度小写十六进制摘要恢复值。
    pub fn from_persistence_key(value: &str) -> Result<Self, OrganizationApprovalError> {
        if value.len() != DIGEST_HEX_BYTES {
            return Err(OrganizationApprovalError::InvalidTemplateDigest);
        }
        let mut bytes = [0_u8; DIGEST_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high =
                decode_hex(pair[0]).ok_or(OrganizationApprovalError::InvalidTemplateDigest)?;
            let low =
                decode_hex(pair[1]).ok_or(OrganizationApprovalError::InvalidTemplateDigest)?;
            bytes[index] = (high << 4) | low;
        }
        Self::new(bytes)
    }

    /// 返回数据库使用的固定长度小写十六进制摘要。
    #[must_use]
    pub fn persistence_key(self) -> String {
        encode_hex(&self.0)
    }
}

impl fmt::Debug for OrganizationApprovalTemplateDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationApprovalTemplateDigest(<redacted>)")
    }
}

macro_rules! approval_key {
    ($(#[$meta:meta])* $name:ident, $debug_name:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; KEY_BYTES]);

        impl $name {
            /// 校验并构造非零的一次性事实键。
            pub const fn new(bytes: [u8; KEY_BYTES]) -> Result<Self, OrganizationApprovalError> {
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] != 0 {
                        return Ok(Self(bytes));
                    }
                    index += 1;
                }
                Err(OrganizationApprovalError::InvalidFactKey)
            }

            /// 从数据库固定键恢复事实标识。
            pub fn from_persistence_key(value: &str) -> Result<Self, OrganizationApprovalError> {
                if value.len() != KEY_HEX_BYTES {
                    return Err(OrganizationApprovalError::InvalidFactKey);
                }
                let mut bytes = [0_u8; KEY_BYTES];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let high = decode_hex(pair[0]).ok_or(OrganizationApprovalError::InvalidFactKey)?;
                    let low = decode_hex(pair[1]).ok_or(OrganizationApprovalError::InvalidFactKey)?;
                    bytes[index] = (high << 4) | low;
                }
                Self::new(bytes)
            }

            /// 返回数据库使用的固定长度小写十六进制键。
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

approval_key!(
    /// 企业审批申请的稳定幂等键。
    OrganizationApprovalRequestKey,
    "OrganizationApprovalRequestKey"
);
approval_key!(
    /// 企业审批决定的稳定幂等键。
    OrganizationApprovalDecisionKey,
    "OrganizationApprovalDecisionKey"
);

/// 审批决定的闭合集合；超时也必须留下服务端事实。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationApprovalDecisionKind {
    Approved = 1,
    Rejected = 2,
    TimedOut = 3,
}

impl OrganizationApprovalDecisionKind {
    /// 返回持久化使用的稳定编码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

impl TryFrom<i16> for OrganizationApprovalDecisionKind {
    type Error = OrganizationApprovalError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Approved),
            2 => Ok(Self::Rejected),
            3 => Ok(Self::TimedOut),
            _ => Err(OrganizationApprovalError::InvalidDecisionKind),
        }
    }
}

/// 企业审批领域错误；不携带资源摘要、模板键或理由原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationApprovalError {
    #[error("企业审批事实键无效")]
    InvalidFactKey,
    #[error("企业审批模板键无效")]
    InvalidTemplateKey,
    #[error("企业审批模板摘要无效")]
    InvalidTemplateDigest,
    #[error("企业审批资源类型无效")]
    InvalidResourceType,
    #[error("企业审批模板版本无效")]
    InvalidTemplateVersion,
    #[error("企业审批资源摘要无效")]
    InvalidResourceSummary,
    #[error("企业审批决定理由无效")]
    InvalidDecisionReason,
    #[error("企业审批时间无效")]
    InvalidTime,
    #[error("企业审批发起人无效")]
    InvalidInitiator,
    #[error("企业审批决定人无效")]
    InvalidDecider,
}

/// 已固化的企业审批模板版本事实。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalTemplateSnapshot {
    organization_id: OrganizationId,
    template_key: OrganizationApprovalTemplateKey,
    resource_type: OrganizationApprovalResourceType,
    version: i64,
    definition_digest: OrganizationApprovalTemplateDigest,
    created_by_user_id: UserId,
}

impl OrganizationApprovalTemplateSnapshot {
    /// 构造模板版本事实；模板正文由摘要在本切片外管理。
    pub fn new(
        organization_id: OrganizationId,
        template_key: OrganizationApprovalTemplateKey,
        resource_type: OrganizationApprovalResourceType,
        version: i64,
        definition_digest: OrganizationApprovalTemplateDigest,
        created_by_user_id: UserId,
    ) -> Result<Self, OrganizationApprovalError> {
        if version <= 0 {
            return Err(OrganizationApprovalError::InvalidTemplateVersion);
        }
        Ok(Self {
            organization_id,
            template_key,
            resource_type,
            version,
            definition_digest,
            created_by_user_id,
        })
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub fn template_key(&self) -> &OrganizationApprovalTemplateKey {
        &self.template_key
    }
    #[must_use]
    pub const fn resource_type(&self) -> OrganizationApprovalResourceType {
        self.resource_type
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn definition_digest(&self) -> OrganizationApprovalTemplateDigest {
        self.definition_digest
    }
    #[must_use]
    pub const fn created_by_user_id(&self) -> UserId {
        self.created_by_user_id
    }
}

/// 企业审批申请事实；资源摘要只允许受限文本，不替代具体资源事实。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationApprovalRequest {
    request_key: OrganizationApprovalRequestKey,
    organization_id: OrganizationId,
    resource_type: OrganizationApprovalResourceType,
    resource_summary: String,
    template_id: OrganizationApprovalTemplateId,
    template_version: i64,
    requested_by_user_id: UserId,
    expires_at: i64,
}

impl fmt::Debug for OrganizationApprovalRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationApprovalRequest")
            .field("request_key", &self.request_key)
            .field("organization_id", &self.organization_id)
            .field("resource_type", &self.resource_type)
            .field("template_id", &self.template_id)
            .field("template_version", &self.template_version)
            .field("requested_by_user_id", &self.requested_by_user_id)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl OrganizationApprovalRequest {
    /// 构造带服务端过期边界的申请事实。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_key: OrganizationApprovalRequestKey,
        organization_id: OrganizationId,
        resource_type: OrganizationApprovalResourceType,
        resource_summary: String,
        template_id: OrganizationApprovalTemplateId,
        template_version: i64,
        requested_by_user_id: UserId,
        expires_at: i64,
    ) -> Result<Self, OrganizationApprovalError> {
        if !valid_text(&resource_summary, MAX_RESOURCE_SUMMARY_BYTES) {
            return Err(OrganizationApprovalError::InvalidResourceSummary);
        }
        if template_version <= 0 {
            return Err(OrganizationApprovalError::InvalidTemplateVersion);
        }
        if expires_at < 0 {
            return Err(OrganizationApprovalError::InvalidTime);
        }
        Ok(Self {
            request_key,
            organization_id,
            resource_type,
            resource_summary,
            template_id,
            template_version,
            requested_by_user_id,
            expires_at,
        })
    }

    #[must_use]
    pub const fn request_key(&self) -> OrganizationApprovalRequestKey {
        self.request_key
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn resource_type(&self) -> OrganizationApprovalResourceType {
        self.resource_type
    }
    #[must_use]
    pub fn resource_summary(&self) -> &str {
        &self.resource_summary
    }
    #[must_use]
    pub const fn template_id(&self) -> OrganizationApprovalTemplateId {
        self.template_id
    }
    #[must_use]
    pub const fn template_version(&self) -> i64 {
        self.template_version
    }
    #[must_use]
    pub const fn requested_by_user_id(&self) -> UserId {
        self.requested_by_user_id
    }
    #[must_use]
    pub const fn expires_at(&self) -> i64 {
        self.expires_at
    }
}

/// 一次性企业审批决定事实；超时决定没有自然人决定人。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationApprovalDecision {
    decision_key: OrganizationApprovalDecisionKey,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    kind: OrganizationApprovalDecisionKind,
    decided_by_user_id: Option<UserId>,
    reason: Option<String>,
    decided_at: i64,
}

impl fmt::Debug for OrganizationApprovalDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationApprovalDecision")
            .field("decision_key", &self.decision_key)
            .field("organization_id", &self.organization_id)
            .field("request_id", &self.request_id)
            .field("kind", &self.kind)
            .field("decided_by_user_id", &self.decided_by_user_id)
            .field("decided_at", &self.decided_at)
            .finish_non_exhaustive()
    }
}

impl OrganizationApprovalDecision {
    /// 构造单次决定事实并校验决定人/理由边界。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        decision_key: OrganizationApprovalDecisionKey,
        organization_id: OrganizationId,
        request_id: OrganizationApprovalRequestId,
        kind: OrganizationApprovalDecisionKind,
        decided_by_user_id: Option<UserId>,
        reason: Option<String>,
        decided_at: i64,
    ) -> Result<Self, OrganizationApprovalError> {
        if matches!(kind, OrganizationApprovalDecisionKind::TimedOut) {
            if decided_by_user_id.is_some() {
                return Err(OrganizationApprovalError::InvalidDecider);
            }
        } else if decided_by_user_id.is_none() {
            return Err(OrganizationApprovalError::InvalidDecider);
        }
        if reason
            .as_deref()
            .is_some_and(|value| !valid_text(value, MAX_DECISION_REASON_BYTES))
        {
            return Err(OrganizationApprovalError::InvalidDecisionReason);
        }
        if decided_at < 0 {
            return Err(OrganizationApprovalError::InvalidTime);
        }
        Ok(Self {
            decision_key,
            organization_id,
            request_id,
            kind,
            decided_by_user_id,
            reason,
            decided_at,
        })
    }

    #[must_use]
    pub const fn decision_key(&self) -> OrganizationApprovalDecisionKey {
        self.decision_key
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn request_id(&self) -> OrganizationApprovalRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn kind(&self) -> OrganizationApprovalDecisionKind {
        self.kind
    }
    #[must_use]
    pub const fn decided_by_user_id(&self) -> Option<UserId> {
        self.decided_by_user_id
    }
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
    #[must_use]
    pub const fn decided_at(&self) -> i64 {
        self.decided_at
    }
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn decode_hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OrganizationApprovalDecisionId, OrganizationApprovalRequestId};

    #[test]
    fn keys_and_digest_round_trip_without_debug_material() {
        let key = OrganizationApprovalRequestKey::new([1; KEY_BYTES]).unwrap();
        let digest = OrganizationApprovalTemplateDigest::new([2; DIGEST_BYTES]).unwrap();
        assert_eq!(
            OrganizationApprovalRequestKey::from_persistence_key(&key.persistence_key()),
            Ok(key)
        );
        assert_eq!(
            OrganizationApprovalTemplateDigest::from_persistence_key(&digest.persistence_key()),
            Ok(digest)
        );
        assert!(format!("{key:?}").contains("<redacted>"));
        assert!(format!("{digest:?}").contains("<redacted>"));
    }

    #[test]
    fn decision_requires_actor_except_timeout() {
        let organization_id = OrganizationId::new(1).unwrap();
        let request_id = OrganizationApprovalRequestId::new(2).unwrap();
        let key = OrganizationApprovalDecisionKey::new([3; KEY_BYTES]).unwrap();
        assert!(
            OrganizationApprovalDecision::new(
                key,
                organization_id,
                request_id,
                OrganizationApprovalDecisionKind::Approved,
                None,
                None,
                10,
            )
            .is_err()
        );
        assert!(
            OrganizationApprovalDecision::new(
                key,
                organization_id,
                request_id,
                OrganizationApprovalDecisionKind::TimedOut,
                None,
                None,
                10,
            )
            .is_ok()
        );
        let _ = OrganizationApprovalDecisionId::new(1);
    }
}
