use std::{error::Error, time::Duration};

use af_domain::{
    GroupId, Quota, SubscriptionCycle, SubscriptionOrderId, SubscriptionOrderRequestId,
    SubscriptionOrderStatus, SubscriptionPaymentEventId, SubscriptionPaymentEventType,
    SubscriptionPlanId, SubscriptionPlanStatus, SubscriptionWindow, UserId, UserSubscriptionId,
    UserSubscriptionStatus,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, IntoActiveModel,
    PaginatorTrait, QueryFilter, Set, Statement, entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::Expr,
};

use super::{
    SubscriptionInputError, SubscriptionOrderCreate, SubscriptionOrderCreateOutcome,
    SubscriptionPaymentEventOutcome, SubscriptionPaymentEventRejection,
    SubscriptionPaymentEventWrite, SubscriptionPlanCreateOutcome, SubscriptionPlanDisable,
    SubscriptionPlanDisableOutcome, SubscriptionPlanWrite, SubscriptionRepositoryError,
    SubscriptionResetDueCursor, UserSubscriptionBind, UserSubscriptionBindOutcome,
    UserSubscriptionWindowAdvance, UserSubscriptionWindowAdvanceOutcome, monotonic_updated_at,
};
use crate::{
    AdminUserCreateRecord, AdminUserRepository, DatabaseOptions, DatabasePool, MigrationOptions,
    entity::{
        SensitiveString, groups, subscription_orders, subscription_payment_events,
        subscription_plans, user_notification_events, user_subscriptions,
    },
};

mod lifecycle;

const CREATED_AT: u64 = 1_900_000_000;
const WINDOW_STARTED_AT: u64 = CREATED_AT + 60;
const WINDOW_ENDS_AT: u64 = WINDOW_STARTED_AT + 86_400;
const BOUND_AT: u64 = CREATED_AT + 30;
const PAYMENT_RECEIVED_AT: u64 = CREATED_AT + 60;

#[test]
fn subscription_audit_time_uses_fixed_second_precision_without_rollback() {
    let current = TimeDateTimeWithTimeZone::from_unix_timestamp(4_000_000_000).unwrap();
    let next = TimeDateTimeWithTimeZone::from_unix_timestamp(4_000_000_001)
        .unwrap()
        .replace_nanosecond(123_456_789)
        .unwrap();
    assert_eq!(
        monotonic_updated_at(current, next),
        TimeDateTimeWithTimeZone::from_unix_timestamp(4_000_000_001).unwrap()
    );

    let future_current = current.replace_nanosecond(987_654_321).unwrap();
    assert_eq!(
        monotonic_updated_at(future_current, current),
        future_current
    );
}

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

