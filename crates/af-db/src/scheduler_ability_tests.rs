use std::{error::Error, time::Duration};

use af_domain::{GroupId, Status};
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set, entity::prelude::Json};

use crate::{
    DatabaseOptions, MigrationOptions, SchedulerAbilityRepository, SchedulerAbilityRepositoryError,
    entity::{
        ChannelBaseUrl, HeaderOverrides, SensitiveJson, abilities, channel_groups, channel_models,
        channels, groups,
    },
};

#[tokio::test]
async fn repository_loads_only_enabled_live_channel_abilities() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let default_group = group("default").insert(pool.connection()).await?;
    let other_group = group("other").insert(pool.connection()).await?;

    let low = channel("low", Status::Enabled)
        .insert(pool.connection())
        .await?;
    let high = channel("high", Status::Enabled)
        .insert(pool.connection())
        .await?;
    let disabled_channel = channel("disabled", Status::Disabled)
        .insert(pool.connection())
        .await?;
    let disabled_ability_channel = channel("disabled-ability", Status::Enabled)
        .insert(pool.connection())
        .await?;
    let deleted_channel = channel("deleted", Status::Enabled)
        .insert(pool.connection())
        .await?;
    let mut deleted_update = deleted_channel.clone().into_active_model();
    deleted_update.deleted_at = Set(Some(
        sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc(),
    ));
    deleted_update.update(pool.connection()).await?;

    for channel_id in [
        low.id,
        high.id,
        disabled_channel.id,
        disabled_ability_channel.id,
        deleted_channel.id,
    ] {
        attach(channel_id, default_group.id, "gpt-5")
            .insert(pool.connection())
            .await?;
    }
    channel_group(high.id, other_group.id)
        .insert(pool.connection())
        .await?;
    channel_model(high.id, "gpt-4")
        .insert(pool.connection())
        .await?;

    ability(default_group.id, "gpt-5", low.id, true, 1, 0)
        .insert(pool.connection())
        .await?;
    ability(default_group.id, "gpt-5", high.id, true, 5, 12)
        .insert(pool.connection())
        .await?;
    ability(
        default_group.id,
        "gpt-5",
        disabled_channel.id,
        true,
        100,
        100,
    )
    .insert(pool.connection())
    .await?;
    ability(
        default_group.id,
        "gpt-5",
        deleted_channel.id,
        true,
        100,
        100,
    )
    .insert(pool.connection())
    .await?;
    ability(
        default_group.id,
        "gpt-5",
        disabled_ability_channel.id,
        false,
        100,
        100,
    )
    .insert(pool.connection())
    .await?;
    ability(other_group.id, "gpt-5", high.id, true, 100, 100)
        .insert(pool.connection())
        .await?;
    ability(default_group.id, "gpt-4", high.id, true, 100, 100)
        .insert(pool.connection())
        .await?;
    let mut deleted_other_group = other_group.clone().into_active_model();
    deleted_other_group.deleted_at = Set(Some(
        sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc(),
    ));
    deleted_other_group.update(pool.connection()).await?;

    let records = SchedulerAbilityRepository::new(pool.clone())
        .load(GroupId::new(default_group.id)?, "gpt-5")
        .await?;

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].channel_id().get(), high.id);
    assert_eq!(records[0].priority(), 5);
    assert_eq!(records[0].weight(), 12);
    assert_eq!(records[1].channel_id().get(), low.id);
    assert_eq!(records[1].priority(), 1);
    assert_eq!(records[1].weight(), 0);
    assert!(records.iter().all(|record| record.model() == "gpt-5"));
    assert!(!format!("{:?}", records[0]).contains("gpt-5"));
    assert!(
        SchedulerAbilityRepository::new(pool.clone())
            .load(GroupId::new(other_group.id)?, "gpt-5")
            .await?
            .is_empty()
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn full_snapshot_applies_capacity_per_group_and_model() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = group("default").insert(pool.connection()).await?;

    for index in 0..40 {
        let channel = channel(&format!("channel-{index}"), Status::Enabled)
            .insert(pool.connection())
            .await?;
        channel_group(channel.id, group.id)
            .insert(pool.connection())
            .await?;
        for model in ["gpt-4", "gpt-5"] {
            channel_model(channel.id, model)
                .insert(pool.connection())
                .await?;
            ability(group.id, model, channel.id, true, index, 0)
                .insert(pool.connection())
                .await?;
        }
    }

    let records = SchedulerAbilityRepository::new(pool.clone())
        .load_all()
        .await?;

    assert_eq!(records.len(), 80);
    assert_eq!(
        records
            .iter()
            .filter(|record| record.model() == "gpt-4")
            .count(),
        40
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record.model() == "gpt-5")
            .count(),
        40
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repository_rejects_invalid_input_and_oversized_results() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = group("default").insert(pool.connection()).await?;
    let repository = SchedulerAbilityRepository::new(pool.clone());

    assert_eq!(
        repository.load(GroupId::new(group.id)?, " bad ").await,
        Err(SchedulerAbilityRepositoryError::InvalidModel)
    );
    assert_eq!(
        SchedulerAbilityRepository::with_load_timeout(pool.clone(), Duration::ZERO).unwrap_err(),
        SchedulerAbilityRepositoryError::InvalidConfiguration
    );

    for index in 0..=crate::MAX_SCHEDULER_ABILITY_ENTRIES {
        let channel = channel(&format!("channel-{index}"), Status::Enabled)
            .insert(pool.connection())
            .await?;
        attach(channel.id, group.id, "gpt-5")
            .insert(pool.connection())
            .await?;
        ability(group.id, "gpt-5", channel.id, true, 0, 0)
            .insert(pool.connection())
            .await?;
    }

    assert_eq!(
        repository.load(GroupId::new(group.id)?, "gpt-5").await,
        Err(SchedulerAbilityRepositoryError::Invariant)
    );
    assert_eq!(
        repository.load_all().await,
        Err(SchedulerAbilityRepositoryError::Invariant)
    );

    pool.close().await?;
    Ok(())
}

