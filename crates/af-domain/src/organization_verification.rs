use thiserror::Error;

use crate::{
    OrganizationId, OrganizationVerificationCaseId, OrganizationVerificationMaterialId, UserId,
};

/// 企业认证材料类型最大字节数。
pub const MAX_ORGANIZATION_VERIFICATION_MATERIAL_KIND_BYTES: usize = 64;
/// 受控对象存储引用最大字节数；该引用不会直接返回给普通企业接口。
pub const MAX_ORGANIZATION_VERIFICATION_OBJECT_REFERENCE_BYTES: usize = 256;
/// 企业认证摘要最大字节数。
pub const MAX_ORGANIZATION_VERIFICATION_SUMMARY_BYTES: usize = 512;
/// 企业认证审核/补充理由最大字节数。
pub const MAX_ORGANIZATION_VERIFICATION_REVIEW_REASON_BYTES: usize = 512;

/// 企业认证案件生命周期状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationVerificationStatus {
    /// 申请人已提交材料，等待平台处理。
    Submitted = 1,
    /// 平台正在审核材料。
    InReview = 2,
    /// 平台要求申请人补充材料。
    NeedsSupplement = 3,
    /// 企业认证已通过。
    Approved = 4,
    /// 企业认证已驳回。
    Rejected = 5,
}

impl OrganizationVerificationStatus {
    /// 返回数据库使用的稳定编码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

impl TryFrom<i16> for OrganizationVerificationStatus {
    type Error = OrganizationVerificationError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Submitted),
            2 => Ok(Self::InReview),
            3 => Ok(Self::NeedsSupplement),
            4 => Ok(Self::Approved),
            5 => Ok(Self::Rejected),
            _ => Err(OrganizationVerificationError::InvalidStatus),
        }
    }
}

/// 已持久化的企业认证案件安全投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationVerificationCase {
    id: OrganizationVerificationCaseId,
    organization_id: OrganizationId,
    applicant_user_id: UserId,
    status: OrganizationVerificationStatus,
    version: i64,
    summary: String,
    reviewer_user_id: Option<UserId>,
    review_reason: Option<String>,
    created_at: u64,
    updated_at: u64,
    decided_at: Option<u64>,
}

impl OrganizationVerificationCase {
    /// 构造新认证案件；申请人与企业来自已校验的鉴权上下文。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: OrganizationVerificationCaseId,
        organization_id: OrganizationId,
        applicant_user_id: UserId,
        summary: String,
        created_at: u64,
    ) -> Result<Self, OrganizationVerificationError> {
        validate_text(&summary, MAX_ORGANIZATION_VERIFICATION_SUMMARY_BYTES)
            .then_some(())
            .ok_or(OrganizationVerificationError::InvalidSummary)?;
        if created_at > i64::MAX as u64 {
            return Err(OrganizationVerificationError::InvalidTimestamp);
        }
        Ok(Self {
            id,
            organization_id,
            applicant_user_id,
            status: OrganizationVerificationStatus::Submitted,
            version: 1,
            summary,
            reviewer_user_id: None,
            review_reason: None,
            created_at,
            updated_at: created_at,
            decided_at: None,
        })
    }

    /// 从数据库恢复案件并校验审核字段组合。
    #[allow(clippy::too_many_arguments)]
    pub fn from_persistence(
        id: OrganizationVerificationCaseId,
        organization_id: OrganizationId,
        applicant_user_id: UserId,
        status: OrganizationVerificationStatus,
        version: i64,
        summary: String,
        reviewer_user_id: Option<UserId>,
        review_reason: Option<String>,
        created_at: u64,
        updated_at: u64,
        decided_at: Option<u64>,
    ) -> Result<Self, OrganizationVerificationError> {
        if version <= 0 || updated_at < created_at || created_at > i64::MAX as u64 {
            return Err(OrganizationVerificationError::InvalidState);
        }
        validate_text(&summary, MAX_ORGANIZATION_VERIFICATION_SUMMARY_BYTES)
            .then_some(())
            .ok_or(OrganizationVerificationError::InvalidSummary)?;
        if review_reason.as_deref().is_some_and(|value| {
            !validate_text(value, MAX_ORGANIZATION_VERIFICATION_REVIEW_REASON_BYTES)
        }) {
            return Err(OrganizationVerificationError::InvalidReviewReason);
        }
        if decided_at.is_some_and(|value| value < created_at || value > updated_at) {
            return Err(OrganizationVerificationError::InvalidState);
        }
        let valid_shape = match status {
            OrganizationVerificationStatus::Submitted
            | OrganizationVerificationStatus::InReview => {
                reviewer_user_id.is_none() && review_reason.is_none() && decided_at.is_none()
            }
            OrganizationVerificationStatus::NeedsSupplement => {
                reviewer_user_id.is_some() && review_reason.is_some() && decided_at.is_some()
            }
            OrganizationVerificationStatus::Approved | OrganizationVerificationStatus::Rejected => {
                reviewer_user_id.is_some() && decided_at.is_some()
            }
        };
        if !valid_shape {
            return Err(OrganizationVerificationError::InvalidState);
        }
        Ok(Self {
            id,
            organization_id,
            applicant_user_id,
            status,
            version,
            summary,
            reviewer_user_id,
            review_reason,
            created_at,
            updated_at,
            decided_at,
        })
    }

    #[must_use]
    pub const fn id(&self) -> OrganizationVerificationCaseId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn applicant_user_id(&self) -> UserId {
        self.applicant_user_id
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationVerificationStatus {
        self.status
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }
    #[must_use]
    pub const fn reviewer_user_id(&self) -> Option<UserId> {
        self.reviewer_user_id
    }
    #[must_use]
    pub fn review_reason(&self) -> Option<&str> {
        self.review_reason.as_deref()
    }
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
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

/// 企业认证材料的安全元数据；正文仅由受控对象存储持有。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationVerificationMaterial {
    id: OrganizationVerificationMaterialId,
    case_id: OrganizationVerificationCaseId,
    kind: String,
    object_reference: String,
    file_name: String,
    content_type: String,
    size_bytes: i64,
    content_available: bool,
    created_at: u64,
}

