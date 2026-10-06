use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use af_domain::BillingReservationId;
use thiserror::Error;
use tokio::sync::Notify;

use crate::{RefundSignalOutcome, RefundSignalPort};

/// 内部退款信号有界队列的配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundSignalQueueError {
    /// 队列容量必须大于零，避免 Drop 补偿路径静默关闭。
    #[error("退款信号队列容量必须大于零")]
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

/// 接收 BillingSession RAII 补偿信号的内部非阻塞边界。
///
/// 队列只保存预留标识，不包含用户、额度或请求内容；Accepted 仅表示后台 worker
/// 已经取得补偿责任，真正持久化退款由消费者调用仓储完成。
#[derive(Clone)]
pub struct RefundSignalQueue {
    inner: Arc<QueueInner>,
}

impl RefundSignalQueue {
    /// 创建有界退款信号队列及其消费端。
    pub fn bounded(
        capacity: usize,
    ) -> Result<(Self, RefundSignalReceiver), RefundSignalQueueError> {
        if capacity == 0 {
            return Err(RefundSignalQueueError::ZeroCapacity);
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
            RefundSignalReceiver {
                queue: queue.clone(),
            },
        ))
    }

    /// 关闭接收入口；已接收但未确认的信号仍会被消费者排空。
    pub fn close(&self) {
        let mut state = lock_state(&self.inner);
        state.closed = true;
        self.inner.notify.notify_waiters();
    }

    /// 返回所有已接收退款信号是否都已被可靠确认。
    #[must_use]
    pub fn is_drained(&self) -> bool {
        lock_state(&self.inner).entries.is_empty()
    }
}

impl RefundSignalPort for RefundSignalQueue {
    fn try_signal_refund(&self, reservation_id: BillingReservationId) -> RefundSignalOutcome {
        let mut state = lock_state(&self.inner);
        if state.closed {
            return RefundSignalOutcome::Closed;
        }
        if state.entries.contains_key(&reservation_id) {
            return RefundSignalOutcome::Accepted;
        }
        if state.entries.len() >= self.inner.capacity {
            return RefundSignalOutcome::Saturated;
        }
        state.entries.insert(reservation_id, false);
        state.pending.push_back(reservation_id);
        drop(state);
        self.inner.notify.notify_one();
        RefundSignalOutcome::Accepted
    }
}

/// 退款信号队列的消费端。
#[derive(Clone)]
pub struct RefundSignalReceiver {
    queue: RefundSignalQueue,
}

impl RefundSignalReceiver {
    /// 非阻塞取出一条尚未确认的退款信号。
    pub fn try_take(&self) -> Option<RefundSignalDelivery> {
        let mut state = lock_state(&self.queue.inner);
        let reservation_id = state.pending.pop_front()?;
        let in_flight = state.entries.get_mut(&reservation_id)?;
        *in_flight = true;
        Some(RefundSignalDelivery {
            queue: self.queue.clone(),
            reservation_id,
            acknowledged: false,
        })
    }

    /// 等待并取出一条退款信号；队列关闭且没有待处理信号时返回 None。
    pub async fn recv(&self) -> Option<RefundSignalDelivery> {
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

/// 已从队列取出但尚未确认的退款信号。
pub struct RefundSignalDelivery {
    queue: RefundSignalQueue,
    reservation_id: BillingReservationId,
    acknowledged: bool,
}

impl RefundSignalDelivery {
    /// 返回需要执行内部补偿的预留标识。
    #[must_use]
    pub const fn reservation_id(&self) -> BillingReservationId {
        self.reservation_id
    }

    /// 确认下游已经可靠完成或接受该补偿结果。
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

impl Drop for RefundSignalDelivery {
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
            RefundSignalQueue::bounded(0),
            Err(RefundSignalQueueError::ZeroCapacity)
        ));
    }

    #[tokio::test]
    async fn duplicate_signal_is_idempotent_and_capacity_is_bounded() {
        let (queue, receiver) = RefundSignalQueue::bounded(1).unwrap();
        let first = reservation_id(1);
        assert_eq!(
            queue.try_signal_refund(first),
            RefundSignalOutcome::Accepted
        );
        assert_eq!(
            queue.try_signal_refund(first),
            RefundSignalOutcome::Accepted
        );
        assert_eq!(
            queue.try_signal_refund(reservation_id(2)),
            RefundSignalOutcome::Saturated
        );

        let delivery = receiver.recv().await.unwrap();
        assert_eq!(delivery.reservation_id(), first);
        assert!(delivery.acknowledge());
        assert!(queue.is_drained());
    }

    #[tokio::test]
    async fn unacknowledged_delivery_is_requeued() {
        let (queue, receiver) = RefundSignalQueue::bounded(1).unwrap();
        let id = reservation_id(3);
        assert_eq!(queue.try_signal_refund(id), RefundSignalOutcome::Accepted);
        drop(receiver.recv().await.unwrap());

        let delivery = receiver.recv().await.unwrap();
        assert_eq!(delivery.reservation_id(), id);
        assert!(delivery.acknowledge());
    }

    fn reservation_id(marker: u8) -> BillingReservationId {
        BillingReservationId::new([marker; 16]).unwrap()
    }
}
