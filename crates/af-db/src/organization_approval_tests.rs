use std::error::Error;
use std::time::Duration;

use af_domain::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionKey,
    OrganizationApprovalDecisionKind, OrganizationApprovalRequest, OrganizationApprovalRequestId,
    OrganizationApprovalRequestKey, OrganizationApprovalResourceType,
    OrganizationApprovalTemplateDigest, OrganizationApprovalTemplateKey,
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use crate::entity::{
    groups, organization_approval_notifications, organization_approval_requests,
    organization_audit_logs, users,
};
use crate::{
    DatabaseOptions, MigrationOptions, OrganizationApprovalDecisionOutcome,
    OrganizationApprovalDecisionWrite, OrganizationApprovalRepository,
    OrganizationApprovalRepositoryError, OrganizationApprovalRequestCreateOutcome,
    OrganizationApprovalRequestWrite, OrganizationApprovalTemplateWrite,
    OrganizationApprovalTimeoutCursor, OrganizationApprovalTimeoutLeaseOutcome,
    OrganizationApprovalTimeoutOutcome, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationRepository, PoolOptions,
};
use crate::{
    OrganizationApprovalNotificationCursor, OrganizationApprovalNotificationListQuery,
    OrganizationApprovalNotificationRepository, OrganizationApprovalNotificationRepositoryError,
};

struct Fixture {
    pool: crate::DatabasePool,
    repository: OrganizationApprovalRepository,
    organization_id: OrganizationId,
    owner_id: UserId,
}

