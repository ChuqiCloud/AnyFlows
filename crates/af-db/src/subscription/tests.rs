use std::{error::Error, time::Duration};

use af_domain::{
    GroupId, Quota, SubscriptionCycle, SubscriptionPlanId, SubscriptionPlanStatus, UserId,
    UserSubscriptionId, UserSubscriptionStatus,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use super::{
    SubscriptionInputError, SubscriptionPlanCreateOutcome, SubscriptionPlanDisable,
    SubscriptionPlanDisableOutcome, SubscriptionPlanWrite, SubscriptionRepositoryError,
    UserSubscriptionBind, UserSubscriptionBindOutcome,
};
use crate::{
    AdminUserCreateRecord, AdminUserRepository, DatabaseOptions, DatabasePool, MigrationOptions,
    entity::{SensitiveString, groups, subscription_plans, user_subscriptions},
};

const CREATED_AT: u64 = 1_900_000_000;
const WINDOW_STARTED_AT: u64 = CREATED_AT + 60;
const WINDOW_ENDS_AT: u64 = WINDOW_STARTED_AT + 86_400;
const BOUND_AT: u64 = CREATED_AT + 30;

#[test]
fn subscription_inputs_reject_ambiguous_facts() {
    let creator = UserId::new(1).unwrap();
    let quota = Quota::new(1).unwrap();
    for name in ["", " 前导空格", "尾随空格 ", "控制\n字符"] {
        assert_eq!(
            SubscriptionPlanWrite::new(
                test_plan_id(0x11),
                name.to_owned(),
                creator,
                quota,
                SubscriptionCycle::Monthly,
                CREATED_AT,
            )
            .unwrap_err(),
            SubscriptionInputError::InvalidName
        );
    }
    assert_eq!(
        SubscriptionPlanWrite::new(
            test_plan_id(0x11),
            "零额度".to_owned(),
            creator,
            Quota::new(0).unwrap(),
            SubscriptionCycle::Monthly,
            CREATED_AT,
        )
        .unwrap_err(),
        SubscriptionInputError::InvalidQuota
    );
    for (started_at, ends_at, bound_at) in [
        (WINDOW_STARTED_AT, WINDOW_STARTED_AT, BOUND_AT),
        (WINDOW_ENDS_AT, WINDOW_STARTED_AT, BOUND_AT),
        (WINDOW_STARTED_AT, WINDOW_ENDS_AT, WINDOW_ENDS_AT),
    ] {
        assert_eq!(
            UserSubscriptionBind::new(
                subscription_id(0x21),
                creator,
                test_plan_id(0x11),
                started_at,
                ends_at,
                bound_at,
            )
            .unwrap_err(),
            SubscriptionInputError::InvalidWindow
        );
    }
    assert_eq!(
        SubscriptionPlanDisable::new(test_plan_id(0x11), 0, CREATED_AT).unwrap_err(),
        SubscriptionInputError::InvalidVersion
    );
    assert_eq!(
        UserSubscriptionBind::new(
            subscription_id(0x21),
            creator,
            test_plan_id(0x11),
            i64::MAX as u64 + 1,
            i64::MAX as u64 + 2,
            BOUND_AT,
        )
        .unwrap_err(),
        SubscriptionInputError::InvalidTiming
    );
}

#[tokio::test]
async fn plan_creation_is_idempotent_and_facts_are_immutable() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let write = fixture.plan(0x31, "专业版", 10_000, SubscriptionCycle::Monthly)?;

    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture.repository.create_plan(&write).await.unwrap_err(),
        SubscriptionRepositoryError::OutcomeUnknown
    );
    let SubscriptionPlanCreateOutcome::Existing(existing) =
        fixture.repository.create_plan(&write).await?
    else {
        panic!("提交结果未知后必须按同一计划事实恢复")
    };
    assert_eq!(existing.name(), "专业版");
    assert_eq!(existing.status(), SubscriptionPlanStatus::Active);
    assert_eq!(existing.quota_amount().units(), 10_000);
    assert_eq!(existing.cycle(), SubscriptionCycle::Monthly);
    assert_eq!(existing.version(), 1);

    let conflict = fixture.plan(0x31, "专业版", 20_000, SubscriptionCycle::Monthly)?;
    assert_eq!(
        fixture.repository.create_plan(&conflict).await.unwrap_err(),
        SubscriptionRepositoryError::Conflict
    );
    assert_eq!(
        subscription_plans::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn plan_and_user_subscription_lists_use_stable_cursors() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_ids = [test_plan_id(0x51), test_plan_id(0x52), test_plan_id(0x53)];
    for (index, plan_id) in plan_ids.into_iter().enumerate() {
        fixture
            .repository
            .create_plan(&fixture.plan_with_id(
                plan_id,
                &format!("分页计划-{index}"),
                1_000 + i64::try_from(index)?,
                SubscriptionCycle::Monthly,
            )?)
            .await?;
        fixture
            .repository
            .bind_user(&fixture.bind(
                0x61 + u8::try_from(index)?,
                fixture.first_user_id,
                plan_id,
            )?)
            .await?;
    }

    let first_plans = fixture.repository.list_plans(None, 2).await?;
    assert_eq!(first_plans.plans().len(), 2);
    assert_eq!(first_plans.plans()[0].plan_id(), plan_ids[2]);
    let second_plans = fixture
        .repository
        .list_plans(first_plans.next_cursor(), 2)
        .await?;
    assert_eq!(second_plans.plans().len(), 1);
    assert_eq!(second_plans.plans()[0].plan_id(), plan_ids[0]);
    assert!(second_plans.next_cursor().is_none());
    assert_eq!(
        fixture
            .repository
            .get_plan(plan_ids[1])
            .await?
            .expect("计划必须可按稳定标识读取")
            .created_by_user_id(),
        fixture.admin_user_id
    );

    let first_subscriptions = fixture
        .repository
        .list_user_subscriptions(fixture.first_user_id, None, 2)
        .await?;
    assert_eq!(first_subscriptions.subscriptions().len(), 2);
    assert_eq!(
        first_subscriptions.subscriptions()[0].plan_id(),
        plan_ids[2]
    );
    assert_eq!(
        first_subscriptions.subscriptions()[0].plan_name(),
        "分页计划-2"
    );
    let second_subscriptions = fixture
        .repository
        .list_user_subscriptions(fixture.first_user_id, first_subscriptions.next_cursor(), 2)
        .await?;
    assert_eq!(second_subscriptions.subscriptions().len(), 1);
    assert_eq!(
        second_subscriptions.subscriptions()[0].plan_id(),
        plan_ids[0]
    );
    assert!(second_subscriptions.next_cursor().is_none());
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn disabled_plan_replays_cas_and_rejects_new_binding() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = test_plan_id(0x32);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "停用计划",
            5_000,
            SubscriptionCycle::Weekly,
        )?)
        .await?;
    let disable = SubscriptionPlanDisable::new(plan_id, 1, CREATED_AT + 120)?;

    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture.repository.disable_plan(&disable).await.unwrap_err(),
        SubscriptionRepositoryError::OutcomeUnknown
    );
    let SubscriptionPlanDisableOutcome::Existing(disabled) =
        fixture.repository.disable_plan(&disable).await?
    else {
        panic!("相同停用事实必须恢复为 Existing")
    };
    assert_eq!(disabled.status(), SubscriptionPlanStatus::Disabled);
    assert_eq!(disabled.version(), 2);
    assert_eq!(disabled.disabled_at(), Some(CREATED_AT + 120));

    let bind = fixture.bind(0x41, fixture.first_user_id, plan_id)?;
    assert!(matches!(
        fixture.repository.bind_user(&bind).await?,
        UserSubscriptionBindOutcome::PlanDisabled
    ));
    let conflicting_disable = SubscriptionPlanDisable::new(plan_id, 1, CREATED_AT + 121)?;
    assert_eq!(
        fixture
            .repository
            .disable_plan(&conflicting_disable)
            .await
            .unwrap_err(),
        SubscriptionRepositoryError::Conflict
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn binding_snapshots_plan_and_recovers_unknown_commit() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = test_plan_id(0x33);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "年度计划",
            50_000,
            SubscriptionCycle::Yearly,
        )?)
        .await?;
    let bind = fixture.bind(0x42, fixture.first_user_id, plan_id)?;

    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture.repository.bind_user(&bind).await.unwrap_err(),
        SubscriptionRepositoryError::OutcomeUnknown
    );
    let UserSubscriptionBindOutcome::Existing(existing) =
        fixture.repository.bind_user(&bind).await?
    else {
        panic!("提交结果未知后必须按相同绑定键恢复")
    };
    assert_eq!(existing.user_id(), fixture.first_user_id);
    assert_eq!(existing.plan_id(), plan_id);
    assert_eq!(existing.plan_version(), 1);
    assert_eq!(existing.status(), UserSubscriptionStatus::Active);
    assert_eq!(existing.quota_amount().units(), 50_000);
    assert_eq!(existing.quota_used().units(), 0);
    assert_eq!(existing.cycle(), SubscriptionCycle::Yearly);
    assert_eq!(existing.window_started_at(), WINDOW_STARTED_AT);
    assert_eq!(existing.window_ends_at(), WINDOW_ENDS_AT);
    assert_eq!(existing.bound_at(), BOUND_AT);
    assert_eq!(existing.version(), 1);

    let row = user_subscriptions::Entity::find()
        .one(fixture.pool.connection())
        .await?
        .expect("用户订阅必须存在");
    assert_eq!(row.quota_amount, 50_000);
    assert_eq!(row.quota_used, 0);
    assert_eq!(row.cycle, SubscriptionCycle::Yearly.code());
    assert_eq!(row.plan_version, 1);

    let conflicting = UserSubscriptionBind::new(
        subscription_id(0x42),
        fixture.second_user_id,
        plan_id,
        WINDOW_STARTED_AT,
        WINDOW_ENDS_AT,
        BOUND_AT,
    )?;
    assert_eq!(
        fixture
            .repository
            .bind_user(&conflicting)
            .await
            .unwrap_err(),
        SubscriptionRepositoryError::Conflict
    );

    let plan_database_id = subscription_plans::Entity::find()
        .filter(
            subscription_plans::Column::PlanKey
                .eq(SensitiveString::from(plan_id.persistence_key())),
        )
        .one(fixture.pool.connection())
        .await?
        .expect("订阅计划必须存在")
        .id;
    assert!(
        subscription_plans::Entity::delete_by_id(plan_database_id)
            .exec(fixture.pool.connection())
            .await
            .is_err()
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn binding_reports_missing_principals_without_partial_rows() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let missing_plan = fixture.bind(0x43, fixture.first_user_id, test_plan_id(0x34))?;
    assert!(matches!(
        fixture.repository.bind_user(&missing_plan).await?,
        UserSubscriptionBindOutcome::PlanNotFound
    ));

    let plan_id = test_plan_id(0x35);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "缺失用户计划",
            1_000,
            SubscriptionCycle::Daily,
        )?)
        .await?;
    let missing_user = fixture.bind(0x44, UserId::new(9_999_999)?, plan_id)?;
    assert!(matches!(
        fixture.repository.bind_user(&missing_user).await?,
        UserSubscriptionBindOutcome::UserNotFound
    ));
    let missing_creator = SubscriptionPlanWrite::new(
        test_plan_id(0x36),
        "缺失创建者".to_owned(),
        UserId::new(9_999_998)?,
        Quota::new(1_000)?,
        SubscriptionCycle::Daily,
        CREATED_AT,
    )?;
    assert!(matches!(
        fixture.repository.create_plan(&missing_creator).await?,
        SubscriptionPlanCreateOutcome::CreatorNotFound
    ));
    assert_eq!(
        user_subscriptions::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_same_binding_creates_exactly_one_row() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = test_plan_id(0x37);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "并发计划",
            2_000,
            SubscriptionCycle::Monthly,
        )?)
        .await?;
    let first_bind = fixture.bind(0x45, fixture.first_user_id, plan_id)?;
    let second_bind = fixture.bind(0x45, fixture.first_user_id, plan_id)?;
    let first_repository = fixture.repository.clone();
    let second_repository = fixture.repository.clone();
    let (first, second) = tokio::join!(
        first_repository.bind_user(&first_bind),
        second_repository.bind_user(&second_bind)
    );
    let outcomes = [first?, second?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, UserSubscriptionBindOutcome::Created(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, UserSubscriptionBindOutcome::Existing(_)))
            .count(),
        1
    );
    assert_eq!(
        user_subscriptions::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn active_models_cannot_bypass_subscription_repositories() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = test_plan_id(0x38);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "实体边界",
            3_000,
            SubscriptionCycle::Weekly,
        )?)
        .await?;
    let plan = subscription_plans::Entity::find()
        .one(fixture.pool.connection())
        .await?
        .expect("订阅计划必须存在");
    let mut plan_update = plan.clone().into_active_model();
    plan_update.status = Set(SubscriptionPlanStatus::Disabled.code());
    assert!(plan_update.update(fixture.pool.connection()).await.is_err());

    fixture
        .repository
        .bind_user(&fixture.bind(0x46, fixture.first_user_id, plan_id)?)
        .await?;
    let subscription = user_subscriptions::Entity::find()
        .one(fixture.pool.connection())
        .await?
        .expect("用户订阅必须存在");
    let mut subscription_update = subscription.into_active_model();
    subscription_update.quota_used = Set(1);
    assert!(
        subscription_update
            .update(fixture.pool.connection())
            .await
            .is_err()
    );

    let invalid_time = database_time(CREATED_AT);
    let invalid_plan = subscription_plans::ActiveModel {
        plan_key: Set(SensitiveString::from(test_plan_id(0x39).persistence_key())),
        name: Set("零额度计划".to_owned()),
        created_by_user_id: Set(fixture.admin_user_id.get()),
        status: Set(SubscriptionPlanStatus::Active.code()),
        quota_amount: Set(0),
        cycle: Set(SubscriptionCycle::Daily.code()),
        version: Set(1),
        disabled_at: Set(None),
        created_at: Set(invalid_time),
        updated_at: Set(invalid_time),
        ..Default::default()
    };
    assert!(
        invalid_plan
            .insert(fixture.pool.connection())
            .await
            .is_err()
    );
    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: DatabasePool,
    repository: super::SubscriptionRepository,
    admin_user_id: UserId,
    first_user_id: UserId,
    second_user_id: UserId,
}

