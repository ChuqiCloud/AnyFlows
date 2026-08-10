use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use af_domain::BillingReservationId;
use thiserror::Error;
use tokio::sync::Notify;

use super::{TaskBillingReleaseSignalOutcome, TaskBillingReleaseSignalPort};

/// 任务额度释放队列的配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskBillingReleaseQueueError {
    /// 队列容量必须大于零，避免释放责任被静默丢弃。
    #[error("任务额度释放队列容量必须大于零")]
    ZeroCapacity,
}

struct QueueState {
    closed: bool,
    pending: VecDeque<BillingReservationId>,
    entries: HashMap<BillingReservationId, bool>,
}

struct QueueInner {
    capacity: usize,
    state: Mutex<QueueState>,
    notify: Notify,
}

/// 任务额度释放专用的非阻塞有界队列。
///
/// 队列只保存计费预留标识，不携带用户、额度或任务正文；它与普通请求退款队列
/// 使用完全不同的类型和端口，防止两个预留用途被错误混用。
#[derive(Clone)]
pub struct TaskBillingReleaseQueue {
    inner: Arc<QueueInner>,
}

impl TaskBillingReleaseQueue {
    /// 创建任务释放队列及其消费端。
    pub fn bounded(
        capacity: usize,
    ) -> Result<(Self, TaskBillingReleaseReceiver), TaskBillingReleaseQueueError> {
        if capacity == 0 {
            return Err(TaskBillingReleaseQueueError::ZeroCapacity);
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
            TaskBillingReleaseReceiver {
                queue: queue.clone(),
            },
        ))
    }

    /// 关闭入口；已接收信号仍会被消费端排空。
    pub fn close(&self) {
        let mut state = lock_state(&self.inner);
        state.closed = true;
        drop(state);
        self.inner.notify.notify_waiters();
    }

    /// 返回所有已接收信号是否都已确认。
    #[must_use]
    pub fn is_drained(&self) -> bool {
        lock_state(&self.inner).entries.is_empty()
    }
}

impl TaskBillingReleaseSignalPort for TaskBillingReleaseQueue {
    fn try_signal_release(
        &self,
        reservation_id: BillingReservationId,
    ) -> TaskBillingReleaseSignalOutcome {
        let mut state = lock_state(&self.inner);
        if state.closed {
            return TaskBillingReleaseSignalOutcome::Closed;
        }
        if state.entries.contains_key(&reservation_id) {
            return TaskBillingReleaseSignalOutcome::Accepted;
        }
        if state.entries.len() >= self.inner.capacity {
            return TaskBillingReleaseSignalOutcome::Saturated;
        }
        state.entries.insert(reservation_id, false);
        state.pending.push_back(reservation_id);
        drop(state);
        self.inner.notify.notify_one();
        TaskBillingReleaseSignalOutcome::Accepted
    }
}

/// 任务额度释放队列的消费端。
#[derive(Clone)]
pub struct TaskBillingReleaseReceiver {
    queue: TaskBillingReleaseQueue,
}

impl TaskBillingReleaseReceiver {
    /// 非阻塞取出一条待处理信号。
    pub fn try_take(&self) -> Option<TaskBillingReleaseDelivery> {
        let mut state = lock_state(&self.queue.inner);
        let reservation_id = state.pending.pop_front()?;
        let in_flight = state.entries.get_mut(&reservation_id)?;
        *in_flight = true;
        Some(TaskBillingReleaseDelivery {
            queue: self.queue.clone(),
            reservation_id,
            acknowledged: false,
        })
    }

    /// 等待并取出信号；入口关闭且待处理队列为空时返回 `None`。
    pub async fn recv(&self) -> Option<TaskBillingReleaseDelivery> {
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

/// 已取出但尚未确认的任务释放信号。
pub struct TaskBillingReleaseDelivery {
    queue: TaskBillingReleaseQueue,
    reservation_id: BillingReservationId,
    acknowledged: bool,
}

impl TaskBillingReleaseDelivery {
    /// 返回需要调用批量任务释放端口的预留标识。
    #[must_use]
    pub const fn reservation_id(&self) -> BillingReservationId {
        self.reservation_id
    }

    /// 确认释放结果已可靠完成或已被幂等接受。
    pub fn acknowledge(mut self) -> bool {
        let mut state = lock_state(&self.queue.inner);
        let removed = state
            .entries
            .get(&self.reservation_id)
            .is_some_and(|in_flight| *in_flight);
        if removed {
            state.entries.remove(&self.reservation_id);
            self.acknowledged = true;
        }
        removed
    }
}

impl Drop for TaskBillingReleaseDelivery {
    fn drop(&mut self) {
        if self.acknowledged {
            return;
        }
        let mut state = lock_state(&self.queue.inner);
        let Some(in_flight) = state.entries.get_mut(&self.reservation_id) else {
            return;
        };
        if !*in_flight {
            return;
        }
        *in_flight = false;
        state.pending.push_back(self.reservation_id);
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
    use super::*;

    #[test]
    fn zero_capacity_is_rejected() {
        assert!(matches!(
            TaskBillingReleaseQueue::bounded(0),
            Err(TaskBillingReleaseQueueError::ZeroCapacity)
        ));
    }

    #[tokio::test]
    async fn duplicate_signal_is_idempotent_and_capacity_is_bounded() {
        let (queue, receiver) = TaskBillingReleaseQueue::bounded(1).unwrap();
        let first = reservation_id(1);
        assert_eq!(
            queue.try_signal_release(first),
            TaskBillingReleaseSignalOutcome::Accepted
        );
        assert_eq!(
            queue.try_signal_release(first),
            TaskBillingReleaseSignalOutcome::Accepted
        );
        assert_eq!(
            queue.try_signal_release(reservation_id(2)),
            TaskBillingReleaseSignalOutcome::Saturated
        );

        let delivery = receiver.recv().await.unwrap();
        assert_eq!(delivery.reservation_id(), first);
        assert!(delivery.acknowledge());
        assert!(queue.is_drained());
    }

    #[tokio::test]
    async fn unacknowledged_delivery_is_requeued() {
        let (queue, receiver) = TaskBillingReleaseQueue::bounded(1).unwrap();
        let id = reservation_id(3);
        assert_eq!(
            queue.try_signal_release(id),
            TaskBillingReleaseSignalOutcome::Accepted
        );
        drop(receiver.recv().await.unwrap());
        let delivery = receiver.recv().await.unwrap();
        assert_eq!(delivery.reservation_id(), id);
        assert!(delivery.acknowledge());
    }

    fn reservation_id(marker: u8) -> BillingReservationId {
        BillingReservationId::new([marker; 16]).unwrap()
    }
}
