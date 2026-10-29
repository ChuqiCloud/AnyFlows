use std::{error::Error, time::Duration};

use af_domain::{GroupId, UserId};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter, Statement, TransactionTrait,
    sea_query::Expr,
};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions,
    entity::{groups, users},
    group_key_access::{
        GroupKeyAccessError, GroupKeyAccessRepository, GroupKeySelectionMode,
        user_can_select_key_group,
    },
};

#[tokio::test]
async fn missing_policy_keeps_default_only_and_lists_only_public_fields()
-> Result<(), Box<dyn Error>> {
    let (pool, repository) = fixture().await?;
    let policy = repository.get_policy(GroupId::new(20)?).await?;
    assert_eq!(policy.selection_mode, GroupKeySelectionMode::DefaultOnly);
    assert!(policy.allowed_user_ids.is_empty());
    let page = repository
        .list_user_groups(UserId::new(51)?, None, 50)
        .await?;
    assert_eq!(page.groups.len(), 1);
    assert_eq!(page.groups[0].group_id, GroupId::new(10)?);
    assert!(page.groups[0].is_default);
    assert_eq!(page.groups[0].display_name, "Group 10");
    assert_eq!(page.groups[0].ratio_micros, 1_000_000);
    assert!(page.next_cursor.is_none());
    let transaction = pool.connection().begin().await?;
    assert!(
        user_can_select_key_group(
            &transaction,
            UserId::new(51)?,
            GroupId::new(10)?,
            GroupId::new(10)?
        )
        .await?
    );
    assert!(
        !user_can_select_key_group(
            &transaction,
            UserId::new(51)?,
            GroupId::new(10)?,
            GroupId::new(20)?
        )
        .await?
    );
    transaction.rollback().await?;
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn grants_replace_atomically_and_user_lists_follow_current_policy()
-> Result<(), Box<dyn Error>> {
    let (pool, repository) = fixture().await?;
    let group = GroupId::new(20)?;
    let first = UserId::new(51)?;
    let second = UserId::new(52)?;
    let saved = repository
        .update_policy(
            group,
            GroupKeySelectionMode::SelectedUsers,
            vec![second, first, first],
        )
        .await?;
    assert_eq!(saved.allowed_user_ids, [first, second]);
    assert_eq!(
        repository.get_policy(group).await?.allowed_user_ids,
        [first, second]
    );
    let page = repository.list_user_groups(first, None, 1).await?;
    assert_eq!(page.groups[0].group_id, GroupId::new(10)?);
    assert_eq!(page.next_cursor, Some(GroupId::new(10)?));
    let page = repository
        .list_user_groups(first, page.next_cursor, 1)
        .await?;
    assert_eq!(page.groups[0].group_id, group);
    assert!(!page.groups[0].is_default);
    assert!(page.next_cursor.is_none());

    // 替换中的无效用户不能造成原授权部分删除或模式变化。
    assert_eq!(
        repository
            .update_policy(
                group,
                GroupKeySelectionMode::SelectedUsers,
                vec![first, UserId::new(99_999)?]
            )
            .await
            .unwrap_err(),
        GroupKeyAccessError::InvalidInput
    );
    assert_eq!(
        repository.get_policy(group).await?.allowed_user_ids,
        [first, second]
    );
    repository
        .update_policy(group, GroupKeySelectionMode::SelectedUsers, vec![first])
        .await?;
    assert_eq!(
        repository
            .list_user_groups(second, None, 50)
            .await?
            .groups
            .len(),
        1
    );
    repository
        .update_policy(group, GroupKeySelectionMode::AllUsers, vec![])
        .await?;
    assert!(
        repository
            .get_policy(group)
            .await?
            .allowed_user_ids
            .is_empty()
    );
    assert_eq!(
        repository
            .list_user_groups(second, None, 50)
            .await?
            .groups
            .len(),
        2
    );
    repository
        .update_policy(group, GroupKeySelectionMode::DefaultOnly, vec![])
        .await?;
    assert_eq!(
        repository
            .list_user_groups(first, None, 50)
            .await?
            .groups
            .len(),
        1
    );
    // 同值 PUT 必须幂等，兼容 MySQL affected rows 为零的行为。
    repository
        .update_policy(group, GroupKeySelectionMode::DefaultOnly, vec![])
        .await?;
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn storage_failure_rolls_back_mode_and_all_grant_replacements() -> Result<(), Box<dyn Error>>
{
    let (pool, repository) = fixture().await?;
    let group = GroupId::new(20)?;
    repository
        .update_policy(group, GroupKeySelectionMode::AllUsers, vec![])
        .await?;
    pool.connection().execute(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER reject_test_grant BEFORE INSERT ON user_group_grants WHEN NEW.user_id = 52 BEGIN SELECT RAISE(ABORT, 'test grant rejected'); END".to_owned()
    )).await?;
    assert_eq!(
        repository
            .update_policy(
                group,
                GroupKeySelectionMode::SelectedUsers,
                vec![UserId::new(51)?, UserId::new(52)?]
            )
            .await
            .unwrap_err(),
        GroupKeyAccessError::Internal
    );
    let policy = repository.get_policy(group).await?;
    assert_eq!(policy.selection_mode, GroupKeySelectionMode::AllUsers);
    assert!(policy.allowed_user_ids.is_empty());
    assert_eq!(
        repository
            .list_user_groups(UserId::new(52)?, None, 50)
            .await?
            .groups
            .len(),
        2
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn deleted_groups_users_and_malformed_grants_are_rejected() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = fixture().await?;
    let group = GroupId::new(20)?;
    let user = UserId::new(51)?;
    for mode in [
        GroupKeySelectionMode::DefaultOnly,
        GroupKeySelectionMode::AllUsers,
    ] {
        assert_eq!(
            repository
                .update_policy(group, mode, vec![user])
                .await
                .unwrap_err(),
            GroupKeyAccessError::InvalidInput
        );
    }
    assert_eq!(
        repository
            .update_policy(
                group,
                GroupKeySelectionMode::SelectedUsers,
                vec![user; 1_001]
            )
            .await
            .unwrap_err(),
        GroupKeyAccessError::InvalidInput
    );
    for limit in [0, 101] {
        assert_eq!(
            repository
                .list_user_groups(user, None, limit)
                .await
                .unwrap_err(),
            GroupKeyAccessError::InvalidInput
        );
    }
    users::Entity::update_many()
        .filter(users::Column::Id.eq(52_i64))
        .col_expr(
            users::Column::DeletedAt,
            Expr::value(Some(
                sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc(),
            )),
        )
        .exec(pool.connection())
        .await?;
    assert_eq!(
        repository
            .update_policy(
                group,
                GroupKeySelectionMode::SelectedUsers,
                vec![UserId::new(52)?]
            )
            .await
            .unwrap_err(),
        GroupKeyAccessError::InvalidInput
    );
    assert_eq!(
        repository
            .list_user_groups(UserId::new(52)?, None, 50)
            .await
            .unwrap_err(),
        GroupKeyAccessError::OwnerUnavailable
    );
    repository
        .update_policy(group, GroupKeySelectionMode::AllUsers, vec![])
        .await?;
    groups::Entity::update_many()
        .filter(groups::Column::Id.eq(group.get()))
        .col_expr(
            groups::Column::DeletedAt,
            Expr::value(Some(
                sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc(),
            )),
        )
        .exec(pool.connection())
        .await?;
    assert_eq!(
        repository.get_policy(group).await.unwrap_err(),
        GroupKeyAccessError::NotFound
    );
    assert_eq!(
        repository
            .update_policy(group, GroupKeySelectionMode::AllUsers, vec![])
            .await
            .unwrap_err(),
        GroupKeyAccessError::NotFound
    );
    assert_eq!(
        repository
            .list_user_groups(user, None, 50)
            .await?
            .groups
            .len(),
        1
    );
    let transaction = pool.connection().begin().await?;
    // 软删除组即使被传作默认组也不可用。
    assert!(!user_can_select_key_group(&transaction, user, group, group).await?);
    transaction.rollback().await?;
    pool.close().await?;
    Ok(())
}

async fn fixture() -> Result<(DatabasePool, GroupKeyAccessRepository), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    for id in [10_i64, 20, 30] {
        pool.connection()
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, ?)",
                [
                    id.into(),
                    format!("key-group-{id}").into(),
                    format!("Group {id}").into(),
                    "{}".into(),
                ],
            ))
            .await?;
    }
    for id in [51_i64, 52] {
        pool.connection().execute(Statement::from_sql_and_values(DbBackend::Sqlite, "INSERT INTO users (id, username, status, default_group_id, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?)", [id.into(), format!("key-user-{id}").into(), 1_i16.into(), 10_i64.into(), format!("key-user-{id}").into(), "{}".into()])).await?;
    }
    let repository = GroupKeyAccessRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok((pool, repository))
}
