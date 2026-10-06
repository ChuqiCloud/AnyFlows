use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    AnnouncementAudienceRecord, AnnouncementRecord, AnnouncementRepository,
    AnnouncementRepositoryError, AnnouncementStatusRecord, AnnouncementWriteRecord,
    DatabaseTimestamp,
};
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理员提交的公告正文与可见时间窗口。
pub struct AnnouncementWriteCommand {
    audience: AnnouncementAudience,
    title_zh: String,
    title_en: String,
    body_zh: String,
    body_en: String,
    visible_from: Option<i64>,
    visible_until: Option<i64>,
}

impl AnnouncementWriteCommand {
    /// 将 Unix 秒转换为数据库时间并复用仓储边界校验。
    pub fn new(
        title_zh: String,
        title_en: String,
        body_zh: String,
        body_en: String,
        visible_from: Option<i64>,
        visible_until: Option<i64>,
    ) -> Result<Self, AnnouncementServiceError> {
        Self::new_with_audience(
            AnnouncementAudience::Public,
            title_zh,
            title_en,
            body_zh,
            body_en,
            visible_from,
            visible_until,
        )
    }

    /// 构造指定受众范围的公告写入命令。
    pub fn new_with_audience(
        audience: AnnouncementAudience,
        title_zh: String,
        title_en: String,
        body_zh: String,
        body_en: String,
        visible_from: Option<i64>,
        visible_until: Option<i64>,
    ) -> Result<Self, AnnouncementServiceError> {
        let visible_from = visible_from
            .map(DatabaseTimestamp::from_unix_timestamp)
            .transpose()
            .map_err(|_| AnnouncementServiceError::InvalidRequest)?;
        let visible_until = visible_until
            .map(DatabaseTimestamp::from_unix_timestamp)
            .transpose()
            .map_err(|_| AnnouncementServiceError::InvalidRequest)?;
        AnnouncementWriteRecord::new_with_audience(
            audience.into_record(),
            title_zh.clone(),
            title_en.clone(),
            body_zh.clone(),
            body_en.clone(),
            visible_from,
            visible_until,
        )
        .map_err(|_| AnnouncementServiceError::InvalidRequest)?;
        Ok(Self {
            audience,
            title_zh,
            title_en,
            body_zh,
            body_en,
            visible_from: visible_from.map(|value| value.unix_timestamp()),
            visible_until: visible_until.map(|value| value.unix_timestamp()),
        })
    }

    fn into_record(self) -> Result<AnnouncementWriteRecord, AnnouncementServiceError> {
        AnnouncementWriteRecord::new_with_audience(
            self.audience.into_record(),
            self.title_zh,
            self.title_en,
            self.body_zh,
            self.body_en,
            self.visible_from
                .map(DatabaseTimestamp::from_unix_timestamp)
                .transpose()
                .map_err(|_| AnnouncementServiceError::InvalidRequest)?,
            self.visible_until
                .map(DatabaseTimestamp::from_unix_timestamp)
                .transpose()
                .map_err(|_| AnnouncementServiceError::InvalidRequest)?,
        )
        .map_err(|_| AnnouncementServiceError::InvalidRequest)
    }
}

/// 公告是否可被未登录访客读取；登录用户两种范围都会产生站内投影。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnouncementAudience {
    Public,
    Authenticated,
}

impl AnnouncementAudience {
    fn into_record(self) -> AnnouncementAudienceRecord {
        match self {
            Self::Public => AnnouncementAudienceRecord::Public,
            Self::Authenticated => AnnouncementAudienceRecord::Authenticated,
        }
    }
}

impl fmt::Debug for AnnouncementWriteCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AnnouncementWriteCommand(<redacted>)")
    }
}

/// 公告当前的服务端生命周期状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnouncementStatus {
    Draft,
    Published,
    Revoked,
}

/// 公告对管理端和公开端共享的只读投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnouncementView {
    id: i64,
    version: i64,
    status: AnnouncementStatus,
    audience: AnnouncementAudience,
    title_zh: String,
    title_en: String,
    body_zh: String,
    body_en: String,
    visible_from: Option<i64>,
    visible_until: Option<i64>,
    created_by: i64,
    published_at: Option<i64>,
    revoked_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

