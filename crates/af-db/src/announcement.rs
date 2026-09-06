use std::time::Duration;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseTransaction, DbBackend,
    EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::LockType,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{announcements, users},
};

pub const MAX_ANNOUNCEMENT_PAGE_SIZE: usize = 100;
pub const MAX_ANNOUNCEMENT_TITLE_BYTES: usize = 160;
pub const MAX_ANNOUNCEMENT_BODY_BYTES: usize = 8 * 1_024;

const DRAFT: i16 = 1;
const PUBLISHED: i16 = 2;
const REVOKED: i16 = 3;
const ENABLED_USER_STATUS: i16 = 1;
const PUBLIC_AUDIENCE: i16 = 1;
const AUTHENTICATED_AUDIENCE: i16 = 2;

/// 公告公开端与站内通知使用的受众范围。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum AnnouncementAudienceRecord {
    Public = PUBLIC_AUDIENCE,
    Authenticated = AUTHENTICATED_AUDIENCE,
}

impl TryFrom<i16> for AnnouncementAudienceRecord {
    type Error = AnnouncementRepositoryError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            PUBLIC_AUDIENCE => Ok(Self::Public),
            AUTHENTICATED_AUDIENCE => Ok(Self::Authenticated),
            _ => Err(AnnouncementRepositoryError::Invariant),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum AnnouncementStatusRecord {
    Draft = DRAFT,
    Published = PUBLISHED,
    Revoked = REVOKED,
}

impl TryFrom<i16> for AnnouncementStatusRecord {
    type Error = AnnouncementRepositoryError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            DRAFT => Ok(Self::Draft),
            PUBLISHED => Ok(Self::Published),
            REVOKED => Ok(Self::Revoked),
            _ => Err(AnnouncementRepositoryError::Invariant),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnouncementWriteRecord {
    audience: AnnouncementAudienceRecord,
    title_zh: String,
    title_en: String,
    body_zh: String,
    body_en: String,
    visible_from: Option<TimeDateTimeWithTimeZone>,
    visible_until: Option<TimeDateTimeWithTimeZone>,
}

impl AnnouncementWriteRecord {
    pub fn new(
        title_zh: String,
        title_en: String,
        body_zh: String,
        body_en: String,
        visible_from: Option<TimeDateTimeWithTimeZone>,
        visible_until: Option<TimeDateTimeWithTimeZone>,
    ) -> Result<Self, AnnouncementRepositoryError> {
        Self::new_with_audience(
            AnnouncementAudienceRecord::Public,
            title_zh,
            title_en,
            body_zh,
            body_en,
            visible_from,
            visible_until,
        )
    }

    pub fn new_with_audience(
        audience: AnnouncementAudienceRecord,
        title_zh: String,
        title_en: String,
        body_zh: String,
        body_en: String,
        visible_from: Option<TimeDateTimeWithTimeZone>,
        visible_until: Option<TimeDateTimeWithTimeZone>,
    ) -> Result<Self, AnnouncementRepositoryError> {
        let record = Self {
            audience,
            title_zh,
            title_en,
            body_zh,
            body_en,
            visible_from,
            visible_until,
        };
        record.validate()?;
        Ok(record)
    }

    pub const fn audience(&self) -> AnnouncementAudienceRecord {
        self.audience
    }

    fn validate(&self) -> Result<(), AnnouncementRepositoryError> {
        if !valid_title(&self.title_zh)
            || !valid_title(&self.title_en)
            || !valid_body(&self.body_zh)
            || !valid_body(&self.body_en)
            || self
                .visible_until
                .zip(self.visible_from)
                .is_some_and(|(until, from)| until <= from)
        {
            return Err(AnnouncementRepositoryError::InvalidInput);
        }
        Ok(())
    }

    pub fn title_zh(&self) -> &str {
        &self.title_zh
    }

    pub fn title_en(&self) -> &str {
        &self.title_en
    }

    pub fn body_zh(&self) -> &str {
        &self.body_zh
    }

    pub fn body_en(&self) -> &str {
        &self.body_en
    }

    pub const fn visible_from(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.visible_from
    }

