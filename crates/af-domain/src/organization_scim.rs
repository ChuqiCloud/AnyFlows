use std::{collections::HashSet, fmt};

use crate::{
    OrganizationId, organization::OrganizationMembershipStatus,
    organization_entitlement::OrganizationEntitlementStatus,
};
use getrandom::fill;
use sha2::{Digest, Sha256};
use thiserror::Error;

const SCIM_TOKEN_ENTROPY_BYTES: usize = 32;
const SCIM_TOKEN_PREFIX: &str = "scim_";
const SCIM_TOKEN_HEX_BYTES: usize = SCIM_TOKEN_ENTROPY_BYTES * 2;
const SCIM_TOKEN_TOTAL_BYTES: usize = SCIM_TOKEN_PREFIX.len() + SCIM_TOKEN_HEX_BYTES;
const SCIM_TOKEN_DIGEST_CONTEXT: &[u8] = b"anyflows-enterprise-scim-token-v1";
const SCIM_TOKEN_KEY_BYTES: usize = 16;
const SCIM_TOKEN_KEY_HEX_BYTES: usize = SCIM_TOKEN_KEY_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// SCIM 外部资源标识的最大字节数；更大的标识不能进入同步边界。
pub const MAX_ORGANIZATION_SCIM_RESOURCE_ID_BYTES: usize = 256;
/// SCIM 用户名和组名的最大字节数。
pub const MAX_ORGANIZATION_SCIM_DISPLAY_NAME_BYTES: usize = 256;
/// 单个 SCIM 组允许携带的成员引用数量上限。
pub const MAX_ORGANIZATION_SCIM_GROUP_MEMBER_COUNT: usize = 1024;

/// 企业 SCIM 令牌构造错误；错误中不携带令牌或摘要原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimTokenError {
    /// 令牌不是固定前缀加 256 位小写十六进制随机数。
    #[error("企业 SCIM 令牌格式无效")]
    InvalidEncoding,
    /// 操作系统无法提供安全随机数。
    #[error("企业 SCIM 令牌随机数不可用")]
    EntropyUnavailable,
}

/// 仅在内存中保留的高熵 SCIM 令牌；持久化层只能保存摘要。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationScimToken(String);

impl OrganizationScimToken {
    /// 生成带固定前缀的 256 位随机令牌。
    pub fn generate() -> Result<Self, OrganizationScimTokenError> {
        let mut bytes = [0_u8; SCIM_TOKEN_ENTROPY_BYTES];
        fill(&mut bytes).map_err(|_| OrganizationScimTokenError::EntropyUnavailable)?;
        let mut value = String::with_capacity(SCIM_TOKEN_TOTAL_BYTES);
        value.push_str(SCIM_TOKEN_PREFIX);
        for byte in bytes {
            value.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            value.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        Self::new(value)
    }

    /// 从受控输入恢复令牌；调用方不得将返回值写入数据库或日志。
    pub fn new(value: impl Into<String>) -> Result<Self, OrganizationScimTokenError> {
        let value = value.into();
        let Some(entropy) = value.strip_prefix(SCIM_TOKEN_PREFIX) else {
            return Err(OrganizationScimTokenError::InvalidEncoding);
        };
        if value.len() != SCIM_TOKEN_TOTAL_BYTES
            || entropy.len() != SCIM_TOKEN_HEX_BYTES
            || !entropy
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || entropy.bytes().all(|byte| byte == b'0')
        {
            return Err(OrganizationScimTokenError::InvalidEncoding);
        }
        Ok(Self(value))
    }

    /// 返回仅用于一次创建响应或摘要计算的令牌原文。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 返回可展示的固定前缀，不包含随机材料。
    #[must_use]
    pub const fn prefix(&self) -> &'static str {
        SCIM_TOKEN_PREFIX
    }

    /// 计算带领域分隔的 SHA-256 摘要；数据库只保存该摘要文本。
    #[must_use]
    pub fn digest(&self) -> OrganizationScimTokenDigest {
        let mut hasher = Sha256::new();
        hasher.update(SCIM_TOKEN_DIGEST_CONTEXT);
        hasher.update([0]);
        hasher.update(self.0.as_bytes());
        OrganizationScimTokenDigest::from_bytes(hasher.finalize().into())
    }
}

