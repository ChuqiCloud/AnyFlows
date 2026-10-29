use std::{error::Error, time::Duration};

use af_domain::{GroupId, UserId};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter, Statement, TransactionTrait,
    sea_query::Expr,
};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, UserTokenMutationOutcome, UserTokenRepository,
    UserTokenRepositoryError, UserTokenWriteRecord,
    entity::{groups, tokens, users},
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
async fn revoking_group_access_disables_only_active_keys_that_lose_access()
-> Result<(), Box<dyn Error>> {
    let (pool, repository) = fixture().await?;
    let group = GroupId::new(20)?;
    let first = UserId::new(51)?;
    let second = UserId::new(52)?;
    let default_user = UserId::new(53)?;
    for (id, default_group_id) in [(53_i64, group.get()), (54_i64, 10_i64)] {
        pool.connection()
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO users (id, username, status, default_group_id, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?)",
                [
                    id.into(),
                    format!("key-user-{id}").into(),
                    1_i16.into(),
                    default_group_id.into(),
                    format!("key-user-{id}").into(),
                    "{}".into(),
                ],
            ))
            .await?;
    }

    // 策略从全部用户收窄时，应按已存在的 Key 逐一处理。
    repository
        .update_policy(group, GroupKeySelectionMode::AllUsers, vec![])
        .await?;
    let deleted_at = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
    insert_test_token(&pool, 501, first.get(), group.get(), 1, None).await?;
    insert_test_token(&pool, 502, first.get(), group.get(), 1, None).await?;
    insert_test_token(&pool, 503, second.get(), group.get(), 1, None).await?;
    insert_test_token(&pool, 504, default_user.get(), group.get(), 1, None).await?;
    insert_test_token(&pool, 505, first.get(), group.get(), 2, None).await?;
    insert_test_token(&pool, 506, first.get(), group.get(), 1, Some(deleted_at)).await?;
    insert_test_token(&pool, 507, first.get(), GroupId::new(30)?.get(), 1, None).await?;
    // 组织 Key 的所有者同样失去个人选组权，但组织令牌不能被个人授权策略停用或计数。
    insert_test_token(&pool, 510, 54, group.get(), 1, None).await?;
    tokens::Entity::update_many()
        .filter(tokens::Column::Id.eq(510_i64))
        .col_expr(tokens::Column::OrganizationId, Expr::value(Some(900_i64)))
        .exec(pool.connection())
        .await?;

    let skipped = repository
        .update_policy_with_key_revocation(
            group,
            GroupKeySelectionMode::SelectedUsers,
            vec![second],
            false,
        )
        .await?;
    assert_eq!(skipped.disabled_key_count, 0);
    assert_eq!(skipped.disabled_user_count, 0);
    assert_eq!(token_status(&pool, 501).await?, 1);
    assert_eq!(token_status(&pool, 502).await?, 1);
    let user_tokens = UserTokenRepository::new(pool.clone(), Duration::from_secs(5))?;
    assert!(matches!(
        user_tokens
            .update(
                first,
                af_domain::TokenId::new(501)?,
                UserTokenWriteRecord::new("still-active".to_owned(), 1, 0, false, None, None, None),
            )
            .await?,
        UserTokenMutationOutcome::Mutated(_)
    ));
    repository
        .update_policy(group, GroupKeySelectionMode::AllUsers, vec![])
        .await?;

    let update = repository
        .update_policy_with_key_revocation(
            group,
            GroupKeySelectionMode::SelectedUsers,
            vec![second],
            true,
        )
        .await?;
    assert_eq!(update.disabled_key_count, 2);
    assert_eq!(update.disabled_user_count, 1);
    assert_eq!(update.policy.allowed_user_ids, [second]);
    assert!(matches!(
        user_tokens
            .update(
                first,
                af_domain::TokenId::new(501)?,
                UserTokenWriteRecord::new("reactivation".to_owned(), 1, 0, false, None, None, None),
            )
            .await,
        Err(UserTokenRepositoryError::GroupUnavailable)
    ));
    for (id, expected_status) in [
        (501, 2),
        (502, 2),
        (503, 1),
        (504, 1),
        (505, 2),
        (506, 1),
        (507, 1),
        (510, 1),
    ] {
        assert_eq!(
            token_status(&pool, id).await?,
            expected_status,
            "token {id}"
        );
    }

    // 已授权用户失去自选权，但默认分组用户的 Key 继续有效。
    let update = repository
        .update_policy_with_key_revocation(group, GroupKeySelectionMode::DefaultOnly, vec![], true)
        .await?;
    assert_eq!(update.disabled_key_count, 1);
    assert_eq!(update.disabled_user_count, 1);
    assert_eq!(token_status(&pool, 503).await?, 2);
    assert_eq!(token_status(&pool, 504).await?, 1);
    assert_eq!(token_status(&pool, 510).await?, 1);

    // 默认组策略下原本无权使用该组的 Key 不属于“失去授权”。
    insert_test_token(&pool, 508, second.get(), group.get(), 1, None).await?;
    let update = repository
        .update_policy_with_key_revocation(
            group,
            GroupKeySelectionMode::SelectedUsers,
            vec![second],
            true,
        )
        .await?;
    assert_eq!(update.disabled_key_count, 0);
    assert_eq!(update.disabled_user_count, 0);
    assert_eq!(token_status(&pool, 508).await?, 1);

    // 扩大到全部用户时不应停用 Key。
    insert_test_token(&pool, 509, first.get(), group.get(), 1, None).await?;
    let update = repository
        .update_policy_with_key_revocation(group, GroupKeySelectionMode::AllUsers, vec![], true)
        .await?;
    assert_eq!(update.disabled_key_count, 0);
    assert_eq!(update.disabled_user_count, 0);
    assert_eq!(token_status(&pool, 509).await?, 1);

    // Key 停用失败时，策略与授权必须随同事务回滚。
    pool.connection()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "CREATE TRIGGER reject_key_revocation BEFORE UPDATE ON tokens WHEN OLD.id = 509 AND NEW.status = 2 BEGIN SELECT RAISE(ABORT, 'test key revocation rejected'); END".to_owned(),
        ))
        .await?;
    assert_eq!(
        repository
            .update_policy_with_key_revocation(
                group,
                GroupKeySelectionMode::SelectedUsers,
                vec![second],
                true,
            )
            .await
            .unwrap_err(),
        GroupKeyAccessError::Internal
    );
    assert_eq!(token_status(&pool, 509).await?, 1);
    let policy = repository.get_policy(group).await?;
    assert_eq!(policy.selection_mode, GroupKeySelectionMode::AllUsers);
    assert!(policy.allowed_user_ids.is_empty());

    pool.close().await?;
    Ok(())
}

