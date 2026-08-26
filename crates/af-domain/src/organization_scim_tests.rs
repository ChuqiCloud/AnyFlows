use super::*;

#[test]
fn scim_token_is_high_entropy_and_redacted() {
    let token = OrganizationScimToken::generate().expect("系统随机源应可用");
    assert_eq!(token.as_str().len(), 69);
    assert!(token.as_str().starts_with("scim_"));
    assert_eq!(token.prefix(), "scim_");
    assert_eq!(format!("{token:?}"), "OrganizationScimToken(<redacted>)");
    assert_eq!(token.digest().as_str().len(), 64);
    assert_eq!(
        format!("{:?}", token.digest()),
        "OrganizationScimTokenDigest(<redacted>)"
    );
    let key = OrganizationScimTokenKey::generate().expect("系统随机源应可用");
    assert_eq!(key.as_str().len(), 32);
    assert_eq!(format!("{key:?}"), "OrganizationScimTokenKey(<redacted>)");
}

#[test]
fn scim_token_rejects_noncanonical_material() {
    assert!(
        OrganizationScimToken::new(
            "scim_0000000000000000000000000000000000000000000000000000000000000000"
        )
        .is_err()
    );
    assert!(
        OrganizationScimToken::new(
            "scim_ABCDEF00000000000000000000000000000000000000000000000000000000"
        )
        .is_err()
    );
    assert!(
        OrganizationScimToken::new(
            "token_1111111111111111111111111111111111111111111111111111111111111111"
        )
        .is_err()
    );
    assert!(OrganizationScimTokenDigest::from_persistence_key("A".repeat(64)).is_err());
}

#[test]
fn scim_resource_scope_is_closed() {
    let users = OrganizationScimResourceSet::users();
    assert!(users.contains(OrganizationScimResourceKind::Users));
    assert!(!users.contains(OrganizationScimResourceKind::Groups));
    let both = OrganizationScimResourceSet::users_and_groups();
    assert!(both.contains(OrganizationScimResourceKind::Users));
    assert!(both.contains(OrganizationScimResourceKind::Groups));
    assert!(OrganizationScimResourceSet::from_code(0).is_err());
    assert!(OrganizationScimResourceSet::from_code(4).is_err());
    assert_eq!(
        OrganizationScimTokenStatus::from_code(1).unwrap(),
        OrganizationScimTokenStatus::Active
    );
    assert!(OrganizationScimTokenStatus::from_code(3).is_err());
}

#[test]
fn scim_resource_id_and_version_are_canonical() {
    let resource_id = OrganizationScimResourceId::new("directory-user-1").unwrap();
    assert_eq!(resource_id.as_str(), "directory-user-1");
    assert_eq!(
        format!("{resource_id:?}"),
        "OrganizationScimResourceId(<redacted>)"
    );
    for invalid in ["", " user", "user/1", "user?1", "user#1", "user%1"] {
        assert!(OrganizationScimResourceId::new(invalid).is_err());
    }
    let current = OrganizationScimResourceVersion::new(4).unwrap();
    let same = OrganizationScimResourceVersion::new(4).unwrap();
    let newer = OrganizationScimResourceVersion::new(5).unwrap();
    let older = OrganizationScimResourceVersion::new(3).unwrap();
    assert_eq!(
        classify_organization_scim_sync(None, current),
        OrganizationScimSyncDisposition::Apply
    );
    assert_eq!(
        classify_organization_scim_sync(Some(current), same),
        OrganizationScimSyncDisposition::Noop
    );
    assert_eq!(
        classify_organization_scim_sync(Some(current), newer),
        OrganizationScimSyncDisposition::Apply
    );
    assert_eq!(
        classify_organization_scim_sync(Some(current), older),
        OrganizationScimSyncDisposition::RejectStale
    );
    assert!(OrganizationScimResourceVersion::new(0).is_err());
    assert!(OrganizationScimResourceVersion::from_persistence(0).is_err());
}

