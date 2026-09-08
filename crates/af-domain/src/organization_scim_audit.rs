use std::fmt;

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    OrganizationId, OrganizationScimResourceId, OrganizationScimResourceKind,
    OrganizationScimResourceVersion, OrganizationScimTokenKey,
};

const RESOURCE_DIGEST_CONTEXT: &[u8] = b"anyflows-enterprise-scim-resource-audit-v1";
const DIGEST_HEX_BYTES: usize = 64;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// SCIM 审计事件的闭合类型集合。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationScimAuditEventType {
    /// 创建令牌的管理事实。
    TokenIssued,
    /// 撤销令牌的管理事实。
    TokenRevoked,
    /// 令牌认证成功的请求事实。
    RequestAccepted,
    /// 令牌或请求校验拒绝事实。
    RequestRejected,
    /// 用户或组资源同步事实。
    ResourceSynchronized,
    /// 用户生命周期变更事实。
    LifecycleTransition,
    /// 固定窗口限流拒绝事实。
    RateLimitRejected,
}

impl OrganizationScimAuditEventType {
    /// 返回供持久化和指标使用的稳定事件码。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::TokenIssued => "token_issued",
            Self::TokenRevoked => "token_revoked",
            Self::RequestAccepted => "request_accepted",
            Self::RequestRejected => "request_rejected",
            Self::ResourceSynchronized => "resource_synchronized",
            Self::LifecycleTransition => "lifecycle_transition",
            Self::RateLimitRejected => "rate_limit_rejected",
        }
    }
}

/// SCIM 审计结果；不把上游正文或错误文本写入事实。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationScimAuditResult {
    /// 请求产生了新的持久化事实。
    Applied,
    /// 请求是已保存事实的幂等重放。
    Noop,
    /// 请求被权限、边界、版本或限流规则拒绝。
    Rejected,
    /// 请求进入处理流程但因内部故障未完成。
    Failed,
}

impl OrganizationScimAuditResult {
    /// 返回供持久化和指标使用的稳定结果码。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Noop => "noop",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
        }
    }
}

/// SCIM 审计拒绝/失败原因；使用枚举避免自由文本泄露。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationScimAuditReason {
    InvalidToken,
    TokenRevoked,
    TokenExpired,
    CrossOrganization,
    StaleResource,
    EntitlementUnavailable,
    SeatCapacityExceeded,
    RateLimitExceeded,
    StorageUnavailable,
    InvalidRequest,
    InternalFailure,
}

impl OrganizationScimAuditReason {
    /// 返回供持久化和指标使用的稳定原因码。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidToken => "invalid_token",
            Self::TokenRevoked => "token_revoked",
            Self::TokenExpired => "token_expired",
            Self::CrossOrganization => "cross_organization",
            Self::StaleResource => "stale_resource",
            Self::EntitlementUnavailable => "entitlement_unavailable",
            Self::SeatCapacityExceeded => "seat_capacity_exceeded",
            Self::RateLimitExceeded => "rate_limit_exceeded",
            Self::StorageUnavailable => "storage_unavailable",
            Self::InvalidRequest => "invalid_request",
            Self::InternalFailure => "internal_failure",
        }
    }
}

/// 资源 ID 的不可逆审计摘要；审计事实不会保存资源原文。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationScimResourceDigest([u8; 32]);

impl OrganizationScimResourceDigest {
    /// 使用企业、资源种类和资源标识计算审计摘要；资源原文只在本次调用栈短暂存在。
    #[must_use]
    pub fn from_resource(
        organization_id: OrganizationId,
        kind: OrganizationScimResourceKind,
        resource_id: &OrganizationScimResourceId,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(RESOURCE_DIGEST_CONTEXT);
        hasher.update([0]);
        hasher.update(organization_id.get().to_be_bytes());
        hasher.update([match kind {
            OrganizationScimResourceKind::Users => 1,
            OrganizationScimResourceKind::Groups => 2,
        }]);
        hasher.update(resource_id.as_str().as_bytes());
        Self(hasher.finalize().into())
    }

