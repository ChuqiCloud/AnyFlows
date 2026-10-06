use std::{
    collections::HashMap,
    fmt,
    num::NonZeroUsize,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use af_domain::{ChannelId, CredentialId};
use tokio::{runtime::Handle, sync::watch, time::sleep};

use super::{OAuthRefreshCoordinatorError, OAuthRefreshCoordinatorOutcome};

type SharedRefreshResult = Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError>;

/// 一个 flight 的稳定非敏感身份；OAuth 版本阻止重新授权后的候选加入旧任务。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct OAuthRefreshFlightKey {
    channel_id: ChannelId,
    credential_id: CredentialId,
    expected_revision: i64,
}

impl OAuthRefreshFlightKey {
    pub(super) const fn new(
        channel_id: ChannelId,
        credential_id: CredentialId,
        expected_revision: i64,
    ) -> Self {
        Self {
            channel_id,
            credential_id,
            expected_revision,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OAuthRefreshFlightState {
    Running,
    Completed(SharedRefreshResult),
}

struct OAuthRefreshFlightIdentity;

struct OAuthRefreshFlightEntry {
    identity: Arc<OAuthRefreshFlightIdentity>,
    sender: watch::Sender<OAuthRefreshFlightState>,
    completed_at: Option<Instant>,
}

/// 新建 flight 时可能出现的闭合同步错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OAuthRefreshSingleflightError {
    CapacityExceeded,
    Unavailable,
}

pub(super) enum OAuthRefreshFlightAcquisition {
    Leader(OAuthRefreshLeaderFlight),
    Follower(OAuthRefreshFollowerFlight),
}

/// 有界 singleflight registry；锁内只做 Map 与 watch 状态切换，不执行 HTTP 或数据库 IO。
pub(super) struct OAuthRefreshSingleflight {
    entries: Mutex<HashMap<OAuthRefreshFlightKey, OAuthRefreshFlightEntry>>,
    max_entries: NonZeroUsize,
    completed_retention: Duration,
}

impl OAuthRefreshSingleflight {
    pub(super) fn new(max_entries: NonZeroUsize, completed_retention: Duration) -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(HashMap::with_capacity(max_entries.get().min(64))),
            max_entries,
            completed_retention,
        })
    }

    pub(super) fn acquire(
        self: &Arc<Self>,
        key: OAuthRefreshFlightKey,
    ) -> Result<OAuthRefreshFlightAcquisition, OAuthRefreshSingleflightError> {
        let now = Instant::now();
        let mut entries = self.lock_entries()?;
        retain_unexpired(&mut entries, now, self.completed_retention);
        if let Some(entry) = entries.get(&key) {
            return Ok(OAuthRefreshFlightAcquisition::Follower(
                OAuthRefreshFollowerFlight {
                    receiver: entry.sender.subscribe(),
                },
            ));
        }

        if entries.len() >= self.max_entries.get() {
            // 已完成缓存不得挤占真实并发容量；必要时提前淘汰最早完成项。
            let oldest_completed = entries
                .iter()
                .filter_map(|(key, entry)| {
                    entry.completed_at.map(|completed_at| (*key, completed_at))
                })
                .min_by_key(|(_, completed_at)| *completed_at)
                .map(|(key, _)| key);
            if let Some(oldest_completed) = oldest_completed {
                entries.remove(&oldest_completed);
            } else {
                return Err(OAuthRefreshSingleflightError::CapacityExceeded);
            }
        }

        let identity = Arc::new(OAuthRefreshFlightIdentity);
        let (sender, _) = watch::channel(OAuthRefreshFlightState::Running);
        entries.insert(
            key,
            OAuthRefreshFlightEntry {
                identity: Arc::clone(&identity),
                sender: sender.clone(),
                completed_at: None,
            },
        );
        drop(entries);
        Ok(OAuthRefreshFlightAcquisition::Leader(
            OAuthRefreshLeaderFlight {
                registry: Arc::clone(self),
                key,
                identity,
                sender,
                completed: false,
            },
        ))
    }

    fn complete(
        self: &Arc<Self>,
        key: OAuthRefreshFlightKey,
        identity: &Arc<OAuthRefreshFlightIdentity>,
        sender: &watch::Sender<OAuthRefreshFlightState>,
        result: SharedRefreshResult,
    ) {
        let completed_at = Instant::now();
        let recorded = self.entries.lock().is_ok_and(|mut entries| {
            let Some(entry) = entries.get_mut(&key) else {
                return false;
            };
            if !Arc::ptr_eq(&entry.identity, identity) {
                return false;
            }
            entry.completed_at = Some(completed_at);
            entry
                .sender
                .send_replace(OAuthRefreshFlightState::Completed(result));
            true
        });
        if !recorded {
            // registry 不可用时仍优先唤醒已经取得 receiver 的 follower。
            sender.send_replace(OAuthRefreshFlightState::Completed(result));
        }
        self.schedule_cleanup(key, identity, completed_at);
    }

    fn schedule_cleanup(
        self: &Arc<Self>,
        key: OAuthRefreshFlightKey,
        identity: &Arc<OAuthRefreshFlightIdentity>,
        completed_at: Instant,
    ) {
        if self.completed_retention.is_zero() {
            self.remove_completed(key, identity, completed_at);
            return;
        }
        let Ok(runtime) = Handle::try_current() else {
            // 无 Tokio runtime 时仍由下一次 acquire 的惰性清理保证容量上限。
            return;
        };
        let registry = Arc::downgrade(self);
        let identity = Arc::downgrade(identity);
        let retention = self.completed_retention;
        // 丢弃 JoinHandle 只解除等待，不会取消短生命周期清理任务。
        drop(runtime.spawn(async move {
            sleep(retention).await;
            let (Some(registry), Some(identity)) = (registry.upgrade(), identity.upgrade()) else {
                return;
            };
            registry.remove_completed(key, &identity, completed_at);
        }));
    }

    fn remove_completed(
        &self,
        key: OAuthRefreshFlightKey,
        identity: &Arc<OAuthRefreshFlightIdentity>,
        completed_at: Instant,
    ) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let should_remove = entries.get(&key).is_some_and(|entry| {
            Arc::ptr_eq(&entry.identity, identity)
                && entry.completed_at == Some(completed_at)
                && completed_at.elapsed() >= self.completed_retention
        });
        if should_remove {
            entries.remove(&key);
        }
    }

    fn lock_entries(
        &self,
    ) -> Result<
        MutexGuard<'_, HashMap<OAuthRefreshFlightKey, OAuthRefreshFlightEntry>>,
        OAuthRefreshSingleflightError,
    > {
        self.entries
            .lock()
            .map_err(|_| OAuthRefreshSingleflightError::Unavailable)
    }

    #[cfg(test)]
    fn entry_count(&self) -> usize {
        self.entries.lock().map_or(0, |entries| entries.len())
    }
}