async fn insert_test_token(
    pool: &DatabasePool,
    id: i64,
    user_id: i64,
    group_id: i64,
    status: i16,
    deleted_at: Option<sea_orm::entity::prelude::TimeDateTimeWithTimeZone>,
) -> Result<(), sea_orm::DbErr> {
    pool.connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO tokens (id, user_id, key_hash, key_prefix, name, status, group_id, deleted_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            [
                id.into(),
                user_id.into(),
                format!("{id:064x}").into(),
                format!("sk-af-{id:012}").into(),
                format!("test-token-{id}").into(),
                status.into(),
                group_id.into(),
                deleted_at.into(),
            ],
        ))
        .await?;
    Ok(())
}

async fn token_status(pool: &DatabasePool, id: i64) -> Result<i16, sea_orm::DbErr> {
    tokens::Entity::find_by_id(id)
        .one(pool.connection())
        .await?
        .map(|token| token.status)
        .ok_or_else(|| sea_orm::DbErr::Custom(format!("token {id} not found")))
}

#[tokio::test]
async fn revocation_counts_users_once_across_locked_key_update_batches()
-> Result<(), Box<dyn Error>> {
    let (pool, repository) = fixture().await?;
    let group = GroupId::new(20)?;
    repository
        .update_policy(group, GroupKeySelectionMode::AllUsers, vec![])
        .await?;
    for id in 1_000_i64..1_403 {
        let owner = if id < 1_401 { 51 } else { 52 };
        insert_test_token(&pool, id, owner, group.get(), 1, None).await?;
    }
    let update = repository
        .update_policy_with_key_revocation(group, GroupKeySelectionMode::DefaultOnly, vec![], true)
        .await?;
    assert_eq!(update.disabled_key_count, 403);
    assert_eq!(update.disabled_user_count, 2);
    for id in 1_000_i64..1_403 {
        assert_eq!(token_status(&pool, id).await?, 2);
    }
    let repeated = repository
        .update_policy_with_key_revocation(group, GroupKeySelectionMode::DefaultOnly, vec![], true)
        .await?;
    assert_eq!(repeated.disabled_key_count, 0);
    assert_eq!(repeated.disabled_user_count, 0);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn revocation_rolls_back_if_a_locked_key_is_not_updated() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = fixture().await?;
    let group = GroupId::new(20)?;
    repository
        .update_policy(group, GroupKeySelectionMode::AllUsers, vec![])
        .await?;
    insert_test_token(&pool, 601, 51, group.get(), 1, None).await?;
    insert_test_token(&pool, 602, 52, group.get(), 1, None).await?;
    pool.connection()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "CREATE TRIGGER skip_key_revocation BEFORE UPDATE ON tokens WHEN OLD.id = 602 AND NEW.status = 2 BEGIN SELECT RAISE(IGNORE); END".to_owned(),
        ))
        .await?;
    assert_eq!(
        repository
            .update_policy_with_key_revocation(
                group,
                GroupKeySelectionMode::DefaultOnly,
                vec![],
                true
            )
            .await
            .unwrap_err(),
        GroupKeyAccessError::Internal
    );
    assert_eq!(token_status(&pool, 601).await?, 1);
    assert_eq!(token_status(&pool, 602).await?, 1);
    assert_eq!(
        repository.get_policy(group).await?.selection_mode,
        GroupKeySelectionMode::AllUsers
    );
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