    /// 从持久化摘要恢复固定 256 位值。
    pub fn from_persistence_key(value: &str) -> Result<Self, OrganizationScimAuditFactsError> {
        if value.len() != DIGEST_HEX_BYTES {
            return Err(OrganizationScimAuditFactsError::InvalidDigest);
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex(pair[0]).ok_or(OrganizationScimAuditFactsError::InvalidDigest)?;
            let low = decode_hex(pair[1]).ok_or(OrganizationScimAuditFactsError::InvalidDigest)?;
            bytes[index] = (high << 4) | low;
        }
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(OrganizationScimAuditFactsError::InvalidDigest);
        }
        Ok(Self(bytes))
    }

    /// 返回固定摘要字节，供存储适配器写入。
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// 返回小写十六进制持久化文本。
    #[must_use]
    pub fn persistence_key(&self) -> String {
        encode_hex(&self.0)
    }
}

impl fmt::Debug for OrganizationScimResourceDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimResourceDigest(<redacted>)")
    }
}

/// 请求关联指纹；调用方只能提交已计算的固定摘要，不得把请求正文放入审计。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationScimRequestFingerprint([u8; 32]);

impl OrganizationScimRequestFingerprint {
    /// 从固定长度摘要构造指纹，拒绝全零占位值。
    pub const fn new(bytes: [u8; 32]) -> Result<Self, OrganizationScimAuditFactsError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(OrganizationScimAuditFactsError::InvalidRequestFingerprint)
    }

    /// 返回固定摘要字节，供存储适配器写入。
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for OrganizationScimRequestFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimRequestFingerprint(<redacted>)")
    }
}

/// SCIM 审计中可记录的资源投影；不包含资源原文、邮箱或个人空间身份。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationScimAuditResource {
    kind: OrganizationScimResourceKind,
    digest: OrganizationScimResourceDigest,
    version: OrganizationScimResourceVersion,
}

impl OrganizationScimAuditResource {
    /// 组合资源种类、不可逆标识摘要和已校验版本。
    #[must_use]
    pub const fn new(
        kind: OrganizationScimResourceKind,
        digest: OrganizationScimResourceDigest,
        version: OrganizationScimResourceVersion,
    ) -> Self {
        Self {
            kind,
            digest,
            version,
        }
    }

    /// 返回固定资源种类。
    #[must_use]
    pub const fn kind(&self) -> OrganizationScimResourceKind {
        self.kind
    }

    /// 返回不可逆资源摘要。
    #[must_use]
    pub fn digest(&self) -> &OrganizationScimResourceDigest {
        &self.digest
    }

    /// 返回单调资源版本。
    #[must_use]
    pub const fn version(&self) -> OrganizationScimResourceVersion {
        self.version
    }
}

/// SCIM 审计事实构造错误；错误不携带任何外部原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimAuditFactsError {
    /// 请求指纹缺失或为全零占位值。
    #[error("企业 SCIM 审计请求指纹无效")]
    InvalidRequestFingerprint,
    /// 资源摘要不是固定的小写十六进制值。
    #[error("企业 SCIM 审计资源摘要无效")]
    InvalidDigest,
    /// 事件与资源投影形状不一致。
    #[error("企业 SCIM 审计资源投影无效")]
    InvalidResourceProjection,
    /// 事件、结果、原因或限流元数据组合不一致。
    #[error("企业 SCIM 审计结果组合无效")]
    InvalidResultShape,
}

/// 已脱敏、可追加的 SCIM 审计事实。
pub struct OrganizationScimAuditFacts {
    organization_id: OrganizationId,
    token_key: Option<OrganizationScimTokenKey>,
    event_type: OrganizationScimAuditEventType,
    result: OrganizationScimAuditResult,
    resource: Option<OrganizationScimAuditResource>,
    request_fingerprint: OrganizationScimRequestFingerprint,
    reason: Option<OrganizationScimAuditReason>,
    retry_after_seconds: Option<u32>,
}

