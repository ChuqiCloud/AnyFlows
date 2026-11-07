use super::*;

use af_billing::PricingContext;
use af_db::{
    GroupKeyAccessRepository, GroupKeySelectionMode, GroupModelPolicyRepository, QuotaRepository,
    RequestOutcomeRepository, UserRouteMemberWrite, UserRouteRepository, UserRouteWrite,
};
use af_domain::{UpstreamError, UserRouteStrategy};

#[derive(Clone, Copy)]
enum FirstGroupResult {
    Success,
    Retryable,
    ProtocolError,
    RequestAborted,
    BadRequest,
    AccountConcurrencyLimited,
    FullWithoutUsage,
    UnsupportedPlan,
}

struct GroupPlanner {
    first: FirstGroupResult,
    attempts: Arc<Mutex<Vec<GroupId>>>,
    pricing_model_override: Option<String>,
}

impl ChatRoutePlanner for GroupPlanner {
    fn plan<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        _: Option<ConcurrencyLimit>,
        request: CanonicalRequestEnvelope,
        _: Protocol,
        _: &'a str,
        _: RelayDiagnosticInput,
    ) -> ChatRoutePlanFuture<'a> {
        if principal.group_id().get() == 3
            && matches!(self.first, FirstGroupResult::UnsupportedPlan)
        {
            return Box::pin(async { Err(UpstreamError::ModelUnsupported.into()) });
        }
        let execution = GroupExecution {
            group_id: principal.group_id(),
            request,
            first: self.first,
            attempts: self.attempts.clone(),
            pricing_model_override: self.pricing_model_override.clone(),
        };
        Box::pin(async move { Ok(Box::new(execution) as Box<dyn PlannedChatExecution>) })
    }
}

struct GroupExecution {
    group_id: GroupId,
    request: CanonicalRequestEnvelope,
    first: FirstGroupResult,
    attempts: Arc<Mutex<Vec<GroupId>>>,
    pricing_model_override: Option<String>,
}

impl PlannedChatExecution for GroupExecution {
    fn target_group_id(&self) -> GroupId {
        self.group_id
    }
    fn pricing_model(&self) -> &str {
        self.pricing_model_override
            .as_deref()
            .unwrap_or(&self.request.canonical().model)
    }
    fn canonical_request(&self) -> &CanonicalRequestEnvelope {
        &self.request
    }
    fn execute(self: Box<Self>) -> PlannedChatExecutionFuture {
        Box::pin(async move {
            self.attempts.lock().unwrap().push(self.group_id);
            if self.group_id.get() == 3 {
                match self.first {
                    FirstGroupResult::Success => {}
                    FirstGroupResult::Retryable => return Err(UpstreamError::overloaded().into()),
                    FirstGroupResult::ProtocolError => {
                        return Err(UpstreamError::ProtocolError.into());
                    }
                    FirstGroupResult::RequestAborted => {
                        return Err(PlannedExecutionFailure::terminal(
                            UpstreamError::ProtocolError.into(),
                        ));
                    }
                    FirstGroupResult::BadRequest => return Err(UpstreamError::BadRequest.into()),
                    FirstGroupResult::AccountConcurrencyLimited => {
                        return Err(AfError::ConcurrencyLimited.into());
                    }
                    FirstGroupResult::FullWithoutUsage => {
                        return Ok(crate::RoutedExecution::new(
                            ChatResponse::Full {
                                body: Bytes::from_static(b"{\"complete\":true}"),
                                usage: Err(UsageResolutionError::InvalidSequence),
                            },
                            ChannelId::new(1).unwrap(),
                        ));
                    }
                    FirstGroupResult::UnsupportedPlan => {
                        unreachable!("unsupported plans never execute")
                    }
                }
            }
            Ok(crate::RoutedExecution::new(
                ChatResponse::Full {
                    body: Bytes::from_static(b"{}"),
                    usage: Ok(usage()),
                },
                ChannelId::new(1).unwrap(),
            ))
        })
    }
}

