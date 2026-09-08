use std::{error::Error, time::Duration};

use af_domain::{
    BillingReservationId, GatewayPrincipal, GroupId, OrganizationEntitlementCapacity,
    OrganizationEntitlementSnapshot, OrganizationGatewayPrincipal, OrganizationId,
    OrganizationServiceAccountName, OrganizationServiceAccountRuntimeIdentity, UserId,
};
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};

use crate::entity::{groups, organization_service_account_audit_logs, users};
use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationRepository, OrganizationServiceAccountAuditQuery,
    OrganizationServiceAccountAuditRepository, OrganizationServiceAccountAuditRepositoryError,
    OrganizationServiceAccountAuditWrite, OrganizationServiceAccountRepository,
    OrganizationTokenCreateRecord, OrganizationTokenRepository, OrganizationTokenWriteRecord,
    UsageLogBillingMode, UsageLogRepository, UsageLogSemantics, UsageLogSource, UsageLogUsage,
    UsageLogWrite,
};

struct AuditFixture {
    pool: crate::DatabasePool,
    first_organization_id: OrganizationId,
    second_organization_id: OrganizationId,
    first_principal: GatewayPrincipal,
    second_principal: GatewayPrincipal,
    first_identity: OrganizationServiceAccountRuntimeIdentity,
    second_identity: OrganizationServiceAccountRuntimeIdentity,
    second_enterprise_identity: OrganizationServiceAccountRuntimeIdentity,
    first_secret: String,
    first_digest: String,
}

