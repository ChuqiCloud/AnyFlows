//! Model admission is shared by every request planner before credential ordering.

use af_db::{SchedulerRuntimeCredentialRecord, SchedulerRuntimeTargetRecord};
use af_domain::CredentialQuotaDimension;
use std::collections::{BTreeMap, BTreeSet};

use af_domain::{AfError, CredentialId, Protocol};
use af_scheduler::IndexedRoutePlan;

use crate::{credential_order::CredentialLoad, health_runtime::RuntimeHealthState};

use super::ScheduledChatService;

pub(super) fn eligible_credentials(
    target: &SchedulerRuntimeTargetRecord,
    canonical_model: &str,
    upstream_model: &str,
) -> Vec<SchedulerRuntimeCredentialRecord> {
    let quota_dimension = CredentialQuotaDimension::for_canonical_model(canonical_model);
    target
        .credentials()
        .iter()
        .filter(|credential| {
            credential.quota_dimension() == quota_dimension
                && target.allows_credential_model(credential, upstream_model)
        })
        .cloned()
        .collect()
}

pub(super) fn has_eligible_credentials(
    target: &SchedulerRuntimeTargetRecord,
    canonical_model: &str,
    upstream_model: &str,
) -> bool {
    let quota_dimension = CredentialQuotaDimension::for_canonical_model(canonical_model);
    target.credentials().iter().any(|credential| {
        credential.quota_dimension() == quota_dimension
            && target.allows_credential_model(credential, upstream_model)
    })
}

fn compact_credentials<'a>(
    route: &'a IndexedRoutePlan,
    model: &str,
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    let dimension = CredentialQuotaDimension::for_canonical_model(model);
    let mut credentials = Vec::new();
    for candidate in route.candidates() {
        let target = candidate.runtime_target();
        if !super::target_matches_protocol(target, Protocol::OpenAiResponses) {
            continue;
        }
        let Some(upstream) = target.schedulable_responses_compact_model(model) else {
            continue;
        };
        credentials.extend(target.credentials().iter().filter(|credential| {
            credential.quota_dimension() == dimension
                && target.allows_credential_model(credential, upstream)
        }));
    }
    credentials
}

impl ScheduledChatService {
    pub(super) async fn load_compact_account_concurrency(
        &self,
        route: &IndexedRoutePlan,
        model: &str,
    ) -> Result<Option<BTreeMap<i64, CredentialLoad>>, AfError> {
        let Some(concurrency) = &self.concurrency else {
            return Ok(None);
        };
        let mut seen = BTreeSet::new();
        let accounts = compact_credentials(route, model)
            .into_iter()
            .filter(|credential| seen.insert(credential.concurrency_owner_id()))
            .take(af_relay::RelayStateMachine::MAX_CANDIDATES)
            .map(|credential| (credential.concurrency_owner_id(), credential.concurrency()))
            .collect::<Vec<_>>();
        concurrency
            .account_loads(&accounts)
            .await
            .map(Some)
            .map_err(|_| AfError::Internal)
    }

    pub(super) async fn load_compact_credential_health(
        &self,
        route: &IndexedRoutePlan,
        model: &str,
    ) -> Result<Option<BTreeMap<i64, RuntimeHealthState>>, AfError> {
        let Some(store) = &self.health else {
            return Ok(None);
        };
        let mut seen = BTreeSet::new();
        let mut identities = Vec::new();
        for credential in compact_credentials(route, model) {
            let id =
                CredentialId::new(credential.credential_id()).map_err(|_| AfError::Internal)?;
            if !seen.insert(id) {
                return Err(AfError::Internal);
            }
            identities.push((id, credential.shared_health_id()));
            if identities.len() == af_relay::RelayStateMachine::MAX_CANDIDATES {
                break;
            }
        }
        crate::health_runtime::load_credential_health(store, &identities)
            .await
            .map(Some)
            .map_err(|_| AfError::Internal)
    }
}
