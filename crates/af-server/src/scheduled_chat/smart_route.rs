use std::{future::Future, pin::Pin};

use af_db::{
    SmartRouteAttemptFeedback, SmartRouteRuntimeRepository, SmartRouteRuntimeRepositoryError,
    SmartRouteRuntimeRule,
};
use af_domain::{AfError, RouteChannelId};
use af_relay::RelayAttemptReport;
use af_scheduler::BoundRouteCandidate;

/// 单次智能路由规则读取的异步结果。
pub(super) type SmartRouteRulesFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<SmartRouteRuntimeRule>, SmartRouteRuntimeRepositoryError>>
            + Send
            + 'a,
    >,
>;

/// 智能路由规则读取的可替换边界，测试可以注入静态规则而不伪造数据库内部调用。
pub(super) trait SmartRouteResolver: Send + Sync {
    fn matching_rules<'a>(&'a self, requested_model: &'a str) -> SmartRouteRulesFuture<'a>;
}

impl SmartRouteResolver for SmartRouteRuntimeRepository {
    fn matching_rules<'a>(&'a self, requested_model: &'a str) -> SmartRouteRulesFuture<'a> {
        Box::pin(SmartRouteRuntimeRepository::matching_rules(
            self,
            requested_model,
        ))
    }
}

/// 把数据库规则候选转换为调度器的强类型输入。
pub(super) fn bound_candidates(
    rule: &SmartRouteRuntimeRule,
) -> Result<Vec<BoundRouteCandidate>, AfError> {
    rule.candidates()
        .iter()
        .map(|candidate| {
            BoundRouteCandidate::new(
                candidate.route_channel_id(),
                candidate.channel_id(),
                candidate.credential_id(),
                candidate.priority(),
                candidate.weight(),
                candidate.last_selected_at_millis(),
            )
            .map_err(|_| AfError::Internal)
        })
        .collect()
}

/// 记录真实尝试对应的智能路由候选统计；失败不覆盖业务响应。
pub(super) async fn persist_feedback(
    repository: Option<&SmartRouteRuntimeRepository>,
    route_channel_ids: &[Option<RouteChannelId>],
    report: &RelayAttemptReport,
) {
    let Some(repository) = repository else {
        return;
    };
    let mut feedback = Vec::with_capacity(report.failures().len() + 1);
    for failure in report.failures() {
        let Some(route_channel_id) = route_channel_ids
            .get(failure.candidate_index())
            .copied()
            .flatten()
        else {
            continue;
        };
        let Ok(item) = SmartRouteAttemptFeedback::new(
            route_channel_id,
            false,
            duration_millis(failure.elapsed()),
        ) else {
            tracing::warn!(
                target: "af_server::smart_route",
                error_kind = "smart_route_feedback_invalid",
                "智能路由失败反馈超出持久化边界"
            );
            return;
        };
        feedback.push(item);
    }
    if let (Some(index), Some(elapsed)) = (
        report.successful_candidate_index(),
        report.successful_elapsed(),
    ) && let Some(route_channel_id) = route_channel_ids.get(index).copied().flatten()
    {
        let Ok(item) =
            SmartRouteAttemptFeedback::new(route_channel_id, true, duration_millis(elapsed))
        else {
            tracing::warn!(
                target: "af_server::smart_route",
                error_kind = "smart_route_feedback_invalid",
                "智能路由成功反馈超出持久化边界"
            );
            return;
        };
        feedback.push(item);
    }
    if feedback.is_empty() {
        return;
    }
    if let Err(error) = repository.record_attempts(&feedback).await {
        let error_kind = match error {
            SmartRouteRuntimeRepositoryError::Query => "smart_route_feedback_query",
            SmartRouteRuntimeRepositoryError::Timeout => "smart_route_feedback_timeout",
            SmartRouteRuntimeRepositoryError::Invariant => "smart_route_feedback_invariant",
        };
        tracing::warn!(
            target: "af_server::smart_route",
            error_kind,
            "智能路由候选统计写入失败，当前业务响应保持不变"
        );
    }
}

fn duration_millis(duration: std::time::Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
