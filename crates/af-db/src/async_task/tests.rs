use std::{error::Error, time::Duration};

use af_domain::{
    AsyncTaskId, AsyncTaskRequestId, ChannelId, CredentialId, GatewayPrincipal, GroupId, Protocol,
    TaskFailure, TaskFailureKind, TaskProgress, TaskState, TaskStatus, TokenId, UpstreamTaskId,
    UserId,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ActiveModelTrait, Set, entity::prelude::Json};

use crate::{
    DatabaseOptions, MigrationOptions,
    entity::{
        EncryptedJson, HeaderOverrides, SensitiveJson, TokenHash, channels, credentials, groups,
        tokens, users,
    },
};

use super::{
    AsyncTaskCreate, AsyncTaskCreateOutcome, AsyncTaskRepository, AsyncTaskRepositoryError,
    AsyncTaskTransition, AsyncTaskTransitionOutcome,
};

const CREATED_AT: u64 = 1_800_000_000;
const UPDATED_AT: u64 = CREATED_AT + 30;

#[tokio::test]
async fn create_is_idempotent_per_owner_and_rejects_payload_conflicts() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let first_write = fixture.task(&fixture.first, 0x11, 0x21, "gpt-video", "grok-video")?;
    let first = fixture.repository.create(first_write).await?;
    let AsyncTaskCreateOutcome::Created(first) = first else {
        panic!("首次写入必须创建任务")
    };
    assert_eq!(first.version(), 1);
    assert_eq!(first.status().state(), TaskState::Submitted);

    let replay = fixture
        .repository
        .create(fixture.task(&fixture.first, 0x11, 0x21, "gpt-video", "grok-video")?)
        .await?;
    assert!(matches!(replay, AsyncTaskCreateOutcome::Existing(_)));

    let conflict = fixture
        .repository
        .create(fixture.task(
            &fixture.first,
            0x11,
            0x21,
            "gpt-video",
            "different-upstream-model",
        )?)
        .await;
    assert!(matches!(conflict, Err(AsyncTaskRepositoryError::Conflict)));

    let other_owner = fixture
        .repository
        .create(fixture.task(&fixture.second, 0x12, 0x21, "gpt-video", "grok-video")?)
        .await?;
    assert!(matches!(other_owner, AsyncTaskCreateOutcome::Created(_)));
    assert!(
        fixture
            .repository
            .find(fixture.second.principal.user_id(), task_id(0x11))
            .await?
            .is_none()
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn create_rejects_mismatched_owner_and_binding_references() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let wrong_owner = GatewayPrincipal::new(
        fixture.first.principal.token_id(),
        fixture.second.principal.user_id(),
        fixture.first.principal.group_id(),
    );
    let write = AsyncTaskCreate::new(
        task_id(0x31),
        request_id(0x41),
        wrong_owner,
        Protocol::XaiVideo,
        "gpt-video".to_owned(),
        "grok-video".to_owned(),
        fixture.channel_id,
        fixture.credential_id,
        u64::MAX - 7,
        UpstreamTaskId::new("upstream-owner-mismatch")?,
        submitted(),
        CREATED_AT,
    )?;
    assert!(matches!(
        fixture.repository.create(write).await?,
        AsyncTaskCreateOutcome::NotFound
    ));

    let missing_credential = AsyncTaskCreate::new(
        task_id(0x32),
        request_id(0x42),
        fixture.first.principal,
        Protocol::XaiVideo,
        "gpt-video".to_owned(),
        "grok-video".to_owned(),
        fixture.channel_id,
        CredentialId::new(i64::MAX)?,
        u64::MAX - 7,
        UpstreamTaskId::new("upstream-binding-mismatch")?,
        submitted(),
        CREATED_AT,
    )?;
    assert!(matches!(
        fixture.repository.create(missing_credential).await?,
        AsyncTaskCreateOutcome::NotFound
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn state_machine_applies_only_declared_forward_transitions() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .repository
        .create(fixture.task(&fixture.first, 0x51, 0x61, "gpt-video", "grok-video")?)
        .await?;

    let queued = fixture
        .repository
        .transition(transition(
            0x51,
            fixture.first.principal.user_id(),
            1,
            TaskStatus::Queued {
                progress: TaskProgress::new(100)?,
            },
            UPDATED_AT,
        )?)
        .await?;
    let AsyncTaskTransitionOutcome::Applied(queued) = queued else {
        panic!("submitted 必须允许推进到 queued")
    };
    assert_eq!(queued.version(), 2);

    let running = fixture
        .repository
        .transition(transition(
            0x51,
            fixture.first.principal.user_id(),
            2,
            TaskStatus::Running {
                progress: TaskProgress::new(5_000)?,
            },
            UPDATED_AT + 1,
        )?)
        .await?;
    assert!(matches!(running, AsyncTaskTransitionOutcome::Applied(_)));

    let succeeded = fixture
        .repository
        .transition(transition(
            0x51,
            fixture.first.principal.user_id(),
            3,
            TaskStatus::Succeeded,
            UPDATED_AT + 2,
        )?)
        .await?;
    let AsyncTaskTransitionOutcome::Applied(succeeded) = succeeded else {
        panic!("running 必须允许推进到 succeeded")
    };
    assert_eq!(succeeded.version(), 4);
    assert_eq!(succeeded.terminal_at(), Some(UPDATED_AT + 2));

    let replay = fixture
        .repository
        .transition(transition(
            0x51,
            fixture.first.principal.user_id(),
            3,
            TaskStatus::Succeeded,
            UPDATED_AT + 3,
        )?)
        .await?;
    assert!(matches!(replay, AsyncTaskTransitionOutcome::Existing(_)));

    let rollback = fixture
        .repository
        .transition(transition(
            0x51,
            fixture.first.principal.user_id(),
            4,
            TaskStatus::Running {
                progress: TaskProgress::new(9_000)?,
            },
            UPDATED_AT + 4,
        )?)
        .await;
    assert!(matches!(rollback, Err(AsyncTaskRepositoryError::Conflict)));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn state_machine_rejects_version_conflicts_and_normalizes_terminal_failure_replays()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .repository
        .create(fixture.task(&fixture.first, 0x71, 0x81, "gpt-video", "grok-video")?)
        .await?;

    let wrong_version = fixture
        .repository
        .transition(transition(
            0x71,
            fixture.first.principal.user_id(),
            2,
            TaskStatus::Running {
                progress: TaskProgress::new(500)?,
            },
            UPDATED_AT,
        )?)
        .await;
    assert!(matches!(
        wrong_version,
        Err(AsyncTaskRepositoryError::Conflict)
    ));

    let running = fixture
        .repository
        .transition(transition(
            0x71,
            fixture.first.principal.user_id(),
            1,
            TaskStatus::Running {
                progress: TaskProgress::new(500)?,
            },
            UPDATED_AT,
        )?)
        .await?;
    assert!(matches!(running, AsyncTaskTransitionOutcome::Applied(_)));

    let same_version_different_state = fixture
        .repository
        .transition(transition(
            0x71,
            fixture.first.principal.user_id(),
            1,
            failed("first failure reason")?,
            UPDATED_AT + 1,
        )?)
        .await;
    assert!(matches!(
        same_version_different_state,
        Err(AsyncTaskRepositoryError::Conflict)
    ));

    let failed_outcome = fixture
        .repository
        .transition(transition(
            0x71,
            fixture.first.principal.user_id(),
            2,
            failed("first failure reason")?,
            UPDATED_AT + 2,
        )?)
        .await?;
    assert!(matches!(
        failed_outcome,
        AsyncTaskTransitionOutcome::Applied(_)
    ));

    let replay = fixture
        .repository
        .transition(transition(
            0x71,
            fixture.first.principal.user_id(),
            2,
            failed("different redacted reason")?,
            UPDATED_AT + 3,
        )?)
        .await?;
    assert!(matches!(replay, AsyncTaskTransitionOutcome::Existing(_)));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn public_debug_output_hides_models_and_upstream_task_identifiers()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let write = fixture.task(
        &fixture.first,
        0x91,
        0xa1,
        "private-requested-model",
        "private-upstream-model",
    )?;
    let write_debug = format!("{write:?}");
    assert!(!write_debug.contains("private-requested-model"));
    assert!(!write_debug.contains("private-upstream-model"));
    assert!(!write_debug.contains("upstream-task-secret"));

    let AsyncTaskCreateOutcome::Created(record) = fixture.repository.create(write).await? else {
        panic!("首次写入必须创建任务")
    };
    let record_debug = format!("{record:?}");
    assert!(!record_debug.contains(record.requested_model()));
    assert!(!record_debug.contains(record.upstream_model()));
    assert!(!record_debug.contains(record.upstream_task_id().as_str()));
    assert_eq!(record.credential_revision(), u64::MAX - 7);

    fixture.pool.close().await?;
    Ok(())
}

struct Owner {
    principal: GatewayPrincipal,
}

struct Fixture {
    pool: crate::DatabasePool,
    repository: AsyncTaskRepository,
    first: Owner,
    second: Owner,
    channel_id: ChannelId,
    credential_id: CredentialId,
}

impl Fixture {
    fn task(
        &self,
        owner: &Owner,
        task_marker: u8,
        request_marker: u8,
        requested_model: &str,
        upstream_model: &str,
    ) -> Result<AsyncTaskCreate, Box<dyn Error>> {
        Ok(AsyncTaskCreate::new(
            task_id(task_marker),
            request_id(request_marker),
            owner.principal,
            Protocol::XaiVideo,
            requested_model.to_owned(),
            upstream_model.to_owned(),
            self.channel_id,
            self.credential_id,
            u64::MAX - 7,
            UpstreamTaskId::new("upstream-task-secret")?,
            submitted(),
            CREATED_AT,
        )?)
    }
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let first = insert_owner(&pool, "first", 0x11).await?;
    let second = insert_owner(&pool, "second", 0x22).await?;
    let channel = channels::ActiveModel {
        name: Set("异步任务测试渠道".to_owned()),
        r#type: Set("xai".to_owned()),
        protocol: Set("xai_video".to_owned()),
        model_mapping: Set(Json::Object(Default::default())),
        param_override: Set(Json::Object(Default::default())),
        header_override: Set(HeaderOverrides::validate(Json::Object(Default::default()))?),
        settings: Set(SensitiveJson::from(Json::Object(Default::default()))),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let credential = credentials::ActiveModel {
        channel_id: Set(channel.id),
        kind: Set("api_key".to_owned()),
        secret: Set(EncryptedJson::from_envelope(encrypted_envelope(
            "async-task",
        ))?),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(Fixture {
        repository: AsyncTaskRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        first,
        second,
        channel_id: ChannelId::new(channel.id)?,
        credential_id: CredentialId::new(credential.id)?,
    })
}

async fn insert_owner(
    pool: &crate::DatabasePool,
    marker: &str,
    hash_marker: u8,
) -> Result<Owner, Box<dyn Error>> {
    let group = groups::ActiveModel {
        name: Set(format!("async-task-{marker}")),
        display_name: Set(format!("异步任务测试-{marker}")),
        flags: Set(Json::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set(format!("async-task-{marker}")),
        default_group_id: Set(group.id),
        aff_code: Set(format!("async-task-aff-{marker}")),
        settings: Set(Json::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let token = tokens::ActiveModel {
        user_id: Set(user.id),
        key_hash: Set(TokenHash::parse(&format!("{hash_marker:02x}").repeat(32))?),
        key_prefix: Set(format!("sk-{marker}")),
        name: Set(format!("异步任务令牌-{marker}")),
        group_id: Set(Some(group.id)),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(Owner {
        principal: GatewayPrincipal::new(
            TokenId::new(token.id)?,
            UserId::new(user.id)?,
            GroupId::new(group.id)?,
        ),
    })
}

fn transition(
    marker: u8,
    user_id: UserId,
    version: u64,
    status: TaskStatus,
    observed_at: u64,
) -> Result<AsyncTaskTransition, crate::AsyncTaskInputError> {
    AsyncTaskTransition::new(task_id(marker), user_id, version, status, observed_at)
}

fn submitted() -> TaskStatus {
    TaskStatus::Submitted {
        progress: TaskProgress::ZERO,
    }
}

fn failed(reason: &str) -> Result<TaskStatus, af_domain::TaskFailureError> {
    Ok(TaskStatus::Failed {
        failure: TaskFailure::with_reason(TaskFailureKind::Upstream, reason)?,
    })
}

fn task_id(marker: u8) -> AsyncTaskId {
    AsyncTaskId::new([marker; 16]).expect("测试任务标识必须非零")
}

fn request_id(marker: u8) -> AsyncTaskRequestId {
    AsyncTaskRequestId::new([marker; 16]).expect("测试幂等标识必须非零")
}

fn encrypted_envelope(marker: &str) -> Json {
    Json::Object(
        [
            ("version".to_owned(), Json::from(1)),
            (
                "algorithm".to_owned(),
                Json::String("xchacha20poly1305".to_owned()),
            ),
            ("key_id".to_owned(), Json::String(marker.to_owned())),
            (
                "nonce".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([0x42; 24])),
            ),
            (
                "ciphertext".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([0xa5; 32])),
            ),
        ]
        .into_iter()
        .collect(),
    )
}
