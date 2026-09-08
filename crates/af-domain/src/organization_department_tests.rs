use super::organization_department::*;
use crate::{OrganizationDepartmentId, OrganizationId};

fn node(
    organization: i64,
    department: i64,
    parent: Option<i64>,
    depth: u8,
    status: OrganizationDepartmentStatus,
) -> OrganizationDepartmentNode {
    OrganizationDepartmentNode::new(
        OrganizationId::new(organization).unwrap(),
        OrganizationDepartmentId::new(department).unwrap(),
        parent.map(|value| OrganizationDepartmentId::new(value).unwrap()),
        depth,
        status,
        1,
    )
    .unwrap()
}

#[test]
fn department_key_is_canonical_and_redacted() {
    let key = OrganizationDepartmentKey::new([0x2a; 16]).unwrap();
    assert_eq!(key.persistence_key(), "2a".repeat(16));
    assert_eq!(
        OrganizationDepartmentKey::from_persistence_key(&key.persistence_key()).unwrap(),
        key
    );
    assert_eq!(format!("{key:?}"), "OrganizationDepartmentKey(<redacted>)");
    assert_eq!(
        OrganizationDepartmentKey::new([0; 16]),
        Err(OrganizationDepartmentKeyError::AllZero)
    );
    assert!(OrganizationDepartmentKey::from_persistence_key(&"A".repeat(32)).is_err());
}

#[test]
fn department_tree_rejects_cycles_and_keeps_depth_bounded() {
    let organization = OrganizationId::new(7).unwrap();
    let first = OrganizationDepartmentId::new(1).unwrap();
    let second = OrganizationDepartmentId::new(2).unwrap();
    let cyclic = [
        OrganizationDepartmentNode::new(
            organization,
            first,
            Some(second),
            1,
            OrganizationDepartmentStatus::Active,
            1,
        )
        .unwrap(),
        OrganizationDepartmentNode::new(
            organization,
            second,
            Some(first),
            2,
            OrganizationDepartmentStatus::Active,
            1,
        )
        .unwrap(),
    ];
    assert_eq!(
        validate_organization_department_tree(&cyclic),
        Err(OrganizationDepartmentTreeError::CycleDetected)
    );
    assert!(
        OrganizationDepartmentNode::new(
            organization,
            first,
            None,
            MAX_ORGANIZATION_DEPARTMENT_DEPTH + 1,
            OrganizationDepartmentStatus::Active,
            1,
        )
        .is_err()
    );
}

#[test]
fn department_move_rejects_descendant_and_disabled_parent_and_checks_descendants() {
    let organization = OrganizationId::new(7).unwrap();
    let nodes = [
        node(7, 1, None, 1, OrganizationDepartmentStatus::Active),
        node(7, 2, Some(1), 2, OrganizationDepartmentStatus::Active),
        node(7, 3, Some(2), 3, OrganizationDepartmentStatus::Active),
        node(7, 4, None, 1, OrganizationDepartmentStatus::Disabled),
    ];
    assert_eq!(
        validate_organization_department_move(
            &nodes,
            organization,
            OrganizationDepartmentId::new(2).unwrap(),
            Some(OrganizationDepartmentId::new(3).unwrap()),
        ),
        Err(OrganizationDepartmentTreeError::CycleDetected)
    );
    assert_eq!(
        validate_organization_department_move(
            &nodes,
            organization,
            OrganizationDepartmentId::new(2).unwrap(),
            Some(OrganizationDepartmentId::new(4).unwrap()),
        ),
        Err(OrganizationDepartmentTreeError::ParentDisabled)
    );
    assert_eq!(
        validate_organization_department_move(
            &nodes,
            organization,
            OrganizationDepartmentId::new(3).unwrap(),
            None,
        ),
        Ok(1)
    );
}

#[test]
fn department_disable_requires_members_children_and_bindings_to_be_moved_first() {
    let active = OrganizationDepartmentDisableFacts::new(
        OrganizationDepartmentStatus::Active,
        false,
        false,
        false,
        false,
    );
    assert_eq!(active.validate(), Ok(()));
    assert_eq!(
        OrganizationDepartmentDisableFacts::new(
            OrganizationDepartmentStatus::Active,
            true,
            false,
            false,
            false,
        )
        .validate(),
        Err(OrganizationDepartmentDisableRejection::ActiveChildren)
    );
    assert_eq!(
        OrganizationDepartmentDisableFacts::new(
            OrganizationDepartmentStatus::Active,
            false,
            false,
            false,
            true,
        )
        .validate(),
        Err(OrganizationDepartmentDisableRejection::ActiveServiceAccountBindings)
    );
    assert_eq!(
        OrganizationDepartmentDisableFacts::new(
            OrganizationDepartmentStatus::Disabled,
            false,
            false,
            false,
            false,
        )
        .validate(),
        Err(OrganizationDepartmentDisableRejection::AlreadyDisabled)
    );
}
