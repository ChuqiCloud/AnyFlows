//! Personal route policy selection stays outside the per-group relay and billing lifecycle.

use std::{
    cmp::Ordering,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, SystemTime},
};

use af_billing::{
    PricingContext, RequestPricingSnapshot, RequestPricingSnapshotError,
    RequestPricingSnapshotSource,
};
use af_cache::{RequestRateLimitOutcome, RequestRateLimitRule, RequestRateLimitSubject};
use af_db::{
    AdminGroupLookupOutcome, AdminGroupRepository, GroupModelPolicyRepository,
    GroupModelPolicyRepositoryError, UserRouteMemberRecord, UserRouteModelHealth, UserRouteRecord,
    UserRouteRepository,
};
use af_domain::{
    AfError, GatewayPrincipal, GroupId, NetworkFailureKind, Operation, Protocol, RateLimitScope,
    UpstreamError, UserRouteStrategy,
};
use af_protocol::Usage;
use af_scheduler::InMemoryChannelIndex;

#[derive(Clone)]
pub(crate) struct UserRouteRuntime {
    repository: UserRouteRepository,
    group_model_policy: Option<GroupModelPolicyRepository>,
    channel_index: Option<InMemoryChannelIndex>,
    group_admission: Option<GroupAdmission>,
}

#[derive(Clone)]
struct GroupAdmission {
    groups: AdminGroupRepository,
    store: Option<Arc<dyn crate::request_rate_limit::RequestRateLimitStore>>,
}

#[derive(Clone)]
pub(crate) struct UserRouteRequestPlan {
    pub(crate) route: UserRouteRecord,
    model: String,
    health: Vec<UserRouteModelHealth>,
}

pub(crate) struct UserRouteCandidate {
    pub(crate) group_id: GroupId,
    pub(crate) snapshot: RequestPricingSnapshot,
    member: UserRouteMemberRecord,
    quoted_quota: i64,
    health: Option<UserRouteModelHealth>,
}

struct QuotedGroup {
    group_id: GroupId,
    member: UserRouteMemberRecord,
    quoted_quota: i64,
    health: Option<UserRouteModelHealth>,
}

trait CandidateOrdering {
    fn group_id(&self) -> GroupId;
    fn member(&self) -> &UserRouteMemberRecord;
    fn quote(&self) -> i64;
    fn health(&self) -> Option<&UserRouteModelHealth>;
}

impl CandidateOrdering for UserRouteCandidate {
    fn group_id(&self) -> GroupId {
        self.group_id
    }
    fn member(&self) -> &UserRouteMemberRecord {
        &self.member
    }
    fn quote(&self) -> i64 {
        self.quoted_quota
    }
    fn health(&self) -> Option<&UserRouteModelHealth> {
        self.health.as_ref()
    }
}

impl CandidateOrdering for QuotedGroup {
    fn group_id(&self) -> GroupId {
        self.group_id
    }
    fn member(&self) -> &UserRouteMemberRecord {
        &self.member
    }
    fn quote(&self) -> i64 {
        self.quoted_quota
    }
    fn health(&self) -> Option<&UserRouteModelHealth> {
        self.health.as_ref()
    }
}

impl UserRouteRuntime {
    pub(crate) fn new(repository: UserRouteRepository) -> Self {
        Self {
            repository,
            group_model_policy: None,
            channel_index: None,
            group_admission: None,
        }
    }

    pub(crate) fn with_channel_index(mut self, index: InMemoryChannelIndex) -> Self {
        self.channel_index = Some(index);
        self
    }

    pub(crate) fn with_group_admission(
        mut self,
        groups: AdminGroupRepository,
        store: Option<Arc<dyn crate::request_rate_limit::RequestRateLimitStore>>,
    ) -> Self {
        self.group_admission = Some(GroupAdmission { groups, store });
        self
    }