impl AnnouncementView {
    fn from_record(record: AnnouncementRecord) -> Result<Self, AnnouncementServiceError> {
        let status = match record.status() {
            AnnouncementStatusRecord::Draft => AnnouncementStatus::Draft,
            AnnouncementStatusRecord::Published => AnnouncementStatus::Published,
            AnnouncementStatusRecord::Revoked => AnnouncementStatus::Revoked,
        };
        let audience = match record.audience() {
            AnnouncementAudienceRecord::Public => AnnouncementAudience::Public,
            AnnouncementAudienceRecord::Authenticated => AnnouncementAudience::Authenticated,
        };
        Ok(Self {
            id: record.id(),
            version: record.version(),
            status,
            audience,
            title_zh: record.title_zh().to_owned(),
            title_en: record.title_en().to_owned(),
            body_zh: record.body_zh().to_owned(),
            body_en: record.body_en().to_owned(),
            visible_from: record.visible_from().map(|value| value.unix_timestamp()),
            visible_until: record.visible_until().map(|value| value.unix_timestamp()),
            created_by: record.created_by(),
            published_at: record.published_at().map(|value| value.unix_timestamp()),
            revoked_at: record.revoked_at().map(|value| value.unix_timestamp()),
            created_at: record.created_at().unix_timestamp(),
            updated_at: record.updated_at().unix_timestamp(),
        })
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn status(&self) -> AnnouncementStatus {
        self.status
    }
    #[must_use]
    pub const fn audience(&self) -> AnnouncementAudience {
        self.audience
    }
    #[must_use]
    pub fn title_zh(&self) -> &str {
        &self.title_zh
    }
    #[must_use]
    pub fn title_en(&self) -> &str {
        &self.title_en
    }
    #[must_use]
    pub fn body_zh(&self) -> &str {
        &self.body_zh
    }
    #[must_use]
    pub fn body_en(&self) -> &str {
        &self.body_en
    }
    #[must_use]
    pub const fn visible_from(&self) -> Option<i64> {
        self.visible_from
    }
    #[must_use]
    pub const fn visible_until(&self) -> Option<i64> {
        self.visible_until
    }
    #[must_use]
    pub const fn created_by(&self) -> i64 {
        self.created_by
    }
    #[must_use]
    pub const fn published_at(&self) -> Option<i64> {
        self.published_at
    }
    #[must_use]
    pub const fn revoked_at(&self) -> Option<i64> {
        self.revoked_at
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AnnouncementServiceError {
    #[error("公告请求无效")]
    InvalidRequest,
    #[error("公告权限不足")]
    Forbidden,
    #[error("公告不存在")]
    NotFound,
    #[error("公告版本冲突")]
    Conflict,
    #[error("公告内部失败")]
    Internal,
}

pub type AnnouncementListFuture<'a> = Pin<
    Box<dyn Future<Output = Result<Vec<AnnouncementView>, AnnouncementServiceError>> + Send + 'a>,
>;
pub type AnnouncementFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AnnouncementView, AnnouncementServiceError>> + Send + 'a>>;

/// 公告公开读取与管理员版本化写入端口。
pub trait AnnouncementService: Send + Sync {
    /// 只返回当前已发布且处于可见窗口内的公告。
    fn list_public(&self) -> AnnouncementListFuture<'_>;
    /// 管理员读取所有草稿、已发布和已撤回版本。
    fn list_admin(&self, principal: SessionPrincipal) -> AnnouncementListFuture<'_>;
    /// 创建版本为 1 的草稿。
    fn create(
        &self,
        principal: SessionPrincipal,
        command: AnnouncementWriteCommand,
    ) -> AnnouncementFuture<'_>;
    /// 仅允许按版本 CAS 修改草稿。
    fn update_draft(
        &self,
        principal: SessionPrincipal,
        id: i64,
        expected_version: i64,
        command: AnnouncementWriteCommand,
    ) -> AnnouncementFuture<'_>;
    /// 将草稿发布为新版本。
    fn publish(
        &self,
        principal: SessionPrincipal,
        id: i64,
        expected_version: i64,
    ) -> AnnouncementFuture<'_>;
    /// 将已发布版本撤回为新版本。
    fn revoke(
        &self,
        principal: SessionPrincipal,
        id: i64,
        expected_version: i64,
    ) -> AnnouncementFuture<'_>;
}

/// 使用公告数据库仓储实现应用服务。
pub struct DatabaseAnnouncementService {
    repository: Arc<AnnouncementRepository>,
}

impl DatabaseAnnouncementService {
    #[must_use]
    pub fn new(repository: Arc<AnnouncementRepository>) -> Self {
        Self { repository }
    }
}

impl AnnouncementService for DatabaseAnnouncementService {
    fn list_public(&self) -> AnnouncementListFuture<'_> {
        Box::pin(async move {
            let records = self
                .repository
                .list_public(DatabaseTimestamp::now_utc())
                .await
                .map_err(map_repository_error)?;
            records
                .into_iter()
                .map(AnnouncementView::from_record)
                .collect()
        })
    }

    fn list_admin(&self, principal: SessionPrincipal) -> AnnouncementListFuture<'_> {
        Box::pin(async move {
            require_admin(principal)?;
            let records = self
                .repository
                .list_admin()
                .await
                .map_err(map_repository_error)?;
            records
                .into_iter()
                .map(AnnouncementView::from_record)
                .collect()
        })
    }

    fn create(
        &self,
        principal: SessionPrincipal,
        command: AnnouncementWriteCommand,
    ) -> AnnouncementFuture<'_> {
        Box::pin(async move {
            require_admin(principal)?;
            let record = self
                .repository
                .create(principal.user_id().get(), command.into_record()?)
                .await
                .map_err(map_repository_error)?;
            AnnouncementView::from_record(record)
        })
    }

