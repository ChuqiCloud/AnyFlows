use super::*;
use af_db::{
    AsyncTaskCreateOutcome, AsyncTaskSubmissionAccept, AsyncTaskSubmissionBegin,
    AsyncTaskSubmissionClaim, AsyncTaskSubmissionClaimOutcome, AsyncTaskSubmissionMutationOutcome,
    AsyncTaskTransition, AsyncTaskTransitionOutcome, MerchantPoolRepository,
    SchedulerRuntimeRepository,
};
use af_domain::{AsyncTaskAttemptId, AsyncTaskRequestFingerprint, MerchantBillingSnapshot};

const POOL_ID: i64 = 901;
const MERCHANT_ID: i64 = 902;
const SERVICE_ID: i64 = 903;
const REVISION_ID: i64 = 904;
const PRICE_ID: i64 = 905;

#[tokio::test]
async fn retired_merchant_supply_only_remains_available_for_bound_completed_results() {
    let (proxy_address, proxy_server) = spawn_proxy(vec![ProxyStep::new(
        "GET http://upstream.example/v1/videos/retired-video-task",
        "Bearer private-key-second",
        "200 OK",
        br#"{"status":"done","video":{"url":"https://vidgen.x.ai/retired.mp4","duration":8,"respect_moderation":true},"model":"grok-imagine-video-1.5"}"#,
    )]);
    let source = MutableSource::new(vec![single_runtime_record(
        proxy_address,
        SECOND_CREDENTIAL_ID,
        0x32,
        "private-key-second",
    )]);
    let (service, database, principal) = service(source.clone()).await;
    seed_merchant_supply(&database, proxy_address).await;
    let merchants = MerchantPoolRepository::new(database.pool().clone(), Duration::from_secs(5));
    let service = service.with_merchant_pools(merchants.clone());
    let repository = SchedulerRuntimeRepository::new(database.pool().clone());
    assert_eq!(repository.load_all().await.unwrap().len(), 1);

    let target = service
        .scheduler
        .runtime_target(ChannelId::new(CHANNEL_ID).unwrap())
        .unwrap()
        .unwrap();
    let credential = &target.credentials()[0];
    let binding_target = SubmissionBindingTarget {
        channel_id: target.channel_id(),
        credential_id: CredentialId::new(credential.credential_id()).unwrap(),
        credential_revision: credential.credential_revision(),
        snapshot: BoundVideoTargetSnapshot::new(
            &target,
            credential,
            VideoModel::new(REQUESTED_MODEL).unwrap(),
            VideoModel::new(UPSTREAM_MODEL).unwrap(),
            Duration::from_secs(5),
        ),
    };
    let binding = binding_target.into_binding(
        principal.group_id(),
        UpstreamTaskId::new("retired-video-task").unwrap(),
    );
    let submissions =
        AsyncTaskSubmissionRepository::new(database.pool().clone(), Duration::from_secs(5))
            .unwrap();
    let tasks = AsyncTaskRepository::new(database.pool().clone(), Duration::from_secs(5)).unwrap();
    let AsyncTaskSubmissionClaimOutcome::Created(claim) = submissions
        .claim(
            AsyncTaskSubmissionClaim::new(
                AsyncTaskId::new([0xa1; 16]).unwrap(),
                AsyncTaskRequestId::new([0xa2; 16]).unwrap(),
                principal,
                Protocol::XaiVideo,
                REQUESTED_MODEL.to_owned(),
                AsyncTaskRequestFingerprint::new([0xa3; 32]),
                Some(8),
                1_700_000_000,
            )
            .unwrap(),
        )
        .await
        .unwrap()
    else {
        panic!("claim must be created")
    };
    let terms = MerchantBillingSnapshot::new(
        POOL_ID,
        MERCHANT_ID,
        SERVICE_ID,
        PRICE_ID,
        1_000_000,
        500,
        0,
    )
    .unwrap();
    let claim = submissions
        .bind_merchant(
            &claim,
            principal
                .with_user_route_id(Some(906))
                .with_merchant_target(terms, target.channel_id()),
        )
        .await
        .unwrap();
    let attempt_id = AsyncTaskAttemptId::new([0xa4; 16]).unwrap();
    let AsyncTaskSubmissionMutationOutcome::Applied(claim) = submissions
        .begin(
            AsyncTaskSubmissionBegin::new(
                claim.task_id(),
                principal.user_id(),
                claim.version(),
                attempt_id,
                1_700_000_001,
            )
            .unwrap(),
        )
        .await
        .unwrap()
    else {
        panic!("claim must be begun")
    };
    let AsyncTaskSubmissionMutationOutcome::Applied(claim) = submissions
        .accept(
            AsyncTaskSubmissionAccept::new(
                claim.task_id(),
                principal.user_id(),
                claim.version(),
                attempt_id,
                principal.group_id(),
                UPSTREAM_MODEL.to_owned(),
                binding.channel_id(),
                binding.credential_id(),
                binding.credential_revision(),
                binding.upstream_task_id().clone(),
                binding.binding_fingerprint(),
                binding.attempt_timeout(),
                None,
                TaskStatus::Submitted {
                    progress: af_domain::TaskProgress::ZERO,
                },
                1_700_000_002,
            )
            .unwrap(),
        )
        .await
        .unwrap()
    else {
        panic!("claim must be accepted")
    };
    let AsyncTaskCreateOutcome::Created(task) = tasks
        .create(claim.task_create().unwrap().unwrap())
        .await
        .unwrap()
    else {
        panic!("task must be created")
    };
    let AsyncTaskTransitionOutcome::Applied(task) = tasks
        .transition(
            AsyncTaskTransition::new(
                task.task_id(),
                principal.user_id(),
                task.version(),
                TaskStatus::Succeeded,
                1_700_000_003,
            )
            .unwrap(),
        )
        .await
        .unwrap()
    else {
        panic!("task must be completed")
    };

    merchants
        .delete_pool(principal.user_id(), POOL_ID)
        .await
        .unwrap();
    source.replace(Vec::new());
    service.scheduler.refresh().await.unwrap();
    assert!(repository.load_all().await.unwrap().is_empty());
    assert!(
        service
            .scheduler
            .runtime_target(binding.channel_id())
            .unwrap()
            .is_none()
    );
    assert!(
        service
            .submit_video_task(
                principal.group_id(),
                request(),
                "new-request-after-deletion"
            )
            .await
            .is_err()
    );
    assert_eq!(
        service
            .poll_video_task(&binding, "unrestored-binding")
            .await,
        Err(AfError::Internal)
    );
    let restored = service
        .restore_video_task_binding(&task, &claim)
        .await
        .unwrap();
    assert!(restored.retired_target.is_some());
    let poll = service
        .poll_video_task(&restored, "historical-result-after-deletion")
        .await
        .unwrap();
    assert_eq!(poll.status(), &TaskStatus::Succeeded);
    assert!(poll.output().unwrap().as_video().is_some());
    proxy_server.join().unwrap();

    database
        .seed()
        .execute_unprepared(&format!(
            "UPDATE merchant_service_revisions SET credential_id={FIRST_CREDENTIAL_ID} WHERE id={REVISION_ID}"
        ))
        .await
        .unwrap();
    assert!(matches!(
        service.restore_video_task_binding(&task, &claim).await,
        Err(AfError::Internal)
    ));
    database
        .seed()
        .execute_unprepared(&format!(
            "UPDATE merchant_service_revisions SET credential_id={SECOND_CREDENTIAL_ID} WHERE id={REVISION_ID}"
        ))
        .await
        .unwrap();
    database
        .seed()
        .execute_unprepared(&format!(
            "UPDATE channels SET header_override='{{\"x-video-runtime\":\"changed\"}}' WHERE id={CHANNEL_ID}"
        ))
        .await
        .unwrap();
    assert!(matches!(
        service.restore_video_task_binding(&task, &claim).await,
        Err(AfError::Internal)
    ));
    database.close().await;
}

