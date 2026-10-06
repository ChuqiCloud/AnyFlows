use std::{error::Error, time::Duration};

use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

use super::{
    DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    InitialSetupStatus, MigrationOptions, UserSessionLookupOutcome, UserSessionRepository,
};
use crate::entity::{groups, options, users};

const USERNAME: &str = "owner";
const PASSWORD: &str = "correct horse battery staple";

#[tokio::test]
async fn setup_creates_login_ready_admin_and_canonical_group() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?;
    assert_eq!(repository.status().await?, InitialSetupStatus::Required);

    let InitialSetupOutcome::Initialized { user_id } = repository
        .initialize(InitialSetupRecord::new(
            USERNAME.to_owned(),
            PASSWORD.to_owned(),
        ))
        .await?
    else {
        panic!("空数据库必须完成首次安装");
    };
    assert_eq!(repository.status().await?, InitialSetupStatus::Complete);

    let group = groups::Entity::find()
        .one(pool.connection())
        .await?
        .expect("首次安装必须创建默认分组");
    assert_eq!(group.name, "default");
    assert_eq!(group.display_name, "默认分组");
    assert_eq!(group.ratio_micros, 1_000_000);
    assert_eq!(group.daily_limit, None);
    assert!(!group.is_exclusive);

    let user = users::Entity::find_by_id(user_id.get())
        .one(pool.connection())
        .await?
        .expect("首次安装必须创建管理员");
    assert_eq!(user.username, USERNAME);
    assert_eq!(user.role, 1);
    assert_eq!(user.status, 1);
    assert_eq!(user.default_group_id, group.id);
    assert_eq!(user.quota, 0);
    assert!(user.password_hash.is_some());

    let sessions = UserSessionRepository::new(pool.clone(), Duration::from_secs(5))?;
    assert_eq!(
        sessions.login(USERNAME, PASSWORD.as_bytes()).await?,
        UserSessionLookupOutcome::Authenticated {
            user_id,
            role: 1,
            session_version: 1,
            totp_secret: None,
        }
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn setup_is_idempotently_closed_after_success() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?;
    repository
        .initialize(InitialSetupRecord::new(
            USERNAME.to_owned(),
            PASSWORD.to_owned(),
        ))
        .await?;

    assert_eq!(
        repository
            .initialize(InitialSetupRecord::new(
                "second-owner".to_owned(),
                "another secure password".to_owned(),
            ))
            .await?,
        InitialSetupOutcome::AlreadyInitialized
    );
    assert_eq!(users::Entity::find().count(pool.connection()).await?, 1);
    assert_eq!(groups::Entity::find().count(pool.connection()).await?, 1);
    assert_eq!(options::Entity::find().count(pool.connection()).await?, 1);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn any_historical_user_permanently_closes_setup() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?;
    repository
        .initialize(InitialSetupRecord::new(
            USERNAME.to_owned(),
            PASSWORD.to_owned(),
        ))
        .await?;
    users::Entity::update_many()
        .filter(users::Column::Id.gt(0))
        .col_expr(
            users::Column::DeletedAt,
            sea_orm::sea_query::Expr::value(
                sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc(),
            ),
        )
        .exec(pool.connection())
        .await?;

    assert_eq!(repository.status().await?, InitialSetupStatus::Complete);
    assert_eq!(
        repository
            .initialize(InitialSetupRecord::new(
                "replacement".to_owned(),
                "replacement secure password".to_owned(),
            ))
            .await?,
        InitialSetupOutcome::AlreadyInitialized
    );
    pool.close().await?;
    Ok(())
}