impl fmt::Debug for OrganizationScimToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimToken(<redacted>)")
    }
}

/// SCIM 令牌的固定 256 位持久化摘要。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationScimTokenDigest(String);

impl OrganizationScimTokenDigest {
    fn from_bytes(bytes: [u8; 32]) -> Self {
        let mut value = String::with_capacity(64);
        for byte in bytes {
            value.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            value.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        Self(value)
    }

    /// 从数据库恢复规范摘要文本。
    pub fn from_persistence_key(
        value: impl Into<String>,
    ) -> Result<Self, OrganizationScimTokenError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(OrganizationScimTokenError::InvalidEncoding);
        }
        Ok(Self(value))
    }

    /// 返回数据库查询使用的摘要文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OrganizationScimTokenDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimTokenDigest(<redacted>)")
    }
}

/// 不暴露数据库自增序号的 SCIM 令牌公开标识。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationScimTokenKey(String);

impl OrganizationScimTokenKey {
    /// 生成非零随机令牌标识。
    pub fn generate() -> Result<Self, OrganizationScimTokenError> {
        let mut bytes = [0_u8; SCIM_TOKEN_KEY_BYTES];
        fill(&mut bytes).map_err(|_| OrganizationScimTokenError::EntropyUnavailable)?;
        let mut value = String::with_capacity(SCIM_TOKEN_KEY_HEX_BYTES);
        for byte in bytes {
            value.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            value.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        Self::new(value)
    }

    /// 从持久化文本恢复公开标识。
    pub fn new(value: impl Into<String>) -> Result<Self, OrganizationScimTokenError> {
        let value = value.into();
        if value.len() != SCIM_TOKEN_KEY_HEX_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || value.bytes().all(|byte| byte == b'0')
        {
            return Err(OrganizationScimTokenError::InvalidEncoding);
        }
        Ok(Self(value))
    }

    /// 返回接口与数据库共用的稳定文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OrganizationScimTokenKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimTokenKey(<redacted>)")
    }
}

/// SCIM 目录资源的固定种类，不允许扩展为任意路径。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationScimResourceKind {
    /// 企业成员资源。
    Users,
    /// 企业团队资源。
    Groups,
}

/// 令牌允许访问的企业目录资源集合。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OrganizationScimResourceSet(u8);

impl OrganizationScimResourceSet {
    const USERS: u8 = 0b01;
    const GROUPS: u8 = 0b10;

    /// 仅允许用户资源。
    #[must_use]
    pub const fn users() -> Self {
        Self(Self::USERS)
    }

    /// 仅允许组资源。
    #[must_use]
    pub const fn groups() -> Self {
        Self(Self::GROUPS)
    }

    /// 同时允许用户和组资源。
    #[must_use]
    pub const fn users_and_groups() -> Self {
        Self(Self::USERS | Self::GROUPS)
    }

    /// 从持久化位掩码恢复资源集合，拒绝未知位和空集合。
    pub const fn from_code(code: i16) -> Result<Self, OrganizationScimTokenError> {
        if code >= 1 && code <= 3 {
            Ok(Self(code as u8))
        } else {
            Err(OrganizationScimTokenError::InvalidEncoding)
        }
    }

    /// 返回数据库使用的稳定位掩码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self.0 as i16
    }

    /// 判断令牌是否允许访问指定目录资源。
    #[must_use]
    pub const fn contains(self, kind: OrganizationScimResourceKind) -> bool {
        let bit = match kind {
            OrganizationScimResourceKind::Users => Self::USERS,
            OrganizationScimResourceKind::Groups => Self::GROUPS,
        };
        self.0 & bit != 0
    }
}

/// 企业 SCIM 令牌生命周期状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationScimTokenStatus {
    /// 令牌可用于后续受控同步请求。
    Active = 1,
    /// 令牌已撤销且不可恢复。
    Revoked = 2,
}