async fn routed_fixture(label: &str) -> (SqliteTestDatabase, UserRouteRuntime, GatewayPrincipal) {
    let database = SqliteTestDatabase::new(label).await;
    let backend = database.seed().get_database_backend();
    for sql in [
        "INSERT INTO groups (id, name, display_name, flags) VALUES (3, 'route-a', 'A', '{}'), (4, 'route-b', 'B', '{}')".to_owned(),
        "INSERT INTO users (id, username, status, default_group_id, aff_code, quota, settings) VALUES (2, 'routed-user', 1, 3, 'routed-aff', 1000000000, '{}')".to_owned(),
        format!("INSERT INTO tokens (id, user_id, key_hash, key_prefix, name, status, remain_quota) VALUES (1, 2, '{}', 'sk-route', 'routed-key', 1, 1000000000)", "ab".repeat(32)),
        "INSERT INTO channels (id, name, \"type\", protocol, model_mapping, param_override, header_override, settings) VALUES (1, 'route-channel', 'openai', 'openai_chat', '{}', '{}', '{}', '{}')".to_owned(),
    ] {
        database.seed().execute(Statement::from_string(backend, sql)).await.unwrap();
    }
    let timeout = Duration::from_secs(5);
    GroupKeyAccessRepository::new(database.pool().clone(), timeout)
        .unwrap()
        .update_policy(
            GroupId::new(4).unwrap(),
            GroupKeySelectionMode::AllUsers,
            Vec::new(),
        )
        .await
        .unwrap();
    let repository = UserRouteRepository::new(database.pool().clone(), timeout).unwrap();
    let route = repository
        .create(
            UserId::new(2).unwrap(),
            UserRouteWrite {
                name: "ordered route".to_owned(),
                strategy: UserRouteStrategy::Ordered,
                enabled: true,
                max_group_attempts: 2,
                max_price_ratio_micros: None,
                members: vec![
                    UserRouteMemberWrite {
                        group_id: GroupId::new(3).unwrap(),
                        priority: 0,
                        weight: 1,
                    },
                    UserRouteMemberWrite {
                        group_id: GroupId::new(4).unwrap(),
                        priority: 1,
                        weight: 1,
                    },
                ],
            },
        )
        .await
        .unwrap();
    database
        .seed()
        .execute(Statement::from_sql_and_values(
            backend,
            "UPDATE tokens SET user_route_id = ? WHERE id = 1",
            [route.id.into()],
        ))
        .await
        .unwrap();
    let runtime = UserRouteRuntime::new(repository).with_group_model_policy(
        GroupModelPolicyRepository::new(database.pool().clone(), timeout).unwrap(),
    );
    (
        database,
        runtime,
        principal().with_user_route_id(Some(route.id)),
    )
}

fn paid_service(
    database: &SqliteTestDatabase,
    runtime: UserRouteRuntime,
    planner: GroupPlanner,
    pricing: CachedRequestPricingSnapshotSource,
    usage_port: Arc<UsageStub>,
) -> BillingChatService {
    let quota = Arc::new(QuotaRepository::new(database.pool().clone()));
    BillingChatService::new(
        Arc::new(planner),
        Arc::new(pricing),
        Some(quota.clone()),
        Some(Arc::new(RefundStub)),
        Some(quota),
        usage_port,
    )
    .with_user_route_runtime(runtime)
}