#[test]
fn scim_user_and_group_resources_keep_enterprise_scope() {
    let organization_id = OrganizationId::new(7).unwrap();
    let user_id = OrganizationScimResourceId::new("user-1").unwrap();
    let user = OrganizationScimUserResource::new(
        organization_id,
        user_id.clone(),
        OrganizationScimResourceVersion::new(1).unwrap(),
        "alice".to_owned(),
        Some("Alice".to_owned()),
        true,
    )
    .unwrap();
    assert_eq!(user.key().organization_id(), organization_id);
    assert_eq!(user.key().kind(), OrganizationScimResourceKind::Users);
    assert_eq!(user.key().resource_id().as_str(), "user-1");
    assert_eq!(user.user_name(), "alice");
    assert_eq!(user.display_name(), Some("Alice"));
    assert!(!format!("{user:?}").contains("alice"));
    assert!(
        OrganizationScimUserResource::new(
            organization_id,
            user_id,
            OrganizationScimResourceVersion::new(1).unwrap(),
            "".to_owned(),
            None,
            true,
        )
        .is_err()
    );

    let group_id = OrganizationScimResourceId::new("group-1").unwrap();
    let group = OrganizationScimGroupResource::new(
        organization_id,
        group_id,
        OrganizationScimResourceVersion::new(2).unwrap(),
        "Engineering".to_owned(),
        vec![OrganizationScimResourceId::new("user-1").unwrap()],
    )
    .unwrap();
    assert_eq!(group.key().organization_id(), organization_id);
    assert_eq!(group.key().kind(), OrganizationScimResourceKind::Groups);
    assert_eq!(group.member_ids().len(), 1);
    assert!(!format!("{group:?}").contains("Engineering"));
}

#[test]
fn scim_group_rejects_duplicate_or_oversized_members() {
    let organization_id = OrganizationId::new(8).unwrap();
    let group_id = OrganizationScimResourceId::new("group-1").unwrap();
    let version = OrganizationScimResourceVersion::new(1).unwrap();
    let member = OrganizationScimResourceId::new("user-1").unwrap();
    assert_eq!(
        OrganizationScimGroupResource::new(
            organization_id,
            group_id.clone(),
            version,
            "Engineering".to_owned(),
            vec![member.clone(), member],
        )
        .unwrap_err(),
        OrganizationScimResourceError::DuplicateGroupMember
    );
    let members = (0..=MAX_ORGANIZATION_SCIM_GROUP_MEMBER_COUNT)
        .map(|index| OrganizationScimResourceId::new(format!("user-{index}")).unwrap())
        .collect();
    assert_eq!(
        OrganizationScimGroupResource::new(
            organization_id,
            group_id,
            version,
            "Engineering".to_owned(),
            members,
        )
        .unwrap_err(),
        OrganizationScimResourceError::TooManyGroupMembers
    );
}

#[test]
fn scim_lifecycle_facts_reject_invalid_seat_snapshots() {
    assert_eq!(
        OrganizationScimLifecycleFacts::new(OrganizationEntitlementStatus::Active, -1, 4),
        Err(OrganizationScimLifecycleFactsError::InvalidSeatFacts)
    );
    assert_eq!(
        OrganizationScimLifecycleFacts::new(OrganizationEntitlementStatus::Active, 5, 4),
        Err(OrganizationScimLifecycleFactsError::InvalidSeatFacts)
    );
    assert_eq!(
        OrganizationScimLifecycleFacts::new(OrganizationEntitlementStatus::Active, 0, 0),
        Err(OrganizationScimLifecycleFactsError::InvalidSeatFacts)
    );
}

