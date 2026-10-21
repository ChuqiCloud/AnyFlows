use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
};

use af_cache::{
    MAX_HEALTH_BATCH_SIZE, RedisHealthEvent, RedisHealthFailure, RedisHealthSnapshot,
    RedisHealthStore, RedisHealthTarget,
};
use af_domain::{ChannelId, CredentialId, RateLimitScope, UpstreamError};
use af_relay::RelayAttemptReport;
use af_scheduler::ChannelRoutingHealth;
use thiserror::Error;

use crate::credential_feedback::CredentialAttemptTarget;

/// 健康存储对象安全操作的异步返回类型。
pub(crate) type SchedulerHealthStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, SchedulerHealthRuntimeError>> + Send + 'a>>;

/// 调度健康状态端口；生产使用 Redis，单元测试可注入受控替身。
pub(crate) trait SchedulerHealthStore: Send + Sync {
    /// 读取与输入顺序严格一致的健康快照。
    fn states<'a>(
        &'a self,
        targets: &'a [RedisHealthTarget],
    ) -> SchedulerHealthStoreFuture<'a, Vec<RuntimeHealthState>>;

    /// 按输入顺序原子应用一批成功或失败事件。
    fn apply<'a>(&'a self, events: &'a [RedisHealthEvent]) -> SchedulerHealthStoreFuture<'a, ()>;
}

impl SchedulerHealthStore for RedisHealthStore {
    fn states<'a>(
        &'a self,
        targets: &'a [RedisHealthTarget],
    ) -> SchedulerHealthStoreFuture<'a, Vec<RuntimeHealthState>> {
        Box::pin(async move {
            RedisHealthStore::states(self, targets)
                .await
                .map(|states| states.into_iter().map(RuntimeHealthState::from).collect())
                .map_err(|_| SchedulerHealthRuntimeError::Store)
        })
    }

    fn apply<'a>(&'a self, events: &'a [RedisHealthEvent]) -> SchedulerHealthStoreFuture<'a, ()> {
        Box::pin(async move {
            RedisHealthStore::apply(self, events)
                .await
                .map(|_| ())
                .map_err(|_| SchedulerHealthRuntimeError::Store)
        })
    }
}

/// 请求级固定的凭据健康状态，不包含冷却截止绝对时间。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeHealthState {
    target: RedisHealthTarget,
    penalty_micros: u64,
    cooling: bool,
}

impl RuntimeHealthState {
    #[cfg(test)]
    pub(crate) const fn new(target: RedisHealthTarget, penalty_micros: u64, cooling: bool) -> Self {
        Self {
            target,
            penalty_micros,
            cooling,
        }
    }

    /// 返回当前衰减惩罚分。
    #[must_use]
    pub(crate) const fn penalty_micros(self) -> u64 {
        self.penalty_micros
    }

    /// 返回目标是否仍处于熔断冷却期。
    #[must_use]
    pub(crate) const fn is_cooling(self) -> bool {
        self.cooling
    }
}

impl From<RedisHealthSnapshot> for RuntimeHealthState {
    fn from(snapshot: RedisHealthSnapshot) -> Self {
        Self {
            target: snapshot.target(),
            penalty_micros: snapshot.penalty_micros(),
            cooling: snapshot.is_cooling(),
        }
    }
}

/// Redis 健康读取或端口契约损坏时的闭合错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum SchedulerHealthRuntimeError {
    #[error("调度健康状态存储不可用")]
    Store,
    #[error("调度健康状态返回违反请求级不变量")]
    Invariant,
}

/// 一次批量读取渠道健康状态并转换为调度层固定精度输入。
pub(crate) async fn load_channel_health(
    store: &Arc<dyn SchedulerHealthStore>,
    channel_ids: &[ChannelId],
) -> Result<BTreeMap<ChannelId, ChannelRoutingHealth>, SchedulerHealthRuntimeError> {
    let targets = channel_ids
        .iter()
        .copied()
        .map(RedisHealthTarget::Channel)
        .collect::<Vec<_>>();
    let states = load_exact_states(store, &targets).await?;
    Ok(states
        .into_iter()
        .zip(channel_ids.iter().copied())
        .map(|(state, channel_id)| {
            (
                channel_id,
                ChannelRoutingHealth::new(state.penalty_micros, state.cooling),
            )
        })
        .collect())
}

