use std::error::Error;

use sea_orm::{ActiveModelBehavior, ActiveModelTrait, IntoActiveModel, Set};

use crate::{
    DatabaseOptions, GroupPricingRepository, GroupPricingRepositoryError, MigrationOptions,
    entity::{group_model_ratios, groups},
};

#[tokio::test]
async fn repository_loads_active_groups_and_inter_group_overrides() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let source = group("source", 1_200_000, None);
    let target = group(
        "target",
        800_000,
        Some((2_000_000, (23 * 3_600) + 30 * 60, 30 * 60)),
    );
    let source = source.insert(pool.connection()).await?;
    let target = target.insert(pool.connection()).await?;
    group_model_ratios::ActiveModel {
        source_group_id: Set(source.id),
        target_group_id: Set(target.id),
        ratio_micros: Set(750_000),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    let catalog = GroupPricingRepository::new(pool.clone())
        .load_snapshot()
        .await?;
    assert_eq!(catalog.groups().len(), 2);
    assert_eq!(catalog.group_model_ratios().len(), 1);
    let target_record = catalog
        .groups()
        .iter()
        .find(|record| record.group_id().get() == target.id)
        .expect("目标分组必须存在");
    let peak = target_record.peak().expect("高峰窗口必须保留");
    assert_eq!(peak.ratio_micros(), 2_000_000);
    assert_eq!(peak.start_second(), 23 * 3_600 + 30 * 60);
    assert_eq!(peak.end_second(), 30 * 60);
    let override_record = catalog.group_model_ratios()[0];
    assert_eq!(override_record.source_group_id().get(), source.id);
    assert_eq!(override_record.target_group_id().get(), target.id);
    assert_eq!(override_record.ratio_micros(), 750_000);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repository_rejects_orphaned_override_as_invariant() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let source = group("source", 1_000_000, None)
        .insert(pool.connection())
        .await?;
    let target = group("target", 1_000_000, None)
        .insert(pool.connection())
        .await?;
    group_model_ratios::ActiveModel {
        source_group_id: Set(source.id),
        target_group_id: Set(target.id),
        ratio_micros: Set(1_000_000),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    // 软删除目标分组后，历史覆盖行仍存在；读取边界必须拒绝这类不完整目录。
    let mut deleted_target = target.into_active_model();
    deleted_target.deleted_at = Set(Some(
        sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc(),
    ));
    deleted_target.update(pool.connection()).await?;
    assert_eq!(
        GroupPricingRepository::new(pool.clone())
            .load_snapshot()
            .await,
        Err(GroupPricingRepositoryError::Invariant)
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn active_models_enforce_group_pricing_invariants_without_database_checks()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;

    let invalid_group = group("negative", -1, None);
    assert!(
        invalid_group
            .before_save(pool.connection(), true)
            .await
            .is_err()
    );
    let mut incomplete_peak = group("incomplete", 1_000_000, None);
    incomplete_peak.peak_start = Set(Some(
        sea_orm::entity::prelude::TimeTime::from_hms(9, 0, 0).unwrap(),
    ));
    assert!(
        incomplete_peak
            .before_save(pool.connection(), true)
            .await
            .is_err()
    );

    let source = group("source", 1_000_000, None)
        .insert(pool.connection())
        .await?;
    let target = group("target", 1_000_000, None)
        .insert(pool.connection())
        .await?;
    let invalid_override = group_model_ratios::ActiveModel {
        source_group_id: Set(source.id),
        target_group_id: Set(target.id),
        ratio_micros: Set(-1),
        ..Default::default()
    };
    assert!(
        invalid_override
            .before_save(pool.connection(), true)
            .await
            .is_err()
    );

    pool.close().await?;
    Ok(())
}

fn group(name: &str, ratio_micros: i64, peak: Option<(i64, u32, u32)>) -> groups::ActiveModel {
    let (peak_ratio_micros, peak_start, peak_end) =
        peak.map_or((None, None, None), |(ratio, start, end)| {
            (
                Some(ratio),
                Some(
                    sea_orm::entity::prelude::TimeTime::from_hms(
                        (start / 3_600) as u8,
                        ((start / 60) % 60) as u8,
                        (start % 60) as u8,
                    )
                    .unwrap(),
                ),
                Some(
                    sea_orm::entity::prelude::TimeTime::from_hms(
                        (end / 3_600) as u8,
                        ((end / 60) % 60) as u8,
                        (end % 60) as u8,
                    )
                    .unwrap(),
                ),
            )
        });
    groups::ActiveModel {
        name: Set(name.to_owned()),
        display_name: Set(name.to_owned()),
        ratio_micros: Set(ratio_micros),
        peak_ratio_micros: Set(peak_ratio_micros),
        peak_start: Set(peak_start),
        peak_end: Set(peak_end),
        flags: Set(sea_orm::entity::prelude::Json::Object(Default::default())),
        ..Default::default()
    }
}
