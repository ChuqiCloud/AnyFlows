use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationScimAuditEventType, OrganizationScimAuditFacts, OrganizationScimAuditResource,
    OrganizationScimAuditResult, OrganizationScimRequestFingerprint,
    OrganizationScimResourceDigest, OrganizationScimResourceId, OrganizationScimResourceKind,
    OrganizationScimResourceVersion, OrganizationScimTokenKey, UserId,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationRepository, OrganizationScimAuditRepository, OrganizationScimAuditRepositoryError,
    OrganizationScimAuditWriteOutcome,
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
    assert_eq!(stored.request_fingerprint, "42".repeat(64));
    assert!(!format!("{stored:?}").contains("directory-user-1"));
    assert!(!format!("{stored:?}").contains("example.com"));
    let mut update = stored.clone().into_active_model();
    update.result = Set("noop".to_owned());
    assert!(update.update(pool.connection()).await.is_err());
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
