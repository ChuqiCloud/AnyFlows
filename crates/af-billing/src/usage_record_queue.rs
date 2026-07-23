use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use af_domain::BillingReservationId;
use thiserror::Error;
use tokio::sync::Notify;

use crate::{BillingUsageRecord, UsageRecordOutcome, UsageRecordPort};

/// 用量记录有界接收队列的配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UsageRecordQueueError {
    /// 队列容量必须大于零，避免把所有请求静默判定为饱和。
    #[error("用量记录队列容量必须大于零")]
    ZeroCapacity,
}

struct QueueState {
    closed: bool,
    pending: VecDeque<BillingReservationId>,
    entries: HashMap<BillingReservationId, QueueEntry>,
}

struct QueueEntry {
    record: BillingUsageRecord,
    in_flight: bool,
}

struct QueueInner {
    capacity: usize,
    state: Mutex<QueueState>,
    notify: Notify,
}

/// 带幂等键、背压与未确认重投语义的用量记录接收边界。
///
/// `Accepted` 只表示记录已经进入此边界；消费方必须在可靠下游确认后调用
/// [`UsageRecordDelivery::acknowledge`]。未确认的投递被丢弃或发生 panic 时会自动重新入队。
/// 进程崩溃恢复仍由后续持久化 sink/WAL 负责，本类型不伪装成磁盘事务。
#[derive(Clone)]
pub struct UsageRecordQueue {
    inner: Arc<QueueInner>,
}

impl UsageRecordQueue {
    /// 创建一个有界用量记录队列及其消费端。
    pub fn bounded(capacity: usize) -> Result<(Self, UsageRecordReceiver), UsageRecordQueueError> {
        if capacity == 0 {
            return Err(UsageRecordQueueError::ZeroCapacity);
        }
        let queue = Self {
            inner: Arc::new(QueueInner {
                capacity,
                state: Mutex::new(QueueState {
                    closed: false,
                    pending: VecDeque::with_capacity(capacity),
                    entries: HashMap::with_capacity(capacity),
                }),
                notify: Notify::new(),
            }),
        };
        Ok((
            queue.clone(),
            UsageRecordReceiver {
                queue: queue.clone(),
            },
        ))
    }

    /// 关闭接收入口；已在途投递仍会在未确认时重新入队，供消费者排空。
    pub fn close(&self) {
        let mut state = lock_state(&self.inner);
        state.closed = true;
        self.inner.notify.notify_waiters();
    }

    /// 返回队列是否已经关闭。
    #[must_use]
    pub fn is_closed(&self) -> bool {
        lock_state(&self.inner).closed
    }

    /// 返回所有已接收投递是否都已经获得确认。
    #[must_use]
    pub fn is_drained(&self) -> bool {
        lock_state(&self.inner).entries.is_empty()
    }
}

impl UsageRecordPort for UsageRecordQueue {
    fn try_record(&self, record: BillingUsageRecord) -> UsageRecordOutcome {
        let mut state = lock_state(&self.inner);
        if state.closed {
            return UsageRecordOutcome::Closed;
        }
        if let Some(existing) = state.entries.get(&record.event_id()) {
            return if existing.record == record {
                UsageRecordOutcome::Accepted
            } else {
                UsageRecordOutcome::Conflict
            };
        }
        if state.entries.len() >= self.inner.capacity {
            return UsageRecordOutcome::Saturated;
        }
        state.entries.insert(
            record.event_id(),
            QueueEntry {
                record,
                in_flight: false,
            },
        );
        state.pending.push_back(record.event_id());
        drop(state);
        self.inner.notify.notify_one();
        UsageRecordOutcome::Accepted
    }
}

/// 用量记录队列的消费端。
#[derive(Clone)]
pub struct UsageRecordReceiver {
    queue: UsageRecordQueue,
}

impl UsageRecordReceiver {
    /// 非阻塞取出一条尚未确认的投递。
    pub fn try_take(&self) -> Option<UsageRecordDelivery> {
        let mut state = lock_state(&self.queue.inner);
        let event_id = state.pending.pop_front()?;
        let entry = state.entries.get_mut(&event_id)?;
        entry.in_flight = true;
        Some(UsageRecordDelivery {
            queue: self.queue.clone(),
            record: entry.record,
            acknowledged: false,
        })
    }

    /// 等待并取出一条投递；队列关闭且没有待处理记录时返回 `None`。
    pub async fn recv(&self) -> Option<UsageRecordDelivery> {
        loop {
            if let Some(delivery) = self.try_take() {
                return Some(delivery);
            }
            let notified = self.queue.inner.notify.notified();
            let closed_and_empty = {
                let state = lock_state(&self.queue.inner);
                state.closed && state.pending.is_empty()
            };
            if closed_and_empty {
                return None;
            }
            notified.await;
        }
    }
}

/// 已从队列取出但尚未确认的用量记录。
pub struct UsageRecordDelivery {
    queue: UsageRecordQueue,
    record: BillingUsageRecord,
    acknowledged: bool,
}

