use std::{error::Error, time::Duration};

use af_domain::GroupId;
use sea_orm::{ActiveModelTrait, EntityTrait, JsonValue, Set};

use super::{
    AdminGroupLookupOutcome, AdminGroupRepository, AdminGroupRepositoryConfigError,
    AdminGroupRepositoryError, DatabaseOptions, MigrationOptions,
};
use crate::entity::groups;

#[tokio::test]
async fn list_uses_stable_cursor_and_excludes_soft_deleted_groups() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first = fixture.repository.list(None, 2).await?;
    let (groups, next_cursor) = first.into_parts();
    assert_eq!(
        groups.iter().map(|group| group.name()).collect::<Vec<_>>(),
        ["default", "vip"]
    );
    let next_cursor = next_cursor.expect("仍有第三个有效分组时必须返回游标");
    assert_eq!(next_cursor, groups[1].group_id());

    let second = fixture.repository.list(Some(next_cursor), 2).await?;
    let (groups, next_cursor) = second.into_parts();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name(), "auto");
    assert_eq!(next_cursor, None);
    assert_eq!(format!("{:?}", groups[0]), "AdminGroupRecord(<redacted>)");

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn detail_validates_pricing_limits_fallback_and_flags() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let AdminGroupLookupOutcome::Found(group) = fixture.repository.get(fixture.vip_id).await?
    else {
        panic!("有效分组详情必须存在");
    };
    assert_eq!(group.name(), "vip");
    assert_eq!(group.display_name(), "VIP");
    assert_eq!(group.ratio_micros(), 1_250_000);
    let peak = group.peak().expect("VIP 分组必须包含高峰窗口");
    assert_eq!(peak.ratio_micros(), 1_500_000);
    assert_eq!(peak.start_second(), 8 * 3_600);
    assert_eq!(peak.end_second(), 20 * 3_600);
    assert!(group.is_exclusive());
    assert_eq!(group.daily_limit(), Some(10_000));
    assert_eq!(group.weekly_limit(), Some(60_000));
    assert_eq!(group.monthly_limit(), Some(200_000));
    assert_eq!(group.rpm_limit(), Some(120));
    assert_eq!(group.fallback_group_id(), Some(fixture.default_id));
    assert_eq!(group.flags()["claude_code_only"], true);
    assert!(matches!(
        fixture.repository.get(fixture.deleted_id).await?,
        AdminGroupLookupOutcome::NotFound
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_limits_corrupt_flags_and_closed_pool_fail_closed() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for limit in [0, 101] {
        assert_eq!(
            fixture.repository.list(None, limit).await.unwrap_err(),
            AdminGroupRepositoryError::Invariant
        );
    }
    let mut invalid = groups::ActiveModel::from(
        groups::Entity::find_by_id(fixture.vip_id.get())
            .one(fixture.pool.connection())
            .await?
            .expect("测试分组必须存在"),
    );
    invalid.flags = Set(JsonValue::Array(Vec::new()));
    invalid.update(fixture.pool.connection()).await?;
    assert_eq!(
        fixture.repository.get(fixture.vip_id).await.unwrap_err(),
        AdminGroupRepositoryError::Invariant
    );

    fixture.pool.clone().close().await?;
    assert_eq!(
        fixture.repository.list(None, 1).await.unwrap_err(),
        AdminGroupRepositoryError::Query
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
        AdminGroupRepository::new(pool.clone(), Duration::ZERO),
        Err(AdminGroupRepositoryConfigError::ZeroLookupTimeout)
    ));
    runtime.block_on(pool.close())?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: AdminGroupRepository,
    default_id: GroupId,
    vip_id: GroupId,
    deleted_id: GroupId,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let default = insert_group(&pool, "default", "Default", None, false).await?;
    let vip = insert_group(&pool, "vip", "VIP", Some(default.id), true).await?;
    let deleted = insert_group(&pool, "deleted", "Deleted", None, false).await?;
    let mut deleted_model = groups::ActiveModel::from(deleted.clone());
    deleted_model.deleted_at = Set(Some(
        sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc(),
    ));
    deleted_model.update(pool.connection()).await?;
    insert_group(&pool, "auto", "Auto", None, false).await?;
    Ok(Fixture {
        repository: AdminGroupRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
        default_id: GroupId::new(default.id)?,
        vip_id: GroupId::new(vip.id)?,
        deleted_id: GroupId::new(deleted.id)?,
    })
}

async fn insert_group(
    pool: &super::DatabasePool,
    name: &str,
    display_name: &str,
    fallback_group_id: Option<i64>,
    with_limits: bool,
) -> Result<groups::Model, sea_orm::DbErr> {
    groups::ActiveModel {
        name: Set(name.to_owned()),
        display_name: Set(display_name.to_owned()),
        ratio_micros: Set(if with_limits { 1_250_000 } else { 1_000_000 }),
        peak_ratio_micros: Set(with_limits.then_some(1_500_000)),
        peak_start: Set(
            with_limits.then(|| sea_orm::entity::prelude::TimeTime::from_hms(8, 0, 0).unwrap())
        ),
        peak_end: Set(
            with_limits.then(|| sea_orm::entity::prelude::TimeTime::from_hms(20, 0, 0).unwrap())
        ),
        is_exclusive: Set(with_limits),
        daily_limit: Set(with_limits.then_some(10_000)),
        weekly_limit: Set(with_limits.then_some(60_000)),
        monthly_limit: Set(with_limits.then_some(200_000)),
        rpm_limit: Set(with_limits.then_some(120)),
        fallback_group_id: Set(fallback_group_id),
        flags: Set(if with_limits {
            serde_json::json!({"claude_code_only": true})
        } else {
            JsonValue::Object(Default::default())
        }),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}