    /// Personal route keys admit their user once at authentication, and each real group here.
    pub(crate) async fn admit_group(&self, group_id: GroupId) -> Result<(), AfError> {
        let Some(admission) = &self.group_admission else {
            return Ok(());
        };
        let group = match admission
            .groups
            .get(group_id)
            .await
            .map_err(|_| AfError::Internal)?
        {
            AdminGroupLookupOutcome::Found(group) => group,
            AdminGroupLookupOutcome::NotFound => return Err(AfError::ModelNotAllowed),
        };
        let limit = match group.rpm_limit() {
            None | Some(0) => return Ok(()),
            Some(value) => u32::try_from(value)
                .ok()
                .and_then(NonZeroU32::new)
                .ok_or(AfError::Internal)?,
        };
        let rule = RequestRateLimitRule::new(
            RequestRateLimitSubject::Group(group_id),
            limit,
            Duration::from_secs(60),
        )
        .map_err(|_| AfError::Internal)?;
        let store = admission.store.as_ref().ok_or(AfError::Internal)?;
        match store.admit(&[rule]).await.map_err(|_| AfError::Internal)? {
            RequestRateLimitOutcome::Admitted => Ok(()),
            RequestRateLimitOutcome::Limited(_) => {
                Err(UpstreamError::rate_limited(RateLimitScope::Window).into())
            }
        }
    }

    pub(crate) fn with_group_model_policy(
        mut self,
        repository: GroupModelPolicyRepository,
    ) -> Self {
        self.group_model_policy = Some(repository);
        self
    }

    pub(crate) async fn load(
        &self,
        principal: GatewayPrincipal,
        model: &str,
    ) -> Result<Option<UserRouteRequestPlan>, AfError> {
        if principal.user_route_id().is_none() {
            return Ok(None);
        }
        if principal.is_playground() || principal.organization_principal().is_some() {
            return Err(AfError::InvalidApiKey);
        }
        let route = self
            .repository
            .lookup_for_token(principal.user_id(), principal.token_id())
            .await
            .map_err(map_repository_error)?
            .ok_or(AfError::InvalidApiKey)?;
        if Some(route.id) != principal.user_route_id() {
            return Err(AfError::InvalidApiKey);
        }
        let health = self
            .repository
            .load_model_health(route.id, model)
            .await
            .map_err(map_repository_error)?;
        Ok(Some(UserRouteRequestPlan {
            route,
            model: model.to_owned(),
            health,
        }))
    }

    /// Authorization and model permission are rechecked immediately before each attempt.
    pub(crate) async fn check_access(
        &self,
        principal: GatewayPrincipal,
        group_id: GroupId,
        model: &str,
    ) -> Result<bool, AfError> {
        if !self
            .repository
            .check_route_member(
                principal.user_id(),
                principal.user_route_id().ok_or(AfError::InvalidApiKey)?,
                group_id,
            )
            .await
            .map_err(map_repository_error)?
        {
            return Ok(false);
        }
        let policy = self.group_model_policy.as_ref().ok_or(AfError::Internal)?;
        match policy.policy_for_group(group_id).await {
            Ok(policy) => Ok(policy.allows(model)),
            Err(GroupModelPolicyRepositoryError::NotFound) => Ok(false),
            Err(_) => Err(AfError::Internal),
        }
    }

    pub(crate) async fn selected(&self, route_id: i64, group_id: GroupId) -> Result<(), AfError> {
        self.repository
            .record_selection(route_id, group_id)
            .await
            .map_err(map_repository_error)
    }

    pub(crate) async fn feedback(
        &self,
        plan: &UserRouteRequestPlan,
        group_id: GroupId,
        success: bool,
        latency_ms: u64,
        retryable_failure: bool,
    ) {
        self.feedback_for(
            plan.route.id,
            &plan.model,
            group_id,
            success,
            latency_ms,
            retryable_failure,
        )
        .await;
    }

