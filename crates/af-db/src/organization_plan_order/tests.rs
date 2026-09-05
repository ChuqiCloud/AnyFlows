use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationPlanFeatures, OrganizationPlanId, OrganizationPlanOrderId,
    OrganizationPlanOrderKind, OrganizationPlanOrderRequestId, OrganizationPlanOrderStatus,
    OrganizationPlanPaymentEventId, OrganizationPlanPaymentEventType, OrganizationPlanPrice,
    OrganizationPlanSnapshot, UserId,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationPlanOrderCreate, OrganizationPlanOrderCreateOutcome,
    OrganizationPlanOrderRepository, OrganizationPlanOrderRepositoryError,
    OrganizationPlanOrderSubmission, OrganizationPlanOrderSubmitOutcome,
    OrganizationPlanPaymentConfirmationOutcome, OrganizationPlanPaymentEventOutcome,
    OrganizationPlanPaymentEventRejection, OrganizationPlanPaymentEventWrite,
    OrganizationPlanRepository, OrganizationPlanWrite, OrganizationRepository,
    entity::{
        groups, organization_audit_logs, organization_entitlements, organization_plan_orders,
        organization_plan_payment_events, users,
    },
};

const CREATED_AT: u64 = 1_900_000_000;

#[test]
fn order_create_rejects_plan_version_outside_database_range() {
    let result = OrganizationPlanOrderCreate::new(
        OrganizationPlanOrderId::new([1; 16]).expect("订单标识有效"),
        OrganizationPlanOrderRequestId::new([2; 16]).expect("幂等标识有效"),
        OrganizationId::new(1).expect("企业标识有效"),
        UserId::new(1).expect("用户标识有效"),
        OrganizationPlanId::new([3; 16]).expect("套餐标识有效"),
        u64::MAX,
        "stripe".to_owned(),
        "USD".to_owned(),
        100,
        CREATED_AT,
        None,
    );
    assert!(matches!(
        result,
        Err(OrganizationPlanOrderRepositoryError::InvalidInput)
    ));
}

#[test]
fn order_change_facts_preserve_action_and_effective_time() {
    let order = OrganizationPlanOrderCreate::new_with_change(
        OrganizationPlanOrderId::new([4; 16]).expect("订单标识有效"),
        OrganizationPlanOrderRequestId::new([5; 16]).expect("幂等标识有效"),
        OrganizationId::new(1).expect("企业标识有效"),
        UserId::new(1).expect("用户标识有效"),
        OrganizationPlanId::new([6; 16]).expect("套餐标识有效"),
        1,
        "stripe".to_owned(),
        "USD".to_owned(),
        100,
        OrganizationPlanOrderKind::Downgrade,
        Some(CREATED_AT + 3_600),
        CREATED_AT,
        Some(CREATED_AT + 7_200),
    )
    .expect("变更订单事实有效");
    assert_eq!(order.kind, OrganizationPlanOrderKind::Downgrade);
    assert_eq!(order.effective_at, Some(CREATED_AT + 3_600));
}

struct Fixture {
    pool: crate::DatabasePool,
    repository: OrganizationPlanOrderRepository,
    plan_repository: OrganizationPlanRepository,
    organization_repository: OrganizationRepository,
    users: [UserId; 2],
}