/// 一次批量读取凭据健康状态，供账号过滤与请求内稳定调权使用。
pub(crate) async fn load_credential_health(
    store: &Arc<dyn SchedulerHealthStore>,
    credential_ids: &[(CredentialId, CredentialId)],
) -> Result<BTreeMap<i64, RuntimeHealthState>, SchedulerHealthRuntimeError> {
    let targets = credential_ids
        .iter()
        .flat_map(|(routing_id, shared_health_id)| {
            [
                RedisHealthTarget::Credential(*routing_id),
                RedisHealthTarget::CredentialShared(*shared_health_id),
            ]
        })
        .collect::<Vec<_>>();
    let states = load_exact_states(store, &targets).await?;
    states
        .chunks_exact(2)
        .zip(credential_ids.iter().copied())
        .map(|(states, (routing_id, _))| {
            let [local, shared] = states else {
                return Err(SchedulerHealthRuntimeError::Invariant);
            };
            Ok((
                routing_id.get(),
                RuntimeHealthState {
                    target: RedisHealthTarget::Credential(routing_id),
                    penalty_micros: local.penalty_micros.max(shared.penalty_micros),
                    cooling: local.cooling || shared.cooling,
                },
            ))
        })
        .collect()
}

/// 最佳努力写入渠道与凭据双层反馈；失败不覆盖已确定的业务响应。
pub(crate) async fn persist_scheduler_health_feedback(
    store: Option<&Arc<dyn SchedulerHealthStore>>,
    targets: &[CredentialAttemptTarget],
    report: &RelayAttemptReport,
) {
    let Some(store) = store else {
        return;
    };
    let Some(events) = feedback_events(targets, report) else {
        tracing::error!(
            target: "af_server::scheduler_health",
            error_kind = "scheduler_health_feedback_invariant",
            "调度健康反馈候选索引无效"
        );
        return;
    };
    for batch in events.chunks(MAX_HEALTH_BATCH_SIZE) {
        if let Err(error) = store.apply(batch).await {
            tracing::warn!(
                target: "af_server::scheduler_health",
                error_kind = error.as_str(),
                "写入调度健康反馈失败，当前业务结果继续返回"
            );
            break;
        }
    }
}

fn feedback_events(
    targets: &[CredentialAttemptTarget],
    report: &RelayAttemptReport,
) -> Option<Vec<RedisHealthEvent>> {
    let outcome_count = report
        .failures()
        .len()
        .checked_add(usize::from(report.successful_candidate_index().is_some()))?;
    let mut events = Vec::with_capacity(outcome_count.checked_mul(2)?);
    for failure in report.failures() {
        append_failure(
            &mut events,
            *targets.get(failure.candidate_index())?,
            failure.error(),
        );
    }
    if let Some(index) = report.successful_candidate_index() {
        append_success(&mut events, *targets.get(index)?);
    }
    Some(events)
}

fn append_failure(
    events: &mut Vec<RedisHealthEvent>,
    target: CredentialAttemptTarget,
    error: UpstreamError,
) {
    match error {
        UpstreamError::BadRequest => {}
        UpstreamError::AuthExpired
        | UpstreamError::AuthRevoked
        | UpstreamError::AccountDisabled => events.push(RedisHealthEvent::failed(
            RedisHealthTarget::CredentialShared(target.shared_health_id()),
            RedisHealthFailure::Authentication,
        )),
        UpstreamError::QuotaExhausted => events.push(RedisHealthEvent::failed(
            RedisHealthTarget::Credential(target.credential_id()),
            RedisHealthFailure::Quota,
        )),
        UpstreamError::RateLimited {
            scope: RateLimitScope::Model,
            ..
        } => {}
        UpstreamError::RateLimited { .. } => events.push(RedisHealthEvent::failed(
            RedisHealthTarget::Credential(target.credential_id()),
            RedisHealthFailure::RateLimited,
        )),
        UpstreamError::Overloaded { .. } => events.push(RedisHealthEvent::failed(
            RedisHealthTarget::Credential(target.credential_id()),
            RedisHealthFailure::Overloaded,
        )),
        UpstreamError::ModelUnsupported | UpstreamError::ProtocolError => {
            events.push(RedisHealthEvent::failed(
                RedisHealthTarget::Credential(target.credential_id()),
                health_failure(error).expect("模型和协议错误必须有闭合健康分类"),
            ));
            if !target.pool_mode() {
                events.push(RedisHealthEvent::failed(
                    RedisHealthTarget::Channel(target.channel_id()),
                    health_failure(error).expect("模型和协议错误必须有闭合健康分类"),
                ));
            }
        }
        UpstreamError::Network { .. } | UpstreamError::ServerError { .. } => {
            let failure = health_failure(error).expect("网络和服务端错误必须有闭合健康分类");
            events.push(RedisHealthEvent::failed(
                RedisHealthTarget::Credential(target.credential_id()),
                failure,
            ));
            if !target.pool_mode() {
                events.push(RedisHealthEvent::failed(
                    RedisHealthTarget::Channel(target.channel_id()),
                    failure,
                ));
            }
        }
    }
}