impl UsageRecordDelivery {
    /// 返回本次投递携带的规范化用量记录。
    #[must_use]
    pub const fn record(&self) -> BillingUsageRecord {
        self.record
    }

    /// 确认下游已经可靠接收；确认后同一幂等键才会从内存边界释放。
    pub fn acknowledge(mut self) -> bool {
        let mut state = lock_state(&self.queue.inner);
        let removed = state
            .entries
            .get(&self.record.event_id())
            .is_some_and(|entry| entry.in_flight && entry.record == self.record);
        if removed {
            state.entries.remove(&self.record.event_id());
        }
        if removed {
            self.acknowledged = true;
        }
        removed
    }
}

impl Drop for UsageRecordDelivery {
    fn drop(&mut self) {
        if self.acknowledged {
            return;
        }
        let mut state = lock_state(&self.queue.inner);
        let Some(entry) = state.entries.get_mut(&self.record.event_id()) else {
            return;
        };
        if !entry.in_flight || entry.record != self.record {
            return;
        }
        entry.in_flight = false;
        state.pending.push_back(self.record.event_id());
        drop(state);
        self.queue.inner.notify.notify_one();
    }
}

fn lock_state(inner: &QueueInner) -> std::sync::MutexGuard<'_, QueueState> {
    inner
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use af_domain::{GatewayPrincipal, GroupId, Quota, TokenId, UserId};
    use af_protocol::{TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource};

    use super::*;
    use crate::BillingMode;

    #[test]
    fn zero_capacity_is_rejected() {
        assert!(matches!(
            UsageRecordQueue::bounded(0),
            Err(UsageRecordQueueError::ZeroCapacity)
        ));
    }

    #[tokio::test]
    async fn duplicate_is_idempotent_and_conflicting_payload_is_rejected() {
        let (queue, receiver) = UsageRecordQueue::bounded(1).unwrap();
        let record = record(1, 2);
        assert_eq!(queue.try_record(record), UsageRecordOutcome::Accepted);
        assert_eq!(queue.try_record(record), UsageRecordOutcome::Accepted);
        assert_eq!(
            queue.try_record(record_with_quota(1, 3)),
            UsageRecordOutcome::Conflict
        );

        let delivery = receiver.recv().await.unwrap();
        assert_eq!(delivery.record(), record);
        assert!(delivery.acknowledge());
        assert!(receiver.try_take().is_none());
    }

    #[tokio::test]
    async fn unacknowledged_delivery_is_requeued_and_capacity_is_bounded() {
        let (queue, receiver) = UsageRecordQueue::bounded(1).unwrap();
        let queued = record(2, 4);
        assert_eq!(queue.try_record(queued), UsageRecordOutcome::Accepted);
        assert_eq!(
            queue.try_record(record(3, 5)),
            UsageRecordOutcome::Saturated
        );
        {
            let _delivery = receiver.recv().await.unwrap();
        }
        let redelivered = receiver.recv().await.unwrap();
        assert_eq!(redelivered.record(), queued);
        assert!(redelivered.acknowledge());
    }

    #[tokio::test]
    async fn close_rejects_new_records_and_finishes_receiver() {
        let (queue, receiver) = UsageRecordQueue::bounded(1).unwrap();
        queue.close();
        assert_eq!(queue.try_record(record(4, 1)), UsageRecordOutcome::Closed);
        assert!(receiver.recv().await.is_none());
    }

    #[tokio::test]
    async fn close_keeps_unacknowledged_delivery_available_for_drain() {
        let (queue, receiver) = UsageRecordQueue::bounded(1).unwrap();
        let queued = record(5, 1);
        assert_eq!(queue.try_record(queued), UsageRecordOutcome::Accepted);
        let delivery = receiver.recv().await.unwrap();
        queue.close();
        drop(delivery);

        let redelivered = receiver.recv().await.unwrap();
        assert_eq!(redelivered.record(), queued);
        assert!(redelivered.acknowledge());
        assert!(receiver.recv().await.is_none());
    }

    fn record(marker: u8, quota: i64) -> BillingUsageRecord {
        BillingUsageRecord::new(
            BillingReservationId::new([marker; 16]).unwrap(),
            GatewayPrincipal::new(
                TokenId::new(1).unwrap(),
                UserId::new(2).unwrap(),
                GroupId::new(3).unwrap(),
            ),
            Usage::new(
                TokenCount::new(1).unwrap(),
                TokenCount::new(1).unwrap(),
                UsageDetails::new(
                    TokenCount::ZERO,
                    TokenCount::ZERO,
                    TokenCount::ZERO,
                    TokenCount::ZERO,
                    TokenCount::ZERO,
                    TokenCount::ZERO,
                ),
                UsageSource::Estimated,
                UsageSemantics::Inclusive,
            )
            .unwrap(),
            BillingMode::PerToken,
            Quota::new(quota).unwrap(),
        )
    }

    fn record_with_quota(marker: u8, quota: i64) -> BillingUsageRecord {
        record(marker, quota)
    }
}
