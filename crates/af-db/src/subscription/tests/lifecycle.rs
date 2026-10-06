use std::error::Error;

use af_domain::{
    SubscriptionCycle, SubscriptionPlanId, SubscriptionWindow, UserSubscriptionId,
    UserSubscriptionStatus,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, sea_query::Expr};

use crate::{
    SubscriptionExpirationDueCursor, SubscriptionInputError, SubscriptionRepositoryError,
    UserSubscriptionBind, UserSubscriptionLifecycleTransition,
    UserSubscriptionLifecycleTransitionOutcome, UserSubscriptionWindowAdvance,
    UserSubscriptionWindowAdvanceOutcome,
    entity::{SensitiveString, user_subscriptions},
};

use super::{CREATED_AT, Fixture, setup_fixture, subscription_id, test_plan_id};

#[test]
fn lifecycle_inputs_reject_invalid_commands_and_cursors() {
    let id = subscription_id(0x91);
    assert_eq!(
        UserSubscriptionLifecycleTransition::new(
            id,
            1,
            UserSubscriptionStatus::Active,
            UserSubscriptionStatus::Expired,
            CREATED_AT,
            CREATED_AT + 100,
            CREATED_AT + 10,
        )
        .unwrap_err(),
        SubscriptionInputError::InvalidLifecycleTransition
    );
    assert_eq!(
        UserSubscriptionLifecycleTransition::new(
            id,
            0,
            UserSubscriptionStatus::Active,
            UserSubscriptionStatus::Suspended,
            CREATED_AT,
            CREATED_AT + 100,
            CREATED_AT + 10,
        )
        .unwrap_err(),
        SubscriptionInputError::InvalidVersion
    );
    assert_eq!(
        SubscriptionExpirationDueCursor::new(CREATED_AT, 0),
        Err(SubscriptionInputError::InvalidCursor)
    );
}