/// SCIM 资源字段构造错误；错误中不携带外部目录原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimResourceError {
    /// 资源标识为空、过长或包含路径/控制字符。
    #[error("企业 SCIM 资源标识无效")]
    InvalidResourceId,
    /// 同步版本必须是非零的单调数值。
    #[error("企业 SCIM 资源版本无效")]
    InvalidVersion,
    /// 用户名为空、过长或包含首尾空白。
    #[error("企业 SCIM 用户名无效")]
    InvalidUserName,
    /// 展示名称为空、过长或包含控制字符。
    #[error("企业 SCIM 展示名称无效")]
    InvalidDisplayName,
    /// 组成员引用数量超过固定上限。
    #[error("企业 SCIM 组成员数量超限")]
    TooManyGroupMembers,
    /// 同一个组内不能重复引用同一资源。
    #[error("企业 SCIM 组成员引用重复")]
    DuplicateGroupMember,
}

/// 外部目录资源的稳定标识；不允许把路径、查询或片段语法带入同步键。
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationScimResourceId(String);

impl OrganizationScimResourceId {
    /// 校验资源标识，不做大小写或 Unicode 归一化，避免幂等键漂移。
    pub fn new(value: impl Into<String>) -> Result<Self, OrganizationScimResourceError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_ORGANIZATION_SCIM_RESOURCE_ID_BYTES
            || value.trim() != value
            || value.chars().any(char::is_control)
            || value
                .chars()
                .any(|character| matches!(character, '/' | '\\' | '?' | '#' | '%'))
        {
            return Err(OrganizationScimResourceError::InvalidResourceId);
        }
        Ok(Self(value))
    }

    /// 返回企业内同步和后续持久化共用的规范资源标识。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OrganizationScimResourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimResourceId(<redacted>)")
    }
}

/// 由目录适配器归一化后的单调资源版本；零值不能作为幂等事实。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationScimResourceVersion(u64);

impl OrganizationScimResourceVersion {
    /// 创建非零版本。
    pub const fn new(value: u64) -> Result<Self, OrganizationScimResourceError> {
        if value == 0 {
            Err(OrganizationScimResourceError::InvalidVersion)
        } else {
            Ok(Self(value))
        }
    }

    /// 从数据库整数恢复版本，拒绝负数和零。
    pub const fn from_persistence(value: i64) -> Result<Self, OrganizationScimResourceError> {
        if value <= 0 {
            return Err(OrganizationScimResourceError::InvalidVersion);
        }
        Ok(Self(value as u64))
    }

    /// 返回用于比较和持久化的数值。
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// 按资源版本判定同步请求，重复请求不重复写入，旧版本失败关闭。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationScimSyncDisposition {
    /// 当前资源不存在或请求版本更新，应进入后续原子写入。
    Apply,
    /// 请求版本与已保存版本相同，可安全视为幂等重放。
    Noop,
    /// 请求版本早于已保存版本，不能覆盖较新的目录事实。
    RejectStale,
}

/// 比较目录请求版本与已保存版本；不执行任何数据库或成员写入。
#[must_use]
pub const fn classify_organization_scim_sync(
    current: Option<OrganizationScimResourceVersion>,
    incoming: OrganizationScimResourceVersion,
) -> OrganizationScimSyncDisposition {
    match current {
        None => OrganizationScimSyncDisposition::Apply,
        Some(current) => {
            if incoming.get() > current.get() {
                OrganizationScimSyncDisposition::Apply
            } else if incoming.get() == current.get() {
                OrganizationScimSyncDisposition::Noop
            } else {
                OrganizationScimSyncDisposition::RejectStale
            }
        }
    }
}

/// SCIM 用户资源希望落入的企业成员生命周期状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationScimLifecycleIntent {
    /// 目录声明用户可以进入企业。
    Active,
    /// 目录声明用户暂时停用，但保留企业归属。
    Suspended,
    /// 目录明确删除用户；只允许把已有成员标记为不可逆移除。
    Removed,
}

impl OrganizationScimLifecycleIntent {
    /// 从 SCIM 用户的活动标记构造受限意图；删除必须由独立的强类型命令表达。
    #[must_use]
    pub const fn from_active(active: bool) -> Self {
        if active {
            Self::Active
        } else {
            Self::Suspended
        }
    }
}