#[tokio::test]
async fn order_creation_is_idempotent_and_scoped_to_organization() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (first, _) = fixture
        .organization_repository
        .create(organization("企业一", "enterprise-one", fixture.users[0]))
        .await?
        .into_parts();
    let (second, _) = fixture
        .organization_repository
        .create(organization("企业二", "enterprise-two", fixture.users[1]))
        .await?
        .into_parts();
    let plan = plan(&fixture, 0x11, "企业专业版")?;
    fixture.plan_repository.create(plan).await?;

    let first_write = order(first.id(), fixture.users[0], 0x11, 0x21, 0x31, 100)?;
    fixture.repository.inject_outcome_unknown_after_commit();
    assert!(matches!(
        fixture.repository.create_order(first_write).await,
        Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown)
    ));
    let first_write = order(first.id(), fixture.users[0], 0x11, 0x21, 0x31, 100)?;
    let OrganizationPlanOrderCreateOutcome::Existing(existing) =
        fixture.repository.create_order(first_write).await?
    else {
        panic!("结果未知后必须恢复同一订单")
    };
    assert_eq!(existing.status(), OrganizationPlanOrderStatus::Created);
    assert_eq!(existing.organization_id(), first.id());

    let changed = order(first.id(), fixture.users[0], 0x11, 0x22, 0x31, 101)?;
    assert!(matches!(
        fixture.repository.create_order(changed).await,
        Err(OrganizationPlanOrderRepositoryError::Conflict)
    ));

    // 同一个幂等键在不同企业内独立生效，不允许跨企业恢复订单。
    let second_write = order(second.id(), fixture.users[1], 0x11, 0x23, 0x31, 100)?;
    assert!(matches!(
        fixture.repository.create_order(second_write).await?,
        OrganizationPlanOrderCreateOutcome::Created(_)
    ));
    assert!(
        fixture
            .repository
            .get_order(second.id(), existing.order_id())
            .await?
            .is_none()
    );
    assert_eq!(
        organization_plan_orders::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        2
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn payment_events_are_append_only_idempotent_and_do_not_grant_entitlement()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .organization_repository
        .create(organization(
            "事件企业",
            "event-enterprise",
            fixture.users[0],
        ))
        .await?
        .into_parts();
    fixture
        .plan_repository
        .create(plan(&fixture, 0x41, "事件套餐")?)
        .await?;
    let order = order(organization.id(), fixture.users[0], 0x41, 0x42, 0x43, 100)?;
    let order_id = order.order_id;
    fixture.repository.create_order(order).await?;
    let first_event = event(organization.id(), order_id, 0x51, "provider-event-1", 100);

    fixture.repository.inject_outcome_unknown_after_commit();
    assert!(matches!(
        fixture.repository.record_payment_event(first_event).await,
        Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown)
    ));
    let replay = fixture
        .repository
        .record_payment_event(event(
            organization.id(),
            order_id,
            0x51,
            "provider-event-1",
            100,
        ))
        .await?;
    assert!(matches!(
        replay,
        OrganizationPlanPaymentEventOutcome::Existing(_)
    ));
    assert_eq!(
        organization_plan_payment_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );

    let changed = event(organization.id(), order_id, 0x51, "provider-event-1", 101);
    assert!(matches!(
        fixture.repository.record_payment_event(changed).await,
        Err(OrganizationPlanOrderRepositoryError::Conflict)
    ));
    let stored = fixture
        .repository
        .get_order(organization.id(), order_id)
        .await?
        .expect("订单必须仍为可恢复事实");
    assert_eq!(stored.status(), OrganizationPlanOrderStatus::Created);
    assert!(stored.paid_at().is_none());
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn payment_submission_is_cas_idempotent_and_organization_scoped() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .organization_repository
        .create(organization(
            "提交企业",
            "submit-enterprise",
            fixture.users[0],
        ))
        .await?
        .into_parts();
    fixture
        .plan_repository
        .create(plan(&fixture, 0x61, "提交套餐")?)
        .await?;
    let write = order(organization.id(), fixture.users[0], 0x61, 0x62, 0x63, 100)?;
    let order_id = write.order_id;
    fixture.repository.create_order(write).await?;

    let submission = OrganizationPlanOrderSubmission::new(
        organization.id(),
        order_id,
        1,
        "stripe".to_owned(),
        "provider-order-1".to_owned(),
        "card".to_owned(),
        CREATED_AT + 10,
        CREATED_AT + 3_600,
    )?;
    fixture.repository.inject_outcome_unknown_after_commit();
    assert!(matches!(
        fixture.repository.submit_order(submission.clone()).await,
        Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown)
    ));
    let replay = fixture.repository.submit_order(submission).await?;
    let OrganizationPlanOrderSubmitOutcome::Existing(existing) = replay else {
        panic!("结果未知后必须恢复同一 Provider 绑定")
    };
    assert_eq!(existing.status(), OrganizationPlanOrderStatus::Pending);
    assert_eq!(existing.version(), 2);
    assert_eq!(existing.provider_order_id(), Some("provider-order-1"));
    assert_eq!(existing.payment_method(), Some("card"));

    let changed = OrganizationPlanOrderSubmission::new(
        organization.id(),
        order_id,
        1,
        "stripe".to_owned(),
        "provider-order-2".to_owned(),
        "card".to_owned(),
        CREATED_AT + 10,
        CREATED_AT + 3_600,
    )?;
    assert!(matches!(
        fixture.repository.submit_order(changed).await,
        Err(OrganizationPlanOrderRepositoryError::Conflict)
    ));

    let other_organization = OrganizationId::new(99).expect("企业标识有效");
    let cross_organization = OrganizationPlanOrderSubmission::new(
        other_organization,
        order_id,
        1,
        "stripe".to_owned(),
        "provider-order-1".to_owned(),
        "card".to_owned(),
        CREATED_AT + 10,
        CREATED_AT + 3_600,
    )?;
    assert!(matches!(
        fixture.repository.submit_order(cross_organization).await?,
        OrganizationPlanOrderSubmitOutcome::NotFound
    ));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn payment_confirmation_atomically_grants_entitlement_and_audit() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .organization_repository
        .create(organization(
            "确认企业",
            "confirmation-enterprise",
            fixture.users[0],
        ))
        .await?
        .into_parts();
    fixture
        .plan_repository
        .create(plan(&fixture, 0x71, "确认套餐")?)
        .await?;
    let order_write = order(organization.id(), fixture.users[0], 0x71, 0x72, 0x73, 100)?;
    let order_id = order_write.order_id;
    fixture.repository.create_order(order_write).await?;
    fixture
        .repository
        .submit_order(OrganizationPlanOrderSubmission::new(
            organization.id(),
            order_id,
            1,
            "stripe".to_owned(),
            "provider-order-confirm".to_owned(),
            "card".to_owned(),
            CREATED_AT + 10,
            CREATED_AT + 3_600,
        )?)
        .await?;

    let payment = event(
        organization.id(),
        order_id,
        0x74,
        "provider-event-confirm",
        100,
    );
    fixture.repository.inject_outcome_unknown_after_commit();
    assert!(matches!(
        fixture.repository.confirm_payment_event(payment).await,
        Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown)
    ));

    let replay = fixture
        .repository
        .confirm_payment_event(event(
            organization.id(),
            order_id,
            0x74,
            "provider-event-confirm",
            100,
        ))
        .await?;
    let OrganizationPlanPaymentConfirmationOutcome::Existing {
        order: existing,
        entitlement,
    } = replay
    else {
        panic!("结果未知后必须恢复同一支付确认事实")
    };
    assert_eq!(existing.status(), OrganizationPlanOrderStatus::Paid);
    assert_eq!(existing.version(), 3);
    assert_eq!(existing.payment_method(), Some("card"));
    let entitlement = entitlement.expect("成功支付必须返回企业授权");
    assert_eq!(entitlement.version(), 2);
    let order_key = order_id.persistence_key();
    assert_eq!(entitlement.external_reference(), Some(order_key.as_str()));
    assert_eq!(entitlement.snapshot().valid_from(), CREATED_AT as i64 + 60);
    assert_eq!(
        organization_plan_payment_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        organization_audit_logs::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        organization_entitlements::Entity::find()
            .filter(organization_entitlements::Column::OrganizationId.eq(organization.id().get()))
            .one(fixture.pool.connection())
            .await?
            .expect("授权事实必须存在")
            .version,
        2
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn payment_confirmation_rejects_mismatch_without_advancing_order()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .organization_repository
        .create(organization(
            "冲突企业",
            "conflict-enterprise",
            fixture.users[0],
        ))
        .await?
        .into_parts();
    fixture
        .plan_repository
        .create(plan(&fixture, 0x81, "冲突套餐")?)
        .await?;
    let order_write = order(organization.id(), fixture.users[0], 0x81, 0x82, 0x83, 100)?;
    let order_id = order_write.order_id;
    fixture.repository.create_order(order_write).await?;
    fixture
        .repository
        .submit_order(OrganizationPlanOrderSubmission::new(
            organization.id(),
            order_id,
            1,
            "stripe".to_owned(),
            "provider-order-conflict".to_owned(),
            "card".to_owned(),
            CREATED_AT + 10,
            CREATED_AT + 3_600,
        )?)
        .await?;

    let result = fixture
        .repository
        .confirm_payment_event(event(
            organization.id(),
            order_id,
            0x84,
            "provider-event-conflict",
            101,
        ))
        .await?;
    let OrganizationPlanPaymentConfirmationOutcome::RecordedUnprocessed { order, reason } = result
    else {
        panic!("金额冲突必须保存事件但拒绝推进订单")
    };
    assert_eq!(
        reason,
        OrganizationPlanPaymentEventRejection::AmountMismatch
    );
    assert_eq!(order.status(), OrganizationPlanOrderStatus::Pending);
    assert_eq!(order.version(), 2);
    assert_eq!(order.payment_method(), Some("card"));
    let stored = fixture
        .repository
        .get_order(organization.id(), order_id)
        .await?
        .expect("订单仍应可恢复");
    assert_eq!(stored.status(), OrganizationPlanOrderStatus::Pending);
    assert_eq!(stored.version(), 2);
    assert_eq!(
        organization_plan_payment_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        organization_plan_payment_events::Entity::find()
            .one(fixture.pool.connection())
            .await?
            .expect("冲突事件必须保留")
            .processed_at,
        None
    );
    assert_eq!(
        organization_audit_logs::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn future_plan_change_is_paid_then_applied_once_when_due() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .organization_repository
        .create(organization(
            "待生效企业",
            "scheduled-enterprise",
            fixture.users[0],
        ))
        .await?
        .into_parts();
    fixture
        .plan_repository
        .create(plan(&fixture, 0xb1, "待生效套餐")?)
        .await?;
    let effective_at = CREATED_AT + 3_600;
    let order_write = OrganizationPlanOrderCreate::new_with_change(
        OrganizationPlanOrderId::new([0xb2; 16])?,
        OrganizationPlanOrderRequestId::new([0xb3; 16])?,
        organization.id(),
        fixture.users[0],
        OrganizationPlanId::new([0xb1; 16])?,
        1,
        "stripe".to_owned(),
        "USD".to_owned(),
        100,
        OrganizationPlanOrderKind::Upgrade,
        Some(effective_at),
        CREATED_AT,
        Some(CREATED_AT + 7_200),
    )?;
    let order_id = order_write.order_id;
    fixture.repository.create_order(order_write).await?;
    fixture
        .repository
        .submit_order(OrganizationPlanOrderSubmission::new(
            organization.id(),
            order_id,
            1,
            "stripe".to_owned(),
            "provider-order-scheduled".to_owned(),
            "card".to_owned(),
            CREATED_AT + 10,
            CREATED_AT + 3_600,
        )?)
        .await?;
    let result = fixture
        .repository
        .confirm_payment_event(event(
            organization.id(),
            order_id,
            0xb4,
            "provider-event-scheduled",
            100,
        ))
        .await?;
    let OrganizationPlanPaymentConfirmationOutcome::Applied { entitlement, order } = result else {
        panic!("未来生效订单应先确认支付")
    };
    assert!(entitlement.is_none());
    assert_eq!(order.status(), OrganizationPlanOrderStatus::Paid);
    assert_eq!(order.effective_at(), Some(effective_at));
    assert_eq!(order.version(), 3);

    fixture
        .repository
        .apply_due_entitlement_change(organization.id(), effective_at - 1)
        .await?;
    assert_eq!(
        fixture
            .organization_repository
            .entitlement(organization.id())
            .await?
            .entitlement()
            .version(),
        1
    );
    fixture
        .repository
        .apply_due_entitlement_change(organization.id(), effective_at)
        .await?;
    let applied = fixture
        .organization_repository
        .entitlement(organization.id())
        .await?;
    assert_eq!(applied.entitlement().version(), 2);
    fixture
        .repository
        .apply_due_entitlement_change(organization.id(), effective_at + 1)
        .await?;
    assert_eq!(
        fixture
            .organization_repository
            .entitlement(organization.id())
            .await?
            .entitlement()
            .version(),
        2
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn failed_payment_closes_order_without_clearing_submission_facts()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .organization_repository
        .create(organization(
            "失败企业",
            "failed-enterprise",
            fixture.users[0],
        ))
        .await?
        .into_parts();
    fixture
        .plan_repository
        .create(plan(&fixture, 0x91, "失败套餐")?)
        .await?;
    let order_write = order(organization.id(), fixture.users[0], 0x91, 0x92, 0x93, 100)?;
    let order_id = order_write.order_id;
    fixture.repository.create_order(order_write).await?;
    fixture
        .repository
        .submit_order(OrganizationPlanOrderSubmission::new(
            organization.id(),
            order_id,
            1,
            "stripe".to_owned(),
            "provider-order-failed".to_owned(),
            "card".to_owned(),
            CREATED_AT + 10,
            CREATED_AT + 3_600,
        )?)
        .await?;

    let failed = OrganizationPlanPaymentEventWrite::new(
        OrganizationPlanPaymentEventId::new([0x94; 16])?,
        organization.id(),
        order_id,
        "stripe".to_owned(),
        "provider-event-failed".to_owned(),
        None,
        None,
        None,
        None,
        OrganizationPlanPaymentEventType::Failed,
        [0x21; 32],
        [0x94; 32],
        CREATED_AT + 60,
    )?;
    let result = fixture.repository.confirm_payment_event(failed).await?;
    let OrganizationPlanPaymentConfirmationOutcome::Applied { order, entitlement } = result else {
        panic!("失败支付必须关闭订单")
    };
    assert_eq!(order.status(), OrganizationPlanOrderStatus::Failed);
    assert_eq!(order.payment_method(), Some("card"));
    assert!(order.paid_at().is_none());
    assert_eq!(entitlement, None);
    assert_eq!(
        organization_entitlements::Entity::find()
            .filter(organization_entitlements::Column::OrganizationId.eq(organization.id().get()))
            .one(fixture.pool.connection())
            .await?
            .expect("授权事实必须保留")
            .version,
        1
    );
    assert_eq!(
        organization_audit_logs::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn entitlement_write_failure_rolls_back_order_and_payment_event() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .organization_repository
        .create(organization(
            "回滚企业",
            "rollback-enterprise",
            fixture.users[0],
        ))
        .await?
        .into_parts();
    fixture
        .plan_repository
        .create(plan(&fixture, 0xa1, "回滚套餐")?)
        .await?;
    let order_write = order(organization.id(), fixture.users[0], 0xa1, 0xa2, 0xa3, 100)?;
    let order_id = order_write.order_id;
    fixture.repository.create_order(order_write).await?;
    fixture
        .repository
        .submit_order(OrganizationPlanOrderSubmission::new(
            organization.id(),
            order_id,
            1,
            "stripe".to_owned(),
            "provider-order-rollback".to_owned(),
            "card".to_owned(),
            CREATED_AT + 10,
            CREATED_AT + 3_600,
        )?)
        .await?;
    organization_entitlements::Entity::update_many()
        .filter(organization_entitlements::Column::OrganizationId.eq(organization.id().get()))
        .col_expr(
            organization_entitlements::Column::Version,
            sea_orm::sea_query::Expr::value(i64::MAX),
        )
        .exec(fixture.pool.connection())
        .await?;

    let result = fixture
        .repository
        .confirm_payment_event(event(
            organization.id(),
            order_id,
            0xa4,
            "provider-event-rollback",
            100,
        ))
        .await;
    assert!(matches!(
        result,
        Err(OrganizationPlanOrderRepositoryError::Invariant)
    ));
    let order = fixture
        .repository
        .get_order(organization.id(), order_id)
        .await?
        .expect("回滚后订单必须可恢复");
    assert_eq!(order.status(), OrganizationPlanOrderStatus::Pending);
    assert_eq!(order.version(), 2);
    assert_eq!(
        organization_plan_payment_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        organization_audit_logs::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    fixture.pool.close().await?;
    Ok(())
}

fn order(
    organization_id: OrganizationId,
    payer_user_id: UserId,
    plan_marker: u8,
    order_marker: u8,
    request_marker: u8,
    amount_minor: i64,
) -> Result<OrganizationPlanOrderCreate, OrganizationPlanOrderRepositoryError> {
    OrganizationPlanOrderCreate::new(
        OrganizationPlanOrderId::new([order_marker; 16]).expect("订单标识有效"),
        OrganizationPlanOrderRequestId::new([request_marker; 16]).expect("幂等键有效"),
        organization_id,
        payer_user_id,
        OrganizationPlanId::new([plan_marker; 16]).expect("套餐标识有效"),
        1,
        "stripe".to_owned(),
        "USD".to_owned(),
        amount_minor,
        CREATED_AT,
        Some(CREATED_AT + 3_600),
    )
}

fn event(
    organization_id: OrganizationId,
    order_id: OrganizationPlanOrderId,
    marker: u8,
    provider_event_id: &str,
    amount_minor: u64,
) -> OrganizationPlanPaymentEventWrite {
    OrganizationPlanPaymentEventWrite::new(
        OrganizationPlanPaymentEventId::new([marker; 16]).expect("事件标识有效"),
        organization_id,
        order_id,
        "stripe".to_owned(),
        provider_event_id.to_owned(),
        Some("trade-enterprise-1".to_owned()),
        Some(amount_minor),
        Some("USD".to_owned()),
        Some("card".to_owned()),
        OrganizationPlanPaymentEventType::Succeeded,
        [0x11; 32],
        [marker; 32],
        CREATED_AT + 60,
    )
    .expect("事件事实有效")
}

fn plan(
    fixture: &Fixture,
    marker: u8,
    name: &str,
) -> Result<OrganizationPlanWrite, Box<dyn Error>> {
    Ok(OrganizationPlanWrite::new(
        OrganizationPlanId::new([marker; 16])?,
        name.to_owned(),
        None,
        OrganizationPlanSnapshot::new(
            OrganizationPlanFeatures::new(true, true, false),
            10,
            3,
            2,
            90,
            30,
        )?,
        OrganizationPlanPrice::new("stripe".to_owned(), "USD".to_owned(), 100)?,
        fixture.users[0],
        CREATED_AT,
    )?)
}

fn organization(name: &str, slug: &str, owner: UserId) -> OrganizationCreate {
    OrganizationCreate::new(
        name.to_owned(),
        slug.to_owned(),
        owner,
        OrganizationEntitlementWrite::new(
            OrganizationEntitlementSnapshot::new(
                OrganizationEntitlementCapacity::new(10, 3, 2).expect("容量有效"),
                true,
                false,
                false,
                90,
                946_684_800,
                4_102_444_800,
                Some(4_102_444_800),
            )
            .expect("授权有效"),
            "contract".to_owned(),
            None,
            None,
        )
        .expect("授权写入有效"),
    )
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("organization-plan-order-test".to_owned()),
        display_name: Set("企业订单测试".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let mut user_ids = Vec::new();
    for index in 0..2 {
        let user = users::ActiveModel {
            username: Set(format!("organization-plan-order-user-{index}")),
            email: Set(Some(format!("organization-plan-order-{index}@example.com"))),
            password_hash: Set(None),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set(format!("organization-plan-order-aff-{index}")),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        user_ids.push(UserId::new(user.id)?);
    }
    Ok(Fixture {
        repository: OrganizationPlanOrderRepository::new(pool.clone(), Duration::from_secs(5))?,
        plan_repository: OrganizationPlanRepository::new(pool.clone(), Duration::from_secs(5))?,
        organization_repository: OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        users: user_ids.try_into().expect("测试用户数量固定"),
    })
}