    fn update_draft(
        &self,
        principal: SessionPrincipal,
        id: i64,
        expected_version: i64,
        command: AnnouncementWriteCommand,
    ) -> AnnouncementFuture<'_> {
        Box::pin(async move {
            require_admin(principal)?;
            let record = self
                .repository
                .update_draft(id, expected_version, command.into_record()?)
                .await
                .map_err(map_repository_error)?;
            AnnouncementView::from_record(record)
        })
    }

    fn publish(
        &self,
        principal: SessionPrincipal,
        id: i64,
        expected_version: i64,
    ) -> AnnouncementFuture<'_> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .publish(id, expected_version, DatabaseTimestamp::now_utc())
                .await
                .map_err(map_repository_error)
                .and_then(AnnouncementView::from_record)
        })
    }

    fn revoke(
        &self,
        principal: SessionPrincipal,
        id: i64,
        expected_version: i64,
    ) -> AnnouncementFuture<'_> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .revoke(id, expected_version, DatabaseTimestamp::now_utc())
                .await
                .map_err(map_repository_error)
                .and_then(AnnouncementView::from_record)
        })
    }
}

impl fmt::Debug for DatabaseAnnouncementService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAnnouncementService(<repository>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AnnouncementServiceError> {
    (principal.role() == SessionRole::Admin)
        .then_some(())
        .ok_or(AnnouncementServiceError::Forbidden)
}

fn map_repository_error(error: AnnouncementRepositoryError) -> AnnouncementServiceError {
    match error {
        AnnouncementRepositoryError::InvalidInput => AnnouncementServiceError::InvalidRequest,
        AnnouncementRepositoryError::Conflict => AnnouncementServiceError::Conflict,
        AnnouncementRepositoryError::NotFound => AnnouncementServiceError::NotFound,
        AnnouncementRepositoryError::Query
        | AnnouncementRepositoryError::Timeout
        | AnnouncementRepositoryError::Invariant => AnnouncementServiceError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_domain::UserId;

    fn valid_command() -> AnnouncementWriteCommand {
        AnnouncementWriteCommand::new(
            "维护通知".to_owned(),
            "Maintenance notice".to_owned(),
            "服务将在窗口内短暂维护。".to_owned(),
            "The service will have a short maintenance window.".to_owned(),
            Some(1_800_000_000),
            Some(1_800_003_600),
        )
        .expect("测试公告参数有效")
    }

    #[test]
    fn write_command_rejects_invalid_window_and_control_characters() {
        assert_eq!(
            AnnouncementWriteCommand::new(
                "标题".to_owned(),
                "Title".to_owned(),
                "正文".to_owned(),
                "Body".to_owned(),
                Some(100),
                Some(100),
            )
            .unwrap_err(),
            AnnouncementServiceError::InvalidRequest
        );
        assert_eq!(
            AnnouncementWriteCommand::new(
                "标题\u{0000}".to_owned(),
                "Title".to_owned(),
                "正文".to_owned(),
                "Body".to_owned(),
                None,
                None,
            )
            .unwrap_err(),
            AnnouncementServiceError::InvalidRequest
        );
    }

    #[test]
    fn write_command_rejects_oversized_fields_and_preserves_unix_seconds() {
        assert_eq!(
            AnnouncementWriteCommand::new(
                "a".repeat(af_db::MAX_ANNOUNCEMENT_TITLE_BYTES + 1),
                "Title".to_owned(),
                "Body".to_owned(),
                "Body".to_owned(),
                None,
                None,
            )
            .unwrap_err(),
            AnnouncementServiceError::InvalidRequest
        );
        let command = valid_command();
        assert_eq!(command.visible_from, Some(1_800_000_000));
        assert_eq!(command.visible_until, Some(1_800_003_600));
    }

    #[test]
    fn only_admin_principals_pass_authorization_and_debug_is_redacted() {
        let user = SessionPrincipal::new(UserId::new(7).expect("用户标识有效"), SessionRole::User);
        let admin =
            SessionPrincipal::new(UserId::new(8).expect("用户标识有效"), SessionRole::Admin);
        assert_eq!(
            require_admin(user),
            Err(AnnouncementServiceError::Forbidden)
        );
        assert_eq!(require_admin(admin), Ok(()));
        assert_eq!(
            format!("{:?}", valid_command()),
            "AnnouncementWriteCommand(<redacted>)"
        );
    }
}
