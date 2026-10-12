use super::*;
use af_admin::{ServiceAccountAuthentication, ServiceAccountModelAuthentication};
use af_billing::ServiceAccountBillingPort;
use af_domain::{
    GatewayIdentity, OrganizationId, OrganizationServiceAccountId, OrganizationServiceAccountKey,
    OrganizationServiceAccountLocator, OrganizationServiceAccountRuntimeIdentity,
    ServiceModelPolicy,
};
use std::num::NonZeroU32;

fn authentication() -> ServiceAccountModelAuthentication {
    ServiceAccountModelAuthentication::new(
        ServiceAccountAuthentication::new(
            OrganizationServiceAccountRuntimeIdentity::new(
                OrganizationServiceAccountLocator::new(
                    OrganizationId::new(11).unwrap(),
                    OrganizationServiceAccountId::new(12).unwrap(),
                ),
                OrganizationServiceAccountKey::new([13; 16]).unwrap(),
                OrganizationServiceAccountKey::new([14; 16]).unwrap(),
            ),
            None,
            None,
        ),
        GroupId::new(3).unwrap(),
        ServiceModelPolicy::new(vec!["gpt-test".into()]).unwrap(),
        ConcurrencyLimit::new(1).unwrap(),
        NonZeroU32::new(10).unwrap(),
    )
}

struct ServicePlanner {
    fail: bool,
}
impl ChatRoutePlanner for ServicePlanner {
    fn plan<'a>(
        &'a self,
        _: &'a GatewayPrincipal,
        _: Option<ConcurrencyLimit>,
        _: CanonicalRequestEnvelope,
        _: Protocol,
        _: &'a str,
        _: RelayDiagnosticInput,
    ) -> ChatRoutePlanFuture<'a> {
        panic!("service request must not use personal planner")
    }
    fn plan_service_account<'a>(
        &'a self,
        identity: OrganizationServiceAccountRuntimeIdentity,
        group: GroupId,
        limit: ConcurrencyLimit,
        request: CanonicalRequestEnvelope,
        _: Protocol,
        _: &'a str,
        _: RelayDiagnosticInput,
    ) -> ChatRoutePlanFuture<'a> {
        assert_eq!(identity, authentication().authentication().identity());
        assert_eq!(group.get(), 3);
        assert_eq!(limit.get(), 1);
        Box::pin(async move {
            if self.fail {
                return Ok(Box::new(FailedExecution(request)) as Box<dyn PlannedChatExecution>);
            }
            Ok(Box::new(FullExecution {
                target_group_id: group,
                pricing_model: request.canonical().model.clone(),
                request,
            }) as Box<dyn PlannedChatExecution>)
        })
    }
}

struct FailedExecution(CanonicalRequestEnvelope);
impl PlannedChatExecution for FailedExecution {
    fn target_group_id(&self) -> GroupId {
        GroupId::new(3).unwrap()
    }
    fn pricing_model(&self) -> &str {
        "gpt-test"
    }
    fn canonical_request(&self) -> &CanonicalRequestEnvelope {
        &self.0
    }
    fn execute(self: Box<Self>) -> PlannedChatExecutionFuture {
        Box::pin(async { Err(AfError::Internal) })
    }
}

type PrechargeFact = (
    BillingReservationId,
    OrganizationServiceAccountRuntimeIdentity,
    Quota,
    i64,
);
#[derive(Default)]
struct ServiceFunds {
    precharges: Mutex<Vec<PrechargeFact>>,
    settlements: Mutex<
        Vec<(
            OrganizationServiceAccountRuntimeIdentity,
            af_billing::SettlementRequest,
        )>,
    >,
    refunds: Mutex<
        Vec<(
            OrganizationServiceAccountRuntimeIdentity,
            BillingReservationId,
        )>,
    >,
}
impl ServiceAccountBillingPort for ServiceFunds {
    fn precharge<'a>(
        &'a self,
        reservation: BillingReservationId,
        identity: OrganizationServiceAccountRuntimeIdentity,
        amount: Quota,
        expires_at: i64,
    ) -> BillingPrechargeFuture<'a> {
        Box::pin(async move {
            let mut calls = self.precharges.lock().unwrap();
            calls.push((reservation, identity, amount, expires_at));
            if calls.len() == 1 {
                Err(BillingPrechargeError::OutcomeUnknown)
            } else {
                Ok(())
            }
        })
    }
    fn settle<'a>(
        &'a self,
        identity: OrganizationServiceAccountRuntimeIdentity,
        request: af_billing::SettlementRequest,
    ) -> BillingSettlementFuture<'a> {
        Box::pin(async move {
            self.settlements.lock().unwrap().push((identity, request));
            Ok(())
        })
    }
    fn try_signal_refund(
        &self,
        identity: OrganizationServiceAccountRuntimeIdentity,
        reservation: BillingReservationId,
    ) -> RefundSignalOutcome {
        self.refunds.lock().unwrap().push((identity, reservation));
        RefundSignalOutcome::Accepted
    }
}

#[tokio::test]
async fn service_call_uses_independent_funds_and_stable_precharge_facts() {
    let auth = authentication();
    let funds = Arc::new(ServiceFunds::default());
    let usage = Arc::new(UsageStub::default());
    let service = BillingChatService::new(
        Arc::new(ServicePlanner { fail: false }),
        Arc::new(metered_snapshot_source().await),
        None,
        None,
        None,
        usage.clone(),
    )
    .with_service_account_billing(Some(funds.clone()));
    let response = service
        .service_account_chat_completions(
            &auth,
            request(),
            Protocol::OpenAiChat,
            "service-request",
            diagnostic(),
        )
        .await
        .unwrap();
    assert!(matches!(response, ChatResponse::Full { .. }));
    let calls = funds.precharges.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], calls[1]);
    assert_eq!(
        funds.settlements.lock().unwrap()[0].0,
        auth.authentication().identity()
    );
    assert!(funds.refunds.lock().unwrap().is_empty());
    let records = usage.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].identity(),
        GatewayIdentity::ServiceAccount(auth.authentication().identity())
    );
    assert_eq!(records[0].principal(), None);
}

#[tokio::test]
async fn upstream_failure_refunds_only_the_confirmed_service_reservation() {
    let auth = authentication();
    let funds = Arc::new(ServiceFunds::default());
    let usage = Arc::new(UsageStub::default());
    let service = BillingChatService::new(
        Arc::new(ServicePlanner { fail: true }),
        Arc::new(metered_snapshot_source().await),
        None,
        None,
        None,
        usage.clone(),
    )
    .with_service_account_billing(Some(funds.clone()));
    assert!(
        service
            .service_account_chat_completions(
                &auth,
                request(),
                Protocol::OpenAiChat,
                "service-failure",
                diagnostic()
            )
            .await
            .is_err()
    );
    let calls = funds.precharges.lock().unwrap();
    assert_eq!(
        funds.refunds.lock().unwrap().as_slice(),
        &[(auth.authentication().identity(), calls[0].0)]
    );
    assert!(funds.settlements.lock().unwrap().is_empty());
    assert!(usage.records.lock().unwrap().is_empty());
}
