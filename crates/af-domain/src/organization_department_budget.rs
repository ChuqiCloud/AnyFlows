use thiserror::Error;

use crate::{OrganizationDepartmentId, OrganizationDepartmentNode, OrganizationId};

/// 部门预算路径最多固化八层，与部门树深度上限保持一致。
pub const MAX_ORGANIZATION_DEPARTMENT_BUDGET_PATH: usize = 8;

/// 请求开始时固化的根到归属部门路径；后续移动或禁用不改写该值。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationDepartmentBudgetPath {
    organization_id: OrganizationId,
    nodes: Vec<OrganizationDepartmentNode>,
}

impl OrganizationDepartmentBudgetPath {
    /// 校验路径连续、同企业、活动且从根开始，避免运行时重新解析树。
    pub fn new(
        organization_id: OrganizationId,
        nodes: Vec<OrganizationDepartmentNode>,
    ) -> Result<Self, OrganizationDepartmentBudgetPathError> {
        if nodes.is_empty() || nodes.len() > MAX_ORGANIZATION_DEPARTMENT_BUDGET_PATH {
            return Err(OrganizationDepartmentBudgetPathError::InvalidLength);
        }
        for (index, node) in nodes.iter().enumerate() {
            if node.organization_id() != organization_id || !node.status().is_active() {
                return Err(OrganizationDepartmentBudgetPathError::InvalidNode);
            }
            let expected_depth = u8::try_from(index + 1)
                .map_err(|_| OrganizationDepartmentBudgetPathError::InvalidLength)?;
            if node.depth() != expected_depth {
                return Err(OrganizationDepartmentBudgetPathError::InvalidDepth);
            }
            let expected_parent = index
                .checked_sub(1)
                .and_then(|parent_index| nodes.get(parent_index))
                .map(|parent| parent.department_id());
            if node.parent_id() != expected_parent {
                return Err(OrganizationDepartmentBudgetPathError::Disconnected);
            }
        }
        if nodes[0].parent_id().is_some() {
            return Err(OrganizationDepartmentBudgetPathError::RootHasParent);
        }
        Ok(Self {
            organization_id,
            nodes,
        })
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回按根到叶顺序排列的不可变节点快照。
    #[must_use]
    pub fn nodes(&self) -> &[OrganizationDepartmentNode] {
        &self.nodes
    }

    #[must_use]
    pub fn leaf_id(&self) -> OrganizationDepartmentId {
        self.nodes
            .last()
            .expect("部门预算路径构造时已保证非空")
            .department_id()
    }

    /// 返回路径中的部门标识，供预算窗口按顺序扣减。
    #[must_use]
    pub fn department_ids(&self) -> impl Iterator<Item = OrganizationDepartmentId> + '_ {
        self.nodes.iter().map(|node| node.department_id())
    }
}

/// 部门预算路径不变量错误；不携带企业或部门原始输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationDepartmentBudgetPathError {
    #[error("企业部门预算路径长度无效")]
    InvalidLength,
    #[error("企业部门预算路径节点无效")]
    InvalidNode,
    #[error("企业部门预算路径深度不一致")]
    InvalidDepth,
    #[error("企业部门预算路径不连续")]
    Disconnected,
    #[error("企业部门预算路径根节点不能有父级")]
    RootHasParent,
}