#[tokio::test]
async fn user_route_releases_first_group_then_bills_actual_successful_group() {
    let (database, runtime, principal) = routed_fixture("user-route-billing").await;
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let usage_port = Arc::new(UsageStub::default());
    let pricing = metered_snapshot_source_with_prices(
        GroupId::new(4).unwrap(),
        TokenPrices::new(
            Decimal::ONE,
            Decimal::ONE,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        )
        .unwrap(),
    )
    .await;
    let expected = pricing
        .capture(
            "gpt-test",
            GroupId::new(3).unwrap(),
            GroupId::new(4).unwrap(),
            0,
        )
        .unwrap()
        .resolver()
        .resolve(&PricingContext::new(&usage()))
        .unwrap()
        .quota();
    let service = paid_service(
        &database,
        runtime,
        GroupPlanner {
            first: FirstGroupResult::Retryable,
            attempts: attempts.clone(),
            pricing_model_override: None,
        },
        pricing,
        usage_port.clone(),
    )
    .with_request_outcomes(crate::RequestOutcomeRuntime::new(
        RequestOutcomeRepository::new(database.pool().clone()),
    ));
    assert!(matches!(
        service
            .chat_completions(
                &principal,
                None,
                request(),
                Protocol::OpenAiChat,
                "routed-billed",
                diagnostic(),
            )
            .await
            .unwrap(),
        ChatResponse::Full { .. }
    ));
    assert_eq!(
        attempts.lock().unwrap().as_slice(),
        &[GroupId::new(3).unwrap(), GroupId::new(4).unwrap()]
    );
    {
        let records = usage_port.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].principal().unwrap().group_id(),
            GroupId::new(4).unwrap()
        );
        assert_eq!(
            records[0].principal().unwrap().billing_source_group_id(),
            GroupId::new(3).unwrap()
        );
        assert_eq!(records[0].quota(), expected);
    }
    let backend = database.seed().get_database_backend();
    let reservations = database
        .seed()
        .query_all(Statement::from_string(
            backend,
            "SELECT group_id, status FROM billing_reservations ORDER BY group_id".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(reservations.len(), 2);
    assert_eq!(reservations[0].try_get::<i64>("", "group_id").unwrap(), 3);
    assert_eq!(reservations[0].try_get::<i16>("", "status").unwrap(), 4);
    assert_eq!(reservations[1].try_get::<i64>("", "group_id").unwrap(), 4);
    assert_eq!(reservations[1].try_get::<i16>("", "status").unwrap(), 3);
    let wallet = database
        .seed()
        .query_one(Statement::from_string(
            backend,
            "SELECT quota, used_quota, frozen_quota FROM users WHERE id = 2".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        wallet.try_get::<i64>("", "quota").unwrap(),
        1_000_000_000 - expected.units()
    );
    assert_eq!(
        wallet.try_get::<i64>("", "used_quota").unwrap(),
        expected.units()
    );
    assert_eq!(wallet.try_get::<i64>("", "frozen_quota").unwrap(), 0);
    let outcome = database.seed().query_one(Statement::from_string(backend,
        "SELECT group_id, channel_id, outcome FROM request_outcome_logs WHERE request_id = 'routed-billed'".to_owned(),
    )).await.unwrap().unwrap();
    assert_eq!(outcome.try_get::<i64>("", "group_id").unwrap(), 4);
    assert_eq!(outcome.try_get::<i64>("", "channel_id").unwrap(), 1);
    assert_eq!(outcome.try_get::<i16>("", "outcome").unwrap(), 1);
    database.close().await;
}

#[tokio::test]
async fn user_route_attempt_limit_counts_upstream_calls_and_skips_unsupported_groups() {
    for (label, first, expected_group) in [
        ("route-limit-call", FirstGroupResult::Retryable, 3_i64),
        ("route-limit-plan", FirstGroupResult::UnsupportedPlan, 4_i64),
    ] {
        let (database, runtime, principal) = routed_fixture(label).await;
        database
            .seed()
            .execute(Statement::from_string(
                database.seed().get_database_backend(),
                "UPDATE user_routes SET max_group_attempts = 1".to_owned(),
            ))
            .await
            .unwrap();
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let service = paid_service(
            &database,
            runtime,
            GroupPlanner {
                first,
                attempts: attempts.clone(),
                pricing_model_override: None,
            },
            metered_snapshot_source_with_target(GroupId::new(4).unwrap()).await,
            Arc::new(UsageStub::default()),
        );
        let result = service
            .chat_completions(
                &principal,
                None,
                request(),
                Protocol::OpenAiChat,
                label,
                diagnostic(),
            )
            .await;
        assert_eq!(result.is_ok(), expected_group == 4);
        assert_eq!(
            attempts.lock().unwrap().as_slice(),
            &[GroupId::new(expected_group).unwrap()]
        );
        database.close().await;
    }
}

#[tokio::test]
async fn user_route_fails_over_when_preferred_group_account_slots_are_full() {
    let (database, runtime, principal) = routed_fixture("user-route-account-concurrency").await;
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let service = paid_service(
        &database,
        runtime,
        GroupPlanner {
            first: FirstGroupResult::AccountConcurrencyLimited,
            attempts: attempts.clone(),
            pricing_model_override: None,
        },
        metered_snapshot_source_with_target(GroupId::new(4).unwrap()).await,
        Arc::new(UsageStub::default()),
    );
    let result = service
        .chat_completions(
            &principal,
            None,
            request(),
            Protocol::OpenAiChat,
            "routed-account-concurrency",
            diagnostic(),
        )
        .await;

    assert!(matches!(result, Ok(ChatResponse::Full { .. })));
    assert_eq!(
        attempts.lock().unwrap().as_slice(),
        &[GroupId::new(3).unwrap(), GroupId::new(4).unwrap()]
    );
    database.close().await;
}

#[tokio::test]
async fn user_route_request_error_and_completed_response_are_never_replayed() {
    for (index, first) in [
        FirstGroupResult::BadRequest,
        FirstGroupResult::FullWithoutUsage,
    ]
    .into_iter()
    .enumerate()
    {
        let (database, runtime, principal) =
            routed_fixture(&format!("user-route-terminal-{index}")).await;
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let service = paid_service(
            &database,
            runtime,
            GroupPlanner {
                first,
                attempts: attempts.clone(),
                pricing_model_override: None,
            },
            metered_snapshot_source_with_target(GroupId::new(4).unwrap()).await,
            Arc::new(UsageStub::default()),
        );
        let result = service
            .chat_completions(
                &principal,
                None,
                request(),
                Protocol::OpenAiChat,
                "routed-terminal",
                diagnostic(),
            )
            .await;
        assert!(result.is_err());
        assert_eq!(
            attempts.lock().unwrap().as_slice(),
            &[GroupId::new(3).unwrap()]
        );
        database.close().await;
    }
}

#[tokio::test]
async fn user_route_preserves_request_abort_without_disabling_protocol_error_failover() {
    for (index, first, should_failover) in [
        (0, FirstGroupResult::ProtocolError, true),
        (1, FirstGroupResult::RequestAborted, false),
    ] {
        let label = format!("user-route-request-abort-{index}");
        let (database, runtime, principal) = routed_fixture(&label).await;
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let service = paid_service(
            &database,
            runtime,
            GroupPlanner {
                first,
                attempts: attempts.clone(),
                pricing_model_override: None,
            },
            metered_snapshot_source_with_target(GroupId::new(4).unwrap()).await,
            Arc::new(UsageStub::default()),
        );
        let result = service
            .chat_completions(
                &principal,
                None,
                request(),
                Protocol::OpenAiChat,
                &label,
                diagnostic(),
            )
            .await;
        if should_failover {
            assert!(matches!(result, Ok(ChatResponse::Full { .. })));
            assert_eq!(
                attempts.lock().unwrap().as_slice(),
                &[GroupId::new(3).unwrap(), GroupId::new(4).unwrap()]
            );
        } else {
            assert!(matches!(
                result,
                Err(AfError::Upstream(UpstreamError::ProtocolError))
            ));
            assert_eq!(
                attempts.lock().unwrap().as_slice(),
                &[GroupId::new(3).unwrap()]
            );
        }
        database.close().await;
    }
}

#[tokio::test]
async fn user_route_stops_if_failed_group_reservation_cannot_be_released() {
    let (database, runtime, principal) = routed_fixture("user-route-refund-failed").await;
    let attempts = Arc::new(Mutex::new(Vec::new()));
    // The default cancellation port fails closed; a queued refund does not authorize another charge.
    let precharge = Arc::new(PrechargeStub::default());
    let service = BillingChatService::new(
        Arc::new(GroupPlanner {
            first: FirstGroupResult::Retryable,
            attempts: attempts.clone(),
            pricing_model_override: None,
        }),
        Arc::new(metered_snapshot_source_with_target(GroupId::new(4).unwrap()).await),
        Some(precharge.clone()),
        Some(Arc::new(RefundStub)),
        Some(Arc::new(SettlementStub::default())),
        Arc::new(UsageStub::default()),
    )
    .with_user_route_runtime(runtime);
    assert!(matches!(
        service
            .chat_completions(
                &principal,
                None,
                request(),
                Protocol::OpenAiChat,
                "routed-refund-failed",
                diagnostic(),
            )
            .await,
        Err(AfError::Internal)
    ));
    assert_eq!(
        attempts.lock().unwrap().as_slice(),
        &[GroupId::new(3).unwrap()]
    );
    assert_eq!(precharge.calls.lock().unwrap().len(), 1);
    database.close().await;
}

#[tokio::test]
async fn user_route_feedback_uses_requested_model_after_pricing_mapping() {
    let (database, runtime, principal) = routed_fixture("user-route-feedback-model").await;
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let service = paid_service(
        &database,
        runtime,
        GroupPlanner {
            first: FirstGroupResult::Success,
            attempts,
            pricing_model_override: Some("gpt-routed".to_owned()),
        },
        metered_snapshot_source_with_target(GroupId::new(3).unwrap()).await,
        Arc::new(UsageStub::default()),
    );
    service
        .chat_completions(
            &principal,
            None,
            request(),
            Protocol::OpenAiChat,
            "routed-feedback-model",
            diagnostic(),
        )
        .await
        .unwrap();

    let repository =
        UserRouteRepository::new(database.pool().clone(), Duration::from_secs(5)).unwrap();
    let route_id = principal.user_route_id().unwrap();
    let health = repository
        .load_model_health(route_id, "gpt-test")
        .await
        .unwrap();
    assert_eq!(health.len(), 1);
    assert_eq!(health[0].group_id, GroupId::new(3).unwrap());
    assert_eq!(health[0].success_count, 1);
    assert_eq!(health[0].fail_count, 0);
    assert!(
        repository
            .load_model_health(route_id, "gpt-routed")
            .await
            .unwrap()
            .is_empty()
    );
    database.close().await;
}