fn append_success(events: &mut Vec<RedisHealthEvent>, target: CredentialAttemptTarget) {
    if !target.pool_mode() {
        events.push(RedisHealthEvent::succeeded(RedisHealthTarget::Channel(
            target.channel_id(),
        )));
    }
    events.push(RedisHealthEvent::succeeded(RedisHealthTarget::Credential(
        target.credential_id(),
    )));
    events.push(RedisHealthEvent::succeeded(
        RedisHealthTarget::CredentialShared(target.shared_health_id()),
    ));
}

const fn health_failure(error: UpstreamError) -> Option<RedisHealthFailure> {
    match error {
        UpstreamError::BadRequest => None,
        UpstreamError::ModelUnsupported => Some(RedisHealthFailure::ModelUnsupported),
        UpstreamError::ProtocolError => Some(RedisHealthFailure::Protocol),
        UpstreamError::AuthExpired | UpstreamError::AuthRevoked => {
            Some(RedisHealthFailure::Authentication)
        }
        UpstreamError::AccountDisabled => Some(RedisHealthFailure::Authentication),
        UpstreamError::QuotaExhausted => Some(RedisHealthFailure::Quota),
        // 当前健康存储只有渠道/凭据粒度，模型级限流不能扩大到整个账号。
        UpstreamError::RateLimited {
            scope: RateLimitScope::Model,
            ..
        } => None,
        UpstreamError::RateLimited { .. } => Some(RedisHealthFailure::RateLimited),
        UpstreamError::Overloaded { .. } => Some(RedisHealthFailure::Overloaded),
        UpstreamError::Network { .. } => Some(RedisHealthFailure::Network),
        UpstreamError::ServerError { .. } => Some(RedisHealthFailure::Server),
    }
}

async fn load_exact_states(
    store: &Arc<dyn SchedulerHealthStore>,
    targets: &[RedisHealthTarget],
) -> Result<Vec<RuntimeHealthState>, SchedulerHealthRuntimeError> {
    // Shadows share authentication health; read each target once and restore input order.
    let unique = targets
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut states_by_target = BTreeMap::new();
    for batch in unique.chunks(MAX_HEALTH_BATCH_SIZE) {
        let states = store.states(batch).await?;
        if states.len() != batch.len()
            || states
                .iter()
                .zip(batch)
                .any(|(state, target)| state.target != *target)
        {
            return Err(SchedulerHealthRuntimeError::Invariant);
        }
        states_by_target.extend(states.into_iter().map(|state| (state.target, state)));
    }
    targets
        .iter()
        .map(|target| {
            states_by_target
                .get(target)
                .copied()
                .ok_or(SchedulerHealthRuntimeError::Invariant)
        })
        .collect()
}

pub(crate) struct AdminCredentialHealthRuntime(pub(crate) Arc<dyn SchedulerHealthStore>);

impl af_http::AdminCredentialHealthReader for AdminCredentialHealthRuntime {
    fn snapshots<'a>(
        &'a self,
        ids: &'a [(CredentialId, CredentialId)],
    ) -> af_http::CredentialHealthFuture<'a> {
        Box::pin(async move {
            let states = load_credential_health(&self.0, ids).await.map_err(|_| ())?;
            ids.iter()
                .map(|(id, _)| {
                    let state = states.get(&id.get()).ok_or(())?;
                    Ok(af_http::CredentialHealthSnapshot {
                        credential_id: id.get(),
                        penalty_micros: state.penalty_micros,
                        cooling: state.cooling,
                        weight_multiplier_micros: crate::credential_order::health_weight_multiplier(
                            state.penalty_micros,
                        ),
                    })
                })
                .collect()
        })
    }
}

impl SchedulerHealthRuntimeError {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Store => "scheduler_health_store",
            Self::Invariant => "scheduler_health_invariant",
        }
    }
}

impl fmt::Debug for dyn SchedulerHealthStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SchedulerHealthStore(<受控>)")
    }
}

#[cfg(test)]
mod tests {
    use af_domain::{CredentialKind, RateLimitScope, UpstreamServerStatus};

    use super::*;

    #[derive(Default)]
    struct BatchHealthStore {
        requests: std::sync::Mutex<Vec<Vec<RedisHealthTarget>>>,
        reverse: bool,
    }

