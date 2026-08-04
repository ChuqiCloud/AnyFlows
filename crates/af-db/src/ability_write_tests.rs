use std::{error::Error, time::Duration};

use af_domain::{ChannelType, GroupId, Protocol, Status};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
    entity::prelude::Json,
};

use crate::{
    AdminChannelLookupOutcome, AdminChannelMutationOutcome, AdminChannelRepository,
    AdminChannelWriteRecord, AdminChannelWriteRepositoryError, DatabaseOptions,
    MAX_SCHEDULER_ABILITY_ENTRIES, MigrationOptions,
    entity::{abilities, channels, groups, scheduler_outbox_events},
};

#[tokio::test]
async fn channel_routing_diff_preserves_unchanged_abilities_and_rolls_back_invalid_references()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first_group = create_group(&fixture, "first").await?;
    let second_group = create_group(&fixture, "second").await?;
    let third_group = create_group(&fixture, "third").await?;
    let created = fixture
        .repository
        .create_channel(channel_record(
            "routing",
            Status::Enabled,
            20,
            10,
            Some("initial"),
            &["model-a", "model-b"],
            &[first_group, second_group],
        ))
        .await?;
    let channel_id = created.channel_id();
    assert_eq!(channel_outbox_count(&fixture, channel_id.get()).await?, 1);
    assert_eq!(created.models(), &["model-a", "model-b"]);
    assert_eq!(created.group_ids(), &[first_group, second_group]);

    let before = ability_rows(&fixture, channel_id.get()).await?;
    assert_eq!(before.len(), 4);
    let preserved_created_at = before
        .iter()
        .find(|row| row.group_id == first_group.get() && row.model == "model-a")
        .expect("初始能力必须包含保留组合")
        .created_at;

    let AdminChannelMutationOutcome::Mutated(updated) = fixture
        .repository
        .update_channel(
            channel_id,
            channel_record(
                "routing-updated",
                Status::Disabled,
                30,
                11,
                Some("updated"),
                &["model-a", "model-c"],
                &[first_group, third_group],
            ),
        )
        .await?
    else {
        panic!("有效渠道必须完成路由差量更新");
    };
    assert_eq!(updated.models(), &["model-a", "model-c"]);
    assert_eq!(updated.group_ids(), &[first_group, third_group]);
    assert_eq!(channel_outbox_count(&fixture, channel_id.get()).await?, 2);

    let after = ability_rows(&fixture, channel_id.get()).await?;
    assert_eq!(after.len(), 4);
    assert!(after.iter().all(|row| {
        !row.enabled
            && row.priority == 30
            && row.weight == 11
            && row.tag.as_deref() == Some("updated")
    }));
    assert_eq!(
        after
            .iter()
            .find(|row| row.group_id == first_group.get() && row.model == "model-a")
            .expect("未变化能力必须保留")
            .created_at,
        preserved_created_at,
        "未变化能力不得删除重建"
    );
    assert_eq!(
        after
            .iter()
            .map(|row| (row.group_id, row.model.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (first_group.get(), "model-a"),
            (first_group.get(), "model-c"),
            (third_group.get(), "model-a"),
            (third_group.get(), "model-c"),
        ]
    );

    let missing_group = GroupId::new(9_999).unwrap();
    assert_eq!(
        fixture
            .repository
            .update_channel(
                channel_id,
                channel_record(
                    "must-roll-back",
                    Status::Enabled,
                    1,
                    1,
                    None,
                    &["model-z"],
                    &[missing_group],
                ),
            )
            .await
            .unwrap_err(),
        AdminChannelWriteRepositoryError::InvalidReference
    );
    let AdminChannelLookupOutcome::Found(rolled_back) = fixture.repository.get(channel_id).await?
    else {
        panic!("失败更新后渠道必须仍然存在");
    };
    assert_eq!(rolled_back.name(), "routing-updated");
    assert_eq!(rolled_back.models(), &["model-a", "model-c"]);
    assert_eq!(rolled_back.group_ids(), &[first_group, third_group]);
    assert_eq!(
        channel_outbox_count(&fixture, channel_id.get()).await?,
        2,
        "失败的路由事务不得提交 outbox 事件"
    );

    assert_eq!(
        fixture
            .repository
            .update_channel(
                channel_id,
                channel_record(
                    "duplicate",
                    Status::Enabled,
                    1,
                    1,
                    None,
                    &["model-a", "model-a"],
                    &[first_group],
                ),
            )
            .await
            .unwrap_err(),
        AdminChannelWriteRepositoryError::InvalidInput
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn channel_routing_rejects_the_sixty_fifth_candidate_atomically() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let group_id = create_group(&fixture, "shared").await?;
    for index in 0..MAX_SCHEDULER_ABILITY_ENTRIES {
        fixture
            .repository
            .create_channel(channel_record(
                &format!("channel-{index}"),
                Status::Enabled,
                0,
                0,
                None,
                &["shared-model"],
                &[group_id],
            ))
            .await?;
    }
    let before = channels::Entity::find()
        .count(fixture.pool.connection())
        .await?;
    assert_eq!(
        fixture
            .repository
            .create_channel(channel_record(
                "overflow",
                Status::Enabled,
                0,
                0,
                None,
                &["shared-model"],
                &[group_id],
            ))
            .await
            .unwrap_err(),
        AdminChannelWriteRepositoryError::InvalidInput
    );
    assert_eq!(
        channels::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        before,
        "候选超限必须回滚已插入渠道"
    );

    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: crate::DatabasePool,
    repository: AdminChannelRepository,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    Ok(Fixture {
        repository: AdminChannelRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
    })
}

async fn create_group(fixture: &Fixture, name: &str) -> Result<GroupId, Box<dyn Error>> {
    let model = groups::ActiveModel {
        name: Set(name.to_owned()),
        display_name: Set(name.to_owned()),
        flags: Set(Json::Object(Default::default())),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?;
    Ok(GroupId::new(model.id)?)
}

async fn channel_outbox_count(fixture: &Fixture, channel_id: i64) -> Result<u64, sea_orm::DbErr> {
    scheduler_outbox_events::Entity::find()
        .filter(scheduler_outbox_events::Column::SubjectKind.eq(1_i16))
        .filter(scheduler_outbox_events::Column::SubjectId.eq(channel_id))
        .count(fixture.pool.connection())
        .await
}

fn channel_record(
    name: &str,
    status: Status,
    priority: i32,
    weight: i32,
    tag: Option<&str>,
    models: &[&str],
    group_ids: &[GroupId],
) -> AdminChannelWriteRecord {
    AdminChannelWriteRecord::new(
        name.to_owned(),
        ChannelType::OpenAi,
        Protocol::OpenAiChat,
        Some("https://api.example.com/v1".to_owned()),
        None,
        status,
        weight,
        priority,
        true,
        models.iter().map(|model| (*model).to_owned()).collect(),
        group_ids.to_vec(),
        serde_json::json!({}),
        serde_json::json!({}),
        serde_json::json!({}),
        serde_json::json!({}),
        tag.map(str::to_owned),
    )
}

async fn ability_rows(
    fixture: &Fixture,
    channel_id: i64,
) -> Result<Vec<abilities::Model>, sea_orm::DbErr> {
    abilities::Entity::find()
        .filter(abilities::Column::ChannelId.eq(channel_id))
        .order_by_asc(abilities::Column::GroupId)
        .order_by_asc(abilities::Column::Model)
        .all(fixture.pool.connection())
        .await
}