    pub(crate) async fn feedback_for(
        &self,
        route_id: i64,
        model: &str,
        group_id: GroupId,
        success: bool,
        latency_ms: u64,
        retryable_failure: bool,
    ) {
        if !success && !retryable_failure {
            return;
        }
        if self
            .repository
            .record_attempt(
                route_id,
                group_id,
                model,
                success,
                latency_ms,
                retryable_failure,
            )
            .await
            .is_err()
        {
            // Observation failure must never replay a successful upstream call.
            tracing::warn!(
                target: "af_server::user_route",
                error_kind = "user_route_feedback_unavailable",
                "用户路由健康反馈暂不可用"
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn candidates_for_operation(
        &self,
        plan: &UserRouteRequestPlan,
        principal: GatewayPrincipal,
        upper_bound: &Usage,
        pricing: &dyn RequestPricingSnapshotSource,
        protocol: Protocol,
        operation: Option<Operation>,
    ) -> Result<Vec<UserRouteCandidate>, AfError> {
        let time = crate::utc_time::current_pricing_time().ok_or(AfError::Internal)?;
        let now = now_millis()?;
        let mut candidates = Vec::with_capacity(plan.route.members.len());
        let mut permitted = false;
        for member in &plan.route.members {
            if !self
                .check_access(principal, member.group_id, &plan.model)
                .await?
            {
                continue;
            }
            permitted = true;
            if !self.supports_operation(member.group_id, &plan.model, protocol, operation)? {
                continue;
            }
            let health = plan
                .health
                .iter()
                .find(|health| health.group_id == member.group_id)
                .cloned();
            if health
                .as_ref()
                .and_then(|health| health.cooldown_until_millis)
                .is_some_and(|until| until > now)
            {
                continue;
            }
            let snapshot = match pricing
                .capture_for_request_at(
                    &plan.model,
                    principal.billing_source_group_id(),
                    member.group_id,
                    None,
                    protocol,
                    time,
                )
                .await
            {
                Ok(snapshot) => snapshot,
                Err(RequestPricingSnapshotError::TargetGroupNotFound) => continue,
                Err(_) => return Err(AfError::Internal),
            };
            if !price_ratio_allowed(&snapshot, plan.route.max_price_ratio_micros)? {
                continue;
            }
            let quoted_quota = snapshot
                .resolver()
                .resolve(&PricingContext::new(upper_bound))
                .map_err(|_| AfError::Internal)?
                .quota()
                .units();
            candidates.push(UserRouteCandidate {
                group_id: member.group_id,
                snapshot,
                member: member.clone(),
                quoted_quota,
                health,
            });
        }
        if candidates.is_empty() {
            return Err(if permitted {
                UpstreamError::overloaded().into()
            } else {
                AfError::ModelNotAllowed
            });
        }
        order_candidates(plan.route.strategy, &mut candidates)?;
        if plan.route.strategy == UserRouteStrategy::RoundRobin {
            let ids = candidates
                .iter()
                .map(|value| value.group_id)
                .collect::<Vec<_>>();
            let selected = self
                .repository
                .claim_round_robin(plan.route.id, &ids)
                .await
                .map_err(map_repository_error)?;
            if let Some(index) = candidates
                .iter()
                .position(|value| value.group_id == selected)
            {
                candidates.rotate_left(index);
            } else {
                return Err(AfError::Internal);
            }
        }
        Ok(candidates)
    }

    /// Fixed-price asynchronous tasks share strategy ordering without constructing token prices.
    pub(crate) async fn order_quoted_groups(
        &self,
        plan: &UserRouteRequestPlan,
        quoted: Vec<(GroupId, i64)>,
    ) -> Result<Vec<GroupId>, AfError> {
        let now = now_millis()?;
        let mut seen = std::collections::BTreeSet::new();
        let mut candidates = Vec::with_capacity(quoted.len());
        for (group_id, quoted_quota) in quoted {
            if quoted_quota < 0 || !seen.insert(group_id) {
                return Err(AfError::Internal);
            }
            let member = plan
                .route
                .members
                .iter()
                .find(|member| member.group_id == group_id)
                .ok_or(AfError::ModelNotAllowed)?
                .clone();
            let health = plan
                .health
                .iter()
                .find(|health| health.group_id == group_id)
                .cloned();
            if health
                .as_ref()
                .and_then(|health| health.cooldown_until_millis)
                .is_some_and(|until| until > now)
            {
                continue;
            }
            candidates.push(QuotedGroup {
                group_id,
                member,
                quoted_quota,
                health,
            });
        }
        if candidates.is_empty() {
            return Err(UpstreamError::overloaded().into());
        }
        order_candidates(plan.route.strategy, &mut candidates)?;
        let mut groups = candidates
            .into_iter()
            .map(|candidate| candidate.group_id)
            .collect::<Vec<_>>();
        if plan.route.strategy == UserRouteStrategy::RoundRobin {
            let selected = self
                .repository
                .claim_round_robin(plan.route.id, &groups)
                .await
                .map_err(map_repository_error)?;
            let index = groups
                .iter()
                .position(|group| *group == selected)
                .ok_or(AfError::Internal)?;
            groups.rotate_left(index);
        }
        Ok(groups)
    }

    pub(crate) fn supports_protocol(
        &self,
        group_id: GroupId,
        model: &str,
        protocol: Protocol,
    ) -> Result<bool, AfError> {
        self.supports_operation(group_id, model, protocol, None)
    }

    fn supports_operation(
        &self,
        group_id: GroupId,
        model: &str,
        protocol: Protocol,
        operation: Option<Operation>,
    ) -> Result<bool, AfError> {
        let Some(index) = &self.channel_index else {
            return Ok(true);
        };
        let snapshot = index.snapshot().map_err(|_| AfError::Internal)?;
        for candidate in snapshot
            .candidates(group_id, model)
            .map_err(|_| AfError::InvalidRequest)?
        {
            let target = snapshot
                .runtime_target(candidate.channel_id())
                .ok_or(AfError::Internal)?;
            if operation == Some(Operation::ResponsesCompact)
                && target.schedulable_responses_compact_model(model).is_none()
            {
                continue;
            }
            if compatible_protocol(protocol, target.protocol())
                && target.channel_type().supports_runtime(target.protocol())
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn compatible_protocol(requested: Protocol, upstream: Protocol) -> bool {
    let conversation = |protocol| {
        matches!(
            protocol,
            Protocol::OpenAiChat
                | Protocol::OpenAiResponses
                | Protocol::Anthropic
                | Protocol::Gemini
        )
    };
    requested == upstream
        || (conversation(requested) && conversation(upstream))
        || (requested == Protocol::OpenAiImages && upstream == Protocol::OpenAiChat)
        || (matches!(requested, Protocol::JinaRerank | Protocol::CohereRerank)
            && matches!(upstream, Protocol::JinaRerank | Protocol::CohereRerank))
}

pub(crate) fn price_ratio_allowed(
    snapshot: &RequestPricingSnapshot,
    maximum_micros: Option<i64>,
) -> Result<bool, AfError> {
    let Some(maximum) = maximum_micros else {
        return Ok(true);
    };
    if maximum < 0 {
        return Err(AfError::Internal);
    }
    let ratios = snapshot.ratios();
    ratio_product_allowed(
        [
            ratios.group().micros(),
            ratios.group_model().micros(),
            ratios.applied_peak().micros(),
        ],
        maximum,
    )
}

pub(crate) fn ratio_product_allowed(ratios: [i64; 3], maximum: i64) -> Result<bool, AfError> {
    if ratios.iter().any(|ratio| *ratio < 0) || maximum < 0 {
        return Err(AfError::Internal);
    }
    // A zero layer is free even when the remaining configured layers are very large.
    if ratios.contains(&0) {
        return Ok(true);
    }
    let maximum = i128::from(maximum) * 1_000_000_i128 * 1_000_000_i128;
    let mut product = 1_i128;
    for ratio in ratios {
        let Some(next) = product.checked_mul(i128::from(ratio)) else {
            return Ok(false);
        };
        product = next;
    }
    Ok(product <= maximum)
}

pub(crate) fn permits_group_failover(error: &AfError) -> bool {
    match error {
        AfError::Upstream(
            UpstreamError::RateLimited { .. }
            | UpstreamError::Overloaded { .. }
            | UpstreamError::AuthExpired
            | UpstreamError::AuthRevoked
            | UpstreamError::AccountDisabled
            | UpstreamError::QuotaExhausted
            | UpstreamError::ModelUnsupported
            | UpstreamError::ProtocolError
            | UpstreamError::ServerError { .. },
        ) => true,
        AfError::Upstream(UpstreamError::Network { kind }) => *kind != NetworkFailureKind::Policy,
        _ => false,
    }
}

fn order_candidates<T: CandidateOrdering>(
    strategy: UserRouteStrategy,
    candidates: &mut Vec<T>,
) -> Result<(), AfError> {
    match strategy {
        UserRouteStrategy::Ordered => {
            candidates.sort_by_key(|candidate| (candidate.member().priority, candidate.group_id()))
        }
        UserRouteStrategy::RoundRobin => candidates.sort_by_key(|candidate| {
            (
                candidate.member().last_selected_at_millis,
                candidate.member().priority,
                candidate.group_id(),
            )
        }),
        UserRouteStrategy::PriceFirst => candidates.sort_by_key(|candidate| {
            (
                candidate.quote(),
                candidate.member().priority,
                candidate.group_id(),
            )
        }),
        UserRouteStrategy::StableFirst => candidates.sort_by(|left, right| {
            compare_health(left.health(), right.health())
                .then_with(|| left.member().priority.cmp(&right.member().priority))
                .then_with(|| left.group_id().cmp(&right.group_id()))
        }),
        UserRouteStrategy::Weighted => {
            let mut remaining = std::mem::take(candidates);
            while !remaining.is_empty() {
                let total = remaining.iter().try_fold(0_u64, |sum, candidate| {
                    sum.checked_add(u64::from(candidate.member().weight))
                        .ok_or(AfError::Internal)
                })?;
                if total == 0 {
                    return Err(AfError::Internal);
                }
                let mut ticket = random_below(total)?;
                let index = remaining
                    .iter()
                    .position(|candidate| {
                        let weight = u64::from(candidate.member().weight);
                        if ticket < weight {
                            true
                        } else {
                            ticket -= weight;
                            false
                        }
                    })
                    .ok_or(AfError::Internal)?;
                candidates.push(remaining.remove(index));
            }
        }
    }
    Ok(())
}

fn compare_health(
    left: Option<&UserRouteModelHealth>,
    right: Option<&UserRouteModelHealth>,
) -> Ordering {
    let facts = |health: Option<&UserRouteModelHealth>| {
        health.map_or((1_u128, 2_u128, 0_u128), |health| {
            let successes = u128::from(health.success_count);
            let failures = u128::from(health.fail_count) + 1;
            let count = successes + failures + 1;
            let latency = u128::from(health.total_latency_ms) / successes.max(1);
            (failures, count, latency)
        })
    };
    let (left_failures, left_count, left_latency) = facts(left);
    let (right_failures, right_count, right_latency) = facts(right);
    (left_failures * right_count)
        .cmp(&(right_failures * left_count))
        .then_with(|| left_latency.cmp(&right_latency))
}

fn random_below(bound: u64) -> Result<u64, AfError> {
    let ceiling = u64::MAX - u64::MAX % bound;
    loop {
        let mut bytes = [0_u8; 8];
        getrandom::fill(&mut bytes).map_err(|_| AfError::Internal)?;
        let value = u64::from_ne_bytes(bytes);
        if value < ceiling {
            return Ok(value % bound);
        }
    }
}

fn now_millis() -> Result<i64, AfError> {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .ok_or(AfError::Internal)
}

pub(crate) fn map_repository_error(error: af_db::UserRouteError) -> AfError {
    use af_db::UserRouteError;
    match error {
        UserRouteError::Unavailable | UserRouteError::NotFound => AfError::InvalidApiKey,
        _ => AfError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_requests_accept_chat_upstreams_without_broadening_conversation_compatibility() {
        for upstream in [Protocol::OpenAiImages, Protocol::OpenAiChat] {
            assert!(compatible_protocol(Protocol::OpenAiImages, upstream));
        }
        for upstream in [
            Protocol::OpenAiResponses,
            Protocol::Anthropic,
            Protocol::Gemini,
            Protocol::JinaRerank,
            Protocol::CohereRerank,
        ] {
            assert!(!compatible_protocol(Protocol::OpenAiImages, upstream));
        }
        for requested in [
            Protocol::OpenAiChat,
            Protocol::OpenAiResponses,
            Protocol::Anthropic,
            Protocol::Gemini,
        ] {
            assert!(!compatible_protocol(requested, Protocol::OpenAiImages));
        }
    }

    #[test]
    fn failover_rejects_request_and_policy_errors() {
        for error in [
            AfError::InvalidRequest,
            AfError::InvalidApiKey,
            AfError::ModelNotAllowed,
            AfError::InsufficientQuota,
            AfError::Internal,
            UpstreamError::BadRequest.into(),
            UpstreamError::network(NetworkFailureKind::Policy).into(),
        ] {
            assert!(!permits_group_failover(&error));
        }
        for error in [
            UpstreamError::overloaded().into(),
            UpstreamError::network(NetworkFailureKind::ConnectTimeout).into(),
            UpstreamError::AuthExpired.into(),
        ] {
            assert!(permits_group_failover(&error));
        }
    }

    #[test]
    fn price_ceiling_multiplies_all_layers_without_rounding_down() {
        assert!(ratio_product_allowed([500_000, 1_000_000, 2_000_000], 1_000_000).unwrap());
        assert!(!ratio_product_allowed([500_000, 1_000_000, 2_000_001], 1_000_000).unwrap());
        assert!(!ratio_product_allowed([i64::MAX; 3], i64::MAX).unwrap());
        assert!(ratio_product_allowed([i64::MAX, i64::MAX, 0], 0).unwrap());
    }

    #[test]
    fn stable_order_compares_failure_rate_with_smoothed_cold_start() {
        let healthy = health(19, 0, 1_900);
        let unhealthy = health(10, 8, 100);
        assert_eq!(
            compare_health(Some(&healthy), Some(&unhealthy)),
            Ordering::Less
        );
        assert_eq!(compare_health(Some(&healthy), None), Ordering::Less);
        assert_eq!(compare_health(None, Some(&unhealthy)), Ordering::Greater);
    }

    #[test]
    fn order_and_price_preferences_use_configured_facts() {
        let mut groups = vec![quoted(1, 2, 20), quoted(2, 0, 30), quoted(3, 1, 10)];
        order_candidates(UserRouteStrategy::Ordered, &mut groups).unwrap();
        assert_eq!(ids(&groups), vec![2, 3, 1]);
        order_candidates(UserRouteStrategy::PriceFirst, &mut groups).unwrap();
        assert_eq!(ids(&groups), vec![3, 1, 2]);
        groups[1].member.last_selected_at_millis = Some(10);
        groups[0].member.last_selected_at_millis = Some(20);
        order_candidates(UserRouteStrategy::RoundRobin, &mut groups).unwrap();
        assert_eq!(ids(&groups), vec![2, 1, 3]);
    }

    #[test]
    fn weighted_order_never_repeats_or_drops_a_member() {
        let mut groups = vec![quoted(1, 0, 10), quoted(2, 1, 10), quoted(3, 2, 10)];
        groups[0].member.weight = 10_000;
        order_candidates(UserRouteStrategy::Weighted, &mut groups).unwrap();
        let mut groups = ids(&groups);
        groups.sort_unstable();
        assert_eq!(groups, vec![1, 2, 3]);
    }

    fn quoted(group: i64, priority: i32, quoted_quota: i64) -> QuotedGroup {
        let group_id = GroupId::new(group).unwrap();
        QuotedGroup {
            group_id,
            member: UserRouteMemberRecord {
                group_id,
                priority,
                weight: 1,
                last_selected_at_millis: None,
            },
            quoted_quota,
            health: None,
        }
    }

    fn ids(groups: &[QuotedGroup]) -> Vec<i64> {
        groups.iter().map(|group| group.group_id.get()).collect()
    }

    fn health(success_count: u64, fail_count: u64, total_latency_ms: u64) -> UserRouteModelHealth {
        UserRouteModelHealth {
            group_id: GroupId::new(1).unwrap(),
            success_count,
            fail_count,
            total_latency_ms,
            cooldown_until_millis: None,
            updated_at_millis: 0,
        }
    }
}