#[test]
fn scim_lifecycle_distinguishes_provision_suspend_restore_and_delete() {
    let available =
        OrganizationScimLifecycleFacts::new(OrganizationEntitlementStatus::Active, 2, 3).unwrap();
    assert!(available.has_available_seat());
    assert_eq!(available.active_member_count(), 2);
    assert_eq!(available.seat_limit(), 3);
    assert_eq!(
        classify_organization_scim_lifecycle(
            None,
            OrganizationScimLifecycleIntent::from_active(true),
            available,
        ),
        OrganizationScimLifecycleDisposition::Provision
    );
    assert_eq!(
        classify_organization_scim_lifecycle(
            None,
            OrganizationScimLifecycleIntent::from_active(false),
            available,
        ),
        OrganizationScimLifecycleDisposition::Reject(
            OrganizationScimLifecycleRejection::InactiveResourceCannotProvision
        )
    );
    assert_eq!(
        classify_organization_scim_lifecycle(
            Some(OrganizationMembershipStatus::Active),
            OrganizationScimLifecycleIntent::Suspended,
            available,
        ),
        OrganizationScimLifecycleDisposition::Suspend
    );
    assert_eq!(
        classify_organization_scim_lifecycle(
            Some(OrganizationMembershipStatus::Suspended),
            OrganizationScimLifecycleIntent::Active,
            available,
        ),
        OrganizationScimLifecycleDisposition::Restore
    );
    assert_eq!(
        classify_organization_scim_lifecycle(
            Some(OrganizationMembershipStatus::Active),
            OrganizationScimLifecycleIntent::Removed,
            available,
        ),
        OrganizationScimLifecycleDisposition::SoftDelete
    );
    assert_eq!(
        classify_organization_scim_lifecycle(
            Some(OrganizationMembershipStatus::Suspended),
            OrganizationScimLifecycleIntent::Removed,
            available,
        ),
        OrganizationScimLifecycleDisposition::SoftDelete
    );
}

#[test]
fn scim_lifecycle_is_idempotent_and_never_revives_removed_members() {
    let available =
        OrganizationScimLifecycleFacts::new(OrganizationEntitlementStatus::Active, 1, 2).unwrap();
    assert_eq!(
        classify_organization_scim_lifecycle(
            Some(OrganizationMembershipStatus::Active),
            OrganizationScimLifecycleIntent::Active,
            available,
        ),
        OrganizationScimLifecycleDisposition::Noop
    );
    assert_eq!(
        classify_organization_scim_lifecycle(
            Some(OrganizationMembershipStatus::Suspended),
            OrganizationScimLifecycleIntent::Suspended,
            available,
        ),
        OrganizationScimLifecycleDisposition::Noop
    );
    assert_eq!(
        classify_organization_scim_lifecycle(
            Some(OrganizationMembershipStatus::Removed),
            OrganizationScimLifecycleIntent::Removed,
            available,
        ),
        OrganizationScimLifecycleDisposition::Noop
    );
    assert_eq!(
        classify_organization_scim_lifecycle(
            Some(OrganizationMembershipStatus::Removed),
            OrganizationScimLifecycleIntent::Active,
            available,
        ),
        OrganizationScimLifecycleDisposition::Reject(
            OrganizationScimLifecycleRejection::RemovedMembershipImmutable
        )
    );
}

#[test]
fn scim_lifecycle_rechecks_entitlement_and_seat_capacity() {
    let full =
        OrganizationScimLifecycleFacts::new(OrganizationEntitlementStatus::Active, 2, 2).unwrap();
    assert!(!full.has_available_seat());
    assert_eq!(
        classify_organization_scim_lifecycle(None, OrganizationScimLifecycleIntent::Active, full,),
        OrganizationScimLifecycleDisposition::Reject(
            OrganizationScimLifecycleRejection::SeatCapacityExceeded
        )
    );
    assert_eq!(
        classify_organization_scim_lifecycle(
            Some(OrganizationMembershipStatus::Suspended),
            OrganizationScimLifecycleIntent::Active,
            full,
        ),
        OrganizationScimLifecycleDisposition::Reject(
            OrganizationScimLifecycleRejection::SeatCapacityExceeded
        )
    );

    for status in [
        OrganizationEntitlementStatus::Grace,
        OrganizationEntitlementStatus::ReadOnly,
        OrganizationEntitlementStatus::Suspended,
        OrganizationEntitlementStatus::Closed,
    ] {
        let facts = OrganizationScimLifecycleFacts::new(status, 0, 1).unwrap();
        assert_eq!(
            classify_organization_scim_lifecycle(
                None,
                OrganizationScimLifecycleIntent::Active,
                facts,
            ),
            OrganizationScimLifecycleDisposition::Reject(
                OrganizationScimLifecycleRejection::EntitlementUnavailable
            )
        );
    }
}
