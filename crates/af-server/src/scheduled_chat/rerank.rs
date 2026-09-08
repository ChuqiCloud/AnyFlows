use std::{collections::BTreeMap, sync::Arc};

use af_adapter::{RelayContext, get_adaptor};
use af_domain::{AfError, ConcurrencyLimit, CredentialId, GatewayPrincipal, GroupId, Protocol};
use af_protocol::CanonicalRerankRequest;
use af_relay::{
    RelayCandidate, RelayCandidateRequest, RelayStateMachine,
    build_cohere_rerank_candidate_request, build_jina_rerank_candidate_request,
    relay_rerank_with_report,
};
use af_scheduler::{IndexedRoutePlan, RouteWaitKind};

use crate::{
    PlannedRerankExecution, PlannedRerankExecutionFuture, RerankRoutePlanFuture,
    RerankRoutePlanner,
    adaptor_credential::build_adaptor_credential,
    auto_ban_feedback::{ChannelAutoBanAttemptTarget, persist_channel_auto_ban_feedback},
    concurrency_runtime::RuntimeConcurrencyPermit,
    credential_feedback::{CredentialAttemptTarget, persist_credential_feedback},
    credential_order::{
        CredentialLoad, order_credentials, order_credentials_by_load,
        order_credentials_by_load_and_health, order_credentials_with_health,
    },
    health_runtime::{RuntimeHealthState, persist_scheduler_health_feedback},
};

use super::{
    PreparedConcurrencyPlan, ScheduledChatService, adaptor_settings, header_overrides,
    map_scheduler_error, target_matches_protocol,
};

impl RerankRoutePlanner for ScheduledChatService {
    fn plan<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRerankRequest,
        request_id: &'a str,
    ) -> RerankRoutePlanFuture<'a> {
        Box::pin(async move {
            let route = if self.health.is_some() {
                let (channel_ids, channel_health) = self
                    .load_channel_health(principal.group_id(), request.model())
                    .await?;
                self.scheduler
                    .route_plan_with_health(principal.group_id(), request.model(), &channel_health)
                    .map_err(map_scheduler_error)?
                    .ok_or_else(|| {
                        AfError::from(if channel_ids.is_empty() {
                            af_domain::UpstreamError::ModelUnsupported
                        } else {
                            af_domain::UpstreamError::overloaded()
                        })
                    })?
            } else {
                self.scheduler
                    .route_plan(principal.group_id(), request.model())
                    .map_err(map_scheduler_error)?
                    .ok_or_else(|| AfError::from(af_domain::UpstreamError::ModelUnsupported))?
            };
            let (jina_health, cohere_health) = tokio::join!(
                self.load_credential_health(&route, Protocol::JinaRerank, request.model()),
                self.load_credential_health(&route, Protocol::CohereRerank, request.model()),
            );
            let credential_health = merge_optional_maps(jina_health?, cohere_health?);
            let user_permit = self
                .acquire_user_permit(*principal, user_concurrency)
                .await?;
            let (jina_loads, cohere_loads) = tokio::join!(
                self.load_account_concurrency(&route, Protocol::JinaRerank, request.model()),
                self.load_account_concurrency(&route, Protocol::CohereRerank, request.model()),
            );
            let account_loads = merge_optional_maps(jina_loads?, cohere_loads?);
            let plan = self.build_rerank_plan(
                route,
                request,
                request_id,
                PreparedConcurrencyPlan {
                    account_loads: account_loads.as_ref(),
                    credential_health: credential_health.as_ref(),
                    user_permit,
                },
            )?;
            Ok(Box::new(plan) as Box<dyn PlannedRerankExecution>)
        })
    }
}