fn group(name: &str) -> groups::ActiveModel {
    groups::ActiveModel {
        name: Set(name.to_owned()),
        display_name: Set(name.to_owned()),
        flags: Set(empty_json_object()),
        ..Default::default()
    }
}

fn channel(name: &str, status: Status) -> channels::ActiveModel {
    channels::ActiveModel {
        name: Set(name.to_owned()),
        r#type: Set("openai".to_owned()),
        protocol: Set("openai_chat".to_owned()),
        base_url: Set(Some(
            ChannelBaseUrl::parse("https://api.example.com").unwrap(),
        )),
        status: Set(status.code()),
        model_mapping: Set(empty_json_object()),
        param_override: Set(empty_json_object()),
        header_override: Set(HeaderOverrides::validate(empty_json_object()).unwrap()),
        settings: Set(SensitiveJson::from(empty_json_object())),
        ..Default::default()
    }
}

fn attach(channel_id: i64, group_id: i64, model: &str) -> AttachmentModels {
    AttachmentModels {
        channel_model: channel_model(channel_id, model),
        channel_group: channel_group(channel_id, group_id),
    }
}

fn channel_model(channel_id: i64, model: &str) -> channel_models::ActiveModel {
    channel_models::ActiveModel {
        channel_id: Set(channel_id),
        model: Set(model.to_owned()),
        ..Default::default()
    }
}

fn channel_group(channel_id: i64, group_id: i64) -> channel_groups::ActiveModel {
    channel_groups::ActiveModel {
        channel_id: Set(channel_id),
        group_id: Set(group_id),
        ..Default::default()
    }
}

fn ability(
    group_id: i64,
    model: &str,
    channel_id: i64,
    enabled: bool,
    priority: i32,
    weight: i32,
) -> abilities::ActiveModel {
    abilities::ActiveModel {
        group_id: Set(group_id),
        model: Set(model.to_owned()),
        channel_id: Set(channel_id),
        enabled: Set(enabled),
        priority: Set(priority),
        weight: Set(weight),
        ..Default::default()
    }
}

struct AttachmentModels {
    channel_model: channel_models::ActiveModel,
    channel_group: channel_groups::ActiveModel,
}

impl AttachmentModels {
    async fn insert(self, connection: &sea_orm::DatabaseConnection) -> Result<(), sea_orm::DbErr> {
        self.channel_model.insert(connection).await?;
        self.channel_group.insert(connection).await?;
        Ok(())
    }
}

fn empty_json_object() -> Json {
    Json::Object(Default::default())
}