#[test]
fn subscription_window_commands_validate_versions_and_cursors() {
    assert_eq!(
        SubscriptionResetDueCursor::new(1, 0).unwrap_err(),
        SubscriptionInputError::InvalidCursor
    );
    assert_eq!(
        SubscriptionResetDueCursor::new(i64::MAX as u64 + 1, 1).unwrap_err(),
        SubscriptionInputError::InvalidCursor
    );
    assert_eq!(
        UserSubscriptionWindowAdvance::new(
            subscription_id(0x25),
            0,
            WINDOW_STARTED_AT,
            WINDOW_ENDS_AT,
            WINDOW_ENDS_AT,
        )
        .unwrap_err(),
        SubscriptionInputError::InvalidVersion
    );
    assert_eq!(
        UserSubscriptionWindowAdvance::new(
            subscription_id(0x25),
            1,
            WINDOW_ENDS_AT,
            WINDOW_STARTED_AT,
            WINDOW_ENDS_AT,
        )
        .unwrap_err(),
        SubscriptionInputError::InvalidWindow
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
async fn order_creation_recovers_unknown_outcome_and_rejects_price_conflicts()
-> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = test_plan_id(0x3a);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "订单幂等计划",
            25_000,
            SubscriptionCycle::Monthly,
        )?)
        .await?;
    let second_plan_id = test_plan_id(0x3b);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            second_plan_id,
            "订单其他计划",
            30_000,
            SubscriptionCycle::Monthly,
        )?)
        .await?;

    let create = fixture.order(0x71, 0x72, plan_id, 100)?;
    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture.repository.create_order(&create).await.unwrap_err(),
        SubscriptionRepositoryError::OutcomeUnknown
    );

    let SubscriptionOrderCreateOutcome::Existing(existing) =
        fixture.repository.create_order(&create).await?
    else {
        panic!("订单提交结果未知后必须恢复同一订单")
    };
    assert_eq!(existing.plan_id(), plan_id);
    assert_eq!(existing.plan_version(), 1);
    assert_eq!(existing.provider(), "stripe");
    assert_eq!(existing.currency(), "USD");
    assert_eq!(existing.amount_minor(), 100);
    assert_eq!(existing.quota_amount().units(), 25_000);
    assert_eq!(existing.status(), SubscriptionOrderStatus::Created);

    // 仅改变冗余外键列，验证恢复记录以不可变 plan_key 为准。
    let second_plan_database_id = subscription_plans::Entity::find()
        .filter(
            subscription_plans::Column::PlanKey
                .eq(SensitiveString::from(second_plan_id.persistence_key())),
        )
        .one(fixture.pool.connection())
        .await?
        .expect("第二个订阅计划必须存在")
        .id;
    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE subscription_orders SET plan_id = ? WHERE id = ?",
            [
                second_plan_database_id.into(),
                existing.database_id().into(),
            ],
        ))
        .await?;
    let SubscriptionOrderCreateOutcome::Existing(recovered) =
        fixture.repository.create_order(&create).await?
    else {
        panic!("已存在订单必须返回 Existing")
    };
    assert_eq!(recovered.plan_id(), plan_id);
    assert_eq!(recovered.quota_amount().units(), 25_000);

    let changed_price = fixture.order(0x73, 0x72, plan_id, 101)?;
    assert_eq!(
        fixture
            .repository
            .create_order(&changed_price)
            .await
            .unwrap_err(),
        SubscriptionRepositoryError::Conflict
    );
    assert_eq!(
        subscription_orders::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn verified_payment_binds_subscription_and_replays_without_duplicate_effects()
-> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = test_plan_id(0x7a);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "支付确认计划",
            25_000,
            SubscriptionCycle::Monthly,
        )?)
        .await?;
    let order = fixture.order(0x7b, 0x7c, plan_id, 100)?;
    let order_id = order.order_id;
    fixture.repository.create_order(&order).await?;

    let event = fixture.payment_event(0x7d, order_id, "evt-subscription-paid", 100)?;
    let applied = fixture.repository.accept_verified_event(&event).await?;
    let SubscriptionPaymentEventOutcome::Applied {
        order: paid,
        subscription,
    } = applied
    else {
        panic!("首次成功支付必须推进订单并创建订阅")
    };
    assert_eq!(paid.status(), SubscriptionOrderStatus::Paid);
    assert_eq!(paid.trade_no(), Some("trade-subscription-paid"));
    assert_eq!(paid.payment_method(), Some("card"));
    assert!(subscription.is_some());

    let replay = fixture.repository.accept_verified_event(&event).await?;
    let SubscriptionPaymentEventOutcome::Existing {
        order: replayed,
        subscription,
    } = replay
    else {
        panic!("相同 Provider 事件必须返回 Existing")
    };
    assert_eq!(replayed.status(), SubscriptionOrderStatus::Paid);
    assert!(subscription.is_some());
    assert_eq!(
        subscription_payment_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        user_subscriptions::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        user_notification_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn verified_payment_conflicts_record_facts_and_roll_back_binding_conflicts()
-> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = test_plan_id(0x8a);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "支付冲突计划",
            25_000,
            SubscriptionCycle::Monthly,
        )?)
        .await?;
    let order = fixture.order(0x8b, 0x8c, plan_id, 100)?;
    let order_id = order.order_id;
    fixture.repository.create_order(&order).await?;

    let mismatch = fixture.payment_event(0x8d, order_id, "evt-subscription-mismatch", 101)?;
    let outcome = fixture.repository.accept_verified_event(&mismatch).await?;
    assert!(matches!(
        outcome,
        SubscriptionPaymentEventOutcome::RecordedUnprocessed {
            reason: SubscriptionPaymentEventRejection::AmountMismatch,
            ..
        }
    ));
    let SubscriptionOrderCreateOutcome::Existing(existing) =
        fixture.repository.create_order(&order).await?
    else {
        panic!("冲突事件不应改变订单")
    };
    assert_eq!(existing.status(), SubscriptionOrderStatus::Created);

    fixture
        .repository
        .bind_user(&fixture.bind(0x8b, fixture.first_user_id, plan_id)?)
        .await?;
    let matching =
        fixture.payment_event(0x8f, order_id, "evt-subscription-binding-conflict", 100)?;
    assert_eq!(
        fixture
            .repository
            .accept_verified_event(&matching)
            .await
            .unwrap_err(),
        SubscriptionRepositoryError::BindingConflict
    );
    let order_model = subscription_orders::Entity::find()
        .filter(subscription_orders::Column::OrderKey.eq(order_id.persistence_key()))
        .one(fixture.pool.connection())
        .await?
        .expect("订单必须保留");
    assert_eq!(order_model.status, SubscriptionOrderStatus::Created.code());
    assert_eq!(
        subscription_payment_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        user_notification_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
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
    let subscription = fixture
        .repository
        .get_user_subscription(subscription_id(0x62))
        .await?
        .expect("用户订阅必须可按稳定标识读取");
    assert_eq!(subscription.user_id(), fixture.first_user_id);
    assert_eq!(subscription.plan_id(), plan_ids[1]);
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
async fn reset_due_scan_filters_active_rows_and_pages_by_composite_cursor()
-> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = test_plan_id(0x57);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "到期扫描计划",
            10_000,
            SubscriptionCycle::Daily,
        )?)
        .await?;
    let windows = [
        SubscriptionWindow::initial(SubscriptionCycle::Daily, CREATED_AT)?,
        SubscriptionWindow::initial(SubscriptionCycle::Daily, CREATED_AT + 86_400)?,
        SubscriptionWindow::initial(SubscriptionCycle::Daily, CREATED_AT + 2 * 86_400)?,
        SubscriptionWindow::initial(SubscriptionCycle::Daily, CREATED_AT + 4 * 86_400)?,
    ];
    let ids = [
        subscription_id(0x71),
        subscription_id(0x72),
        subscription_id(0x73),
        subscription_id(0x74),
    ];
    for (subscription_id, window) in ids.into_iter().zip(windows) {
        let bind = UserSubscriptionBind::new(
            subscription_id,
            fixture.first_user_id,
            plan_id,
            window.started_at(),
            window.ends_at(),
            window.started_at(),
        )?;
        fixture.repository.bind_user(&bind).await?;
    }
    let suspended_at = windows[1].ends_at() + 10;
    user_subscriptions::Entity::update_many()
        .filter(
            user_subscriptions::Column::SubscriptionKey
                .eq(SensitiveString::from(ids[1].persistence_key())),
        )
        .col_expr(
            user_subscriptions::Column::Status,
            Expr::value(UserSubscriptionStatus::Suspended.code()),
        )
        .col_expr(user_subscriptions::Column::Version, Expr::value(2_i64))
        .col_expr(
            user_subscriptions::Column::StatusChangedAt,
            Expr::value(database_time(suspended_at)),
        )
        .col_expr(
            user_subscriptions::Column::UpdatedAt,
            Expr::value(database_time(suspended_at)),
        )
        .exec(fixture.pool.connection())
        .await?;

    let now = windows[2].ends_at();
    let first = fixture
        .repository
        .list_reset_due_subscriptions(now, None, 1)
        .await?;
    assert_eq!(first.subscriptions()[0].subscription_id(), ids[0]);
    let cursor = first.next_cursor().expect("仍有到期订阅时必须返回复合游标");
    let second = fixture
        .repository
        .list_reset_due_subscriptions(now, Some(cursor), 1)
        .await?;
    assert_eq!(second.subscriptions().len(), 1);
    assert_eq!(second.subscriptions()[0].subscription_id(), ids[2]);
    assert!(second.next_cursor().is_none());
    assert!(
        !second
            .subscriptions()
            .iter()
            .any(|subscription| subscription.subscription_id() == ids[1])
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn advancing_subscription_window_resets_quota_skips_periods_and_replays()
-> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = test_plan_id(0x58);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "周期推进计划",
            10_000,
            SubscriptionCycle::Daily,
        )?)
        .await?;
    let window = SubscriptionWindow::initial(SubscriptionCycle::Daily, CREATED_AT)?;
    let subscription_id = subscription_id(0x75);
    let bind = UserSubscriptionBind::new(
        subscription_id,
        fixture.first_user_id,
        plan_id,
        window.started_at(),
        window.ends_at(),
        CREATED_AT,
    )?;
    fixture.repository.bind_user(&bind).await?;
    user_subscriptions::Entity::update_many()
        .filter(
            user_subscriptions::Column::SubscriptionKey
                .eq(SensitiveString::from(subscription_id.persistence_key())),
        )
        .col_expr(user_subscriptions::Column::QuotaUsed, Expr::value(123_i64))
        .exec(fixture.pool.connection())
        .await?;

    let current_page = fixture
        .repository
        .list_user_subscriptions(fixture.first_user_id, None, 10)
        .await?;
    let current = current_page
        .subscriptions()
        .first()
        .expect("周期推进测试订阅必须存在");
    let now = window.ends_at() + 2 * 86_400 + 1;
    let advance = UserSubscriptionWindowAdvance::from_record(current, now)?;
    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture
            .repository
            .advance_user_subscription_window(&advance)
            .await
            .unwrap_err(),
        SubscriptionRepositoryError::OutcomeUnknown
    );
    let replay = fixture
        .repository
        .advance_user_subscription_window(&advance)
        .await?;
    let UserSubscriptionWindowAdvanceOutcome::Existing(replayed) = replay else {
        panic!("结果未知重试必须恢复同一个订阅窗口推进")
    };
    assert_eq!(replayed.periods_elapsed(), 3);
    assert_eq!(replayed.subscription().quota_used().units(), 0);
    assert_eq!(replayed.subscription().version(), 2);
    assert_eq!(
        replayed.subscription().window_started_at(),
        window.ends_at() + 2 * 86_400
    );

    let current_window = replayed.subscription();
    let not_due = UserSubscriptionWindowAdvance::new(
        subscription_id,
        current_window.version(),
        current_window.window_started_at(),
        current_window.window_ends_at(),
        current_window.window_ends_at() - 1,
    )?;
    assert!(matches!(
        fixture
            .repository
            .advance_user_subscription_window(&not_due)
            .await?,
        UserSubscriptionWindowAdvanceOutcome::NotDue(_)
    ));
    let stale = UserSubscriptionWindowAdvance::new(
        subscription_id,
        1,
        window.started_at(),
        window.ends_at(),
        current_window.window_ends_at() + 1,
    )?;
    assert_eq!(
        fixture
            .repository
            .advance_user_subscription_window(&stale)
            .await
            .unwrap_err(),
        SubscriptionRepositoryError::Conflict
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn inactive_subscription_states_are_not_reset() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = test_plan_id(0x59);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(
            plan_id,
            "生命周期占位计划",
            10_000,
            SubscriptionCycle::Daily,
        )?)
        .await?;
    let window = SubscriptionWindow::initial(SubscriptionCycle::Daily, CREATED_AT)?;
    let statuses = [
        (0x76, UserSubscriptionStatus::Suspended),
        (0x77, UserSubscriptionStatus::Canceled),
        (0x78, UserSubscriptionStatus::Expired),
    ];
    for (marker, status) in statuses {
        let id = subscription_id(marker);
        let bind = UserSubscriptionBind::new(
            id,
            fixture.first_user_id,
            plan_id,
            window.started_at(),
            window.ends_at(),
            CREATED_AT,
        )?;
        fixture.repository.bind_user(&bind).await?;
        let changed_at = window.ends_at() + 20;
        user_subscriptions::Entity::update_many()
            .filter(
                user_subscriptions::Column::SubscriptionKey
                    .eq(SensitiveString::from(id.persistence_key())),
            )
            .col_expr(
                user_subscriptions::Column::Status,
                Expr::value(status.code()),
            )
            .col_expr(user_subscriptions::Column::Version, Expr::value(2_i64))
            .col_expr(
                user_subscriptions::Column::StatusChangedAt,
                Expr::value(database_time(changed_at)),
            )
            .col_expr(
                user_subscriptions::Column::UpdatedAt,
                Expr::value(database_time(changed_at)),
            )
            .exec(fixture.pool.connection())
            .await?;
        let current_page = fixture
            .repository
            .list_user_subscriptions(fixture.first_user_id, None, 10)
            .await?;
        let current = current_page
            .subscriptions()
            .iter()
            .find(|subscription| subscription.subscription_id() == id)
            .expect("生命周期测试订阅必须存在");
        let advance = UserSubscriptionWindowAdvance::from_record(current, changed_at + 1)?;
        let outcome = fixture
            .repository
            .advance_user_subscription_window(&advance)
            .await?;
        let UserSubscriptionWindowAdvanceOutcome::Inactive(skipped) = outcome else {
            panic!("非 Active 订阅不得被窗口推进")
        };
        assert_eq!(skipped.status(), status);
        assert_eq!(skipped.version(), 2);
        assert_eq!(skipped.window_ends_at(), window.ends_at());
    }
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

    fn order(
        &self,
        order_marker: u8,
        request_marker: u8,
        plan_id: SubscriptionPlanId,
        amount_minor: i64,
    ) -> Result<SubscriptionOrderCreate, SubscriptionInputError> {
        SubscriptionOrderCreate::new(
            order_id(order_marker),
            request_id(request_marker),
            self.first_user_id,
            plan_id,
            1,
            "stripe".to_owned(),
            "USD".to_owned(),
            amount_minor,
            CREATED_AT,
            Some(CREATED_AT + 3_600),
        )
    }

    fn payment_event(
        &self,
        marker: u8,
        order_id: SubscriptionOrderId,
        provider_event_id: &str,
        amount_minor: u64,
    ) -> Result<SubscriptionPaymentEventWrite, SubscriptionInputError> {
        SubscriptionPaymentEventWrite::new(
            SubscriptionPaymentEventId::new([marker; 16]).expect("事件标识必须非零"),
            order_id,
            "stripe".to_owned(),
            provider_event_id.to_owned(),
            Some("trade-subscription-paid".to_owned()),
            Some(amount_minor),
            Some("USD".to_owned()),
            Some("card".to_owned()),
            SubscriptionPaymentEventType::Succeeded,
            [0x11; 32],
            [marker; 32],
            PAYMENT_RECEIVED_AT,
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

fn order_id(marker: u8) -> SubscriptionOrderId {
    SubscriptionOrderId::new([marker; 16]).expect("test order id must be non-zero")
}

fn request_id(marker: u8) -> SubscriptionOrderRequestId {
    SubscriptionOrderRequestId::new([marker; 16]).expect("test request id must be non-zero")
}

fn database_time(value: u64) -> TimeDateTimeWithTimeZone {
    TimeDateTimeWithTimeZone::from_unix_timestamp(value as i64).expect("测试时间戳必须有效")
}