impl ScheduledChatService {
    fn build_rerank_plan(
        &self,
        route: IndexedRoutePlan,
        request: CanonicalRerankRequest,
        request_id: &str,
        concurrency: PreparedConcurrencyPlan<'_>,
    ) -> Result<ScheduledRerankExecution, AfError> {
        let PreparedConcurrencyPlan {
            account_loads,
            credential_health,
            user_permit,
        } = concurrency;
        let model = request.model().to_owned();
        let target_group_id = route.target_group_id();
        let mut relay_candidates = Vec::with_capacity(
            route
                .candidates()
                .len()
                .min(RelayStateMachine::MAX_CANDIDATES),
        );
        let mut attempt_targets = Vec::with_capacity(relay_candidates.capacity());
        let mut auto_ban_targets = Vec::with_capacity(relay_candidates.capacity());
        let mut candidate_protocols = Vec::with_capacity(relay_candidates.capacity());
        let mut health_filtered = false;
        'channels: for candidate in route.candidates() {
            let target = candidate.runtime_target();
            let wait_plan = candidate.wait_plan();
            let protocol = target.protocol();
            if !is_rerank_protocol(protocol) || !target_matches_protocol(target, protocol) {
                continue;
            }
            let upstream_model = target.mapped_model(&model).unwrap_or(&model).to_owned();
            let candidate_request =
                rerank_candidate_request(target, &request, &model, &upstream_model)?;
            let adaptor = get_adaptor(
                target.channel_type(),
                adaptor_settings(target, upstream_model)?,
            )
            .map_err(|_| AfError::Internal)?;
            if adaptor.default_protocol() != protocol {
                return Err(AfError::Internal);
            }
            let headers = header_overrides(target)?;
            let ordered_credentials = ordered_credentials(
                request_id,
                target,
                wait_plan.kind(),
                account_loads,
                credential_health,
            );
            health_filtered |= !target.credentials().is_empty() && ordered_credentials.is_empty();
            for runtime_credential in ordered_credentials {
                if relay_candidates.len() == RelayStateMachine::MAX_CANDIDATES {
                    break 'channels;
                }
                let credential_id = CredentialId::new(runtime_credential.credential_id())
                    .map_err(|_| AfError::Internal)?;
                let secret_owner_id = runtime_credential.secret_owner_id();
                let client = self.client_for_credential(target, runtime_credential)?;
                let mut context = RelayContext::new(client);
                if let Some(base_url) = target.base_url() {
                    context = context
                        .with_base_url(base_url)
                        .map_err(|_| AfError::Internal)?;
                }
                context = context
                    .with_request_id(request_id.to_owned())
                    .map_err(|_| AfError::Internal)?;
                let decrypted = self
                    .decryptor
                    .decrypt_envelope(
                        target.channel_id(),
                        secret_owner_id.get(),
                        runtime_credential.credential_kind(),
                        runtime_credential.envelope(),
                    )
                    .map_err(|_| AfError::Internal)?;
                let oauth_has_refresh_token = decrypted.oauth_has_refresh_token();
                let credential =
                    build_adaptor_credential(&decrypted).map_err(|_| AfError::Internal)?;
                let mut relay_candidate = RelayCandidate::new(adaptor.clone(), context, credential)
                    .with_header_overrides(headers.clone())
                    .with_channel_group(target.channel_id());
                if let Some(concurrency) = &self.concurrency {
                    relay_candidate = relay_candidate.with_attempt_gate(concurrency.account_gate(
                        runtime_credential.concurrency_owner_id(),
                        runtime_credential.concurrency(),
                        wait_plan.timeout(),
                    ));
                }
                if let Some(candidate_request) = &candidate_request {
                    relay_candidate = relay_candidate.with_request(candidate_request.clone());
                }
                relay_candidates.push(relay_candidate);
                candidate_protocols.push(protocol);
                attempt_targets.push(CredentialAttemptTarget::with_runtime_identity(
                    target.channel_id(),
                    credential_id,
                    secret_owner_id,
                    runtime_credential.shared_health_id(),
                    runtime_credential.credential_kind(),
                    oauth_has_refresh_token,
                    target.pool_mode(),
                )?);
                auto_ban_targets.push(ChannelAutoBanAttemptTarget::new(
                    target.channel_id(),
                    target.auto_ban_rules(),
                    target.pool_mode(),
                )?);
            }
        }
        if relay_candidates.is_empty() {
            return Err(if health_filtered {
                af_domain::UpstreamError::overloaded().into()
            } else {
                af_domain::UpstreamError::ModelUnsupported.into()
            });
        }
        let machine = RelayStateMachine::new(relay_candidates).map_err(|_| AfError::Internal)?;
        Ok(ScheduledRerankExecution {
            target_group_id,
            machine,
            candidate_protocols,
            attempt_targets,
            auto_ban_targets,
            credential_states: self.credential_states.clone(),
            scheduler: self.scheduler.clone(),
            request,
            user_permit,
            health: self.health.clone(),
            channel_states: self.channel_states.clone(),
        })
    }
}