async fn seed_merchant_supply(database: &SqliteTestDatabase, proxy_address: SocketAddr) {
    for sql in [
        format!(
            "UPDATE channels SET status=1, base_url='http://upstream.example', model_mapping='{{\"video-public\":\"{UPSTREAM_MODEL}\"}}', header_override='{{\"x-video-runtime\":\"enabled\"}}' WHERE id={CHANNEL_ID}"
        ),
        format!(
            "INSERT INTO proxies (id,name,active_name,scheme,host,port,trust_proxy_dns,enabled,version) VALUES ({SECOND_CREDENTIAL_ID},'retired-video','retired-video','http','{}',{},1,1,1)",
            proxy_address.ip(),
            proxy_address.port()
        ),
        format!(
            "UPDATE credentials SET status=1,schedulable=1,priority=10,weight=10,proxy_id={SECOND_CREDENTIAL_ID} WHERE id={SECOND_CREDENTIAL_ID}"
        ),
        format!(
            "INSERT INTO channel_models (channel_id,model) VALUES ({CHANNEL_ID},'{REQUESTED_MODEL}')"
        ),
        format!(
            "INSERT INTO channel_groups (channel_id,group_id) VALUES ({CHANNEL_ID},{GROUP_ID})"
        ),
        format!(
            "INSERT INTO abilities (channel_id,group_id,model,enabled,priority,weight) VALUES ({CHANNEL_ID},{GROUP_ID},'{REQUESTED_MODEL}',1,10,1)"
        ),
        format!(
            "INSERT INTO merchant_profiles (id,owner_user_id,name,description,status) VALUES ({MERCHANT_ID},{USER_ID},'Video merchant','','approved')"
        ),
        format!(
            "INSERT INTO merchant_pools (id,group_id,name,description,enabled,commission_bps,min_ratio_micros,max_ratio_micros,settlement_delay_secs) VALUES ({POOL_ID},{GROUP_ID},'Video pool','',1,500,100000,10000000,0)"
        ),
        format!(
            "INSERT INTO merchant_services (id,merchant_id,pool_id,name,description,status,published_revision_id) VALUES ({SERVICE_ID},{MERCHANT_ID},{POOL_ID},'Video supply','','published',{REVISION_ID})"
        ),
        format!(
            "INSERT INTO merchant_service_revisions (id,service_id,revision,status,name,description,base_url,protocol,default_ratio_micros,models,channel_id,credential_id,timeout_secs) VALUES ({REVISION_ID},{SERVICE_ID},1,'published','Video supply','','http://upstream.example','xai_video',1000000,'[]',{CHANNEL_ID},{SECOND_CREDENTIAL_ID},60)"
        ),
        format!(
            "INSERT INTO merchant_service_prices (id,service_id,revision_id,effective_at,default_ratio_micros,models,status) VALUES ({PRICE_ID},{SERVICE_ID},{REVISION_ID},0,1000000,'[]','active')"
        ),
    ] {
        database.seed().execute_unprepared(&sql).await.unwrap();
    }
    let envelope = encrypted_api_key(SECOND_CREDENTIAL_ID, 0x32, "private-key-second");
    let secret = json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    });
    database
        .seed()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE credentials SET secret=? WHERE id=?",
            [secret.to_string().into(), SECOND_CREDENTIAL_ID.into()],
        ))
        .await
        .unwrap();
}
