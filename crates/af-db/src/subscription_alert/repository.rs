use std::{fmt, future::Future, time::Duration};

use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;
use tokio::time::timeout;

use crate::{
    DatabasePool,
    balance_alert::{BalanceAlertCompletionOutcome, BalanceAlertDeliveryFailureKind},
};

use super::{
    claim, completion, discovery,
    types::{
        SubscriptionBalanceAlertClaimOutcome, SubscriptionBalanceAlertDeliveryLease,
        SubscriptionBalanceAlertEnqueueReport, SubscriptionBalanceAlertRepositoryConfigError,
        SubscriptionBalanceAlertRepositoryError,
    },
};

/// 负责订阅窗口低余额候选去重、租约领取和投递结果推进的数据库仓储。
#[derive(Clone)]
pub struct SubscriptionBalanceAlertRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl SubscriptionBalanceAlertRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, SubscriptionBalanceAlertRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(SubscriptionBalanceAlertRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 扫描一批低剩余额度订阅，并按订阅实例与窗口唯一键写入待投递事件。
    pub async fn enqueue_due(
        &self,
        threshold_percent: i16,
        now: TimeDateTimeWithTimeZone,
        limit: usize,
    ) -> Result<SubscriptionBalanceAlertEnqueueReport, SubscriptionBalanceAlertRepositoryError>
    {
        if limit == 0 || limit > crate::MAX_BALANCE_ALERT_BATCH_SIZE {
            return Err(SubscriptionBalanceAlertRepositoryError::InvalidBatchSize);
        }
        if !(1..=99).contains(&threshold_percent) {
            return Err(SubscriptionBalanceAlertRepositoryError::InvalidThreshold);
        }
        self.run(discovery::enqueue_due(
            &self.pool,
            threshold_percent,
            now,
            limit,
        ))
        .await
    }

    /// 领取一条到期事件；失效用户或订阅会在事务内取消而不返回身份信息。
    pub async fn claim_next(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<SubscriptionBalanceAlertClaimOutcome, SubscriptionBalanceAlertRepositoryError> {
        self.run(claim::claim_next(&self.pool, now)).await
    }

    /// 取消已经结束窗口的待投递事件，避免过期提醒占用领取预算。
    pub async fn cancel_expired(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<u64, SubscriptionBalanceAlertRepositoryError> {
        self.run(claim::cancel_expired(&self.pool, now)).await
    }

    /// 将耗尽最大投递次数且租约已结束的事件推进到终态失败。
    pub async fn fail_exhausted(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<u64, SubscriptionBalanceAlertRepositoryError> {
        self.run(claim::fail_exhausted(&self.pool, now)).await
    }

    /// 使用租约版本把事件推进到已发送；陈旧投递不会覆盖新状态。
    pub async fn mark_sent(
        &self,
        lease: &SubscriptionBalanceAlertDeliveryLease,
        sent_at: TimeDateTimeWithTimeZone,
    ) -> Result<BalanceAlertCompletionOutcome, SubscriptionBalanceAlertRepositoryError> {
        self.run(completion::mark_sent(&self.pool, lease, sent_at))
            .await
    }

    /// 持久化一次投递失败，并按共享退避表决定重试或终止。
    pub async fn record_failure(
        &self,
        lease: &SubscriptionBalanceAlertDeliveryLease,
        failure: BalanceAlertDeliveryFailureKind,
        failed_at: TimeDateTimeWithTimeZone,
    ) -> Result<BalanceAlertCompletionOutcome, SubscriptionBalanceAlertRepositoryError> {
        self.run(completion::record_failure(
            &self.pool, lease, failure, failed_at,
        ))
        .await
    }

    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, SubscriptionBalanceAlertRepositoryError>>,
    ) -> Result<T, SubscriptionBalanceAlertRepositoryError> {
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                SubscriptionBalanceAlertRepositoryError::Timeout,
            )),
        }
    }
}

fn record_internal_error(
    error: SubscriptionBalanceAlertRepositoryError,
) -> SubscriptionBalanceAlertRepositoryError {
    let error_kind = match error {
        SubscriptionBalanceAlertRepositoryError::InvalidBatchSize
        | SubscriptionBalanceAlertRepositoryError::InvalidThreshold => return error,
        SubscriptionBalanceAlertRepositoryError::Query => "subscription_balance_alert_query",
        SubscriptionBalanceAlertRepositoryError::Timeout => "subscription_balance_alert_timeout",
        SubscriptionBalanceAlertRepositoryError::Invariant => {
            "subscription_balance_alert_invariant"
        }
    };
    tracing::error!(
        target: "af_db::subscription_alert",
        error_kind,
        "订阅窗口预警事件仓储发生内部错误"
    );
    error
}

impl fmt::Debug for SubscriptionBalanceAlertRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionBalanceAlertRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}