struct ScheduledRerankExecution {
    target_group_id: GroupId,
    machine: RelayStateMachine,
    candidate_protocols: Vec<Protocol>,
    attempt_targets: Vec<CredentialAttemptTarget>,
    auto_ban_targets: Vec<ChannelAutoBanAttemptTarget>,
    credential_states: af_db::CredentialStateRepository,
    scheduler: af_scheduler::IndexedWeightedScheduler,
    request: CanonicalRerankRequest,
    user_permit: Option<RuntimeConcurrencyPermit>,
    health: Option<Arc<dyn crate::health_runtime::SchedulerHealthStore>>,
    channel_states: Option<af_db::ChannelStateRepository>,
}

impl PlannedRerankExecution for ScheduledRerankExecution {
    fn target_group_id(&self) -> GroupId {
        self.target_group_id
    }

    fn execute(self: Box<Self>) -> PlannedRerankExecutionFuture {
        let Self {
            target_group_id: _,
            machine,
            candidate_protocols,
            attempt_targets,
            auto_ban_targets,
            credential_states,
            scheduler,
            request,
            user_permit,
            health,
            channel_states,
        } = *self;
        Box::pin(async move {
            let (result, report) =
                relay_rerank_with_report(&machine, request, &candidate_protocols)
                    .await
                    .into_parts();
            let successful_channel_id = report
                .successful_candidate_index()
                .and_then(|index| attempt_targets.get(index))
                .map(CredentialAttemptTarget::channel_id);
            let ((), (), ()) = tokio::join!(
                persist_credential_feedback(
                    &credential_states,
                    &scheduler,
                    &attempt_targets,
                    &report,
                ),
                persist_scheduler_health_feedback(health.as_ref(), &attempt_targets, &report),
                persist_channel_auto_ban_feedback(
                    channel_states.as_ref(),
                    &scheduler,
                    &auto_ban_targets,
                    &report,
                ),
            );
            if let Some(permit) = user_permit {
                permit.release().await;
            }
            result.and_then(|response| {
                successful_channel_id
                    .map(|channel_id| crate::RoutedExecution::new(response, channel_id))
                    .ok_or(AfError::Internal)
            })
        })
    }
}

fn rerank_candidate_request(
    target: &af_db::SchedulerRuntimeTargetRecord,
    request: &CanonicalRerankRequest,
    requested_model: &str,
    upstream_model: &str,
) -> Result<Option<RelayCandidateRequest>, AfError> {
    if !target.parameter_overrides().is_empty() {
        return Err(AfError::Internal);
    }
    match target.protocol() {
        Protocol::JinaRerank if upstream_model == requested_model => Ok(None),
        Protocol::JinaRerank => {
            build_jina_rerank_candidate_request(request, upstream_model.to_owned()).map(Some)
        }
        Protocol::CohereRerank => {
            build_cohere_rerank_candidate_request(request, upstream_model.to_owned()).map(Some)
        }
        _ => Err(AfError::Internal),
    }
}

fn is_rerank_protocol(protocol: Protocol) -> bool {
    matches!(protocol, Protocol::JinaRerank | Protocol::CohereRerank)
}

fn merge_optional_maps<T>(
    first: Option<BTreeMap<i64, T>>,
    second: Option<BTreeMap<i64, T>>,
) -> Option<BTreeMap<i64, T>> {
    match (first, second) {
        (None, None) => None,
        (Some(values), None) | (None, Some(values)) => Some(values),
        (Some(mut first), Some(second)) => {
            first.extend(second);
            Some(first)
        }
    }
}

fn ordered_credentials<'a>(
    request_id: &str,
    target: &'a af_db::SchedulerRuntimeTargetRecord,
    wait_kind: RouteWaitKind,
    account_loads: Option<&BTreeMap<i64, CredentialLoad>>,
    credential_health: Option<&BTreeMap<i64, RuntimeHealthState>>,
) -> Vec<&'a af_db::SchedulerRuntimeCredentialRecord> {
    match (wait_kind, account_loads, credential_health) {
        (RouteWaitKind::Fallback, Some(loads), Some(health)) => {
            order_credentials_by_load_and_health(
                request_id,
                target.channel_id(),
                target.credentials(),
                loads,
                health,
            )
        }
        (RouteWaitKind::Fallback, Some(loads), None) => {
            order_credentials_by_load(request_id, target.channel_id(), target.credentials(), loads)
        }
        (_, _, Some(health)) => order_credentials_with_health(
            request_id,
            target.channel_id(),
            target.credentials(),
            health,
        ),
        _ => order_credentials(request_id, target.channel_id(), target.credentials()),
    }
}
