use std::future::Future;

use af_db::{
    SubscriptionExpirationDueCursor, SubscriptionRepository, SubscriptionRepositoryError,
    SubscriptionResetDueCursor, UserSubscriptionLifecycleTransition,
    UserSubscriptionLifecycleTransitionOutcome, UserSubscriptionWindowAdvance,
    UserSubscriptionWindowAdvanceOutcome,
};
use af_domain::UserSubscriptionStatus;

use super::{
    SubscriptionCycleMutationOutcome, SubscriptionExpirationBatch, SubscriptionWindowAdvanceBatch,
};

/// 订阅周期执行器依赖的最小持久化端口。
///
/// 加载方法必须使用传入的同一个 `now` 构造批次内所有命令。写入结果未知时执行器会
/// 保留命令对象并原样重放一次，端口实现不得在调用间重建时间或版本事实。
pub trait SubscriptionCycleStore: Send + Sync {
    /// 按稳定游标加载一批到期 Active 订阅窗口推进命令。
    fn load_window_advances(
        &self,
        now: u64,
        after: Option<SubscriptionResetDueCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<SubscriptionWindowAdvanceBatch, SubscriptionRepositoryError>> + Send;

    /// 执行或重放一条窗口推进命令。
    fn advance_window(
        &self,
        command: &UserSubscriptionWindowAdvance,
    ) -> impl Future<Output = Result<SubscriptionCycleMutationOutcome, SubscriptionRepositoryError>> + Send;

    /// 按稳定游标加载一批到期 Canceled 订阅过期命令。
    fn load_expirations(
        &self,
        now: u64,
        after: Option<SubscriptionExpirationDueCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<SubscriptionExpirationBatch, SubscriptionRepositoryError>> + Send;

    /// 执行或重放一条取消订阅过期命令。
    fn expire_subscription(
        &self,
        command: &UserSubscriptionLifecycleTransition,
    ) -> impl Future<Output = Result<SubscriptionCycleMutationOutcome, SubscriptionRepositoryError>> + Send;
}

impl SubscriptionCycleStore for SubscriptionRepository {
    async fn load_window_advances(
        &self,
        now: u64,
        after: Option<SubscriptionResetDueCursor>,
        limit: usize,
    ) -> Result<SubscriptionWindowAdvanceBatch, SubscriptionRepositoryError> {
        let page = self.list_reset_due_subscriptions(now, after, limit).await?;
        let commands = page
            .subscriptions()
            .iter()
            .map(|record| {
                UserSubscriptionWindowAdvance::from_record(record, now)
                    .map_err(|_| SubscriptionRepositoryError::Invariant)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SubscriptionWindowAdvanceBatch::new(
            commands,
            page.next_cursor(),
        ))
    }

    async fn advance_window(
        &self,
        command: &UserSubscriptionWindowAdvance,
    ) -> Result<SubscriptionCycleMutationOutcome, SubscriptionRepositoryError> {
        self.advance_user_subscription_window(command)
            .await
            .map(|outcome| match outcome {
                UserSubscriptionWindowAdvanceOutcome::Applied(_) => {
                    SubscriptionCycleMutationOutcome::Applied
                }
                UserSubscriptionWindowAdvanceOutcome::Existing(_) => {
                    SubscriptionCycleMutationOutcome::Existing
                }
                UserSubscriptionWindowAdvanceOutcome::NotFound
                | UserSubscriptionWindowAdvanceOutcome::NotDue(_)
                | UserSubscriptionWindowAdvanceOutcome::InUse(_)
                | UserSubscriptionWindowAdvanceOutcome::Inactive(_) => {
                    SubscriptionCycleMutationOutcome::Skipped
                }
            })
    }

    async fn load_expirations(
        &self,
        now: u64,
        after: Option<SubscriptionExpirationDueCursor>,
        limit: usize,
    ) -> Result<SubscriptionExpirationBatch, SubscriptionRepositoryError> {
        let page = self
            .list_expiration_due_subscriptions(now, after, limit)
            .await?;
        let commands = page
            .subscriptions()
            .iter()
            .map(|record| {
                UserSubscriptionLifecycleTransition::from_record(
                    record,
                    UserSubscriptionStatus::Expired,
                    now,
                )
                .map_err(|_| SubscriptionRepositoryError::Invariant)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SubscriptionExpirationBatch::new(
            commands,
            page.next_cursor(),
        ))
    }

    async fn expire_subscription(
        &self,
        command: &UserSubscriptionLifecycleTransition,
    ) -> Result<SubscriptionCycleMutationOutcome, SubscriptionRepositoryError> {
        self.transition_user_subscription_lifecycle(command)
            .await
            .map(|outcome| match outcome {
                UserSubscriptionLifecycleTransitionOutcome::Applied(_) => {
                    SubscriptionCycleMutationOutcome::Applied
                }
                UserSubscriptionLifecycleTransitionOutcome::Existing(_) => {
                    SubscriptionCycleMutationOutcome::Existing
                }
                UserSubscriptionLifecycleTransitionOutcome::NotFound
                | UserSubscriptionLifecycleTransitionOutcome::NotDue(_)
                | UserSubscriptionLifecycleTransitionOutcome::InUse(_) => {
                    SubscriptionCycleMutationOutcome::Skipped
                }
            })
    }
}