    pub const fn visible_until(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.visible_until
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnouncementRecord {
    id: i64,
    version: i64,
    status: AnnouncementStatusRecord,
    audience: AnnouncementAudienceRecord,
    title_zh: String,
    title_en: String,
    body_zh: String,
    body_en: String,
    visible_from: Option<TimeDateTimeWithTimeZone>,
    visible_until: Option<TimeDateTimeWithTimeZone>,
    created_by: i64,
    published_at: Option<TimeDateTimeWithTimeZone>,
    revoked_at: Option<TimeDateTimeWithTimeZone>,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
}

impl AnnouncementRecord {
    pub const fn id(&self) -> i64 {
        self.id
    }
    pub const fn version(&self) -> i64 {
        self.version
    }
    pub const fn status(&self) -> AnnouncementStatusRecord {
        self.status
    }
    pub const fn audience(&self) -> AnnouncementAudienceRecord {
        self.audience
    }
    pub fn title_zh(&self) -> &str {
        &self.title_zh
    }
    pub fn title_en(&self) -> &str {
        &self.title_en
    }
    pub fn body_zh(&self) -> &str {
        &self.body_zh
    }
    pub fn body_en(&self) -> &str {
        &self.body_en
    }
    pub const fn visible_from(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.visible_from
    }
    pub const fn visible_until(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.visible_until
    }
    pub const fn created_by(&self) -> i64 {
        self.created_by
    }
    pub const fn published_at(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.published_at
    }
    pub const fn revoked_at(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.revoked_at
    }
    pub const fn created_at(&self) -> TimeDateTimeWithTimeZone {
        self.created_at
    }
    pub const fn updated_at(&self) -> TimeDateTimeWithTimeZone {
        self.updated_at
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AnnouncementRepositoryConfigError {
    #[error("公告数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AnnouncementRepositoryError {
    #[error("公告数据库操作失败")]
    Query,
    #[error("公告数据库操作超时")]
    Timeout,
    #[error("公告持久化状态损坏")]
    Invariant,
    #[error("公告字段无效")]
    InvalidInput,
    #[error("公告版本冲突")]
    Conflict,
    #[error("公告不存在")]
    NotFound,
}

#[derive(Clone)]
pub struct AnnouncementRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl AnnouncementRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, AnnouncementRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(AnnouncementRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    pub async fn list_admin(&self) -> Result<Vec<AnnouncementRecord>, AnnouncementRepositoryError> {
        match timeout(self.operation_timeout, self.list_admin_inner()).await {
            Ok(result) => result,
            Err(_) => Err(AnnouncementRepositoryError::Timeout),
        }
    }

    pub async fn list_public(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<Vec<AnnouncementRecord>, AnnouncementRepositoryError> {
        match timeout(self.operation_timeout, self.list_public_inner(now)).await {
            Ok(result) => result,
            Err(_) => Err(AnnouncementRepositoryError::Timeout),
        }
    }

    pub async fn create(
        &self,
        created_by: i64,
        record: AnnouncementWriteRecord,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.create_inner(created_by, record),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AnnouncementRepositoryError::Timeout),
        }
    }

    pub async fn update_draft(
        &self,
        id: i64,
        expected_version: i64,
        record: AnnouncementWriteRecord,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.update_draft_inner(id, expected_version, record),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AnnouncementRepositoryError::Timeout),
        }
    }

    pub async fn publish(
        &self,
        id: i64,
        expected_version: i64,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.transition_inner(id, expected_version, DRAFT, PUBLISHED, now),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AnnouncementRepositoryError::Timeout),
        }
    }

