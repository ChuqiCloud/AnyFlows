use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationPlanFeatures, OrganizationPlanId, OrganizationPlanOrderId,
    OrganizationPlanOrderRequestId, OrganizationPlanOrderStatus, OrganizationPlanPaymentEventId,
    OrganizationPlanPaymentEventType, OrganizationPlanPrice, OrganizationPlanSnapshot, UserId,
};
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};

use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationPlanOrderCreate, OrganizationPlanOrderCreateOutcome,
    OrganizationPlanOrderRepository, OrganizationPlanOrderRepositoryError,
    OrganizationPlanPaymentEventOutcome, OrganizationPlanPaymentEventWrite,
    OrganizationPlanRepository, OrganizationPlanWrite, OrganizationRepository,
    entity::{groups, organization_plan_orders, organization_plan_payment_events, users},
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
                true,
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