#[tokio::test]
async fn service_account_audit_projection_is_idempotent_enterprise_scoped_and_redacted()
-> Result<(), Box<dyn Error>> {
    let fixture = create_fixture().await?;
    let repository = OrganizationServiceAccountAuditRepository::new(fixture.pool.clone());
    let usage_repository = UsageLogRepository::new(fixture.pool.clone());

    let first_event = reservation_id(1);
    usage_repository
        .record(&usage_write(first_event, fixture.first_principal))
        .await?;
    let first_write =
        OrganizationServiceAccountAuditWrite::new(first_event, fixture.first_identity);
    repository.record(&first_write).await?;
    repository.record(&first_write).await?;
    assert_eq!(
        organization_service_account_audit_logs::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );

    let same_event_different_account =
        OrganizationServiceAccountAuditWrite::new(first_event, fixture.second_identity);
    assert_eq!(
        repository.record(&same_event_different_account).await,
        Err(OrganizationServiceAccountAuditRepositoryError::Conflict)
    );

    let cross_enterprise_event = reservation_id(2);
    usage_repository
        .record(&usage_write(
            cross_enterprise_event,
            fixture.first_principal,
        ))
        .await?;
    let cross_enterprise_identity = fixture.second_enterprise_identity;
    assert_eq!(
        repository
            .record(&OrganizationServiceAccountAuditWrite::new(
                cross_enterprise_event,
                cross_enterprise_identity,
            ))
            .await,
        Err(OrganizationServiceAccountAuditRepositoryError::Invariant)
    );

    let mismatched_account_key = OrganizationServiceAccountRuntimeIdentity::new(
        fixture.first_identity.locator(),
        fixture.second_identity.account_key(),
        fixture.first_identity.credential_key(),
    );
    let mismatched_account_event = reservation_id(3);
    usage_repository
        .record(&usage_write(
            mismatched_account_event,
            fixture.first_principal,
        ))
        .await?;
    assert_eq!(
        repository
            .record(&OrganizationServiceAccountAuditWrite::new(
                mismatched_account_event,
                mismatched_account_key,
            ))
            .await,
        Err(OrganizationServiceAccountAuditRepositoryError::Invariant)
    );

    let key_owned_by_other_account = OrganizationServiceAccountRuntimeIdentity::new(
        fixture.first_identity.locator(),
        fixture.first_identity.account_key(),
        fixture.second_identity.credential_key(),
    );
    let other_key_event = reservation_id(4);
    usage_repository
        .record(&usage_write(other_key_event, fixture.first_principal))
        .await?;
    assert_eq!(
        repository
            .record(&OrganizationServiceAccountAuditWrite::new(
                other_key_event,
                key_owned_by_other_account,
            ))
            .await,
        Err(OrganizationServiceAccountAuditRepositoryError::Invariant)
    );

    let second_first_enterprise_event = reservation_id(5);
    usage_repository
        .record(&usage_write(
            second_first_enterprise_event,
            fixture.first_principal,
        ))
        .await?;
    repository
        .record(&OrganizationServiceAccountAuditWrite::new(
            second_first_enterprise_event,
            fixture.first_identity,
        ))
        .await?;

    let page = repository
        .list(
            fixture.first_organization_id,
            OrganizationServiceAccountAuditQuery::new(None, 1, None)?,
        )
        .await?;
    let (records, cursor) = page.into_parts();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].organization_id(), fixture.first_organization_id);
    assert_eq!(
        records[0].service_account_id(),
        fixture.first_identity.locator().service_account_id()
    );
    assert_eq!(
        records[0].service_account_key(),
        fixture.first_identity.account_key()
    );
    assert_eq!(
        records[0].credential_key(),
        fixture.first_identity.credential_key()
    );
    let cursor = cursor.expect("两条同企业记录必须返回下一游标");
    let next_page = repository
        .list(
            fixture.first_organization_id,
            OrganizationServiceAccountAuditQuery::new(Some(cursor), 1, None)?,
        )
        .await?
        .into_parts()
        .0;
    assert_eq!(next_page.len(), 1);
    assert_ne!(next_page[0].event_id(), records[0].event_id());

    let second_event = reservation_id(6);
    usage_repository
        .record(&usage_write(second_event, fixture.second_principal))
        .await?;
    repository
        .record(&OrganizationServiceAccountAuditWrite::new(
            second_event,
            fixture.second_enterprise_identity,
        ))
        .await?;
    let second_records = repository
        .list(
            fixture.second_organization_id,
            OrganizationServiceAccountAuditQuery::new(None, 10, None)?,
        )
        .await?
        .into_parts()
        .0;
    assert_eq!(second_records.len(), 1);
    assert_eq!(
        second_records[0].organization_id(),
        fixture.second_organization_id
    );

    let audit_debug = format!("{first_write:?}");
    assert!(audit_debug.contains("<redacted>"));
    assert!(!audit_debug.contains(&fixture.first_secret));
    assert!(!audit_debug.contains(&fixture.first_digest));
    let persisted_debug = format!(
        "{:?}",
        organization_service_account_audit_logs::Entity::find()
            .all(fixture.pool.connection())
            .await?
    );
    assert!(!persisted_debug.contains(&fixture.first_secret));
    assert!(!persisted_debug.contains(&fixture.first_digest));

    fixture.pool.close().await?;
    Ok(())
}

#[test]
fn audit_query_rejects_invalid_bounds() {
    assert!(OrganizationServiceAccountAuditQuery::new(Some(0), 1, None).is_err());
    assert!(OrganizationServiceAccountAuditQuery::new(None, 0, None).is_err());
    assert!(OrganizationServiceAccountAuditQuery::new(None, 101, None).is_err());
}