impl OrganizationVerificationMaterial {
    /// 构造材料元数据，不接受文件正文。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: OrganizationVerificationMaterialId,
        case_id: OrganizationVerificationCaseId,
        kind: String,
        object_reference: String,
        file_name: String,
        content_type: String,
        size_bytes: i64,
        content_available: bool,
        created_at: u64,
    ) -> Result<Self, OrganizationVerificationError> {
        for (value, max, error) in [
            (
                &kind,
                MAX_ORGANIZATION_VERIFICATION_MATERIAL_KIND_BYTES,
                OrganizationVerificationError::InvalidMaterialKind,
            ),
            (
                &object_reference,
                MAX_ORGANIZATION_VERIFICATION_OBJECT_REFERENCE_BYTES,
                OrganizationVerificationError::InvalidObjectReference,
            ),
            (
                &file_name,
                255,
                OrganizationVerificationError::InvalidFileName,
            ),
            (
                &content_type,
                128,
                OrganizationVerificationError::InvalidContentType,
            ),
        ] {
            validate_text(value, max).then_some(()).ok_or(error)?;
        }
        if !(0..=50_000_000).contains(&size_bytes) || created_at > i64::MAX as u64 {
            return Err(OrganizationVerificationError::InvalidMaterial);
        }
        Ok(Self {
            id,
            case_id,
            kind,
            object_reference,
            file_name,
            content_type,
            size_bytes,
            content_available,
            created_at,
        })
    }

    #[must_use]
    pub const fn id(&self) -> OrganizationVerificationMaterialId {
        self.id
    }
    #[must_use]
    pub const fn case_id(&self) -> OrganizationVerificationCaseId {
        self.case_id
    }
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }
    #[must_use]
    pub fn object_reference(&self) -> &str {
        &self.object_reference
    }
    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }
    #[must_use]
    pub fn content_type(&self) -> &str {
        &self.content_type
    }
    #[must_use]
    pub const fn size_bytes(&self) -> i64 {
        self.size_bytes
    }
    #[must_use]
    pub const fn content_available(&self) -> bool {
        self.content_available
    }
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
}

/// 企业认证事实错误；错误不携带材料内容或对象存储引用。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationVerificationError {
    #[error("企业认证状态无效")]
    InvalidStatus,
    #[error("企业认证状态组合无效")]
    InvalidState,
    #[error("企业认证摘要无效")]
    InvalidSummary,
    #[error("企业认证审核理由无效")]
    InvalidReviewReason,
    #[error("企业认证材料类型无效")]
    InvalidMaterialKind,
    #[error("企业认证对象引用无效")]
    InvalidObjectReference,
    #[error("企业认证文件名无效")]
    InvalidFileName,
    #[error("企业认证 MIME 类型无效")]
    InvalidContentType,
    #[error("企业认证材料无效")]
    InvalidMaterial,
    #[error("企业认证时间无效")]
    InvalidTimestamp,
}

fn validate_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