#[tokio::test]
async fn pause_and_current_window_resume_preserve_window_and_quota() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = create_plan(&fixture, 0x91, "暂停恢复计划", SubscriptionCycle::Daily).await?;
    let (id, window) = bind_subscription(&fixture, 0xa1, plan_id, CREATED_AT).await?;
    set_quota_used(&fixture, id, 123).await?;

    let pause = transition_from_current(
        &fixture,
        id,
        UserSubscriptionStatus::Suspended,
        CREATED_AT + 60,
    )
    .await?;
    let UserSubscriptionLifecycleTransitionOutcome::Applied(paused) = fixture
        .repository
        .transition_user_subscription_lifecycle(&pause)
        .await?
    else {
        panic!("Active 订阅必须能够暂停")
    };
    assert_eq!(
        paused.subscription().status(),
        UserSubscriptionStatus::Suspended
    );
    assert_eq!(paused.subscription().quota_used().units(), 123);
    assert_eq!(
        paused.subscription().window_started_at(),
        window.started_at()
    );
    assert_eq!(paused.subscription().window_ends_at(), window.ends_at());

    let resume = UserSubscriptionLifecycleTransition::from_record(
        paused.subscription(),
        UserSubscriptionStatus::Active,
        window.ends_at() - 1,
    )?;
    let UserSubscriptionLifecycleTransitionOutcome::Applied(resumed) = fixture
        .repository
        .transition_user_subscription_lifecycle(&resume)
        .await?
    else {
        panic!("未到期暂停订阅必须能够恢复")
    };
    assert_eq!(resumed.periods_elapsed(), 0);
    assert_eq!(
        resumed.subscription().status(),
        UserSubscriptionStatus::Active
    );
    assert_eq!(resumed.subscription().quota_used().units(), 123);
    assert_eq!(
        resumed.subscription().window_started_at(),
        window.started_at()
    );
    assert_eq!(resumed.subscription().window_ends_at(), window.ends_at());
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn stale_window_resume_advances_and_replays_after_new_usage() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = create_plan(&fixture, 0x92, "陈旧窗口恢复", SubscriptionCycle::Daily).await?;
    let (id, window) = bind_subscription(&fixture, 0xa2, plan_id, CREATED_AT).await?;
    set_quota_used(&fixture, id, 456).await?;

    let pause = transition_from_current(
        &fixture,
        id,
        UserSubscriptionStatus::Suspended,
        CREATED_AT + 60,
    )
    .await?;
    let UserSubscriptionLifecycleTransitionOutcome::Applied(paused) = fixture
        .repository
        .transition_user_subscription_lifecycle(&pause)
        .await?
    else {
        panic!("测试订阅必须先暂停")
    };
    let resumed_at = window.ends_at() + 2 * 86_400 + 1;
    let resume = UserSubscriptionLifecycleTransition::from_record(
        paused.subscription(),
        UserSubscriptionStatus::Active,
        resumed_at,
    )?;

    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture
            .repository
            .transition_user_subscription_lifecycle(&resume)
            .await
            .unwrap_err(),
        SubscriptionRepositoryError::OutcomeUnknown
    );
    set_quota_used(&fixture, id, 7).await?;
    let UserSubscriptionLifecycleTransitionOutcome::Existing(replayed) = fixture
        .repository
        .transition_user_subscription_lifecycle(&resume)
        .await?
    else {
        panic!("提交结果未知后必须恢复同一个生命周期迁移")
    };
    assert_eq!(replayed.periods_elapsed(), 3);
    assert_eq!(
        replayed.subscription().status(),
        UserSubscriptionStatus::Active
    );
    assert_eq!(replayed.subscription().quota_used().units(), 7);
    assert_eq!(
        replayed.subscription().window_started_at(),
        window.ends_at() + 2 * 86_400
    );
    assert_eq!(
        replayed.subscription().window_ends_at(),
        window.ends_at() + 3 * 86_400
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn cancel_paths_preserve_window_and_expire_only_when_due() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = create_plan(&fixture, 0x93, "取消过期计划", SubscriptionCycle::Daily).await?;
    let (active_id, window) = bind_subscription(&fixture, 0xa3, plan_id, CREATED_AT).await?;
    set_quota_used(&fixture, active_id, 55).await?;

    let cancel = transition_from_current(
        &fixture,
        active_id,
        UserSubscriptionStatus::Canceled,
        window.ends_at() - 100,
    )
    .await?;
    let UserSubscriptionLifecycleTransitionOutcome::Applied(canceled) = fixture
        .repository
        .transition_user_subscription_lifecycle(&cancel)
        .await?
    else {
        panic!("Active 订阅必须能够取消")
    };
    assert_eq!(canceled.subscription().quota_used().units(), 55);
    assert_eq!(canceled.subscription().window_ends_at(), window.ends_at());

    let early_expiration = UserSubscriptionLifecycleTransition::from_record(
        canceled.subscription(),
        UserSubscriptionStatus::Expired,
        window.ends_at() - 1,
    )?;
    let UserSubscriptionLifecycleTransitionOutcome::NotDue(not_due) = fixture
        .repository
        .transition_user_subscription_lifecycle(&early_expiration)
        .await?
    else {
        panic!("取消订阅在窗口结束前不得过期")
    };
    assert_eq!(not_due.status(), UserSubscriptionStatus::Canceled);
    assert_eq!(not_due.version(), canceled.subscription().version());

    let expiration = UserSubscriptionLifecycleTransition::from_record(
        canceled.subscription(),
        UserSubscriptionStatus::Expired,
        window.ends_at(),
    )?;
    let UserSubscriptionLifecycleTransitionOutcome::Applied(expired) = fixture
        .repository
        .transition_user_subscription_lifecycle(&expiration)
        .await?
    else {
        panic!("到达窗口终点的取消订阅必须能够过期")
    };
    assert_eq!(
        expired.subscription().status(),
        UserSubscriptionStatus::Expired
    );
    assert_eq!(expired.subscription().quota_used().units(), 55);
    assert_eq!(expired.subscription().window_ends_at(), window.ends_at());

    let (suspended_id, _) = bind_subscription(&fixture, 0xa4, plan_id, CREATED_AT + 1).await?;
    apply_transition(
        &fixture,
        suspended_id,
        UserSubscriptionStatus::Suspended,
        CREATED_AT + 70,
    )
    .await?;
    let suspended_cancel = transition_from_current(
        &fixture,
        suspended_id,
        UserSubscriptionStatus::Canceled,
        CREATED_AT + 80,
    )
    .await?;
    let UserSubscriptionLifecycleTransitionOutcome::Applied(from_suspended) = fixture
        .repository
        .transition_user_subscription_lifecycle(&suspended_cancel)
        .await?
    else {
        panic!("Suspended 订阅必须能够取消")
    };
    assert_eq!(
        from_suspended.subscription().status(),
        UserSubscriptionStatus::Canceled
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn expiration_scan_pages_only_due_canceled_subscriptions() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = create_plan(&fixture, 0x94, "到期扫描计划", SubscriptionCycle::Daily).await?;
    let (first_id, window) = bind_subscription(&fixture, 0xa5, plan_id, CREATED_AT).await?;
    let (second_id, _) = bind_subscription(&fixture, 0xa6, plan_id, CREATED_AT + 1).await?;
    let (expired_id, _) = bind_subscription(&fixture, 0xa7, plan_id, CREATED_AT + 2).await?;
    let (active_id, _) = bind_subscription(&fixture, 0xa8, plan_id, CREATED_AT + 3).await?;
    let (future_id, future_window) =
        bind_subscription(&fixture, 0xa9, plan_id, window.ends_at() + 1).await?;

    for (id, changed_at) in [
        (first_id, CREATED_AT + 10),
        (second_id, CREATED_AT + 11),
        (expired_id, CREATED_AT + 12),
        (future_id, future_window.started_at() + 10),
    ] {
        apply_transition(&fixture, id, UserSubscriptionStatus::Canceled, changed_at).await?;
    }
    apply_transition(
        &fixture,
        expired_id,
        UserSubscriptionStatus::Expired,
        window.ends_at(),
    )
    .await?;

    let first = fixture
        .repository
        .list_expiration_due_subscriptions(window.ends_at(), None, 1)
        .await?;
    assert_eq!(first.subscriptions().len(), 1);
    assert_eq!(first.subscriptions()[0].subscription_id(), first_id);
    let cursor = first.next_cursor().expect("仍有到期取消订阅时必须返回游标");
    let second = fixture
        .repository
        .list_expiration_due_subscriptions(window.ends_at(), Some(cursor), 1)
        .await?;
    assert_eq!(second.subscriptions().len(), 1);
    assert_eq!(second.subscriptions()[0].subscription_id(), second_id);
    assert!(second.next_cursor().is_none());
    assert!(
        second
            .subscriptions()
            .iter()
            .all(|record| record.subscription_id() != expired_id)
    );
    let reset_due = fixture
        .repository
        .list_reset_due_subscriptions(window.ends_at(), None, 100)
        .await?;
    assert_eq!(reset_due.subscriptions().len(), 1);
    assert_eq!(reset_due.subscriptions()[0].subscription_id(), active_id);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn stale_version_status_and_window_commands_conflict() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = create_plan(&fixture, 0x95, "冲突检测计划", SubscriptionCycle::Daily).await?;
    let (id, window) = bind_subscription(&fixture, 0xaa, plan_id, CREATED_AT).await?;
    let pause = transition_from_current(
        &fixture,
        id,
        UserSubscriptionStatus::Suspended,
        CREATED_AT + 20,
    )
    .await?;
    let UserSubscriptionLifecycleTransitionOutcome::Applied(paused) = fixture
        .repository
        .transition_user_subscription_lifecycle(&pause)
        .await?
    else {
        panic!("冲突测试订阅必须先暂停")
    };

    let stale_version = UserSubscriptionLifecycleTransition::new(
        id,
        1,
        UserSubscriptionStatus::Active,
        UserSubscriptionStatus::Canceled,
        window.started_at(),
        window.ends_at(),
        CREATED_AT + 21,
    )?;
    assert_eq!(
        fixture
            .repository
            .transition_user_subscription_lifecycle(&stale_version)
            .await
            .unwrap_err(),
        SubscriptionRepositoryError::Conflict
    );

    let stale_status = UserSubscriptionLifecycleTransition::new(
        id,
        paused.subscription().version(),
        UserSubscriptionStatus::Active,
        UserSubscriptionStatus::Canceled,
        window.started_at(),
        window.ends_at(),
        CREATED_AT + 22,
    )?;
    assert_eq!(
        fixture
            .repository
            .transition_user_subscription_lifecycle(&stale_status)
            .await
            .unwrap_err(),
        SubscriptionRepositoryError::Conflict
    );

    let stale_window = UserSubscriptionLifecycleTransition::new(
        id,
        paused.subscription().version(),
        UserSubscriptionStatus::Suspended,
        UserSubscriptionStatus::Active,
        window.ends_at(),
        window.ends_at() + 86_400,
        window.ends_at() + 1,
    )?;
    assert_eq!(
        fixture
            .repository
            .transition_user_subscription_lifecycle(&stale_window)
            .await
            .unwrap_err(),
        SubscriptionRepositoryError::Conflict
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_same_transition_applies_once_and_replays_once() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = create_plan(&fixture, 0x96, "并发生命周期", SubscriptionCycle::Daily).await?;
    let (id, window) = bind_subscription(&fixture, 0xab, plan_id, CREATED_AT).await?;
    let first = UserSubscriptionLifecycleTransition::new(
        id,
        1,
        UserSubscriptionStatus::Active,
        UserSubscriptionStatus::Suspended,
        window.started_at(),
        window.ends_at(),
        CREATED_AT + 30,
    )?;
    let second = UserSubscriptionLifecycleTransition::new(
        id,
        1,
        UserSubscriptionStatus::Active,
        UserSubscriptionStatus::Suspended,
        window.started_at(),
        window.ends_at(),
        CREATED_AT + 30,
    )?;
    let first_repository = fixture.repository.clone();
    let second_repository = fixture.repository.clone();
    let (first_outcome, second_outcome) = tokio::join!(
        first_repository.transition_user_subscription_lifecycle(&first),
        second_repository.transition_user_subscription_lifecycle(&second)
    );
    let outcomes = [first_outcome?, second_outcome?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                UserSubscriptionLifecycleTransitionOutcome::Applied(_)
            ))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                UserSubscriptionLifecycleTransitionOutcome::Existing(_)
            ))
            .count(),
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn window_advance_and_cancel_competition_converges_by_cas() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture().await?;
    let plan_id = create_plan(&fixture, 0x97, "窗口取消竞争", SubscriptionCycle::Daily).await?;
    let (id, window) = bind_subscription(&fixture, 0xac, plan_id, CREATED_AT).await?;
    let page = fixture
        .repository
        .list_user_subscriptions(fixture.first_user_id, None, 10)
        .await?;
    let current = page.subscriptions().first().expect("竞争测试订阅必须存在");
    let advance = UserSubscriptionWindowAdvance::from_record(current, window.ends_at())?;
    let cancel = UserSubscriptionLifecycleTransition::from_record(
        current,
        UserSubscriptionStatus::Canceled,
        window.ends_at(),
    )?;
    let advance_repository = fixture.repository.clone();
    let cancel_repository = fixture.repository.clone();
    let (advance_outcome, cancel_outcome) = tokio::join!(
        advance_repository.advance_user_subscription_window(&advance),
        cancel_repository.transition_user_subscription_lifecycle(&cancel)
    );
    match (advance_outcome, cancel_outcome) {
        (
            Ok(UserSubscriptionWindowAdvanceOutcome::Applied(_)),
            Err(SubscriptionRepositoryError::Conflict),
        )
        | (
            Err(SubscriptionRepositoryError::Conflict),
            Ok(UserSubscriptionLifecycleTransitionOutcome::Applied(_)),
        ) => {}
        _ => panic!("窗口推进与取消竞争必须仅提交一个 CAS"),
    }

    let page = fixture
        .repository
        .list_user_subscriptions(fixture.first_user_id, None, 10)
        .await?;
    let current = page.subscriptions().first().expect("竞争结果必须可读取");
    assert_eq!(current.version(), 2);
    assert!(
        (current.status() == UserSubscriptionStatus::Active
            && current.window_started_at() == window.ends_at())
            || (current.status() == UserSubscriptionStatus::Canceled
                && current.window_started_at() == window.started_at())
    );
    assert_eq!(current.subscription_id(), id);
    fixture.pool.close().await?;
    Ok(())
}

