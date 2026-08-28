use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use thiserror::Error;

use crate::{OrganizationDepartmentId, OrganizationId};

/// 部门树允许的最大层级，根部门为第 1 层。
pub const MAX_ORGANIZATION_DEPARTMENT_DEPTH: u8 = 8;
const DEPARTMENT_KEY_BYTES: usize = 16;
const DEPARTMENT_KEY_HEX_BYTES: usize = DEPARTMENT_KEY_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 部门公开标识构造错误；错误中不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationDepartmentKeyError {
    /// 全零标识不能作为稳定的部门边界。
    #[error("企业部门公开标识不能全为零")]
    AllZero,
    /// 持久化值不是固定长度的小写十六进制文本。
    #[error("企业部门公开标识格式无效")]
    InvalidEncoding,
}

/// 不暴露数据库顺序的非零 128 位企业部门公开标识。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationDepartmentKey([u8; DEPARTMENT_KEY_BYTES]);

impl OrganizationDepartmentKey {
    /// 校验并构造非零部门公开标识。
    pub const fn new(
        bytes: [u8; DEPARTMENT_KEY_BYTES],
    ) -> Result<Self, OrganizationDepartmentKeyError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(OrganizationDepartmentKeyError::AllZero)
    }

    /// 从数据库使用的 32 位小写十六进制文本恢复标识。
    pub fn from_persistence_key(value: &str) -> Result<Self, OrganizationDepartmentKeyError> {
        if value.len() != DEPARTMENT_KEY_HEX_BYTES {
            return Err(OrganizationDepartmentKeyError::InvalidEncoding);
        }
        let mut bytes = [0_u8; DEPARTMENT_KEY_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high =
                decode_hex(pair[0]).ok_or(OrganizationDepartmentKeyError::InvalidEncoding)?;
            let low = decode_hex(pair[1]).ok_or(OrganizationDepartmentKeyError::InvalidEncoding)?;
            bytes[index] = (high << 4) | low;
        }
        Self::new(bytes)
    }

    /// 返回数据库和接口共用的稳定持久化文本。
    #[must_use]
    pub fn persistence_key(self) -> String {
        let mut encoded = String::with_capacity(DEPARTMENT_KEY_HEX_BYTES);
        for byte in self.0 {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        encoded
    }
}

impl fmt::Debug for OrganizationDepartmentKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationDepartmentKey(<redacted>)")
    }
}

/// 企业部门生命周期状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationDepartmentStatus {
    /// 部门可接收成员归属和新的预算快照。
    Active = 1,
    /// 部门软禁用，仅保留历史事实。
    Disabled = 2,
}

impl OrganizationDepartmentStatus {
    /// 返回数据库使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断部门是否可接收新归属。
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 部门树快照或移动规则的闭合错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationDepartmentTreeError {
    /// 部门状态不属于闭合集合。
    #[error("企业部门状态无效")]
    InvalidStatus,
    /// 部门层级超出有界树限制。
    #[error("企业部门层级超出上限")]
    DepthExceeded,
    /// 部门版本必须是可递增的正数。
    #[error("企业部门版本无效")]
    InvalidVersion,
    /// 部门节点标识不能重复。
    #[error("企业部门标识重复")]
    DuplicateDepartment,
    /// 父节点不存在。
    #[error("企业部门父节点不存在")]
    ParentNotFound,
    /// 父节点和子节点必须属于同一企业。
    #[error("企业部门父子企业不一致")]
    ParentOrganizationMismatch,
    /// 发现父节点环或自环。
    #[error("企业部门树存在循环")]
    CycleDetected,
    /// 移动节点不能挂到自身。
    #[error("企业部门不能挂载到自身")]
    SelfParent,
    /// 新父节点必须是活动部门。
    #[error("企业部门新父节点不可用")]
    ParentDisabled,
    /// 待移动节点不存在或不属于目标企业。
    #[error("企业部门不属于目标企业")]
    DepartmentOrganizationMismatch,
}

impl TryFrom<i16> for OrganizationDepartmentStatus {
    type Error = OrganizationDepartmentTreeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Active),
            2 => Ok(Self::Disabled),
            _ => Err(OrganizationDepartmentTreeError::InvalidStatus),
        }
    }
}

/// 已通过基本边界校验的部门树节点快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationDepartmentNode {
    organization_id: OrganizationId,
    department_id: OrganizationDepartmentId,
    parent_id: Option<OrganizationDepartmentId>,
    depth: u8,
    status: OrganizationDepartmentStatus,
    version: i64,
}

