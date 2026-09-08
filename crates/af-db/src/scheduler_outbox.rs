use af_domain::{ChannelId, GroupId};
use sea_orm::{ActiveModelTrait, ConnectionTrait, DbErr, Set};
use thiserror::Error;

use crate::{DatabaseTimestamp, entity::scheduler_outbox_events};

mod repository;

pub use repository::{
    SchedulerOutboxClaimOutcome, SchedulerOutboxCompletionOutcome, SchedulerOutboxLease,
    SchedulerOutboxRepository, SchedulerOutboxRepositoryConfigError,
    SchedulerOutboxRepositoryError,
};

const SUBJECT_CHANNEL: i16 = 1;
const SUBJECT_GROUP: i16 = 2;
const STATUS_PENDING: i16 = 1;
const STATUS_LEASED: i16 = 2;
const STATUS_PUBLISHED: i16 = 3;
const INITIAL_VERSION: i64 = 1;

/// 调度目录变更事件的闭合主体。
///
/// 事件只携带稳定数据库标识。发布器按主体重新读取数据库真相，从而让重复投递、删除和
/// 多次变更合并都保持幂等，且不会把模型名或敏感运行时配置写入 outbox。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SchedulerCatalogSubject {
    /// 渠道配置、路由能力或所属凭据发生变化。
    Channel(ChannelId),
    /// 分组被删除，需要移除该分组的全部调度键。
    Group(GroupId),
}

impl SchedulerCatalogSubject {
    /// 从持久化判别值恢复闭合主体。
    pub fn try_from_parts(kind: i16, id: i64) -> Result<Self, SchedulerCatalogSubjectError> {
        match kind {
            SUBJECT_CHANNEL => ChannelId::new(id)
                .map(Self::Channel)
                .map_err(|_| SchedulerCatalogSubjectError::InvalidId),
            SUBJECT_GROUP => GroupId::new(id)
                .map(Self::Group)
                .map_err(|_| SchedulerCatalogSubjectError::InvalidId),
            _ => Err(SchedulerCatalogSubjectError::InvalidKind),
        }
    }

    const fn into_parts(self) -> (i16, i64) {
        match self {
            Self::Channel(channel_id) => (SUBJECT_CHANNEL, channel_id.get()),
            Self::Group(group_id) => (SUBJECT_GROUP, group_id.get()),
        }
    }
}

/// 调度 outbox 主体解码错误；不包含原始数据库内容。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SchedulerCatalogSubjectError {
    /// 持久化判别值不是受支持的渠道或分组主体。
    #[error("调度 outbox 主体类型无效")]
    InvalidKind,
    /// 主体数据库标识不是有效正整数。
    #[error("调度 outbox 主体标识无效")]
    InvalidId,
}

/// 在调用方现有事务内追加待投递事件；写入失败必须让业务事务整体回滚。
pub(crate) async fn enqueue_scheduler_catalog_change<C>(
    connection: &C,
    subject: SchedulerCatalogSubject,
    now: DatabaseTimestamp,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let (subject_kind, subject_id) = subject.into_parts();
    scheduler_outbox_events::ActiveModel {
        subject_kind: Set(subject_kind),
        subject_id: Set(subject_id),
        status: Set(STATUS_PENDING),
        attempt_count: Set(0),
        next_attempt_at: Set(now),
        lease_expires_at: Set(None),
        published_at: Set(None),
        version: Set(INITIAL_VERSION),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(connection)
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_contract_rejects_unknown_kinds_and_invalid_ids() {
        assert_eq!(
            SchedulerCatalogSubject::try_from_parts(1, 7),
            Ok(SchedulerCatalogSubject::Channel(ChannelId::new(7).unwrap()))
        );
        assert_eq!(
            SchedulerCatalogSubject::try_from_parts(2, 9),
            Ok(SchedulerCatalogSubject::Group(GroupId::new(9).unwrap()))
        );
        assert_eq!(
            SchedulerCatalogSubject::try_from_parts(3, 1),
            Err(SchedulerCatalogSubjectError::InvalidKind)
        );
        assert_eq!(
            SchedulerCatalogSubject::try_from_parts(1, 0),
            Err(SchedulerCatalogSubjectError::InvalidId)
        );
    }
}