impl OrganizationScimAuditFacts {
    /// 组合企业范围审计事实；所有敏感数据必须在调用方先降级为摘要或枚举。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        token_key: Option<OrganizationScimTokenKey>,
        event_type: OrganizationScimAuditEventType,
        result: OrganizationScimAuditResult,
        resource: Option<OrganizationScimAuditResource>,
        request_fingerprint: OrganizationScimRequestFingerprint,
        reason: Option<OrganizationScimAuditReason>,
        retry_after_seconds: Option<u32>,
    ) -> Result<Self, OrganizationScimAuditFactsError> {
        let is_rejected = matches!(
            result,
            OrganizationScimAuditResult::Rejected | OrganizationScimAuditResult::Failed
        );
        if is_rejected != reason.is_some() {
            return Err(OrganizationScimAuditFactsError::InvalidResultShape);
        }
        if retry_after_seconds.is_some()
            && (!matches!(
                event_type,
                OrganizationScimAuditEventType::RateLimitRejected
            ) || !matches!(result, OrganizationScimAuditResult::Rejected))
        {
            return Err(OrganizationScimAuditFactsError::InvalidResultShape);
        }
        let requires_resource = matches!(
            event_type,
            OrganizationScimAuditEventType::ResourceSynchronized
                | OrganizationScimAuditEventType::LifecycleTransition
        );
        if requires_resource != resource.is_some() {
            return Err(OrganizationScimAuditFactsError::InvalidResourceProjection);
        }
        if matches!(
            event_type,
            OrganizationScimAuditEventType::RateLimitRejected
        ) && (resource.is_some() || !matches!(result, OrganizationScimAuditResult::Rejected))
        {
            return Err(OrganizationScimAuditFactsError::InvalidResultShape);
        }
        Ok(Self {
            organization_id,
            token_key,
            event_type,
            result,
            resource,
            request_fingerprint,
            reason,
            retry_after_seconds,
        })
    }

    /// 返回审计所属企业。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回已脱敏令牌公开标识。
    #[must_use]
    pub fn token_key(&self) -> Option<&OrganizationScimTokenKey> {
        self.token_key.as_ref()
    }

    /// 返回审计事件类型。
    #[must_use]
    pub const fn event_type(&self) -> OrganizationScimAuditEventType {
        self.event_type
    }

    /// 返回审计结果。
    #[must_use]
    pub const fn result(&self) -> OrganizationScimAuditResult {
        self.result
    }

    /// 返回脱敏资源投影。
    #[must_use]
    pub fn resource(&self) -> Option<&OrganizationScimAuditResource> {
        self.resource.as_ref()
    }

    /// 返回请求固定指纹。
    #[must_use]
    pub const fn request_fingerprint(&self) -> &OrganizationScimRequestFingerprint {
        &self.request_fingerprint
    }

    /// 返回固定原因码。
    #[must_use]
    pub const fn reason(&self) -> Option<OrganizationScimAuditReason> {
        self.reason
    }

    /// 返回限流拒绝时的等待秒数。
    #[must_use]
    pub const fn retry_after_seconds(&self) -> Option<u32> {
        self.retry_after_seconds
    }
}

impl fmt::Debug for OrganizationScimAuditFacts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimAuditFacts")
            .field("organization_id", &self.organization_id)
            .field("token_key", &self.token_key)
            .field("event_type", &self.event_type)
            .field("result", &self.result)
            .field("resource", &self.resource)
            .field("request_fingerprint", &self.request_fingerprint)
            .field("reason", &self.reason)
            .field("retry_after_seconds", &self.retry_after_seconds)
            .finish()
    }
}

fn decode_hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn encode_hex(bytes: &[u8; 32]) -> String {
    let mut encoded = String::with_capacity(DIGEST_HEX_BYTES);
    for byte in bytes {
        encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}
