use std::{error::Error, time::Duration};

use af_domain::UserId;
use sea_orm::{EntityTrait, PaginatorTrait};
use serde_json::{Value, json};

use super::{
    DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MAX_PLAYGROUND_CONVERSATIONS_PER_USER, MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES, MigrationOptions,
    PlaygroundConversationDeleteOutcome, PlaygroundConversationRepository,
    PlaygroundConversationRepositoryError, PlaygroundConversationWrite,
};
use crate::entity::playground_conversations;

#[tokio::test]
async fn conversation_round_trip_is_idempotent_versioned_and_owner_scoped()
-> Result<(), Box<dyn Error>> {
    let (pool, owner_user_id) = setup_database().await?;
    let repository = PlaygroundConversationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let conversation_id = conversation_id(1);
    let first_snapshot = snapshot("private question", "private answer");
    let first = repository
        .save(
            owner_user_id,
            write(
                &conversation_id,
                "private question",
                first_snapshot.clone(),
                None,
            ),
        )
        .await?;
    assert_eq!(first.revision(), 1);
    let debug = format!("{first:?}");
    assert!(!debug.contains(&conversation_id));
    assert!(!debug.contains("private question"));

    let replay = repository
        .save(
            owner_user_id,
            write(&conversation_id, "private question", first_snapshot, None),
        )
        .await?;
    assert_eq!(replay.revision(), 1);
    assert_eq!(
        playground_conversations::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );

    let second_snapshot = snapshot("next question", "next answer");
    let updated = repository
        .save(
            owner_user_id,
            write(
                &conversation_id,
                "next question",
                second_snapshot.clone(),
                Some(1),
            ),
        )
        .await?;
    assert_eq!(updated.revision(), 2);
    // 响应丢失后的同版本同正文重放必须返回已有 revision，不得重复递增。
    let update_replay = repository
        .save(
            owner_user_id,
            write(&conversation_id, "next question", second_snapshot, Some(1)),
        )
        .await?;
    assert_eq!(update_replay.revision(), 2);
    assert_eq!(
        repository
            .save(
                owner_user_id,
                write(
                    &conversation_id,
                    "conflicting question",
                    snapshot("conflicting question", "answer"),
                    Some(1),
                ),
            )
            .await,
        Err(PlaygroundConversationRepositoryError::Conflict)
    );

    let summaries = repository.list(owner_user_id).await?;
    assert_eq!(summaries.len(), 1);
    let (listed_id, title, models, revision, _, _) =
        summaries.into_iter().next().unwrap().into_parts();
    assert_eq!(listed_id, conversation_id);
    assert_eq!(title, "next question");
    assert_eq!(models, ["gpt-5"]);
    assert_eq!(revision, 2);

    let stored = repository
        .find(owner_user_id, &conversation_id)
        .await?
        .expect("所有者必须能读取会话");
    let (_, _, _, stored_snapshot, revision, _, _) = stored.into_parts();
    assert_eq!(stored_snapshot, snapshot("next question", "next answer"));
    assert_eq!(revision, 2);

    let other_user_id = UserId::new(owner_user_id.get() + 1)?;
    assert!(
        repository
            .find(other_user_id, &conversation_id)
            .await?
            .is_none()
    );
    assert_eq!(
        repository.delete(other_user_id, &conversation_id).await?,
        PlaygroundConversationDeleteOutcome::NotFound
    );
    assert_eq!(
        repository.delete(owner_user_id, &conversation_id).await?,
        PlaygroundConversationDeleteOutcome::Deleted
    );
    assert_eq!(
        repository.delete(owner_user_id, &conversation_id).await?,
        PlaygroundConversationDeleteOutcome::NotFound
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn conversation_capacity_and_input_bounds_fail_closed() -> Result<(), Box<dyn Error>> {
    let (pool, owner_user_id) = setup_database().await?;
    let repository = PlaygroundConversationRepository::new(pool.clone(), Duration::from_secs(5))?;
    for index in 0..MAX_PLAYGROUND_CONVERSATIONS_PER_USER {
        let id = conversation_id(index + 1);
        repository
            .save(
                owner_user_id,
                write(&id, "bounded title", snapshot("q", "a"), None),
            )
            .await?;
    }
    assert_eq!(repository.list(owner_user_id).await?.len(), 50);
    assert_eq!(
        repository
            .save(
                owner_user_id,
                write(
                    &conversation_id(999),
                    "over capacity",
                    snapshot("q", "a"),
                    None,
                ),
            )
            .await,
        Err(PlaygroundConversationRepositoryError::LimitReached)
    );

    for invalid in ["0".repeat(32), "A".repeat(32), "a".repeat(31)] {
        assert!(repository.find(owner_user_id, &invalid).await?.is_none());
        assert_eq!(
            repository
                .save(
                    owner_user_id,
                    write(&invalid, "title", snapshot("q", "a"), None),
                )
                .await,
            Err(PlaygroundConversationRepositoryError::InvalidInput)
        );
    }

    let oversized = json!({"content": "x".repeat(MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES)});
    for invalid_write in [
        PlaygroundConversationWrite::new(
            conversation_id(1001),
            " title".to_owned(),
            vec!["gpt-5".to_owned()],
            snapshot("q", "a"),
            None,
        ),
        PlaygroundConversationWrite::new(
            conversation_id(1002),
            "title".to_owned(),
            vec!["gpt-5".to_owned(), "gpt-5".to_owned()],
            snapshot("q", "a"),
            None,
        ),
        PlaygroundConversationWrite::new(
            conversation_id(1003),
            "title".to_owned(),
            vec!["gpt-5".to_owned()],
            Value::Array(Vec::new()),
            None,
        ),
        PlaygroundConversationWrite::new(
            conversation_id(1004),
            "title".to_owned(),
            vec!["gpt-5".to_owned()],
            oversized,
            Some(0),
        ),
    ] {
        assert_eq!(
            repository.save(owner_user_id, invalid_write).await,
            Err(PlaygroundConversationRepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        repository
            .save(
                UserId::new(owner_user_id.get() + 1)?,
                write(
                    &conversation_id(1005),
                    "missing owner",
                    snapshot("q", "a"),
                    None,
                ),
            )
            .await,
        Err(PlaygroundConversationRepositoryError::OwnerUnavailable)
    );
    pool.close().await?;
    Ok(())
}

async fn setup_database() -> Result<(crate::DatabasePool, UserId), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let setup = InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let InitialSetupOutcome::Initialized { user_id } = setup
        .initialize(InitialSetupRecord::new(
            "history-owner".to_owned(),
            "correct horse battery staple".to_owned(),
        ))
        .await?
    else {
        panic!("空测试数据库必须完成首次安装")
    };
    Ok((pool, user_id))
}

fn conversation_id(index: usize) -> String {
    format!("{:032x}", index + 1)
}

fn snapshot(user: &str, assistant: &str) -> Value {
    json!({
        "version": 1,
        "sessions": [{
            "model": "gpt-5",
            "messages": [
                {"role": "user", "content": user},
                {"role": "assistant", "content": assistant}
            ]
        }]
    })
}

fn write(
    conversation_id: &str,
    title: &str,
    snapshot: Value,
    expected_revision: Option<i64>,
) -> PlaygroundConversationWrite {
    PlaygroundConversationWrite::new(
        conversation_id.to_owned(),
        title.to_owned(),
        vec!["gpt-5".to_owned()],
        snapshot,
        expected_revision,
    )
}
