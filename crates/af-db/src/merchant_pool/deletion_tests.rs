use super::*;

#[tokio::test]
async fn deleting_pool_retires_all_revision_channels_and_keeps_group_isolation()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let published = publish(&fixture).await?;
    fixture
        .repository
        .write_service(
            UserId::new(1)?,
            Some(published.id),
            service_write(fixture.shared_pool),
            envelope,
        )
        .await?;
    fixture
        .repository
        .delete_pool(UserId::new(3)?, fixture.shared_pool)
        .await?;
    fixture
        .repository
        .delete_pool(UserId::new(3)?, fixture.shared_pool)
        .await?;
    assert!(
        fixture
            .repository
            .list_pools(None, MerchantListQuery::default())
            .await?
            .items
            .is_empty()
    );
    let archived = MerchantListQuery {
        status: Some("deleted".to_owned()),
        ..MerchantListQuery::default()
    };
    assert_eq!(
        fixture
            .repository
            .list_pools(None, archived.clone())
            .await?
            .items
            .len(),
        1
    );
    assert!(
        fixture
            .repository
            .list_pools(Some(UserId::new(2)?), archived)
            .await?
            .items
            .is_empty()
    );
    let mapped = fixture
        .repository
        .pool_for_group(GroupId::new(1)?)
        .await?
        .expect("pool tombstone");
    assert_eq!(mapped.id, fixture.shared_pool);
    assert!(!mapped.enabled);
    assert!(
        fixture
            .repository
            .list_services(None, MerchantListQuery::default())
            .await?
            .items
            .is_empty()
    );
    assert_eq!(
        revisions::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        2
    );
    assert_eq!(
        channels::Entity::find()
            .filter(channels::Column::DeletedAt.is_null())
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        credentials::Entity::find()
            .filter(credentials::Column::DeletedAt.is_null())
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        crate::entity::abilities::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert!(matches!(
        fixture
            .repository
            .pause_service(UserId::new(1)?, published.id, false)
            .await,
        Err(MerchantPoolError::NotFound)
    ));
    assert!(matches!(
        fixture
            .repository
            .review_service(UserId::new(3)?, published.id, review("approve", Some(2)))
            .await,
        Err(MerchantPoolError::NotFound)
    ));
    assert!(matches!(
        fixture
            .repository
            .write_pool(UserId::new(3)?, Some(fixture.shared_pool), pool_write())
            .await,
        Err(MerchantPoolError::NotFound)
    ));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn deleting_merchant_preserves_finance_history_and_prevents_supply_reentry()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let service = publish(&fixture).await?;
    fixture.pool.connection().execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO merchant_finance_entries (event_key,merchant_id,kind,reference,gross_quota,commission_quota,net_quota,created_at) VALUES ('retired-history',?,'test','history',0,0,0,0)",
        [fixture.merchant.into()],
    )).await?;
    fixture
        .repository
        .delete_merchant(UserId::new(3)?, fixture.merchant)
        .await?;
    fixture
        .repository
        .delete_merchant(UserId::new(3)?, fixture.merchant)
        .await?;
    assert!(
        fixture
            .repository
            .list_profiles(MerchantListQuery::default())
            .await?
            .items
            .is_empty()
    );
    assert!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?
            .is_empty()
    );
    assert!(
        fixture
            .repository
            .owner_merchant(UserId::new(1)?)
            .await?
            .is_some()
    );
    assert!(matches!(
        fixture
            .repository
            .apply(
                UserId::new(1)?,
                MerchantApplication {
                    name: "Reentry".to_owned(),
                    description: "".to_owned(),
                }
            )
            .await,
        Err(MerchantPoolError::Conflict)
    ));
    assert!(matches!(
        fixture
            .repository
            .review_merchant(UserId::new(3)?, fixture.merchant, review("approve", None))
            .await,
        Err(MerchantPoolError::NotFound)
    ));
    assert!(matches!(
        fixture.repository.get_service(None, service.id).await,
        Err(MerchantPoolError::NotFound)
    ));
    let finance =
        crate::MerchantFinanceRepository::new(fixture.pool.clone(), Duration::from_secs(5));
    assert_eq!(
        finance
            .entries(fixture.merchant, 20, None)
            .await?
            .items
            .len(),
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_deletion_rejects_unsettled_balances_and_rolls_back_tombstones()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let published = publish(&fixture).await?;
    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO merchant_finance_accounts (merchant_id,available_quota) VALUES (?,1)",
            [fixture.merchant.into()],
        ))
        .await?;
    assert_eq!(
        fixture
            .repository
            .delete_merchant(UserId::new(3)?, fixture.merchant)
            .await,
        Err(MerchantPoolError::InUse)
    );
    assert_eq!(
        fixture
            .repository
            .get_merchant(fixture.merchant)
            .await?
            .status,
        "approved"
    );
    assert_eq!(
        fixture
            .repository
            .get_service(None, published.id)
            .await?
            .status,
        "published"
    );
    assert_eq!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?
            .len(),
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn disabled_owner_does_not_prevent_deleting_unused_merchant_drafts()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .repository
        .write_service(
            UserId::new(1)?,
            None,
            service_write(fixture.shared_pool),
            envelope,
        )
        .await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("UPDATE users SET status=2 WHERE id=1")
        .await?;
    fixture
        .repository
        .delete_merchant(UserId::new(3)?, fixture.merchant)
        .await?;
    assert!(
        fixture
            .repository
            .list_profiles(MerchantListQuery::default())
            .await?
            .items
            .is_empty()
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn accepted_tasks_block_deletion_until_polled_terminal_state() -> Result<(), Box<dyn Error>> {
    use crate::{
        AsyncTaskCreateOutcome, AsyncTaskRepository, AsyncTaskSubmissionAccept,
        AsyncTaskSubmissionBegin, AsyncTaskSubmissionClaim, AsyncTaskSubmissionClaimOutcome,
        AsyncTaskSubmissionMutationOutcome, AsyncTaskSubmissionRepository, AsyncTaskTransition,
    };
    use af_domain::{
        AsyncTaskAttemptId, AsyncTaskBindingFingerprint, AsyncTaskId, AsyncTaskRequestFingerprint,
        AsyncTaskRequestId, GatewayPrincipal, Protocol, TaskProgress, TaskStatus, TokenId,
        UpstreamTaskId,
    };
    let fixture = fixture().await?;
    let service = publish(&fixture).await?;
    let revision = revisions::Entity::find()
        .filter(revisions::Column::ServiceId.eq(service.id))
        .one(fixture.pool.connection())
        .await?
        .expect("published revision");
    fixture.pool.connection().execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO tokens (id,user_id,key_hash,key_prefix,name,status,remain_quota) VALUES (1,2,?,'sk-test','Task consumer',1,1000)",
        ["a".repeat(64).into()],
    )).await?;
    let principal = GatewayPrincipal::new(TokenId::new(1)?, UserId::new(2)?, GroupId::new(1)?);
    let tasks = AsyncTaskRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let submissions =
        AsyncTaskSubmissionRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let task_id = AsyncTaskId::new([21; 16])?;
    let AsyncTaskSubmissionClaimOutcome::Created(claim) = submissions
        .claim(AsyncTaskSubmissionClaim::new(
            task_id,
            AsyncTaskRequestId::new([22; 16])?,
            principal,
            Protocol::XaiVideo,
            "grok-imagine-video".to_owned(),
            AsyncTaskRequestFingerprint::new([23; 32]),
            None,
            1000,
        )?)
        .await?
    else {
        panic!("claim must be created");
    };
    let principal = principal.with_user_route_id(Some(1)).with_merchant_target(
        af_domain::MerchantBillingSnapshot::new(
            fixture.shared_pool,
            fixture.merchant,
            service.id,
            1,
            500_000,
            500,
            0,
        )?,
        ChannelId::new(revision.channel_id)?,
    );
    let bound = submissions.bind_merchant(&claim, principal).await?;
    let attempt = AsyncTaskAttemptId::new([24; 16])?;
    let AsyncTaskSubmissionMutationOutcome::Applied(begun) = submissions
        .begin(AsyncTaskSubmissionBegin::new(
            task_id,
            UserId::new(2)?,
            bound.version(),
            attempt,
            1001,
        )?)
        .await?
    else {
        panic!("claim must begin");
    };
    assert_eq!(
        super::super::deletion::unresolved_merchant_claims()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        fixture
            .repository
            .delete_pool(UserId::new(3)?, fixture.shared_pool)
            .await,
        Err(MerchantPoolError::InUse)
    );
    let AsyncTaskSubmissionMutationOutcome::Applied(accepted) = submissions
        .accept(AsyncTaskSubmissionAccept::new(
            task_id,
            UserId::new(2)?,
            begun.version(),
            attempt,
            GroupId::new(1)?,
            "grok-imagine-video".to_owned(),
            ChannelId::new(revision.channel_id)?,
            CredentialId::new(revision.credential_id)?,
            1,
            UpstreamTaskId::new("retirement-test-task")?,
            AsyncTaskBindingFingerprint::new([25; 32]),
            Duration::from_secs(5),
            None,
            TaskStatus::Submitted {
                progress: TaskProgress::ZERO,
            },
            1002,
        )?)
        .await?
    else {
        panic!("claim must be accepted");
    };
    // Acceptance without a materialized task must still block retirement.
    assert_eq!(
        fixture
            .repository
            .delete_pool(UserId::new(3)?, fixture.shared_pool)
            .await,
        Err(MerchantPoolError::InUse)
    );
    fixture
        .pool
        .connection()
        .execute_unprepared(
            "UPDATE async_task_submission_claims SET status=4,progress_basis_points=10000",
        )
        .await?;
    // A terminal provider acceptance is sufficient when no task was materialized.
    assert_eq!(
        super::super::deletion::unresolved_merchant_claims()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    let AsyncTaskCreateOutcome::Created(task) = tasks
        .create(accepted.task_create()?.expect("accepted task"))
        .await?
    else {
        panic!("task must be created");
    };
    // A materialized live task takes precedence over a terminal claim snapshot.
    assert_eq!(
        super::super::deletion::unresolved_merchant_claims()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        fixture
            .repository
            .delete_merchant(UserId::new(3)?, fixture.merchant)
            .await,
        Err(MerchantPoolError::InUse)
    );
    fixture
        .pool
        .connection()
        .execute_unprepared(
            "UPDATE async_task_submission_claims SET status=1,progress_basis_points=0",
        )
        .await?;
    tasks
        .transition(AsyncTaskTransition::new(
            task_id,
            UserId::new(2)?,
            task.version(),
            TaskStatus::Succeeded,
            1003,
        )?)
        .await?;
    // Polling changes the task only; terminal history must be excluded from the lock set.
    assert_eq!(
        super::super::deletion::unresolved_merchant_claims()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    fixture
        .repository
        .delete_pool(UserId::new(3)?, fixture.shared_pool)
        .await?;
    assert!(
        tasks
            .find_by_request(UserId::new(2)?, AsyncTaskRequestId::new([22; 16])?)
            .await?
            .is_some()
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn unattempted_merchant_claim_does_not_keep_unused_supply_alive() -> Result<(), Box<dyn Error>>
{
    use crate::{
        AsyncTaskSubmissionClaim, AsyncTaskSubmissionClaimOutcome, AsyncTaskSubmissionRepository,
    };
    use af_domain::{
        AsyncTaskId, AsyncTaskRequestFingerprint, AsyncTaskRequestId, GatewayPrincipal, Protocol,
        TokenId,
    };
    let fixture = fixture().await?;
    let service = publish(&fixture).await?;
    let revision = revisions::Entity::find()
        .filter(revisions::Column::ServiceId.eq(service.id))
        .one(fixture.pool.connection())
        .await?
        .expect("published revision");
    fixture.pool.connection().execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO tokens (id,user_id,key_hash,key_prefix,name,status,remain_quota) VALUES (1,2,?,'sk-test','Task consumer',1,1000)",
        ["b".repeat(64).into()],
    )).await?;
    let principal = GatewayPrincipal::new(TokenId::new(1)?, UserId::new(2)?, GroupId::new(1)?);
    let submissions =
        AsyncTaskSubmissionRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let AsyncTaskSubmissionClaimOutcome::Created(claim) = submissions
        .claim(AsyncTaskSubmissionClaim::new(
            AsyncTaskId::new([31; 16])?,
            AsyncTaskRequestId::new([32; 16])?,
            principal,
            Protocol::XaiVideo,
            "grok-imagine-video".to_owned(),
            AsyncTaskRequestFingerprint::new([33; 32]),
            None,
            1000,
        )?)
        .await?
    else {
        panic!("claim must be created");
    };
    let principal = principal.with_user_route_id(Some(1)).with_merchant_target(
        af_domain::MerchantBillingSnapshot::new(
            fixture.shared_pool,
            fixture.merchant,
            service.id,
            1,
            500_000,
            500,
            0,
        )?,
        ChannelId::new(revision.channel_id)?,
    );
    submissions.bind_merchant(&claim, principal).await?;
    assert_eq!(
        super::super::deletion::unresolved_merchant_claims()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    fixture.pool.connection().execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO billing_reservations (idempotency_key,user_id,token_id,group_id,reserved_quota,expires_at) VALUES (?,2,1,1,100,datetime('now','+1 minute'))",
        ["d".repeat(32).into()],
    )).await?;
    fixture.pool.connection().execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO merchant_billing_reservations (reservation_key,pool_id,merchant_id,service_id,pricing_version,sales_ratio_micros,commission_bps,settlement_delay_secs) VALUES (?,?,?,?,1,500000,500,0)",
        ["d".repeat(32).into(), fixture.shared_pool.into(), fixture.merchant.into(), service.id.into()],
    )).await?;
    assert_eq!(
        fixture
            .repository
            .delete_merchant(UserId::new(3)?, fixture.merchant)
            .await,
        Err(MerchantPoolError::InUse)
    );
    fixture
        .pool
        .connection()
        .execute_unprepared("UPDATE merchant_billing_reservations SET status='canceled'")
        .await?;
    fixture
        .repository
        .delete_merchant(UserId::new(3)?, fixture.merchant)
        .await?;
    assert!(
        fixture
            .repository
            .list_profiles(MerchantListQuery::default())
            .await?
            .items
            .is_empty()
    );
    fixture.pool.close().await?;
    Ok(())
}
