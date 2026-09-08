use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot,
    OrganizationScimAuditEventType, OrganizationScimAuditFacts, OrganizationScimAuditReason,
    OrganizationScimAuditResource, OrganizationScimAuditResult, OrganizationScimRequestFingerprint,
    OrganizationScimResourceDigest, OrganizationScimResourceId, OrganizationScimResourceKind,
    OrganizationScimResourceVersion, OrganizationScimTokenKey, UserId,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};

use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationRepository, OrganizationScimAuditQuery, OrganizationScimAuditRepository,
    OrganizationScimAuditRepositoryError, OrganizationScimAuditWriteOutcome,
    entity::{groups, organization_scim_audit_logs, users},
};

#[tokio::test]
async fn scim_audit_is_redacted_append_only_and_idempotent() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("scim-audit-test-group".to_owned()),
        display_name: Set("SCIM Audit Test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("scim-audit-owner".to_owned()),
        email: Set(Some("scim-audit-owner@example.com".to_owned())),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("scim-audit-owner-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (organization, _) = organizations
        .create(OrganizationCreate::new(
            "SCIM Audit Organization".to_owned(),
            "scim-audit-org".to_owned(),
            UserId::new(user.id)?,
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let resource_id = OrganizationScimResourceId::new("directory-user-1".to_owned())?;
    let resource = OrganizationScimAuditResource::new(
        OrganizationScimResourceKind::Users,
        OrganizationScimResourceDigest::from_resource(
            organization.id(),
            OrganizationScimResourceKind::Users,
            &resource_id,
        ),
        OrganizationScimResourceVersion::new(3)?,
    );
    let facts = OrganizationScimAuditFacts::new(
        organization.id(),
        Some(OrganizationScimTokenKey::new(
            "0123456789abcdef0123456789abcdef",
        )?),
        OrganizationScimAuditEventType::ResourceSynchronized,
        OrganizationScimAuditResult::Applied,
        Some(resource),
        OrganizationScimRequestFingerprint::new([0x42; 32])?,
        None,
        None,
    )?;
    let repository = OrganizationScimAuditRepository::new(pool.clone(), Duration::from_secs(5))?;
    assert_eq!(
        repository.record(&facts).await?,
        OrganizationScimAuditWriteOutcome::Applied
    );
    assert_eq!(
        repository.record(&facts).await?,
        OrganizationScimAuditWriteOutcome::Existing
    );
    let conflicting_facts = OrganizationScimAuditFacts::new(
        organization.id(),
        facts.token_key().cloned(),
        OrganizationScimAuditEventType::ResourceSynchronized,
        OrganizationScimAuditResult::Noop,
        facts.resource().cloned(),
        OrganizationScimRequestFingerprint::new([0x42; 32])?,
        None,
        None,
    )?;
    assert_eq!(
        repository.record(&conflicting_facts).await,
        Err(OrganizationScimAuditRepositoryError::Conflict)
    );
    let stored = organization_scim_audit_logs::Entity::find()
        .filter(organization_scim_audit_logs::Column::OrganizationId.eq(organization.id().get()))
        .one(pool.connection())
        .await?
        .expect("SCIM 审计事实必须持久化");
    assert_eq!(stored.event_type, "resource_synchronized");
    assert_eq!(stored.resource_digest.as_deref().unwrap().len(), 64);
    assert_eq!(stored.request_fingerprint, "42".repeat(32));
    assert!(!format!("{stored:?}").contains("directory-user-1"));
    assert!(!format!("{stored:?}").contains("example.com"));
    let mut update = stored.clone().into_active_model();
    update.result = Set("noop".to_owned());
    assert!(update.update(pool.connection()).await.is_err());
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn scim_audit_query_scopes_pages_and_filters_records() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("scim-audit-query-group".to_owned()),
        display_name: Set("SCIM Audit Query".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("scim-audit-query-owner".to_owned()),
        email: Set(Some("scim-audit-query-owner@example.com".to_owned())),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("scim-audit-query-owner-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let owner_id = UserId::new(user.id)?;
    let (organization, _) = organizations
        .create(OrganizationCreate::new(
            "SCIM Audit Query Organization".to_owned(),
            "scim-audit-query-org".to_owned(),
            owner_id,
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let (other_organization, _) = organizations
        .create(OrganizationCreate::new(
            "SCIM Audit Query Other Organization".to_owned(),
            "scim-audit-query-other-org".to_owned(),
            owner_id,
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let repository = OrganizationScimAuditRepository::new(pool.clone(), Duration::from_secs(5))?;
    let primary_token = OrganizationScimTokenKey::new("0123456789abcdef0123456789abcdef")?;
    let other_token = OrganizationScimTokenKey::new("fedcba9876543210fedcba9876543210")?;

    let user_resource = OrganizationScimAuditResource::new(
        OrganizationScimResourceKind::Users,
        OrganizationScimResourceDigest::from_resource(
            organization.id(),
            OrganizationScimResourceKind::Users,
            &OrganizationScimResourceId::new("directory-user-query".to_owned())?,
        ),
        OrganizationScimResourceVersion::new(1)?,
    );
    let group_resource = OrganizationScimAuditResource::new(
        OrganizationScimResourceKind::Groups,
        OrganizationScimResourceDigest::from_resource(
            organization.id(),
            OrganizationScimResourceKind::Groups,
            &OrganizationScimResourceId::new("directory-group-query".to_owned())?,
        ),
        OrganizationScimResourceVersion::new(2)?,
    );
    let other_resource = OrganizationScimAuditResource::new(
        OrganizationScimResourceKind::Users,
        OrganizationScimResourceDigest::from_resource(
            other_organization.id(),
            OrganizationScimResourceKind::Users,
            &OrganizationScimResourceId::new("directory-other-query".to_owned())?,
        ),
        OrganizationScimResourceVersion::new(1)?,
    );

    repository
        .record(&OrganizationScimAuditFacts::new(
            organization.id(),
            Some(primary_token.clone()),
            OrganizationScimAuditEventType::ResourceSynchronized,
            OrganizationScimAuditResult::Applied,
            Some(user_resource),
            OrganizationScimRequestFingerprint::new([0x11; 32])?,
            None,
            None,
        )?)
        .await?;
    repository
        .record(&OrganizationScimAuditFacts::new(
            other_organization.id(),
            Some(other_token),
            OrganizationScimAuditEventType::ResourceSynchronized,
            OrganizationScimAuditResult::Applied,
            Some(other_resource),
            OrganizationScimRequestFingerprint::new([0x22; 32])?,
            None,
            None,
        )?)
        .await?;
    repository
        .record(&OrganizationScimAuditFacts::new(
            organization.id(),
            Some(primary_token.clone()),
            OrganizationScimAuditEventType::ResourceSynchronized,
            OrganizationScimAuditResult::Applied,
            Some(group_resource),
            OrganizationScimRequestFingerprint::new([0x33; 32])?,
            None,
            None,
        )?)
        .await?;
    repository
        .record(&OrganizationScimAuditFacts::new(
            organization.id(),
            Some(primary_token.clone()),
            OrganizationScimAuditEventType::RequestRejected,
            OrganizationScimAuditResult::Rejected,
            None,
            OrganizationScimRequestFingerprint::new([0x44; 32])?,
            Some(OrganizationScimAuditReason::InvalidRequest),
            None,
        )?)
        .await?;

    let first_query = OrganizationScimAuditQuery::new(None, 2, None, None, None, None, None)?;
    let (first_page, next_cursor) = repository
        .list(organization.id(), &first_query)
        .await?
        .into_parts();
    assert_eq!(first_page.len(), 2);
    assert!(first_page[0].id() > first_page[1].id());
    assert!(
        first_page
            .iter()
            .all(|record| record.organization_id() == organization.id())
    );
    let next_cursor = next_cursor.expect("存在下一页时必须返回稳定游标");
    assert_eq!(next_cursor, first_page[1].id());

    let second_query =
        OrganizationScimAuditQuery::new(Some(next_cursor), 2, None, None, None, None, None)?;
    let (second_page, second_cursor) = repository
        .list(organization.id(), &second_query)
        .await?
        .into_parts();
    assert_eq!(second_page.len(), 1);
    assert!(second_page[0].id() < next_cursor);
    assert!(second_cursor.is_none());

    let synchronized_users = OrganizationScimAuditQuery::new(
        None,
        10,
        Some(OrganizationScimAuditEventType::ResourceSynchronized),
        Some(OrganizationScimAuditResult::Applied),
        None,
        Some(primary_token.clone()),
        Some(OrganizationScimResourceKind::Users),
    )?;
    let (synchronized_users, synchronized_users_cursor) = repository
        .list(organization.id(), &synchronized_users)
        .await?
        .into_parts();
    assert_eq!(synchronized_users.len(), 1);
    assert_eq!(
        synchronized_users[0].resource_kind(),
        Some(OrganizationScimResourceKind::Users)
    );
    assert_eq!(synchronized_users[0].token_key(), Some(&primary_token));
    assert!(synchronized_users_cursor.is_none());

    let rejected = OrganizationScimAuditQuery::new(
        None,
        10,
        Some(OrganizationScimAuditEventType::RequestRejected),
        Some(OrganizationScimAuditResult::Rejected),
        Some(OrganizationScimAuditReason::InvalidRequest),
        Some(primary_token),
        None,
    )?;
    let (rejected, rejected_cursor) = repository
        .list(organization.id(), &rejected)
        .await?
        .into_parts();
    assert_eq!(rejected.len(), 1);
    assert_eq!(
        rejected[0].reason(),
        Some(OrganizationScimAuditReason::InvalidRequest)
    );
    assert!(rejected_cursor.is_none());

    pool.close().await?;
    Ok(())
}

fn active_entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
    Ok(OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 10, 10)?,
            true,
            false,
            false,
            90,
            946_684_800,
            4_102_444_800,
            None,
        )?,
        "scim-audit-test".to_owned(),
        None,
        None,
    )?)
}
