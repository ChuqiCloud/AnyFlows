use super::organization_department_budget::*;
use crate::{
    OrganizationDepartmentId, OrganizationDepartmentNode, OrganizationDepartmentStatus,
    OrganizationId,
};

fn node(
    organization_id: i64,
    department_id: i64,
    parent_id: Option<i64>,
    depth: u8,
) -> OrganizationDepartmentNode {
    OrganizationDepartmentNode::new(
        OrganizationId::new(organization_id).unwrap(),
        OrganizationDepartmentId::new(department_id).unwrap(),
        parent_id.map(|value| OrganizationDepartmentId::new(value).unwrap()),
        depth,
        OrganizationDepartmentStatus::Active,
        1,
    )
    .unwrap()
}

#[test]
fn budget_path_keeps_root_to_leaf_order_and_ids() {
    let organization_id = OrganizationId::new(7).unwrap();
    let path = OrganizationDepartmentBudgetPath::new(
        organization_id,
        vec![node(7, 11, None, 1), node(7, 12, Some(11), 2)],
    )
    .unwrap();
    assert_eq!(path.organization_id(), organization_id);
    assert_eq!(path.leaf_id().get(), 12);
    assert_eq!(
        path.department_ids().map(|id| id.get()).collect::<Vec<_>>(),
        vec![11, 12]
    );
}

#[test]
fn budget_path_rejects_cross_org_disabled_and_disconnected_nodes() {
    let organization_id = OrganizationId::new(7).unwrap();
    assert_eq!(
        OrganizationDepartmentBudgetPath::new(organization_id, vec![node(8, 11, None, 1)]),
        Err(OrganizationDepartmentBudgetPathError::InvalidNode)
    );
    assert_eq!(
        OrganizationDepartmentBudgetPath::new(
            organization_id,
            vec![node(7, 11, None, 1), node(7, 12, Some(99), 2)],
        ),
        Err(OrganizationDepartmentBudgetPathError::Disconnected)
    );
}