impl Fixture {
    fn plan(
        &self,
        marker: u8,
        name: &str,
        quota_amount: i64,
        cycle: SubscriptionCycle,
    ) -> Result<SubscriptionPlanWrite, SubscriptionInputError> {
        self.plan_with_id(test_plan_id(marker), name, quota_amount, cycle)
    }

    fn plan_with_id(
        &self,
        plan_id: SubscriptionPlanId,
        name: &str,
        quota_amount: i64,
        cycle: SubscriptionCycle,
    ) -> Result<SubscriptionPlanWrite, SubscriptionInputError> {
        SubscriptionPlanWrite::new(
            plan_id,
            name.to_owned(),
            self.admin_user_id,
            Quota::new(quota_amount).expect("测试订阅额度必须非负"),
            cycle,
            CREATED_AT,
        )
    }

    fn bind(
        &self,
        marker: u8,
        user_id: UserId,
        plan_id: SubscriptionPlanId,
    ) -> Result<UserSubscriptionBind, SubscriptionInputError> {
        UserSubscriptionBind::new(
            subscription_id(marker),
            user_id,
            plan_id,
            WINDOW_STARTED_AT,
            WINDOW_ENDS_AT,
            BOUND_AT,
        )
    }
}

async fn setup_fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("subscription-test".to_owned()),
        display_name: Set("订阅测试".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let admin = users
        .create(AdminUserCreateRecord::new(
            "subscription-admin".to_owned(),
            None,
            None,
            1,
            1,
            GroupId::new(group.id)?,
            0,
            None,
            None,
        ))
        .await?;
    let first = users
        .create(AdminUserCreateRecord::new(
            "subscription-user-a".to_owned(),
            None,
            None,
            0,
            1,
            GroupId::new(group.id)?,
            0,
            None,
            None,
        ))
        .await?;
    let second = users
        .create(AdminUserCreateRecord::new(
            "subscription-user-b".to_owned(),
            None,
            None,
            0,
            1,
            GroupId::new(group.id)?,
            0,
            None,
            None,
        ))
        .await?;
    Ok(Fixture {
        repository: super::SubscriptionRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        admin_user_id: admin.user_id(),
        first_user_id: first.user_id(),
        second_user_id: second.user_id(),
    })
}

fn test_plan_id(marker: u8) -> SubscriptionPlanId {
    SubscriptionPlanId::new([marker; 16]).expect("测试计划标识必须非零")
}

fn subscription_id(marker: u8) -> UserSubscriptionId {
    UserSubscriptionId::new([marker; 16]).expect("测试用户订阅标识必须非零")
}

fn database_time(value: u64) -> TimeDateTimeWithTimeZone {
    TimeDateTimeWithTimeZone::from_unix_timestamp(value as i64).expect("测试时间戳必须有效")
}