/// 生命周期判定所需的企业授权和席位事实。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationScimLifecycleFacts {
    entitlement_status: OrganizationEntitlementStatus,
    active_member_count: i64,
    seat_limit: i64,
}

/// 生命周期事实构造失败时的关闭式错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimLifecycleFactsError {
    /// 活动成员数或席位上限不满足非负且有界不变量。
    #[error("企业 SCIM 席位事实无效")]
    InvalidSeatFacts,
}

impl OrganizationScimLifecycleFacts {
    /// 构造只读事实，拒绝损坏的席位快照。
    pub const fn new(
        entitlement_status: OrganizationEntitlementStatus,
        active_member_count: i64,
        seat_limit: i64,
    ) -> Result<Self, OrganizationScimLifecycleFactsError> {
        if seat_limit <= 0 || active_member_count < 0 || active_member_count > seat_limit {
            return Err(OrganizationScimLifecycleFactsError::InvalidSeatFacts);
        }
        Ok(Self {
            entitlement_status,
            active_member_count,
            seat_limit,
        })
    }

    /// 返回当前商业授权状态。
    #[must_use]
    pub const fn entitlement_status(self) -> OrganizationEntitlementStatus {
        self.entitlement_status
    }

    /// 返回已占用的活动成员席位。
    #[must_use]
    pub const fn active_member_count(self) -> i64 {
        self.active_member_count
    }

    /// 返回授权席位上限。
    #[must_use]
    pub const fn seat_limit(self) -> i64 {
        self.seat_limit
    }

    /// 判断是否仍有席位可供新成员或恢复成员使用。
    #[must_use]
    pub const fn has_available_seat(self) -> bool {
        self.active_member_count < self.seat_limit
    }
}

/// SCIM 生命周期拒绝原因；不携带用户、组织或目录原文。
#[derive(Clone, Copy, Debug, Eq, Error, Hash, PartialEq)]
pub enum OrganizationScimLifecycleRejection {
    /// 商业授权不是可写状态，所有 SCIM 生命周期写入均失败关闭。
    #[error("企业 SCIM 商业授权不可用")]
    EntitlementUnavailable,
    /// 当前席位已满，不能入职或恢复成员。
    #[error("企业 SCIM 席位已满")]
    SeatCapacityExceeded,
    /// 非活动目录资源不能首次创建企业成员。
    #[error("企业 SCIM 非活动资源不能入职")]
    InactiveResourceCannotProvision,
    /// 已移除成员不可通过后续同步恢复。
    #[error("企业 SCIM 已移除成员不可恢复")]
    RemovedMembershipImmutable,
    /// 不存在的成员不能直接执行删除。
    #[error("企业 SCIM 不存在的成员不能删除")]
    MissingMembershipForRemoval,
}

/// 企业 SCIM 用户生命周期的下一步判定。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationScimLifecycleDisposition {
    /// 当前成员状态已经满足目录意图，重复请求不产生写入。
    Noop,
    /// 为活动目录用户创建企业成员关系；不创建个人用户。
    Provision,
    /// 将活动成员停用并保留企业归属和审计事实。
    Suspend,
    /// 将已停用成员恢复为活动成员。
    Restore,
    /// 将已有成员标记为不可逆软删除。
    SoftDelete,
    /// 由于授权、席位或生命周期不变量拒绝请求。
    Reject(OrganizationScimLifecycleRejection),
}

