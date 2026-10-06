use af_db::{SchedulerAbilityRepository, SchedulerAbilityRepositoryError};
use af_domain::GroupId;
use thiserror::Error;

use crate::{
    SchedulerCandidate, WeightedRetryPlan, WeightedRetryPlanError, WeightedSelection,
    WeightedSelectionError, WeightedStrategy,
};

/// 数据库能力查询与默认 weighted 策略组合成的 M1 调度入口。
#[derive(Clone, Debug)]
pub struct DatabaseWeightedScheduler {
    repository: SchedulerAbilityRepository,
    strategy: WeightedStrategy,
}

#[cfg(test)]
mod tests {
    use af_domain::{ChannelId, GroupId};

    use super::*;

    #[test]
    fn database_error_type_wraps_retry_plan_failures() {
        let error = DatabaseWeightedSchedulerError::from(WeightedRetryPlanError::DuplicateChannel);

        assert!(matches!(
            error,
            DatabaseWeightedSchedulerError::RetryPlan(WeightedRetryPlanError::DuplicateChannel)
        ));
    }

    #[test]
    fn retry_plan_from_candidates_reuses_scheduler_retry_semantics() {
        let mut plan = WeightedRetryPlan::new(vec![
            SchedulerCandidate::new(group(1), channel(1), 9, 0),
            SchedulerCandidate::new(group(1), channel(2), 1, 0),
        ])
        .unwrap();

        assert_eq!(plan.select_next().unwrap().unwrap().priority_rank(), 0);
        assert_eq!(plan.select_next().unwrap().unwrap().priority_rank(), 1);
        assert_eq!(plan.select_next().unwrap(), None);
    }

    fn group(value: i64) -> GroupId {
        GroupId::new(value).unwrap()
    }

    fn channel(value: i64) -> ChannelId {
        ChannelId::new(value).unwrap()
    }
}

impl DatabaseWeightedScheduler {
    /// 使用已配置的能力仓储创建调度入口。
    #[must_use]
    pub const fn new(repository: SchedulerAbilityRepository) -> Self {
        Self {
            repository,
            strategy: WeightedStrategy,
        }
    }

    /// 查询一个分组与 Canonical 模型的可用渠道，并从最高优先级层加权选择。
    pub async fn select(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<Option<WeightedSelection>, DatabaseWeightedSchedulerError> {
        self.select_at_priority_rank(group_id, model, 0).await
    }

    /// 查询能力后从指定去重优先级层选择，后续失败重试可直接逐层调用。
    pub async fn select_at_priority_rank(
        &self,
        group_id: GroupId,
        model: &str,
        priority_rank: usize,
    ) -> Result<Option<WeightedSelection>, DatabaseWeightedSchedulerError> {
        let candidates = self.load_candidates(group_id, model).await?;
        self.strategy
            .select_at_priority_rank(&candidates, priority_rank)
            .map_err(Into::into)
    }

    /// 一次加载能力候选并创建失败重试计划，避免每次重试重复查询数据库。
    pub async fn retry_plan(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<WeightedRetryPlan, DatabaseWeightedSchedulerError> {
        let candidates = self.load_candidates(group_id, model).await?;
        WeightedRetryPlan::new(candidates).map_err(Into::into)
    }

    async fn load_candidates(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<Vec<SchedulerCandidate>, DatabaseWeightedSchedulerError> {
        self.repository
            .load(group_id, model)
            .await
            .map(|records| {
                records
                    .into_iter()
                    .map(|record| {
                        SchedulerCandidate::new(
                            record.group_id(),
                            record.channel_id(),
                            record.priority(),
                            record.weight(),
                        )
                    })
                    .collect()
            })
            .map_err(Into::into)
    }
}

/// 数据库 weighted 调度失败；模型名和持久化诊断不会进入错误文本。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DatabaseWeightedSchedulerError {
    /// 能力查询失败、超时、超量或读取到损坏状态。
    #[error("读取可调度能力失败")]
    Repository(#[from] SchedulerAbilityRepositoryError),
    /// weighted 策略无法安全完成选择。
    #[error("选择上游渠道失败")]
    Selection(#[from] WeightedSelectionError),
    /// 候选集合无法构造安全的失败重试计划。
    #[error("构造上游重试计划失败")]
    RetryPlan(#[from] WeightedRetryPlanError),
}
