use super::*;
use af_db::{MerchantAttemptWrite, MerchantRouteMember};
use af_domain::{ChannelId, MerchantBillingSnapshot, MerchantRuntimeCandidate};

impl UserRouteRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn merchant_candidates(
        &self,
        plan: &UserRouteRequestPlan,
        principal: GatewayPrincipal,
        upper_bound: &Usage,
        pricing: &dyn RequestPricingSnapshotSource,
        protocol: Protocol,
        operation: Option<Operation>,
        features: Option<crate::user_route_execution::MerchantRequestFeatures>,
    ) -> Result<Vec<UserRouteCandidate>, AfError> {
        if !matches!(operation, Some(Operation::Chat | Operation::Responses)) {
            return Err(AfError::ModelNotAllowed);
        }
        let scope = plan.merchant_scope.as_ref().ok_or(AfError::Internal)?;
        let pools = self.merchant_pools.as_ref().ok_or(AfError::Internal)?;
        if !pools
            .check_pool_access(principal.user_id(), scope.pool_id)
            .await
            .map_err(|_| AfError::Internal)?
        {
            return Err(AfError::ModelNotAllowed);
        }
        let time = crate::utc_time::current_pricing_time().ok_or(AfError::Internal)?;
        let offers = pools
            .published_candidates(scope.pool_id, None, &plan.model)
            .await
            .map_err(|_| AfError::Internal)?;
        let features = features.ok_or(AfError::ModelNotAllowed)?;
        if features.vision {
            return Err(AfError::ModelNotAllowed);
        }
        let statistics = self
            .repository
            .merchant_statistics_filtered(
                scope.pool_id,
                Some(&plan.model),
                900,
                Some(protocol.as_str()),
                Some(features.streaming),
            )
            .await
            .map_err(map_repository_error)?;
        let mut candidates: Vec<UserRouteCandidate> = Vec::new();
        let mut fallback: Vec<UserRouteCandidate> = Vec::new();
        for offer in offers {
            if scope.excluded_merchant_ids.contains(&offer.merchant_id) {
                continue;
            }
            if features.unsupported_media
                || (features.streaming && !offer.model.supports_streaming)
                || (features.tools && !offer.model.supports_tools)
                || offer.model.supports_vision
                || offer
                    .model
                    .max_output_tokens
                    .is_some_and(|n| upper_bound.output_tokens().get() > i64::from(n))
                || offer.model.context_window.is_some_and(|n| {
                    upper_bound.checked_input_tokens().is_ok_and(|input| {
                        input
                            .get()
                            .saturating_add(upper_bound.output_tokens().get())
                            > i64::from(n)
                    })
                })
            {
                continue;
            }
            let member = scope
                .members
                .iter()
                .find(|m| m.merchant_id == offer.merchant_id);
            let is_fallback = member.is_none();
            if is_fallback
                && (!scope.allow_pool_fallback
                    || (!scope.accept_new_merchants
                        && !scope.enrolled_merchants.contains(&offer.merchant_id)))
            {
                continue;
            }
            let member = member.cloned().unwrap_or(MerchantRouteMember {
                merchant_id: offer.merchant_id,
                priority: i32::MAX,
                weight: 1,
                last_selected_at_millis: None,
            });
            let group_id = GroupId::new(offer.group_id).map_err(|_| AfError::Internal)?;
            if !self.check_access(principal, group_id, &plan.model).await? {
                continue;
            }
            let upstream = offer
                .protocol
                .parse::<Protocol>()
                .map_err(|_| AfError::Internal)?;
            if !compatible_protocol(protocol, upstream) {
                continue;
            }
            let snapshot = pricing
                .capture_for_request_at(&plan.model, group_id, group_id, None, protocol, time)
                .await
                .map_err(|_| AfError::Internal)?
                .with_merchant_sales_ratio(offer.sales_ratio_micros)
                .map_err(|_| AfError::ModelNotAllowed)?;
            if !price_ratio_allowed(&snapshot, plan.route.max_price_ratio_micros)? {
                continue;
            }
            let quoted_quota = snapshot
                .resolver()
                .resolve(&PricingContext::new(upper_bound))
                .map_err(|_| AfError::Internal)?
                .quota()
                .units();
            let health = statistics
                .iter()
                .find(|s| s.service_id == offer.service_id && s.sample_count >= 20)
                .map(|s| UserRouteModelHealth {
                    group_id,
                    success_count: s.success_count,
                    fail_count: s.failure_count,
                    total_latency_ms: u64::try_from(s.latency_p50_ms.unwrap_or(0))
                        .unwrap_or(0)
                        .saturating_mul(s.success_count),
                    cooldown_until_millis: None,
                    updated_at_millis: s.updated_at.saturating_mul(1000),
                });
            if health.as_ref().is_some_and(|h| {
                h.success_count == 0
                    && h.fail_count >= 5
                    && now_millis().is_ok_and(|now| now - h.updated_at_millis < 30_000)
            }) {
                continue;
            }
            let candidate = UserRouteCandidate {
                group_id,
                snapshot,
                merchant: Some(offer.clone()),
                member: UserRouteMemberRecord {
                    group_id,
                    priority: member.priority,
                    weight: member.weight,
                    last_selected_at_millis: member.last_selected_at_millis,
                },
                quoted_quota,
                health,
            };
            let target = if is_fallback {
                &mut fallback
            } else {
                &mut candidates
            };
            // One public merchant gets one position, independent of its channel count.
            if let Some(index) = target.iter().position(|c| {
                c.merchant
                    .as_ref()
                    .is_some_and(|m| m.merchant_id == offer.merchant_id)
            }) {
                if candidate.quoted_quota < target[index].quoted_quota {
                    target[index] = candidate;
                }
            } else {
                target.push(candidate);
            }
        }
        order_candidates(plan.route.strategy, &mut candidates)?;
        order_candidates(plan.route.strategy, &mut fallback)?;
        if plan.route.strategy == UserRouteStrategy::RoundRobin && !candidates.is_empty() {
            let ids = candidates
                .iter()
                .filter_map(|c| c.merchant.as_ref().map(|m| m.merchant_id))
                .collect::<Vec<_>>();
            let selected = self
                .repository
                .claim_merchant_round_robin(principal.user_id(), plan.route.id, &ids)
                .await
                .map_err(map_repository_error)?;
            let index = candidates
                .iter()
                .position(|c| {
                    c.merchant
                        .as_ref()
                        .is_some_and(|m| m.merchant_id == selected)
                })
                .ok_or(AfError::Internal)?;
            candidates.rotate_left(index);
        }
        candidates.extend(fallback);
        if candidates.is_empty() {
            return Err(AfError::ModelUnavailable);
        }
        Ok(candidates)
    }

    pub(crate) async fn validate_merchant_candidate(
        &self,
        principal: GatewayPrincipal,
        plan: &UserRouteRequestPlan,
        offer: &MerchantRuntimeCandidate,
    ) -> Result<bool, AfError> {
        let pools = self.merchant_pools.as_ref().ok_or(AfError::Internal)?;
        let current = self
            .repository
            .merchant_route(principal.user_id(), plan.route.id)
            .await
            .map_err(map_repository_error)?;
        if !current.configuration.enabled || current.configuration.pool_id != offer.pool_id {
            return Ok(false);
        }
        let scope = self
            .repository
            .merchant_scope(plan.route.id)
            .await
            .map_err(map_repository_error)?
            .ok_or(AfError::InvalidApiKey)?;
        if scope.excluded_merchant_ids.contains(&offer.merchant_id) {
            return Ok(false);
        }
        if !scope
            .members
            .iter()
            .any(|m| m.merchant_id == offer.merchant_id)
            && (!scope.allow_pool_fallback
                || (!scope.accept_new_merchants
                    && !scope.enrolled_merchants.contains(&offer.merchant_id)))
        {
            return Ok(false);
        }
        if !pools
            .check_pool_access(principal.user_id(), offer.pool_id)
            .await
            .map_err(|_| AfError::Internal)?
        {
            return Ok(false);
        }
        let live = pools
            .published_candidates(offer.pool_id, Some(offer.merchant_id), &plan.model)
            .await
            .map_err(|_| AfError::Internal)?;
        Ok(live.iter().any(|m| {
            m.service_id == offer.service_id
                && m.revision_id == offer.revision_id
                && m.channel_id == offer.channel_id
                && m.sales_ratio_micros == offer.sales_ratio_micros
        }))
    }

    pub(crate) fn merchant_principal(
        principal: GatewayPrincipal,
        offer: &MerchantRuntimeCandidate,
    ) -> Result<GatewayPrincipal, AfError> {
        let terms = MerchantBillingSnapshot::new(
            offer.pool_id,
            offer.merchant_id,
            offer.service_id,
            offer.revision_id,
            offer.sales_ratio_micros,
            offer.commission_bps,
            offer.settlement_delay_secs,
        )
        .map_err(|_| AfError::Internal)?;
        let channel = ChannelId::new(offer.channel_id).map_err(|_| AfError::Internal)?;
        Ok(principal.with_merchant_target(terms, channel))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn merchant_feedback(
        &self,
        terms: MerchantBillingSnapshot,
        route_id: i64,
        request_id: &str,
        model: &str,
        protocol: Protocol,
        streaming: bool,
        success: bool,
        latency_ms: u64,
        ttft_ms: Option<u64>,
        retryable: bool,
    ) {
        let write = MerchantAttemptWrite {
            id: uuid::Uuid::new_v4().to_string(),
            pool_id: terms.pool_id(),
            route_id,
            request_id: request_id.to_owned(),
            merchant_id: terms.merchant_id(),
            service_id: terms.service_id(),
            model: model.to_owned(),
            protocol: protocol.as_str().to_owned(),
            streaming,
            outcome: if success {
                "success"
            } else if retryable {
                "failure"
            } else {
                "ignored"
            }
            .to_owned(),
            latency_ms: i64::try_from(latency_ms).unwrap_or(i64::MAX),
            ttft_ms: ttft_ms.and_then(|n| i64::try_from(n).ok()),
        };
        if self
            .repository
            .record_merchant_attempt(write)
            .await
            .is_err()
        {
            tracing::warn!(target:"af_server::merchant_pool",error_kind="merchant_feedback_unavailable","商户调用统计暂不可用");
        }
    }
}