#[tokio::test]
async fn template_request_and_decision_facts_are_scoped_and_idempotent()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture().await?;
    let template_key = OrganizationApprovalTemplateKey::new("budget-change")?;
    let template = fixture
        .repository
        .create_template(OrganizationApprovalTemplateWrite::new(
            fixture.organization_id,
            template_key.clone(),
            OrganizationApprovalResourceType::Budget,
            OrganizationApprovalTemplateDigest::new([1; 32])?,
            fixture.owner_id,
            None,
        ))
        .await?;
    assert_eq!(template.version(), 1);

    let second = fixture
        .repository
        .create_template(OrganizationApprovalTemplateWrite::new(
            fixture.organization_id,
            template_key.clone(),
            OrganizationApprovalResourceType::Budget,
            OrganizationApprovalTemplateDigest::new([2; 32])?,
            fixture.owner_id,
            Some(1),
        ))
        .await?;
    assert_eq!(second.version(), 2);
    assert_eq!(
        fixture
            .repository
            .create_template(OrganizationApprovalTemplateWrite::new(
                fixture.organization_id,
                template_key,
                OrganizationApprovalResourceType::Budget,
                OrganizationApprovalTemplateDigest::new([3; 32])?,
                fixture.owner_id,
                Some(1),
            ))
            .await,
        Err(OrganizationApprovalRepositoryError::Conflict)
    );

    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    let request = OrganizationApprovalRequest::new(
        OrganizationApprovalRequestKey::new([4; 16])?,
        fixture.organization_id,
        OrganizationApprovalResourceType::Budget,
        "monthly limit 100".to_owned(),
        second.id(),
        second.version(),
        fixture.owner_id,
        now + 3_600,
    )?;
    let created = fixture
        .repository
        .create_request(OrganizationApprovalRequestWrite::new(request.clone()))
        .await?;
    let request_id = match created {
        OrganizationApprovalRequestCreateOutcome::Created(record) => record.id(),
        OrganizationApprovalRequestCreateOutcome::Existing(_) => panic!("首次申请必须创建"),
    };
    assert!(matches!(
        fixture
            .repository
            .create_request(OrganizationApprovalRequestWrite::new(request.clone()))
            .await?,
        OrganizationApprovalRequestCreateOutcome::Existing(_)
    ));
    let conflict = OrganizationApprovalRequest::new(
        OrganizationApprovalRequestKey::new([4; 16])?,
        fixture.organization_id,
        OrganizationApprovalResourceType::Budget,
        "monthly limit 200".to_owned(),
        second.id(),
        second.version(),
        fixture.owner_id,
        now + 3_600,
    )?;
    assert_eq!(
        fixture
            .repository
            .create_request(OrganizationApprovalRequestWrite::new(conflict))
            .await,
        Err(OrganizationApprovalRepositoryError::Conflict)
    );

    let decision = OrganizationApprovalDecision::new(
        OrganizationApprovalDecisionKey::new([5; 16])?,
        fixture.organization_id,
        request_id,
        OrganizationApprovalDecisionKind::Approved,
        Some(fixture.owner_id),
        Some("approved by owner".to_owned()),
        now,
    )?;
    assert!(matches!(
        fixture
            .repository
            .record_decision(OrganizationApprovalDecisionWrite::new(decision.clone()))
            .await?,
        OrganizationApprovalDecisionOutcome::Created(_)
    ));
    assert!(matches!(
        fixture
            .repository
            .record_decision(OrganizationApprovalDecisionWrite::new(decision.clone()))
            .await?,
        OrganizationApprovalDecisionOutcome::Existing(_)
    ));
    let approved = fixture
        .repository
        .decision_for_request(fixture.organization_id, request_id)
        .await?
        .expect("批准决定必须存在");
    let notification = organization_approval_notifications::Entity::find()
        .filter(
            organization_approval_notifications::Column::OrganizationId
                .eq(fixture.organization_id.get()),
        )
        .filter(organization_approval_notifications::Column::DecisionId.eq(approved.id().get()))
        .one(fixture.pool.connection())
        .await?
        .expect("批准决定必须在同一事务写入站内通知事实");
    assert_eq!(
        notification.notification_key,
        "05050505050505050505050505050505"
    );
    assert_eq!(notification.request_id, request_id.get());
    assert_eq!(notification.recipient_user_id, fixture.owner_id.get());
    assert_eq!(
        notification.decision_kind,
        OrganizationApprovalDecisionKind::Approved.code()
    );
    assert_eq!(notification.template_version, "approval-v1");
    let audit = organization_audit_logs::Entity::find()
        .filter(organization_audit_logs::Column::OrganizationId.eq(fixture.organization_id.get()))
        .filter(
            organization_audit_logs::Column::Operation.eq("organization.approval.decision.project"),
        )
        .one(fixture.pool.connection())
        .await?
        .expect("批准决定必须投影为企业审计事实");
    assert_eq!(audit.actor_user_id, fixture.owner_id.get());
    assert!(!audit.platform_proxy);
    assert!(audit.platform_reason.is_none());
    assert_eq!(audit.permission_code, "organization.approvals.decide");
    assert_eq!(audit.target_type, "approval_decision");
    assert_eq!(
        audit.request_id,
        "approval-decision:05050505050505050505050505050505"
    );
    assert_eq!(audit.target_identifier.len(), 65);
    let summary: serde_json::Value = serde_json::from_str(
        audit
            .change_summary
            .as_deref()
            .expect("审批审计摘要必须存在"),
    )?;
    assert_eq!(summary["event_type"], "approval_decision");
    assert_eq!(summary["result"], "approved");
    assert_eq!(summary["reason_code"], "provided");
    assert_eq!(summary["decision_key"], "05050505050505050505050505050505");
    assert_eq!(summary["request_key"], "04040404040404040404040404040404");
    assert!(
        !audit
            .change_summary
            .as_deref()
            .unwrap()
            .contains("monthly limit")
    );
    organization_audit_logs::Entity::delete_by_id(audit.id)
        .exec(fixture.pool.connection())
        .await?;
    assert_eq!(
        fixture
            .repository
            .record_decision(OrganizationApprovalDecisionWrite::new(decision.clone()))
            .await,
        Err(OrganizationApprovalRepositoryError::Invariant)
    );

    let rejected_request = OrganizationApprovalRequest::new(
        OrganizationApprovalRequestKey::new([7; 16])?,
        fixture.organization_id,
        OrganizationApprovalResourceType::Budget,
        "monthly limit 50".to_owned(),
        second.id(),
        second.version(),
        fixture.owner_id,
        now + 3_600,
    )?;
    let rejected_request_id = match fixture
        .repository
        .create_request(OrganizationApprovalRequestWrite::new(rejected_request))
        .await?
    {
        OrganizationApprovalRequestCreateOutcome::Created(record) => record.id(),
        OrganizationApprovalRequestCreateOutcome::Existing(_) => panic!("拒绝测试申请必须创建"),
    };
    let rejected = OrganizationApprovalDecision::new(
        OrganizationApprovalDecisionKey::new([8; 16])?,
        fixture.organization_id,
        rejected_request_id,
        OrganizationApprovalDecisionKind::Rejected,
        Some(fixture.owner_id),
        None,
        now,
    )?;
    assert!(matches!(
        fixture
            .repository
            .record_decision(OrganizationApprovalDecisionWrite::new(rejected))
            .await?,
        OrganizationApprovalDecisionOutcome::Created(_)
    ));
    let notification_count = organization_approval_notifications::Entity::find()
        .filter(
            organization_approval_notifications::Column::OrganizationId
                .eq(fixture.organization_id.get()),
        )
        .count(fixture.pool.connection())
        .await?;
    assert_eq!(notification_count, 2);
    let rejected_audit = organization_audit_logs::Entity::find()
        .filter(organization_audit_logs::Column::OrganizationId.eq(fixture.organization_id.get()))
        .filter(
            organization_audit_logs::Column::RequestId
                .eq("approval-decision:08080808080808080808080808080808"),
        )
        .one(fixture.pool.connection())
        .await?
        .expect("拒绝决定必须投影为企业审计事实");
    assert_eq!(rejected_audit.outcome, 3);
    let rejected_summary: serde_json::Value = serde_json::from_str(
        rejected_audit
            .change_summary
            .as_deref()
            .expect("拒绝审计摘要必须存在"),
    )?;
    assert_eq!(rejected_summary["result"], "rejected");
    assert_eq!(rejected_summary["reason_code"], "none");
    let second_decision = OrganizationApprovalDecision::new(
        OrganizationApprovalDecisionKey::new([6; 16])?,
        fixture.organization_id,
        request_id,
        OrganizationApprovalDecisionKind::Rejected,
        Some(fixture.owner_id),
        None,
        now,
    )?;
    assert_eq!(
        fixture
            .repository
            .record_decision(OrganizationApprovalDecisionWrite::new(second_decision))
            .await,
        Err(OrganizationApprovalRepositoryError::AlreadyDecided)
    );
    organization_approval_notifications::Entity::delete_by_id(notification.id)
        .exec(fixture.pool.connection())
        .await?;
    assert_eq!(
        fixture
            .repository
            .record_decision(OrganizationApprovalDecisionWrite::new(decision))
            .await,
        Err(OrganizationApprovalRepositoryError::Invariant)
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn approval_request_requires_active_member() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture().await?;
    let template = fixture
        .repository
        .create_template(OrganizationApprovalTemplateWrite::new(
            fixture.organization_id,
            OrganizationApprovalTemplateKey::new("member-change")?,
            OrganizationApprovalResourceType::Member,
            OrganizationApprovalTemplateDigest::new([7; 32])?,
            fixture.owner_id,
            None,
        ))
        .await?;
    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    let request = OrganizationApprovalRequest::new(
        OrganizationApprovalRequestKey::new([8; 16])?,
        fixture.organization_id,
        OrganizationApprovalResourceType::Member,
        "invite user".to_owned(),
        template.id(),
        template.version(),
        UserId::new(999)?,
        now + 3_600,
    )?;
    assert_eq!(
        fixture
            .repository
            .create_request(OrganizationApprovalRequestWrite::new(request))
            .await,
        Err(OrganizationApprovalRepositoryError::MemberUnavailable)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn timeout_scan_and_decision_are_bounded_and_idempotent() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture().await?;
    let template = fixture
        .repository
        .create_template(OrganizationApprovalTemplateWrite::new(
            fixture.organization_id,
            OrganizationApprovalTemplateKey::new("timeout-change")?,
            OrganizationApprovalResourceType::Budget,
            OrganizationApprovalTemplateDigest::new([9; 32])?,
            fixture.owner_id,
            None,
        ))
        .await?;
    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    // 直接写入一条满足历史时间约束的事实，避免修改只追加的申请记录或等待真实时钟推进。
    let expired_at = TimeDateTimeWithTimeZone::from_unix_timestamp(now - 10)?;
    let created_at = TimeDateTimeWithTimeZone::from_unix_timestamp(now - 100)?;
    let request_model = organization_approval_requests::ActiveModel {
        request_key: Set(OrganizationApprovalRequestKey::new([10; 16])?.persistence_key()),
        organization_id: Set(fixture.organization_id.get()),
        resource_type: Set(OrganizationApprovalResourceType::Budget.code()),
        resource_summary: Set(crate::entity::SensitiveString::from("timeout test")),
        template_id: Set(template.id().get()),
        template_version: Set(template.version()),
        requested_by_user_id: Set(fixture.owner_id.get()),
        expires_at: Set(expired_at),
        created_at: Set(created_at),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?;
    let request_id = OrganizationApprovalRequestId::new(request_model.id)?;

    assert_eq!(
        OrganizationApprovalTimeoutCursor::new(0),
        Err(OrganizationApprovalRepositoryError::InvalidInput)
    );
    let batch = fixture.repository.list_timeout_due(now, None, 1).await?;
    let (commands, scanned, next_cursor) = batch.into_parts();
    assert_eq!(scanned, 1);
    assert_eq!(commands.len(), 1);
    assert!(next_cursor.is_none());

    assert!(matches!(
        fixture
            .repository
            .acquire_timeout_lease(now, Duration::from_secs(60))
            .await?,
        OrganizationApprovalTimeoutLeaseOutcome::Acquired(_)
    ));
    assert!(matches!(
        fixture.repository.record_timeout(&commands[0], now).await?,
        OrganizationApprovalTimeoutOutcome::Applied(_)
    ));
    assert!(matches!(
        fixture.repository.record_timeout(&commands[0], now).await?,
        OrganizationApprovalTimeoutOutcome::Existing(_)
    ));
    let decision = fixture
        .repository
        .decision_for_request(fixture.organization_id, request_id)
        .await?
        .expect("超时扫描必须留下决定事实");
    assert_eq!(decision.kind(), OrganizationApprovalDecisionKind::TimedOut);
    let notification = organization_approval_notifications::Entity::find()
        .filter(
            organization_approval_notifications::Column::OrganizationId
                .eq(fixture.organization_id.get()),
        )
        .filter(organization_approval_notifications::Column::DecisionId.eq(decision.id().get()))
        .one(fixture.pool.connection())
        .await?
        .expect("超时决定必须在同一事务写入站内通知事实");
    assert_eq!(notification.recipient_user_id, fixture.owner_id.get());
    assert_eq!(
        notification.decision_kind,
        OrganizationApprovalDecisionKind::TimedOut.code()
    );
    let audit = organization_audit_logs::Entity::find()
        .filter(organization_audit_logs::Column::OrganizationId.eq(fixture.organization_id.get()))
        .filter(
            organization_audit_logs::Column::Operation.eq("organization.approval.decision.project"),
        )
        .one(fixture.pool.connection())
        .await?
        .expect("超时决定必须投影为企业审计事实");
    assert_eq!(audit.actor_user_id, fixture.owner_id.get());
    assert!(audit.platform_proxy);
    assert_eq!(audit.actor_role, None);
    assert_eq!(
        audit.platform_reason.as_deref(),
        Some("approval_timeout_scheduler")
    );
    let summary: serde_json::Value = serde_json::from_str(
        audit
            .change_summary
            .as_deref()
            .expect("超时审计摘要必须存在"),
    )?;
    assert_eq!(summary["result"], "timed_out");
    assert_eq!(summary["reason_code"], "expired");
    assert!(
        !audit
            .change_summary
            .as_deref()
            .unwrap()
            .contains("timeout test")
    );
    organization_approval_notifications::Entity::delete_by_id(notification.id)
        .exec(fixture.pool.connection())
        .await?;
    assert_eq!(
        fixture.repository.record_timeout(&commands[0], now).await,
        Err(OrganizationApprovalRepositoryError::Invariant)
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn approval_notification_repository_paginates_and_marks_read_idempotently()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture().await?;
    let notification_repository = OrganizationApprovalNotificationRepository::new(
        fixture.pool.clone(),
        Duration::from_secs(2),
    )?;
    let template = fixture
        .repository
        .create_template(OrganizationApprovalTemplateWrite::new(
            fixture.organization_id,
            OrganizationApprovalTemplateKey::new("notification-list")?,
            OrganizationApprovalResourceType::Budget,
            OrganizationApprovalTemplateDigest::new([11; 32])?,
            fixture.owner_id,
            None,
        ))
        .await?;
    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    let mut notification_ids = Vec::new();
    for (request_key, decision_key) in [([12; 16], [13; 16]), ([14; 16], [15; 16])] {
        let request = OrganizationApprovalRequest::new(
            OrganizationApprovalRequestKey::new(request_key)?,
            fixture.organization_id,
            OrganizationApprovalResourceType::Budget,
            "notification test".to_owned(),
            template.id(),
            template.version(),
            fixture.owner_id,
            now + 3_600,
        )?;
        let request_id = match fixture
            .repository
            .create_request(OrganizationApprovalRequestWrite::new(request))
            .await?
        {
            OrganizationApprovalRequestCreateOutcome::Created(record) => record.id(),
            OrganizationApprovalRequestCreateOutcome::Existing(_) => unreachable!(),
        };
        let decision = OrganizationApprovalDecision::new(
            OrganizationApprovalDecisionKey::new(decision_key)?,
            fixture.organization_id,
            request_id,
            OrganizationApprovalDecisionKind::Approved,
            Some(fixture.owner_id),
            None,
            now,
        )?;
        let decision = match fixture
            .repository
            .record_decision(OrganizationApprovalDecisionWrite::new(decision))
            .await?
        {
            OrganizationApprovalDecisionOutcome::Created(record) => record,
            OrganizationApprovalDecisionOutcome::Existing(_) => unreachable!(),
        };
        let notification = organization_approval_notifications::Entity::find()
            .filter(organization_approval_notifications::Column::DecisionId.eq(decision.id().get()))
            .one(fixture.pool.connection())
            .await?
            .expect("审批决定必须生成通知");
        notification_ids.push(notification.id);
    }

    let page = notification_repository
        .list(
            fixture.owner_id.get(),
            OrganizationApprovalNotificationListQuery::new(None, 1)?,
        )
        .await?;
    assert_eq!(page.entries().len(), 1);
    assert_eq!(page.unread_count(), 2);
    let cursor = page.next_cursor().expect("分页必须返回稳定游标");
    let older = notification_repository
        .list(
            fixture.owner_id.get(),
            OrganizationApprovalNotificationListQuery::new(Some(cursor), 1)?,
        )
        .await?;
    assert_eq!(older.entries().len(), 1);
    assert!(older.next_cursor().is_none());

    let marked = notification_repository
        .mark_read(fixture.owner_id.get(), vec![notification_ids[0]])
        .await?;
    assert_eq!(marked.marked_count(), 1);
    assert_eq!(marked.unread_count(), 1);
    let repeated = notification_repository
        .mark_read(fixture.owner_id.get(), vec![notification_ids[0]])
        .await?;
    assert_eq!(repeated.marked_count(), 1);
    assert_eq!(repeated.unread_count(), 1);
    assert_eq!(
        notification_repository
            .mark_read(999_999, vec![notification_ids[1]])
            .await,
        Err(OrganizationApprovalNotificationRepositoryError::NotFound)
    );
    assert_eq!(
        notification_repository
            .list(
                999_999,
                OrganizationApprovalNotificationListQuery::new(None, 10)?,
            )
            .await?
            .entries()
            .len(),
        0
    );
    assert_eq!(
        OrganizationApprovalNotificationCursor::new(0, notification_ids[0]),
        Err(OrganizationApprovalNotificationRepositoryError::InvalidInput)
    );
    assert_eq!(
        notification_repository
            .mark_read(
                fixture.owner_id.get(),
                vec![notification_ids[0], notification_ids[0]]
            )
            .await,
        Err(OrganizationApprovalNotificationRepositoryError::InvalidInput)
    );
    fixture.pool.close().await?;
    Ok(())
}

async fn new_fixture() -> Result<Fixture, Box<dyn Error>> {
    let options = DatabaseOptions::new("sqlite::memory:")?.with_pool_options(PoolOptions {
        max_connections: 1,
        min_connections: 1,
        ..PoolOptions::default()
    });
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let group = groups::ActiveModel {
        name: Set("approval-test-group".to_owned()),
        display_name: Set("Approval Test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner = users::ActiveModel {
        username: Set("approval-test-owner".to_owned()),
        email: Set(Some("approval-test-owner@example.com".to_owned())),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("approval-test-owner-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner_id = UserId::new(owner.id)?;
    let organization = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?
        .create(OrganizationCreate::new(
            "Approval Test Organization".to_owned(),
            "approval-test-organization".to_owned(),
            owner_id,
            OrganizationEntitlementWrite::new(
                OrganizationEntitlementSnapshot::new(
                    OrganizationEntitlementCapacity::new(10, 10, 10)?,
                    false,
                    false,
                    false,
                    90,
                    946_684_800,
                    4_102_444_800,
                    Some(4_102_444_800),
                )?,
                "approval-test".to_owned(),
                None,
                None,
            )?,
        ))
        .await?
        .into_parts()
        .0;
    let repository = OrganizationApprovalRepository::new(pool.clone(), Duration::from_secs(10))?;
    Ok(Fixture {
        pool,
        repository,
        organization_id: organization.id(),
        owner_id,
    })
}