/// 比较 SCIM 生命周期意图与企业成员状态；不执行数据库、个人用户或团队写入。
#[must_use]
pub const fn classify_organization_scim_lifecycle(
    current: Option<OrganizationMembershipStatus>,
    intent: OrganizationScimLifecycleIntent,
    facts: OrganizationScimLifecycleFacts,
) -> OrganizationScimLifecycleDisposition {
    if !matches!(
        facts.entitlement_status,
        OrganizationEntitlementStatus::Active
    ) {
        return OrganizationScimLifecycleDisposition::Reject(
            OrganizationScimLifecycleRejection::EntitlementUnavailable,
        );
    }

    match (current, intent) {
        (None, OrganizationScimLifecycleIntent::Active) if facts.has_available_seat() => {
            OrganizationScimLifecycleDisposition::Provision
        }
        (None, OrganizationScimLifecycleIntent::Active) => {
            OrganizationScimLifecycleDisposition::Reject(
                OrganizationScimLifecycleRejection::SeatCapacityExceeded,
            )
        }
        (None, OrganizationScimLifecycleIntent::Suspended) => {
            OrganizationScimLifecycleDisposition::Reject(
                OrganizationScimLifecycleRejection::InactiveResourceCannotProvision,
            )
        }
        (None, OrganizationScimLifecycleIntent::Removed) => {
            OrganizationScimLifecycleDisposition::Reject(
                OrganizationScimLifecycleRejection::MissingMembershipForRemoval,
            )
        }
        (Some(OrganizationMembershipStatus::Active), OrganizationScimLifecycleIntent::Active)
        | (
            Some(OrganizationMembershipStatus::Suspended),
            OrganizationScimLifecycleIntent::Suspended,
        )
        | (Some(OrganizationMembershipStatus::Removed), OrganizationScimLifecycleIntent::Removed) => {
            OrganizationScimLifecycleDisposition::Noop
        }
        (
            Some(OrganizationMembershipStatus::Active),
            OrganizationScimLifecycleIntent::Suspended,
        ) => OrganizationScimLifecycleDisposition::Suspend,
        (
            Some(OrganizationMembershipStatus::Suspended),
            OrganizationScimLifecycleIntent::Active,
        ) if facts.has_available_seat() => OrganizationScimLifecycleDisposition::Restore,
        (
            Some(OrganizationMembershipStatus::Suspended),
            OrganizationScimLifecycleIntent::Active,
        ) => OrganizationScimLifecycleDisposition::Reject(
            OrganizationScimLifecycleRejection::SeatCapacityExceeded,
        ),
        (
            Some(OrganizationMembershipStatus::Active | OrganizationMembershipStatus::Suspended),
            OrganizationScimLifecycleIntent::Removed,
        ) => OrganizationScimLifecycleDisposition::SoftDelete,
        (
            Some(OrganizationMembershipStatus::Removed),
            OrganizationScimLifecycleIntent::Active | OrganizationScimLifecycleIntent::Suspended,
        ) => OrganizationScimLifecycleDisposition::Reject(
            OrganizationScimLifecycleRejection::RemovedMembershipImmutable,
        ),
    }
}

/// 企业范围的 SCIM 资源键；资源类型和组织标识共同构成幂等边界。
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct OrganizationScimResourceKey {
    organization_id: OrganizationId,
    kind: OrganizationScimResourceKind,
    resource_id: OrganizationScimResourceId,
}

impl OrganizationScimResourceKey {
    /// 组合已经通过企业范围校验的资源键。
    #[must_use]
    pub fn new(
        organization_id: OrganizationId,
        kind: OrganizationScimResourceKind,
        resource_id: OrganizationScimResourceId,
    ) -> Self {
        Self {
            organization_id,
            kind,
            resource_id,
        }
    }

    /// 返回资源所属企业。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回固定的 SCIM 资源种类。
    #[must_use]
    pub const fn kind(&self) -> OrganizationScimResourceKind {
        self.kind
    }

    /// 返回外部目录资源标识。
    #[must_use]
    pub fn resource_id(&self) -> &OrganizationScimResourceId {
        &self.resource_id
    }
}

impl fmt::Debug for OrganizationScimResourceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimResourceKey")
            .field("organization_id", &self.organization_id)
            .field("kind", &self.kind)
            .field("resource_id", &self.resource_id)
            .finish()
    }
}

/// 企业 SCIM 用户资源；字段仅覆盖后续成员同步所需的受限投影。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationScimUserResource {
    key: OrganizationScimResourceKey,
    user_name: String,
    display_name: Option<String>,
    active: bool,
    version: OrganizationScimResourceVersion,
}