    pub async fn revoke(
        &self,
        id: i64,
        expected_version: i64,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.transition_inner(id, expected_version, PUBLISHED, REVOKED, now),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AnnouncementRepositoryError::Timeout),
        }
    }

    async fn list_admin_inner(
        &self,
    ) -> Result<Vec<AnnouncementRecord>, AnnouncementRepositoryError> {
        let models = announcements::Entity::find()
            .order_by_desc(announcements::Column::UpdatedAt)
            .order_by_desc(announcements::Column::Id)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AnnouncementRepositoryError::Query)?;
        models.into_iter().map(record_from_model).collect()
    }

    async fn list_public_inner(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<Vec<AnnouncementRecord>, AnnouncementRepositoryError> {
        let visible_from = Condition::any()
            .add(announcements::Column::VisibleFrom.is_null())
            .add(announcements::Column::VisibleFrom.lte(now));
        let visible_until = Condition::any()
            .add(announcements::Column::VisibleUntil.is_null())
            .add(announcements::Column::VisibleUntil.gt(now));
        let models = announcements::Entity::find()
            .filter(announcements::Column::Status.eq(PUBLISHED))
            .filter(announcements::Column::Audience.eq(PUBLIC_AUDIENCE))
            .filter(visible_from)
            .filter(visible_until)
            .order_by_desc(announcements::Column::PublishedAt)
            .order_by_desc(announcements::Column::Id)
            .limit(MAX_ANNOUNCEMENT_PAGE_SIZE as u64)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AnnouncementRepositoryError::Query)?;
        models.into_iter().map(record_from_model).collect()
    }

    async fn create_inner(
        &self,
        created_by: i64,
        record: AnnouncementWriteRecord,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        if created_by <= 0 {
            return Err(AnnouncementRepositoryError::InvalidInput);
        }
        record.validate()?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = announcements::ActiveModel {
            id: Default::default(),
            version: Set(1),
            status: Set(DRAFT),
            audience: Set(record.audience as i16),
            title_zh: Set(record.title_zh),
            title_en: Set(record.title_en),
            body_zh: Set(record.body_zh),
            body_en: Set(record.body_en),
            visible_from: Set(record.visible_from),
            visible_until: Set(record.visible_until),
            created_by: Set(created_by),
            published_at: Set(None),
            revoked_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(self.pool.connection())
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AnnouncementRepositoryError::Query)?;
        record_from_model(model)
    }

    async fn update_draft_inner(
        &self,
        id: i64,
        expected_version: i64,
        record: AnnouncementWriteRecord,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        if id <= 0 || expected_version <= 0 || expected_version == i64::MAX {
            return Err(AnnouncementRepositoryError::InvalidInput);
        }
        record.validate()?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AnnouncementRepositoryError::Query)?;
        let existing = lock_row(&transaction, id).await?;
        if existing.version != expected_version || existing.status != DRAFT {
            return Err(AnnouncementRepositoryError::Conflict);
        }
        let mut active = existing.into_active_model();
        active.version = Set(expected_version + 1);
        active.audience = Set(record.audience as i16);
        active.title_zh = Set(record.title_zh);
        active.title_en = Set(record.title_en);
        active.body_zh = Set(record.body_zh);
        active.body_en = Set(record.body_en);
        active.visible_from = Set(record.visible_from);
        active.visible_until = Set(record.visible_until);
        active.updated_at = Set(TimeDateTimeWithTimeZone::now_utc());
        let saved = active
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AnnouncementRepositoryError::Query)?;
        let result = record_from_model(saved)?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AnnouncementRepositoryError::Query)?;
        Ok(result)
    }

    async fn transition_inner(
        &self,
        id: i64,
        expected_version: i64,
        from: i16,
        to: i16,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        if id <= 0 || expected_version <= 0 || expected_version == i64::MAX {
            return Err(AnnouncementRepositoryError::InvalidInput);
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AnnouncementRepositoryError::Query)?;
        let existing = lock_row(&transaction, id).await?;
        if existing.version != expected_version || existing.status != from {
            return Err(AnnouncementRepositoryError::Conflict);
        }
        let mut active = existing.into_active_model();
        active.version = Set(expected_version + 1);
        active.status = Set(to);
        if to == PUBLISHED {
            active.published_at = Set(Some(now));
        }
        if to == REVOKED {
            active.revoked_at = Set(Some(now));
        }
        active.updated_at = Set(now);
        let saved = active
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AnnouncementRepositoryError::Query)?;
        let result = record_from_model(saved)?;
        if to == PUBLISHED && is_visible_at(&result, now) {
            let recipients = users::Entity::find()
                .filter(users::Column::Status.eq(ENABLED_USER_STATUS))
                .filter(users::Column::DeletedAt.is_null())
                .all(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| AnnouncementRepositoryError::Query)?;
            for recipient in recipients {
                crate::notification::insert_product_update(
                    &transaction,
                    &crate::notification::UserNotificationWrite::product_update(
                        recipient.id,
                        result.id(),
                        result.version(),
                        now,
                    ),
                )
                .await
                .map_err(|_| AnnouncementRepositoryError::Query)?;
            }
        }
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AnnouncementRepositoryError::Query)?;
        Ok(result)
    }
}

fn is_visible_at(record: &AnnouncementRecord, now: TimeDateTimeWithTimeZone) -> bool {
    record.visible_from().is_none_or(|value| value <= now)
        && record.visible_until().is_none_or(|value| value > now)
}