impl fmt::Debug for OAuthRefreshSingleflight {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthRefreshSingleflight")
            .field("entries", &"<已脱敏>")
            .field("max_entries", &self.max_entries)
            .field("completed_retention", &self.completed_retention)
            .finish()
    }
}

pub(super) struct OAuthRefreshLeaderFlight {
    registry: Arc<OAuthRefreshSingleflight>,
    key: OAuthRefreshFlightKey,
    identity: Arc<OAuthRefreshFlightIdentity>,
    sender: watch::Sender<OAuthRefreshFlightState>,
    completed: bool,
}

impl OAuthRefreshLeaderFlight {
    pub(super) fn complete(mut self, result: SharedRefreshResult) -> SharedRefreshResult {
        self.completed = true;
        self.registry
            .complete(self.key, &self.identity, &self.sender, result);
        result
    }
}

impl Drop for OAuthRefreshLeaderFlight {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        // Drop 只切换内存状态并调度清理，不执行异步 IO 或阻塞等待。
        self.completed = true;
        self.registry.complete(
            self.key,
            &self.identity,
            &self.sender,
            Err(OAuthRefreshCoordinatorError::LeaderAborted),
        );
    }
}

pub(super) struct OAuthRefreshFollowerFlight {
    receiver: watch::Receiver<OAuthRefreshFlightState>,
}

impl OAuthRefreshFollowerFlight {
    pub(super) async fn wait(
        mut self,
    ) -> Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError> {
        loop {
            let state = *self.receiver.borrow_and_update();
            if let OAuthRefreshFlightState::Completed(result) = state {
                return result;
            }
            if self.receiver.changed().await.is_err() {
                return Err(OAuthRefreshCoordinatorError::LeaderAborted);
            }
        }
    }
}

fn retain_unexpired(
    entries: &mut HashMap<OAuthRefreshFlightKey, OAuthRefreshFlightEntry>,
    now: Instant,
    completed_retention: Duration,
) {
    entries.retain(|_, entry| {
        entry.completed_at.is_none_or(|completed_at| {
            now.saturating_duration_since(completed_at) < completed_retention
        })
    });
}

#[cfg(test)]
mod tests;