impl OrganizationScimUserResource {
    /// 构造用户资源，不接受邮箱、个人 Key 或任意 JSON 扩展字段。
    pub fn new(
        organization_id: OrganizationId,
        resource_id: OrganizationScimResourceId,
        version: OrganizationScimResourceVersion,
        user_name: String,
        display_name: Option<String>,
        active: bool,
    ) -> Result<Self, OrganizationScimResourceError> {
        validate_scim_text(&user_name, OrganizationScimResourceError::InvalidUserName)?;
        if let Some(display_name) = display_name.as_deref() {
            validate_scim_text(
                display_name,
                OrganizationScimResourceError::InvalidDisplayName,
            )?;
        }
        Ok(Self {
            key: OrganizationScimResourceKey::new(
                organization_id,
                OrganizationScimResourceKind::Users,
                resource_id,
            ),
            user_name,
            display_name,
            active,
            version,
        })
    }

    /// 返回企业范围的资源键。
    #[must_use]
    pub fn key(&self) -> &OrganizationScimResourceKey {
        &self.key
    }

    /// 返回目录用户名。
    #[must_use]
    pub fn user_name(&self) -> &str {
        &self.user_name
    }

    /// 返回可选展示名称。
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    /// 返回目录声明的活动状态；不直接映射为个人用户状态。
    #[must_use]
    pub const fn active(&self) -> bool {
        self.active
    }

    /// 返回用于幂等比较的资源版本。
    #[must_use]
    pub const fn version(&self) -> OrganizationScimResourceVersion {
        self.version
    }
}

impl fmt::Debug for OrganizationScimUserResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimUserResource")
            .field("key", &self.key)
            .field("active", &self.active)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 企业 SCIM 组资源；成员引用只保留外部资源标识，不携带个人空间身份。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationScimGroupResource {
    key: OrganizationScimResourceKey,
    display_name: String,
    member_ids: Vec<OrganizationScimResourceId>,
    version: OrganizationScimResourceVersion,
}

impl OrganizationScimGroupResource {
    /// 构造组资源并拒绝重复成员引用，确保后续同步可以稳定重放。
    pub fn new(
        organization_id: OrganizationId,
        resource_id: OrganizationScimResourceId,
        version: OrganizationScimResourceVersion,
        display_name: String,
        member_ids: Vec<OrganizationScimResourceId>,
    ) -> Result<Self, OrganizationScimResourceError> {
        validate_scim_text(
            &display_name,
            OrganizationScimResourceError::InvalidDisplayName,
        )?;
        if member_ids.len() > MAX_ORGANIZATION_SCIM_GROUP_MEMBER_COUNT {
            return Err(OrganizationScimResourceError::TooManyGroupMembers);
        }
        let mut seen = HashSet::with_capacity(member_ids.len());
        for member_id in &member_ids {
            if !seen.insert(member_id) {
                return Err(OrganizationScimResourceError::DuplicateGroupMember);
            }
        }
        Ok(Self {
            key: OrganizationScimResourceKey::new(
                organization_id,
                OrganizationScimResourceKind::Groups,
                resource_id,
            ),
            display_name,
            member_ids,
            version,
        })
    }

    /// 返回企业范围的组资源键。
    #[must_use]
    pub fn key(&self) -> &OrganizationScimResourceKey {
        &self.key
    }

    /// 返回组展示名称。
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 返回去重后的外部成员引用。
    #[must_use]
    pub fn member_ids(&self) -> &[OrganizationScimResourceId] {
        &self.member_ids
    }

    /// 返回用于幂等比较的资源版本。
    #[must_use]
    pub const fn version(&self) -> OrganizationScimResourceVersion {
        self.version
    }
}

impl fmt::Debug for OrganizationScimGroupResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimGroupResource")
            .field("key", &self.key)
            .field("member_count", &self.member_ids.len())
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

fn validate_scim_text(
    value: &str,
    error: OrganizationScimResourceError,
) -> Result<(), OrganizationScimResourceError> {
    if value.is_empty()
        || value.len() > MAX_ORGANIZATION_SCIM_DISPLAY_NAME_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(error);
    }
    Ok(())
}

impl OrganizationScimTokenStatus {
    /// 返回数据库使用的稳定状态码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 从数据库状态码恢复固定集合。
    pub const fn from_code(code: i16) -> Result<Self, OrganizationScimTokenError> {
        match code {
            1 => Ok(Self::Active),
            2 => Ok(Self::Revoked),
            _ => Err(OrganizationScimTokenError::InvalidEncoding),
        }
    }
}
