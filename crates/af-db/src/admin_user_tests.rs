use std::{error::Error, time::Duration};

use af_domain::UserId;
use sea_orm::{ActiveModelTrait, JsonValue, Set, entity::prelude::TimeDateTimeWithTimeZone};

use super::{
    AdminUserLookupOutcome, AdminUserRepository, AdminUserRepositoryConfigError,
    AdminUserRepositoryError, DatabaseOptions, MigrationOptions,
};
use crate::entity::{groups, users};

#[tokio::test]
async fn list_uses_stable_cursor_and_excludes_soft_deleted_users() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first = fixture.repository.list(None, 2).await?;
    let (users, next_cursor) = first.into_parts();
    assert_eq!(
        users.iter().map(|user| user.username()).collect::<Vec<_>>(),
        ["admin", "member"]
    );
    let next_cursor = next_cursor.expect("仍有第三个活跃用户时必须返回游标");
    assert_eq!(next_cursor, users[1].user_id());

    let second = fixture.repository.list(Some(next_cursor), 2).await?;
    let (users, next_cursor) = second.into_parts();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].username(), "disabled");
    assert_eq!(users[0].status(), 2);
    assert_eq!(next_cursor, None);
    assert_eq!(format!("{:?}", users[0]), "AdminUserRecord(<redacted>)");

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn detail_distinguishes_active_and_deleted_users() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let found = fixture.repository.get(fixture.member_id).await?;
    let AdminUserLookupOutcome::Found(user) = found else {
        panic!("活跃用户详情必须存在");
    };
    assert_eq!(user.username(), "member");
    assert_eq!(user.email(), Some("member@example.com"));
    assert_eq!(user.default_group_id().get(), fixture.group_id);
    assert_eq!(user.quota(), 100);
    assert_eq!(user.used_quota(), 20);
    assert_eq!(user.frozen_quota(), 5);
    assert_eq!(user.request_count(), 7);
    assert_eq!(user.rpm_limit(), Some(60));
    assert_eq!(user.concurrency(), Some(2));
    assert!(matches!(
        fixture.repository.get(fixture.deleted_id).await?,
        AdminUserLookupOutcome::NotFound
    ));
    assert!(matches!(
        fixture.repository.get(UserId::new(i64::MAX)?).await?,
        AdminUserLookupOutcome::NotFound
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_limit_and_closed_pool_fail_closed() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for limit in [0, 101] {
        assert_eq!(
            fixture.repository.list(None, limit).await.unwrap_err(),
            AdminUserRepositoryError::Invariant
        );
    }
    fixture.pool.clone().close().await?;
    assert_eq!(
        fixture.repository.list(None, 1).await.unwrap_err(),
        AdminUserRepositoryError::Query
    );
    Ok(())
}

#[test]
fn zero_lookup_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let pool = runtime.block_on(crate::connect(&DatabaseOptions::new("sqlite::memory:")?))?;
    assert!(matches!(
        AdminUserRepository::new(pool.clone(), Duration::ZERO),
        Err(AdminUserRepositoryConfigError::ZeroLookupTimeout)
    ));
    runtime.block_on(pool.close())?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: AdminUserRepository,
    member_id: UserId,
    deleted_id: UserId,
    group_id: i64,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("admin-users".to_owned()),
        display_name: Set("Admin Users".to_owned()),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    insert_user(&pool, group.id, "admin", None, 1, 1, None).await?;
    let member = insert_user(
        &pool,
        group.id,
        "member",
        Some("member@example.com"),
        0,
        1,
        None,
    )
    .await?;
    let deleted = insert_user(
        &pool,
        group.id,
        "deleted",
        None,
        0,
        1,
        Some(TimeDateTimeWithTimeZone::now_utc()),
    )
    .await?;
    insert_user(&pool, group.id, "disabled", None, 0, 2, None).await?;
    Ok(Fixture {
        repository: AdminUserRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
        member_id: UserId::new(member.id)?,
        deleted_id: UserId::new(deleted.id)?,
        group_id: group.id,
    })
}

async fn insert_user(
    pool: &super::DatabasePool,
    group_id: i64,
    username: &str,
    email: Option<&str>,
    role: i16,
    status: i16,
    deleted_at: Option<TimeDateTimeWithTimeZone>,
) -> Result<users::Model, sea_orm::DbErr> {
    users::ActiveModel {
        username: Set(username.to_owned()),
        email: Set(email.map(str::to_owned)),
        role: Set(role),
        status: Set(status),
        default_group_id: Set(group_id),
        quota: Set(if username == "member" { 100 } else { 0 }),
        used_quota: Set(if username == "member" { 20 } else { 0 }),
        frozen_quota: Set(if username == "member" { 5 } else { 0 }),
        request_count: Set(if username == "member" { 7 } else { 0 }),
        aff_code: Set(format!("{username}-aff")),
        rpm_limit: Set((username == "member").then_some(60)),
        concurrency: Set((username == "member").then_some(2)),
        settings: Set(JsonValue::Object(Default::default())),
        deleted_at: Set(deleted_at),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}