impl OrganizationDepartmentNode {
    /// 构造部门节点；深度和版本在领域边界统一校验。
    pub fn new(
        organization_id: OrganizationId,
        department_id: OrganizationDepartmentId,
        parent_id: Option<OrganizationDepartmentId>,
        depth: u8,
        status: OrganizationDepartmentStatus,
        version: i64,
    ) -> Result<Self, OrganizationDepartmentTreeError> {
        if depth == 0 || depth > MAX_ORGANIZATION_DEPARTMENT_DEPTH {
            return Err(OrganizationDepartmentTreeError::DepthExceeded);
        }
        if version <= 0 || version == i64::MAX {
            return Err(OrganizationDepartmentTreeError::InvalidVersion);
        }
        Ok(Self {
            organization_id,
            department_id,
            parent_id,
            depth,
            status,
            version,
        })
    }

    #[must_use]
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }

    #[must_use]
    pub const fn department_id(self) -> OrganizationDepartmentId {
        self.department_id
    }

    #[must_use]
    pub const fn parent_id(self) -> Option<OrganizationDepartmentId> {
        self.parent_id
    }

    #[must_use]
    pub const fn depth(self) -> u8 {
        self.depth
    }

    #[must_use]
    pub const fn status(self) -> OrganizationDepartmentStatus {
        self.status
    }

    #[must_use]
    pub const fn version(self) -> i64 {
        self.version
    }
}

/// 校验企业部门快照是否构成无环且深度一致的树。
pub fn validate_organization_department_tree(
    nodes: &[OrganizationDepartmentNode],
) -> Result<(), OrganizationDepartmentTreeError> {
    let by_id = index_nodes(nodes)?;
    for node in nodes {
        let mut current = *node;
        let mut path = Vec::with_capacity(usize::from(node.depth));
        let mut visited = HashSet::with_capacity(usize::from(node.depth));
        loop {
            if !visited.insert(current.department_id) {
                return Err(OrganizationDepartmentTreeError::CycleDetected);
            }
            path.push(current);
            match current.parent_id {
                None => break,
                Some(parent_id) => {
                    let parent = by_id
                        .get(&parent_id)
                        .ok_or(OrganizationDepartmentTreeError::ParentNotFound)?;
                    if parent.organization_id != current.organization_id {
                        return Err(OrganizationDepartmentTreeError::ParentOrganizationMismatch);
                    }
                    if current.status.is_active() && !parent.status.is_active() {
                        return Err(OrganizationDepartmentTreeError::ParentDisabled);
                    }
                    current = *parent;
                }
            }
        }
        for (index, ancestor) in path.iter().enumerate() {
            let expected_depth = u8::try_from(path.len() - index)
                .map_err(|_| OrganizationDepartmentTreeError::DepthExceeded)?;
            if ancestor.depth != expected_depth {
                return Err(OrganizationDepartmentTreeError::DepthExceeded);
            }
        }
    }
    Ok(())
}

/// 校验部门移动，并返回移动后根节点的新深度。
pub fn validate_organization_department_move(
    nodes: &[OrganizationDepartmentNode],
    organization_id: OrganizationId,
    department_id: OrganizationDepartmentId,
    new_parent_id: Option<OrganizationDepartmentId>,
) -> Result<u8, OrganizationDepartmentTreeError> {
    validate_organization_department_tree(nodes)?;
    let by_id = index_nodes(nodes)?;
    let moving = by_id
        .get(&department_id)
        .ok_or(OrganizationDepartmentTreeError::DepartmentOrganizationMismatch)?;
    if moving.organization_id != organization_id {
        return Err(OrganizationDepartmentTreeError::DepartmentOrganizationMismatch);
    }
    let new_depth = match new_parent_id {
        None => 1,
        Some(parent_id) if parent_id == department_id => {
            return Err(OrganizationDepartmentTreeError::SelfParent);
        }
        Some(parent_id) => {
            let parent = by_id
                .get(&parent_id)
                .ok_or(OrganizationDepartmentTreeError::ParentNotFound)?;
            if parent.organization_id != organization_id {
                return Err(OrganizationDepartmentTreeError::ParentOrganizationMismatch);
            }
            if !parent.status.is_active() {
                return Err(OrganizationDepartmentTreeError::ParentDisabled);
            }
            let mut current = *parent;
            let mut visited = HashSet::new();
            loop {
                if !visited.insert(current.department_id) {
                    return Err(OrganizationDepartmentTreeError::CycleDetected);
                }
                if current.department_id == department_id {
                    return Err(OrganizationDepartmentTreeError::CycleDetected);
                }
                current = match current.parent_id {
                    None => break,
                    Some(next_id) => *by_id
                        .get(&next_id)
                        .ok_or(OrganizationDepartmentTreeError::ParentNotFound)?,
                };
            }
            parent
                .depth
                .checked_add(1)
                .ok_or(OrganizationDepartmentTreeError::DepthExceeded)?
        }
    };
    let delta = i16::from(new_depth) - i16::from(moving.depth);
    for descendant in nodes.iter().filter(|candidate| {
        candidate.organization_id == organization_id
            && candidate.department_id != department_id
            && is_descendant(candidate.department_id, department_id, &by_id)
    }) {
        let adjusted = i16::from(descendant.depth) + delta;
        if !(1..=i16::from(MAX_ORGANIZATION_DEPARTMENT_DEPTH)).contains(&adjusted) {
            return Err(OrganizationDepartmentTreeError::DepthExceeded);
        }
    }
    if new_depth > MAX_ORGANIZATION_DEPARTMENT_DEPTH {
        return Err(OrganizationDepartmentTreeError::DepthExceeded);
    }
    Ok(new_depth)
}