async fn lock_row(
    transaction: &DatabaseTransaction,
    id: i64,
) -> Result<announcements::Model, AnnouncementRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = announcements::Entity::update_many()
            .filter(announcements::Column::Id.eq(id))
            .col_expr(
                announcements::Column::Version,
                sea_orm::sea_query::Expr::col(announcements::Column::Version).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AnnouncementRepositoryError::Query)?;
        if result.rows_affected != 1 {
            return Err(AnnouncementRepositoryError::NotFound);
        }
    }
    let mut query = announcements::Entity::find().filter(announcements::Column::Id.eq(id));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AnnouncementRepositoryError::Query)?
        .ok_or(AnnouncementRepositoryError::NotFound)
}

fn record_from_model(
    model: announcements::Model,
) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
    if model.id <= 0
        || model.version <= 0
        || model.created_by <= 0
        || AnnouncementAudienceRecord::try_from(model.audience).is_err()
        || !valid_title(&model.title_zh)
        || !valid_title(&model.title_en)
        || !valid_body(&model.body_zh)
        || !valid_body(&model.body_en)
        || model
            .visible_until
            .zip(model.visible_from)
            .is_some_and(|(until, from)| until <= from)
    {
        return Err(AnnouncementRepositoryError::Invariant);
    }
    Ok(AnnouncementRecord {
        id: model.id,
        version: model.version,
        status: AnnouncementStatusRecord::try_from(model.status)?,
        audience: AnnouncementAudienceRecord::try_from(model.audience)?,
        title_zh: model.title_zh,
        title_en: model.title_en,
        body_zh: model.body_zh,
        body_en: model.body_en,
        visible_from: model.visible_from,
        visible_until: model.visible_until,
        created_by: model.created_by,
        published_at: model.published_at,
        revoked_at: model.revoked_at,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

fn valid_title(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ANNOUNCEMENT_TITLE_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_body(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ANNOUNCEMENT_BODY_BYTES
        && value.trim() == value
        && !value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatabaseTimestamp;

    #[test]
    fn status_codes_are_closed() {
        assert_eq!(
            AnnouncementStatusRecord::try_from(1),
            Ok(AnnouncementStatusRecord::Draft)
        );
        assert_eq!(
            AnnouncementStatusRecord::try_from(2),
            Ok(AnnouncementStatusRecord::Published)
        );
        assert_eq!(
            AnnouncementStatusRecord::try_from(3),
            Ok(AnnouncementStatusRecord::Revoked)
        );
        assert_eq!(
            AnnouncementStatusRecord::try_from(0),
            Err(AnnouncementRepositoryError::Invariant)
        );
        assert_eq!(
            AnnouncementStatusRecord::try_from(4),
            Err(AnnouncementRepositoryError::Invariant)
        );
    }

    #[test]
    fn audience_codes_are_closed() {
        assert_eq!(
            AnnouncementAudienceRecord::try_from(1),
            Ok(AnnouncementAudienceRecord::Public)
        );
        assert_eq!(
            AnnouncementAudienceRecord::try_from(2),
            Ok(AnnouncementAudienceRecord::Authenticated)
        );
        assert_eq!(
            AnnouncementAudienceRecord::try_from(3),
            Err(AnnouncementRepositoryError::Invariant)
        );
    }

    #[test]
    fn write_record_enforces_content_and_time_window_boundaries() {
        let valid = AnnouncementWriteRecord::new(
            "标题".to_owned(),
            "Title".to_owned(),
            "正文\n可换行".to_owned(),
            "Body\nwith lines".to_owned(),
            Some(DatabaseTimestamp::from_unix_timestamp(100).expect("时间有效")),
            Some(DatabaseTimestamp::from_unix_timestamp(200).expect("时间有效")),
        );
        assert!(valid.is_ok());
        assert!(
            AnnouncementWriteRecord::new(
                " 标题".to_owned(),
                "Title".to_owned(),
                "Body".to_owned(),
                "Body".to_owned(),
                None,
                None,
            )
            .is_err()
        );
        assert!(
            AnnouncementWriteRecord::new(
                "标题".to_owned(),
                "Title".to_owned(),
                "正文\u{0000}".to_owned(),
                "Body".to_owned(),
                None,
                None,
            )
            .is_err()
        );
        assert!(
            AnnouncementWriteRecord::new(
                "标题".to_owned(),
                "Title".to_owned(),
                "正文".to_owned(),
                "Body".to_owned(),
                Some(DatabaseTimestamp::from_unix_timestamp(200).expect("时间有效")),
                Some(DatabaseTimestamp::from_unix_timestamp(100).expect("时间有效")),
            )
            .is_err()
        );
    }
}