async fn create_fixture() -> Result<AuditFixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("service-account-audit-group".to_owned()),
        display_name: Set("Service Account Audit".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("service-account-audit-user".to_owned()),
        email: Set(Some("service-account-audit@example.com".to_owned())),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("service-account-audit-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner_id = UserId::new(user.id)?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let first = organizations
        .create(OrganizationCreate::new(
            "Service Account Audit One".to_owned(),
            "service-account-audit-one".to_owned(),
            owner_id,
            entitlement(),
        ))
        .await?
        .into_parts();
    let second = organizations
        .create(OrganizationCreate::new(
            "Service Account Audit Two".to_owned(),
            "service-account-audit-two".to_owned(),
            owner_id,
            entitlement(),
        ))
        .await?
        .into_parts();
    let accounts = OrganizationServiceAccountRepository::new(pool.clone(), Duration::from_secs(5))?;
    let first_account = accounts
        .create(
            first.0.id(),
            OrganizationServiceAccountName::new("audit-one")?,
        )
        .await?
        .into_parts();
    let second_account = accounts
        .create(
            first.0.id(),
            OrganizationServiceAccountName::new("audit-two")?,
        )
        .await?
        .into_parts();
    let second_enterprise_account = accounts
        .create(
            second.0.id(),
            OrganizationServiceAccountName::new("audit-enterprise-two")?,
        )
        .await?
        .into_parts();
    let tokens = OrganizationTokenRepository::new(pool.clone(), Duration::from_secs(5))?;
    let first_token = tokens
        .create(OrganizationTokenCreateRecord::new(
            first.0.id(),
            first.1.id(),
            owner_id,
            format!("{:064x}", 1),
            "sk-af-000000000001".to_owned(),
            OrganizationTokenWriteRecord::new(
                "audit-one-token".to_owned(),
                1,
                1_000,
                false,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        ))
        .await?;
    let second_token = tokens
        .create(OrganizationTokenCreateRecord::new(
            second.0.id(),
            second.1.id(),
            owner_id,
            format!("{:064x}", 2),
            "sk-af-000000000002".to_owned(),
            OrganizationTokenWriteRecord::new(
                "audit-two-token".to_owned(),
                1,
                1_000,
                false,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        ))
        .await?;
    let first_principal = GatewayPrincipal::organization(
        first_token.token().token_id(),
        owner_id,
        GroupId::new(group.id)?,
        OrganizationGatewayPrincipal::new(first.0.id(), first.1.id(), None),
    );
    let second_principal = GatewayPrincipal::organization(
        second_token.token().token_id(),
        owner_id,
        GroupId::new(group.id)?,
        OrganizationGatewayPrincipal::new(second.0.id(), second.1.id(), None),
    );
    let first_secret = first_account.2.as_str().to_owned();
    let first_digest = first_account.2.digest().as_str().to_owned();
    Ok(AuditFixture {
        pool,
        first_organization_id: first.0.id(),
        second_organization_id: second.0.id(),
        first_principal,
        second_principal,
        first_identity: identity_from_parts(&first_account),
        second_identity: identity_from_parts(&second_account),
        second_enterprise_identity: identity_from_parts(&second_enterprise_account),
        first_secret,
        first_digest,
    })
}

fn identity_from_parts(
    parts: &(
        crate::OrganizationServiceAccountRecord,
        crate::OrganizationServiceAccountKeyRecord,
        af_domain::OrganizationServiceAccountSecret,
    ),
) -> OrganizationServiceAccountRuntimeIdentity {
    OrganizationServiceAccountRuntimeIdentity::new(
        parts.0.locator(),
        parts.0.account().public_key(),
        parts.1.key_key(),
    )
}

fn entitlement() -> OrganizationEntitlementWrite {
    OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 10, 10).expect("测试配额必须有效"),
            true,
            false,
            false,
            90,
            946_684_800,
            4_102_444_800,
            Some(4_102_444_800),
        )
        .expect("测试企业授权必须有效"),
        "audit-test".to_owned(),
        None,
        None,
    )
    .expect("测试企业授权写入必须有效")
}

fn usage_write(event_id: BillingReservationId, principal: GatewayPrincipal) -> UsageLogWrite {
    UsageLogWrite::new(
        event_id,
        principal,
        UsageLogBillingMode::PerToken,
        UsageLogUsage::new(10, 2, 0, 0, 0, 0, 0, 0).expect("测试用量必须有效"),
        UsageLogSource::Upstream,
        UsageLogSemantics::Inclusive,
        af_domain::Quota::new(1).expect("测试额度必须有效"),
    )
}

fn reservation_id(marker: u8) -> BillingReservationId {
    BillingReservationId::new([marker; 16]).expect("测试事件标识必须非零")
}
