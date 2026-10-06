use std::{error::Error, time::Duration};

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DbErr, EntityTrait, JsonValue, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, Query, SimpleExpr},
};

use super::{
    DatabaseOptions, MigrationOptions, UserSessionLookupByIdOutcome, UserSessionLookupOutcome,
    UserSessionRepository, UserSessionRepositoryConfigError,
};
use crate::entity::{PasswordHash, groups, users};

const PASSWORD: &str = "correct-password";

#[tokio::test]
async fn login_and_session_recheck_current_user_state() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    users::ActiveModel {
        username: Set("admin".to_owned()),
        password_hash: Set(Some(password_hash("historical-password"))),
        role: Set(1),
        status: Set(1),
        default_group_id: Set(fixture.group_id),
        aff_code: Set("deleted-admin-aff".to_owned()),
        settings: Set(JsonValue::Object(Default::default())),
        deleted_at: Set(Some(TimeDateTimeWithTimeZone::now_utc())),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?;

    assert!(matches!(
        fixture.repository.login("admin", PASSWORD.as_bytes()).await?,
        UserSessionLookupOutcome::Authenticated {
            user_id,
            role: 1,
            session_version: 1,
            totp_secret: None,
        }
            if user_id == fixture.user_id
    ));
    assert!(matches!(
        fixture.repository.lookup_by_id(fixture.user_id).await?,
        UserSessionLookupByIdOutcome::Authenticated {
            user_id,
            role: 1,
            group_id,
            session_version: 1,
            totp_enabled: false,
        }
            if user_id == fixture.user_id && group_id.get() == fixture.group_id
    ));
    for (username, password) in [
        ("admin", "wrong-password"),
        ("missing", PASSWORD),
        (" admin", PASSWORD),
    ] {
        assert_eq!(
            fixture
                .repository
                .login(username, password.as_bytes())
                .await?,
            UserSessionLookupOutcome::Rejected
        );
    }

    set_user_column(&fixture, users::Column::Status, Expr::value(2_i16)).await?;
    assert_eq!(
        fixture.repository.lookup_by_id(fixture.user_id).await?,
        UserSessionLookupByIdOutcome::Rejected
    );
    assert_eq!(
        fixture
            .repository
            .login("admin", PASSWORD.as_bytes())
            .await?,
        UserSessionLookupOutcome::Rejected
    );

    set_user_column(&fixture, users::Column::Status, Expr::value(1_i16)).await?;
    set_user_column(
        &fixture,
        users::Column::DeletedAt,
        Expr::value(Some(TimeDateTimeWithTimeZone::now_utc())),
    )
    .await?;
    assert_eq!(
        fixture.repository.lookup_by_id(fixture.user_id).await?,
        UserSessionLookupByIdOutcome::Rejected
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn missing_password_is_rejected() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let passwordless = users::ActiveModel {
        username: Set("passwordless".to_owned()),
        role: Set(0),
        status: Set(1),
        default_group_id: Set(fixture.group_id),
        aff_code: Set("passwordless-aff".to_owned()),
        settings: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?;
    assert_eq!(
        fixture
            .repository
            .login("passwordless", PASSWORD.as_bytes())
            .await?,
        UserSessionLookupOutcome::Rejected
    );

    users::Entity::delete_by_id(passwordless.id)
        .exec(fixture.pool.connection())
        .await?;
    fixture.pool.close().await?;
    Ok(())
}

#[test]
fn zero_lookup_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let pool = runtime.block_on(crate::connect(&DatabaseOptions::new("sqlite::memory:")?))?;
    assert!(matches!(
        UserSessionRepository::new(pool.clone(), Duration::ZERO),
        Err(UserSessionRepositoryConfigError::ZeroLookupTimeout)
    ));
    runtime.block_on(pool.close())?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: UserSessionRepository,
    user_id: af_domain::UserId,
    group_id: i64,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("session-group".to_owned()),
        display_name: Set("session-group".to_owned()),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("admin".to_owned()),
        password_hash: Set(Some(password_hash(PASSWORD))),
        role: Set(1),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("admin-aff".to_owned()),
        settings: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user_id = af_domain::UserId::new(user.id)?;
    let repository = UserSessionRepository::new(pool.clone(), Duration::from_secs(2))?;
    Ok(Fixture {
        pool,
        repository,
        user_id,
        group_id: group.id,
    })
}

fn password_hash(password: &str) -> PasswordHash {
    let salt = SaltString::encode_b64(b"anyflows-test-salt").expect("固定测试盐必须有效");
    let encoded = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("固定测试密码必须可哈希")
        .to_string();
    PasswordHash::parse(&encoded).expect("测试哈希必须满足持久化约束")
}

async fn set_user_column(
    fixture: &Fixture,
    column: users::Column,
    value: SimpleExpr,
) -> Result<(), DbErr> {
    let statement = Query::update()
        .table(users::Entity)
        .value(column, value)
        .and_where(Expr::col(users::Column::Id).eq(fixture.user_id.get()))
        .to_owned();
    fixture
        .pool
        .connection()
        .execute(
            fixture
                .pool
                .connection()
                .get_database_backend()
                .build(&statement),
        )
        .await?;
    Ok(())
}