/// 禁用部门前必须满足的事实快照；调用方先完成成员、预算和服务账号迁移。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationDepartmentDisableFacts {
    status: OrganizationDepartmentStatus,
    has_active_children: bool,
    has_active_members: bool,
    has_active_budget_bindings: bool,
    has_active_service_account_bindings: bool,
}

impl OrganizationDepartmentDisableFacts {
    /// 组装禁用前置事实。
    #[must_use]
    pub const fn new(
        status: OrganizationDepartmentStatus,
        has_active_children: bool,
        has_active_members: bool,
        has_active_budget_bindings: bool,
        has_active_service_account_bindings: bool,
    ) -> Self {
        Self {
            status,
            has_active_children,
            has_active_members,
            has_active_budget_bindings,
            has_active_service_account_bindings,
        }
    }

    /// 校验不可逆软禁用的前置事实。
    pub fn validate(self) -> Result<(), OrganizationDepartmentDisableRejection> {
        if !self.status.is_active() {
            return Err(OrganizationDepartmentDisableRejection::AlreadyDisabled);
        }
        if self.has_active_children {
            return Err(OrganizationDepartmentDisableRejection::ActiveChildren);
        }
        if self.has_active_members {
            return Err(OrganizationDepartmentDisableRejection::ActiveMembers);
        }
        if self.has_active_budget_bindings {
            return Err(OrganizationDepartmentDisableRejection::ActiveBudgetBindings);
        }
        if self.has_active_service_account_bindings {
            return Err(OrganizationDepartmentDisableRejection::ActiveServiceAccountBindings);
        }
        Ok(())
    }
}

/// 部门禁用前置条件不满足时的闭合拒绝分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationDepartmentDisableRejection {
    #[error("企业部门已经禁用")]
    AlreadyDisabled,
    #[error("企业部门仍有活动子部门")]
    ActiveChildren,
    #[error("企业部门仍有活动成员归属")]
    ActiveMembers,
    #[error("企业部门仍有活动预算绑定")]
    ActiveBudgetBindings,
    #[error("企业部门仍有活动服务账号归属")]
    ActiveServiceAccountBindings,
}

fn index_nodes(
    nodes: &[OrganizationDepartmentNode],
) -> Result<
    HashMap<OrganizationDepartmentId, OrganizationDepartmentNode>,
    OrganizationDepartmentTreeError,
> {
    let mut by_id = HashMap::with_capacity(nodes.len());
    for node in nodes {
        if by_id.insert(node.department_id, *node).is_some() {
            return Err(OrganizationDepartmentTreeError::DuplicateDepartment);
        }
    }
    Ok(by_id)
}

fn is_descendant(
    candidate_id: OrganizationDepartmentId,
    ancestor_id: OrganizationDepartmentId,
    by_id: &HashMap<OrganizationDepartmentId, OrganizationDepartmentNode>,
) -> bool {
    let mut current_id = candidate_id;
    while let Some(node) = by_id.get(&current_id) {
        match node.parent_id {
            Some(parent_id) if parent_id == ancestor_id => return true,
            Some(parent_id) => current_id = parent_id,
            None => return false,
        }
    }
    false
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
