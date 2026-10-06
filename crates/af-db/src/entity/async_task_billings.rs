//! 异步任务冻结、结算与失败释放的持久化协调实体。

use af_domain::{AsyncTaskId, BillingReservationId};
use sea_orm::entity::prelude::*;

use super::{BillingReservationKey, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "async_task_billings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Char(Some(32))")]
    pub task_key: SensitiveString,
    pub user_id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub reservation_key: BillingReservationKey,
    pub target_group_id: i64,
    pub state: i16,
    pub price_card_version: i16,
    pub billing_resolution: i16,
    pub group_ratio_micros: i64,
    pub group_model_ratio_micros: i64,
    pub peak_ratio_micros: i64,
    pub upper_bound: i64,
    pub rate_microusd: Option<i64>,
    pub fallback_quota: Option<i64>,
    pub actual_quota: Option<i64>,
    pub actual_duration_seconds: Option<i16>,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::async_task_submission_claims::Entity",
        from = "Column::TaskKey",
        to = "super::async_task_submission_claims::Column::TaskKey",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    SubmissionClaim,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::groups::Entity",
        from = "Column::TargetGroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    TargetGroup,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        // 所有状态推进都必须经过带版本条件的仓储 CAS。
        if !insert {
            return Err(DbErr::Custom("异步任务计费必须通过仓储更新".to_owned()));
        }
        let task_key = self.task_key.try_as_ref().map(SensitiveString::as_str);
        let reservation_key = self
            .reservation_key
            .try_as_ref()
            .map(BillingReservationKey::as_str);
        let ratios = [
            self.group_ratio_micros.try_as_ref().copied(),
            self.group_model_ratio_micros.try_as_ref().copied(),
            self.peak_ratio_micros.try_as_ref().copied(),
        ];
        if task_key.is_none_or(|value| AsyncTaskId::from_persistence_key(value).is_err())
            || reservation_key
                .is_none_or(|value| BillingReservationId::from_persistence_key(value).is_err())
            || self.user_id.try_as_ref().is_none_or(|value| *value <= 0)
            || self
                .target_group_id
                .try_as_ref()
                .is_none_or(|value| *value <= 0)
            || self.state.try_as_ref().copied() != Some(1)
            || self
                .price_card_version
                .try_as_ref()
                .is_none_or(|value| *value <= 0)
            || self
                .billing_resolution
                .try_as_ref()
                .is_none_or(|value| !(1..=3).contains(value))
            || ratios
                .into_iter()
                .any(|value| value.is_none_or(|value| value < 0))
            || self
                .upper_bound
                .try_as_ref()
                .is_none_or(|value| *value <= 0)
            || self.rate_microusd.try_as_ref().is_some_and(Option::is_some)
            || self
                .fallback_quota
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self.actual_quota.try_as_ref().is_some_and(Option::is_some)
            || self
                .actual_duration_seconds
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self.version.try_as_ref().copied() != Some(1)
        {
            return Err(DbErr::Custom("异步任务计费持久化字段无效".to_owned()));
        }
        Ok(self)
    }
}