async fn create_plan(
    fixture: &Fixture,
    marker: u8,
    name: &str,
    cycle: SubscriptionCycle,
) -> Result<SubscriptionPlanId, Box<dyn Error>> {
    let plan_id = test_plan_id(marker);
    fixture
        .repository
        .create_plan(&fixture.plan_with_id(plan_id, name, 10_000, cycle)?)
        .await?;
    Ok(plan_id)
}

async fn bind_subscription(
    fixture: &Fixture,
    marker: u8,
    plan_id: SubscriptionPlanId,
    bound_at: u64,
) -> Result<(UserSubscriptionId, SubscriptionWindow), Box<dyn Error>> {
    let window = SubscriptionWindow::initial(SubscriptionCycle::Daily, bound_at)?;
    let id = subscription_id(marker);
    fixture
        .repository
        .bind_user(&UserSubscriptionBind::new(
            id,
            fixture.first_user_id,
            plan_id,
            window.started_at(),
            window.ends_at(),
            bound_at,
        )?)
        .await?;
    Ok((id, window))
}

async fn transition_from_current(
    fixture: &Fixture,
    id: UserSubscriptionId,
    target_status: UserSubscriptionStatus,
    changed_at: u64,
) -> Result<UserSubscriptionLifecycleTransition, Box<dyn Error>> {
    let page = fixture
        .repository
        .list_user_subscriptions(fixture.first_user_id, None, 100)
        .await?;
    let current = page
        .subscriptions()
        .iter()
        .find(|record| record.subscription_id() == id)
        .ok_or("生命周期测试订阅不存在")?;
    Ok(UserSubscriptionLifecycleTransition::from_record(
        current,
        target_status,
        changed_at,
    )?)
}

async fn apply_transition(
    fixture: &Fixture,
    id: UserSubscriptionId,
    target_status: UserSubscriptionStatus,
    changed_at: u64,
) -> Result<(), Box<dyn Error>> {
    let transition = transition_from_current(fixture, id, target_status, changed_at).await?;
    assert!(matches!(
        fixture
            .repository
            .transition_user_subscription_lifecycle(&transition)
            .await?,
        UserSubscriptionLifecycleTransitionOutcome::Applied(_)
    ));
    Ok(())
}

async fn set_quota_used(
    fixture: &Fixture,
    id: UserSubscriptionId,
    quota_used: i64,
) -> Result<(), Box<dyn Error>> {
    let result = user_subscriptions::Entity::update_many()
        .filter(
            user_subscriptions::Column::SubscriptionKey
                .eq(SensitiveString::from(id.persistence_key())),
        )
        .col_expr(
            user_subscriptions::Column::QuotaUsed,
            Expr::value(quota_used),
        )
        .exec(fixture.pool.connection())
        .await?;
    assert_eq!(result.rows_affected, 1);
    Ok(())
}