    impl SchedulerHealthStore for BatchHealthStore {
        fn states<'a>(
            &'a self,
            targets: &'a [RedisHealthTarget],
        ) -> SchedulerHealthStoreFuture<'a, Vec<RuntimeHealthState>> {
            Box::pin(async move {
                assert!(targets.len() <= MAX_HEALTH_BATCH_SIZE);
                self.requests.lock().unwrap().push(targets.to_vec());
                let mut states = targets
                    .iter()
                    .map(|target| RuntimeHealthState::new(*target, 0, false))
                    .collect::<Vec<_>>();
                if self.reverse {
                    states.reverse();
                }
                Ok(states)
            })
        }

        fn apply<'a>(
            &'a self,
            _events: &'a [RedisHealthEvent],
        ) -> SchedulerHealthStoreFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn large_credential_health_read_batches_and_deduplicates_targets() {
        let store = Arc::new(BatchHealthStore::default());
        let reader: Arc<dyn SchedulerHealthStore> = store.clone();
        let mut ids = (1..=130)
            .map(|id| {
                let id = CredentialId::new(id).unwrap();
                (id, id)
            })
            .collect::<Vec<_>>();
        ids.push(ids[0]);
        let states = load_credential_health(&reader, &ids).await.unwrap();
        assert_eq!(states.len(), 130);
        assert!(ids.iter().all(|(id, _)| states.contains_key(&id.get())));
        let requests = store.requests.lock().unwrap();
        assert_eq!(requests.len(), 260_usize.div_ceil(MAX_HEALTH_BATCH_SIZE));
        assert_eq!(requests.iter().map(Vec::len).sum::<usize>(), 260);
    }

    #[tokio::test]
    async fn health_read_rejects_reordered_store_results() {
        let store: Arc<dyn SchedulerHealthStore> = Arc::new(BatchHealthStore {
            reverse: true,
            ..BatchHealthStore::default()
        });
        let id = CredentialId::new(1).unwrap();
        assert_eq!(
            load_credential_health(&store, &[(id, id)]).await,
            Err(SchedulerHealthRuntimeError::Invariant)
        );
    }

    #[test]
    fn health_feedback_is_dual_level_and_skips_bad_request() {
        let target = CredentialAttemptTarget::new(
            ChannelId::new(7).unwrap(),
            9,
            CredentialKind::ApiKey,
            false,
            false,
        )
        .unwrap();
        assert_eq!(health_failure(UpstreamError::BadRequest), None);
        assert_eq!(
            health_failure(UpstreamError::ServerError {
                status: UpstreamServerStatus::new(503).unwrap(),
            }),
            Some(RedisHealthFailure::Server)
        );
        let mut events = Vec::new();
        append_failure(
            &mut events,
            target,
            UpstreamError::Network {
                kind: af_domain::NetworkFailureKind::Connect,
            },
        );
        append_success(&mut events, target);
        assert_eq!(events.len(), 5);
        assert_eq!(
            events[0].target(),
            RedisHealthTarget::Credential(CredentialId::new(9).unwrap())
        );
        assert_eq!(
            events[3].target(),
            RedisHealthTarget::Credential(CredentialId::new(9).unwrap())
        );
        assert_eq!(
            events[4].target(),
            RedisHealthTarget::CredentialShared(CredentialId::new(9).unwrap())
        );
    }

    #[test]
    fn pool_mode_keeps_credential_feedback_but_skips_channel_feedback() {
        let target = CredentialAttemptTarget::new(
            ChannelId::new(7).unwrap(),
            9,
            CredentialKind::ApiKey,
            false,
            true,
        )
        .unwrap();
        let mut events = Vec::new();
        append_failure(
            &mut events,
            target,
            UpstreamError::Network {
                kind: af_domain::NetworkFailureKind::Connect,
            },
        );
        append_success(&mut events, target);
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0].target(),
            RedisHealthTarget::Credential(_)
        ));
        assert!(matches!(
            events[2].target(),
            RedisHealthTarget::CredentialShared(_)
        ));
    }

    #[test]
    fn spark_feedback_separates_local_quota_from_shared_authentication() {
        let parent_id = CredentialId::new(9).unwrap();
        let shadow_id = CredentialId::new(10).unwrap();
        let target = CredentialAttemptTarget::with_runtime_identity(
            ChannelId::new(7).unwrap(),
            shadow_id,
            parent_id,
            parent_id,
            CredentialKind::Oauth,
            true,
            true,
        )
        .unwrap();

        let mut events = Vec::new();
        append_failure(
            &mut events,
            target,
            UpstreamError::rate_limited(RateLimitScope::Credential),
        );
        append_failure(&mut events, target, UpstreamError::AuthRevoked);
        append_success(&mut events, target);

        assert_eq!(events.len(), 4);
        assert_eq!(events[0].target(), RedisHealthTarget::Credential(shadow_id));
        assert_eq!(
            events[1].target(),
            RedisHealthTarget::CredentialShared(parent_id)
        );
        assert_eq!(events[2].target(), RedisHealthTarget::Credential(shadow_id));
        assert_eq!(
            events[3].target(),
            RedisHealthTarget::CredentialShared(parent_id)
        );
    }
}
