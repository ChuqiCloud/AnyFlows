//! Cross-group retries wrap, rather than flatten, the existing per-group billing lifecycle.

use std::{future::Future, time::Instant};

use af_billing::{RequestPricingSnapshot, RequestPricingSnapshotSource};
use af_domain::{AfError, GatewayPrincipal, Operation, Protocol};
use af_protocol::Usage;

use crate::{
    RoutedExecution,
    user_route_runtime::{UserRouteRuntime, permits_group_failover},
};

/// Errors are terminal unless the single-group execution explicitly confirms safe cancellation.
pub(crate) struct UserRouteAttemptError {
    pub(crate) error: AfError,
    retryable: bool,
    counts_attempt: bool,
}

impl UserRouteAttemptError {
    pub(crate) fn before_delivery(error: AfError) -> Self {
        // This constructor is only used after a group's candidate plan has begun execution;
        // ConcurrencyLimited here therefore means that group's account slots were full.
        let retryable = error == AfError::ConcurrencyLimited || permits_group_failover(&error);
        Self {
            error,
            retryable,
            counts_attempt: true,
        }
    }

    pub(crate) fn candidate_plan(error: AfError) -> Self {
        let retryable = matches!(error, AfError::ModelNotAllowed | AfError::ModelUnavailable)
            || permits_group_failover(&error);
        Self {
            error,
            retryable,
            counts_attempt: false,
        }
    }
}

impl From<AfError> for UserRouteAttemptError {
    fn from(error: AfError) -> Self {
        Self {
            error,
            retryable: false,
            counts_attempt: false,
        }
    }
}

pub(crate) async fn cancel_before_failover(
    lifecycle: &mut af_billing::BillingRequestLifecycle,
    precharge: Option<&dyn af_billing::BillingPrechargePort>,
) -> Result<(), AfError> {
    if lifecycle.state() == af_billing::BillingLifecycleState::Free {
        return Ok(());
    }
    let precharge = precharge.ok_or(AfError::Internal)?;
    lifecycle
        .cancel_before_delivery(precharge)
        .await
        .map_err(|_| AfError::Internal)
}

pub(crate) struct UserRouteExecution<'a> {
    pub(crate) principal: GatewayPrincipal,
    pub(crate) model: &'a str,
    pub(crate) protocol: Protocol,
    pub(crate) operation: Operation,
    pub(crate) upper_bound: &'a Usage,
    pub(crate) pricing: &'a dyn RequestPricingSnapshotSource,
    pub(crate) defer_success_feedback: bool,
}

impl UserRouteExecution<'_> {
    pub(crate) async fn run<R, T, F, Fut>(
        &self,
        runtime: Option<&UserRouteRuntime>,
        request: R,
        mut execute: F,
    ) -> Result<RoutedExecution<T>, AfError>
    where
        R: Clone,
        F: FnMut(GatewayPrincipal, R, Option<RequestPricingSnapshot>, Option<i64>) -> Fut,
        Fut: Future<Output = Result<RoutedExecution<T>, UserRouteAttemptError>>,
    {
        if self.principal.user_route_id().is_none() {
            return execute(self.principal, request, None, None)
                .await
                .map_err(|error| error.error);
        }
        let runtime = runtime.ok_or(AfError::Internal)?;
        let plan = runtime
            .load(self.principal, self.model)
            .await?
            .ok_or(AfError::InvalidApiKey)?;
        let candidates = runtime
            .candidates_for_operation(
                &plan,
                self.principal,
                self.upper_bound,
                self.pricing,
                self.protocol,
                Some(self.operation),
            )
            .await?;
        let mut last_error = AfError::ModelNotAllowed;
        let mut attempts = 0;
        for candidate in candidates {
            if attempts >= plan.route.max_group_attempts {
                break;
            }
            if !runtime
                .check_access(self.principal, candidate.group_id, self.model)
                .await?
            {
                continue;
            }
            if let Err(error) = runtime.admit_group(candidate.group_id).await {
                if permits_group_failover(&error) {
                    last_error = error;
                    continue;
                }
                return Err(error);
            }
            runtime.selected(plan.route.id, candidate.group_id).await?;
            let principal = self.principal.with_routing_group_id(candidate.group_id);
            let started = Instant::now();
            let result = execute(
                principal,
                request.clone(),
                Some(candidate.snapshot),
                plan.route.max_price_ratio_micros,
            )
            .await;
            let latency = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            match result {
                Ok(result) => {
                    if !self.defer_success_feedback {
                        runtime
                            .feedback(&plan, candidate.group_id, true, latency, false)
                            .await;
                    }
                    return Ok(result.with_target_group_id(candidate.group_id));
                }
                Err(failure) => {
                    if failure.counts_attempt {
                        attempts += 1;
                        runtime
                            .feedback(&plan, candidate.group_id, false, latency, failure.retryable)
                            .await;
                    }
                    if !failure.retryable {
                        return Err(failure.error);
                    }
                    last_error = failure.error;
                }
            }
        }
        Err(last_error)
    }
}

#[cfg(test)]
mod tests {
    use super::UserRouteAttemptError;
    use af_domain::{AfError, UpstreamError};

    #[test]
    fn execution_concurrency_limit_fails_over_but_planning_concurrency_limit_does_not() {
        let account_limit = UserRouteAttemptError::before_delivery(AfError::ConcurrencyLimited);
        assert!(account_limit.retryable);

        let user_limit = UserRouteAttemptError::candidate_plan(AfError::ConcurrencyLimited);
        assert!(!user_limit.retryable);
        assert!(
            !UserRouteAttemptError::before_delivery(UpstreamError::BadRequest.into()).retryable
        );
    }
}
