use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    AdminGroupDeleteOutcome, AdminGroupMutationOutcome, AdminGroupPeakWriteRecord,
    AdminGroupRepository, AdminGroupRepositoryError, AdminGroupWriteRecord,
    MAX_ADMIN_GROUP_FLAGS_BYTES,
};
use af_domain::GroupId;
use thiserror::Error;

use crate::{AdminGroup, SessionPrincipal, SessionRole};

/// 管理端写入使用的已校验高峰倍率窗口。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminGroupPeakCommand {
    ratio_micros: i64,
    start_second: u32,
    end_second: u32,
}

impl AdminGroupPeakCommand {
    /// 校验定点倍率和午夜起整秒窗口，起止相同的空窗口无效。
    pub fn new(
        ratio_micros: i64,
        start_second: u32,
        end_second: u32,
    ) -> Result<Self, AdminGroupWriteError> {
        if ratio_micros < 0
            || start_second >= 24 * 60 * 60
            || end_second >= 24 * 60 * 60
            || start_second == end_second
        {
            return Err(AdminGroupWriteError::InvalidInput);
        }
        Ok(Self {
            ratio_micros,
            start_second,
            end_second,
        })
    }

    fn into_record(self) -> AdminGroupPeakWriteRecord {
        AdminGroupPeakWriteRecord::new(self.ratio_micros, self.start_second, self.end_second)
    }
}

/// 管理员创建分组时允许写入的完整业务字段。
pub struct AdminGroupCreateCommand {
    fields: AdminGroupWriteFields,
}

impl AdminGroupCreateCommand {
    /// 校验分组名称、定点倍率、限额、回退引用和受限 JSON 对象。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端分组写入契约一一对应"
    )]
    pub fn new(
        name: String,
        display_name: String,
        ratio_micros: i64,
        peak: Option<AdminGroupPeakCommand>,
        is_exclusive: bool,
        daily_limit: Option<i64>,
        weekly_limit: Option<i64>,
        monthly_limit: Option<i64>,
        rpm_limit: Option<i32>,
        fallback_group_id: Option<GroupId>,
        flags: serde_json::Value,
    ) -> Result<Self, AdminGroupWriteError> {
        Ok(Self {
            fields: AdminGroupWriteFields::new(
                name,
                display_name,
                ratio_micros,
                peak,
                is_exclusive,
                daily_limit,
                weekly_limit,
                monthly_limit,
                rpm_limit,
                fallback_group_id,
                flags,
            )?,
        })
    }

    fn into_record(self) -> AdminGroupWriteRecord {
        self.fields.into_record()
    }
}

impl fmt::Debug for AdminGroupCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminGroupCreateCommand(<redacted>)")
    }
}

/// 管理员完整更新分组时允许覆盖的业务字段。
pub struct AdminGroupUpdateCommand {
    fields: AdminGroupWriteFields,
}

impl AdminGroupUpdateCommand {
    /// 校验完整更新边界，避免无效配置进入仓储事务。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端分组写入契约一一对应"
    )]
    pub fn new(
        name: String,
        display_name: String,
        ratio_micros: i64,
        peak: Option<AdminGroupPeakCommand>,
        is_exclusive: bool,
        daily_limit: Option<i64>,
        weekly_limit: Option<i64>,
        monthly_limit: Option<i64>,
        rpm_limit: Option<i32>,
        fallback_group_id: Option<GroupId>,
        flags: serde_json::Value,
    ) -> Result<Self, AdminGroupWriteError> {
        Ok(Self {
            fields: AdminGroupWriteFields::new(
                name,
                display_name,
                ratio_micros,
                peak,
                is_exclusive,
                daily_limit,
                weekly_limit,
                monthly_limit,
                rpm_limit,
                fallback_group_id,
                flags,
            )?,
        })
    }

    fn fallback_group_id(&self) -> Option<GroupId> {
        self.fields.fallback_group_id
    }

    fn into_record(self) -> AdminGroupWriteRecord {
        self.fields.into_record()
    }
}

impl fmt::Debug for AdminGroupUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminGroupUpdateCommand(<redacted>)")
    }
}

struct AdminGroupWriteFields {
    name: String,
    display_name: String,
    ratio_micros: i64,
    peak: Option<AdminGroupPeakCommand>,
    is_exclusive: bool,
    daily_limit: Option<i64>,
    weekly_limit: Option<i64>,
    monthly_limit: Option<i64>,
    rpm_limit: Option<i32>,
    fallback_group_id: Option<GroupId>,
    flags: serde_json::Value,
}

impl AdminGroupWriteFields {
    #[allow(
        clippy::too_many_arguments,
        reason = "仅在统一校验入口组装完整分组字段"
    )]
    fn new(
        name: String,
        display_name: String,
        ratio_micros: i64,
        peak: Option<AdminGroupPeakCommand>,
        is_exclusive: bool,
        daily_limit: Option<i64>,
        weekly_limit: Option<i64>,
        monthly_limit: Option<i64>,
        rpm_limit: Option<i32>,
        fallback_group_id: Option<GroupId>,
        flags: serde_json::Value,
    ) -> Result<Self, AdminGroupWriteError> {
        validate_text(&name, 64)?;
        validate_text(&display_name, 128)?;
        validate_non_negative(ratio_micros)?;
        for limit in [daily_limit, weekly_limit, monthly_limit]
            .into_iter()
            .flatten()
        {
            validate_non_negative(limit)?;
        }
        if rpm_limit.is_some_and(|value| value < 0) {
            return Err(AdminGroupWriteError::InvalidInput);
        }
        let valid_flags = flags.is_object()
            && serde_json::to_vec(&flags)
                .is_ok_and(|encoded| encoded.len() <= MAX_ADMIN_GROUP_FLAGS_BYTES);
        if !valid_flags {
            return Err(AdminGroupWriteError::InvalidInput);
        }
        Ok(Self {
            name,
            display_name,
            ratio_micros,
            peak,
            is_exclusive,
            daily_limit,
            weekly_limit,
            monthly_limit,
            rpm_limit,
            fallback_group_id,
            flags,
        })
    }

    fn into_record(self) -> AdminGroupWriteRecord {
        AdminGroupWriteRecord::new(
            self.name,
            self.display_name,
            self.ratio_micros,
            self.peak.map(AdminGroupPeakCommand::into_record),
            self.is_exclusive,
            self.daily_limit,
            self.weekly_limit,
            self.monthly_limit,
            self.rpm_limit,
            self.fallback_group_id,
            self.flags,
        )
    }
}

/// 管理分组写入失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminGroupWriteError {
    /// 请求字段违反公开边界或引用了无效回退分组。
    #[error("管理分组写入参数无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("管理分组写入权限不足")]
    Forbidden,
    /// 分组名与当前有效分组冲突。
    #[error("管理分组名称冲突")]
    Conflict,
    /// 分组不存在或已经软删除。
    #[error("管理分组不存在")]
    NotFound,
    /// 分组仍被有效用户或有效令牌引用。
    #[error("管理分组仍在使用")]
    InUse,
    /// 数据已提交，但当前进程无法发布新的分组计费快照。
    #[error("管理分组运行时刷新失败")]
    RuntimeRefreshFailed,
    /// 数据库失败或持久化状态损坏。
    #[error("管理分组写入内部失败")]
    Internal,
}

/// 管理分组创建调用的对象安全 Future。
pub type AdminGroupCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminGroup, AdminGroupWriteError>> + Send + 'a>>;

/// 管理分组更新调用的对象安全 Future。
pub type AdminGroupUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminGroup, AdminGroupWriteError>> + Send + 'a>>;

/// 管理分组删除调用的对象安全 Future。
pub type AdminGroupDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminGroupWriteError>> + Send + 'a>>;

/// 分组正式写入后刷新运行时计费快照的稳定错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("运行时分组计费快照刷新失败")]
pub struct GroupPricingRuntimeRefreshError;

/// 分组计费运行时刷新的对象安全 Future。
pub type GroupPricingRuntimeRefreshFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), GroupPricingRuntimeRefreshError>> + Send + 'a>>;

/// 隔离管理应用域与具体计费缓存实现的刷新端口。
pub trait GroupPricingRuntimeRefresher: Send + Sync {
    /// 失效并刷新当前进程使用的完整分组计费快照。
    fn refresh<'a>(&'a self) -> GroupPricingRuntimeRefreshFuture<'a>;
}

/// 管理分组写入应用端口；角色校验必须在进入仓储前完成。
pub trait AdminGroupWriter: Send + Sync {
    /// 创建分组并返回管理快照。
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminGroupCreateCommand,
    ) -> AdminGroupCreateFuture<'a>;

    /// 完整更新分组并返回管理快照。
    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
        command: AdminGroupUpdateCommand,
    ) -> AdminGroupUpdateFuture<'a>;

    /// 安全软删除分组及可解除的直接运行时关系。
    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> AdminGroupDeleteFuture<'a>;
}

/// 在持久化成功后同步发布运行时分组计费快照的写入装饰器。
pub struct RuntimeRefreshingAdminGroupWriter {
    writer: Arc<dyn AdminGroupWriter>,
    refresher: Arc<dyn GroupPricingRuntimeRefresher>,
}

impl RuntimeRefreshingAdminGroupWriter {
    /// 包装现有写入端口，保持数据库事务与运行时发布职责分离。
    #[must_use]
    pub fn new(
        writer: Arc<dyn AdminGroupWriter>,
        refresher: Arc<dyn GroupPricingRuntimeRefresher>,
    ) -> Self {
        Self { writer, refresher }
    }

    async fn refresh_runtime(&self) -> Result<(), AdminGroupWriteError> {
        self.refresher
            .refresh()
            .await
            .map_err(|_| AdminGroupWriteError::RuntimeRefreshFailed)
    }
}

impl AdminGroupWriter for RuntimeRefreshingAdminGroupWriter {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminGroupCreateCommand,
    ) -> AdminGroupCreateFuture<'a> {
        Box::pin(async move {
            let group = self.writer.create(principal, command).await?;
            self.refresh_runtime().await?;
            Ok(group)
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
        command: AdminGroupUpdateCommand,
    ) -> AdminGroupUpdateFuture<'a> {
        Box::pin(async move {
            let group = self.writer.update(principal, group_id, command).await?;
            self.refresh_runtime().await?;
            Ok(group)
        })
    }

    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> AdminGroupDeleteFuture<'a> {
        Box::pin(async move {
            self.writer.delete(principal, group_id).await?;
            self.refresh_runtime().await
        })
    }
}

impl fmt::Debug for RuntimeRefreshingAdminGroupWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeRefreshingAdminGroupWriter(<受控>)")
    }
}

/// 使用数据库仓储实现管理员分组写入。
pub struct DatabaseAdminGroupWriter {
    repository: AdminGroupRepository,
}

impl DatabaseAdminGroupWriter {
    /// 绑定已经配置查询和写入截止时间的分组仓储。
    #[must_use]
    pub const fn new(repository: AdminGroupRepository) -> Self {
        Self { repository }
    }
}

impl AdminGroupWriter for DatabaseAdminGroupWriter {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminGroupCreateCommand,
    ) -> AdminGroupCreateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let record = self
                .repository
                .create(command.into_record())
                .await
                .map_err(map_repository_error)?;
            Ok(AdminGroup::from_record(record))
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
        command: AdminGroupUpdateCommand,
    ) -> AdminGroupUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            if command.fallback_group_id() == Some(group_id) {
                return Err(AdminGroupWriteError::InvalidInput);
            }
            match self
                .repository
                .update(group_id, command.into_record())
                .await
                .map_err(map_repository_error)?
            {
                AdminGroupMutationOutcome::Mutated(record) => Ok(AdminGroup::from_record(*record)),
                AdminGroupMutationOutcome::NotFound => Err(AdminGroupWriteError::NotFound),
            }
        })
    }

    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> AdminGroupDeleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .delete(group_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminGroupDeleteOutcome::Deleted => Ok(()),
                AdminGroupDeleteOutcome::NotFound => Err(AdminGroupWriteError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminGroupWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminGroupWriter(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminGroupWriteError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminGroupWriteError::Forbidden)
    }
}

fn map_repository_error(error: AdminGroupRepositoryError) -> AdminGroupWriteError {
    match error {
        AdminGroupRepositoryError::Conflict => AdminGroupWriteError::Conflict,
        AdminGroupRepositoryError::InvalidReference => AdminGroupWriteError::InvalidInput,
        AdminGroupRepositoryError::InUse => AdminGroupWriteError::InUse,
        AdminGroupRepositoryError::Query
        | AdminGroupRepositoryError::Timeout
        | AdminGroupRepositoryError::Invariant => AdminGroupWriteError::Internal,
    }
}

fn validate_text(value: &str, maximum_bytes: usize) -> Result<(), AdminGroupWriteError> {
    if value.is_empty()
        || value.len() > maximum_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(AdminGroupWriteError::InvalidInput);
    }
    Ok(())
}

fn validate_non_negative(value: i64) -> Result<(), AdminGroupWriteError> {
    if value < 0 {
        return Err(AdminGroupWriteError::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use af_domain::UserId;

    use super::*;

    #[test]
    fn commands_validate_write_boundaries_and_redact_configuration() {
        let peak = AdminGroupPeakCommand::new(1_500_000, 8 * 3_600, 20 * 3_600).unwrap();
        let command = AdminGroupCreateCommand::new(
            "vip".to_owned(),
            "VIP".to_owned(),
            1_250_000,
            Some(peak),
            true,
            Some(10_000),
            None,
            None,
            Some(120),
            None,
            serde_json::json!({"claude_code_only": true}),
        )
        .unwrap();
        assert_eq!(
            format!("{command:?}"),
            "AdminGroupCreateCommand(<redacted>)"
        );

        assert_eq!(
            AdminGroupPeakCommand::new(1, 10, 10),
            Err(AdminGroupWriteError::InvalidInput)
        );
        assert_eq!(
            AdminGroupUpdateCommand::new(
                " bad ".to_owned(),
                "Bad".to_owned(),
                1_000_000,
                None,
                false,
                None,
                None,
                None,
                None,
                None,
                serde_json::json!({}),
            )
            .unwrap_err(),
            AdminGroupWriteError::InvalidInput
        );
        assert_eq!(
            AdminGroupCreateCommand::new(
                "valid".to_owned(),
                "Valid".to_owned(),
                -1,
                None,
                false,
                None,
                None,
                None,
                None,
                None,
                serde_json::json!([]),
            )
            .unwrap_err(),
            AdminGroupWriteError::InvalidInput
        );
    }

    #[test]
    fn normal_user_is_rejected_before_repository_access() {
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminGroupWriteError::Forbidden)
        );
    }

    struct SuccessfulGroupWriter;

    impl AdminGroupWriter for SuccessfulGroupWriter {
        fn create<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _command: AdminGroupCreateCommand,
        ) -> AdminGroupCreateFuture<'a> {
            Box::pin(async { Ok(test_group()) })
        }

        fn update<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _group_id: GroupId,
            _command: AdminGroupUpdateCommand,
        ) -> AdminGroupUpdateFuture<'a> {
            Box::pin(async { Ok(test_group()) })
        }

        fn delete<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _group_id: GroupId,
        ) -> AdminGroupDeleteFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    struct FailingGroupWriter;

    impl AdminGroupWriter for FailingGroupWriter {
        fn create<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _command: AdminGroupCreateCommand,
        ) -> AdminGroupCreateFuture<'a> {
            Box::pin(async { Err(AdminGroupWriteError::Internal) })
        }

        fn update<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _group_id: GroupId,
            _command: AdminGroupUpdateCommand,
        ) -> AdminGroupUpdateFuture<'a> {
            Box::pin(async { Err(AdminGroupWriteError::Internal) })
        }

        fn delete<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _group_id: GroupId,
        ) -> AdminGroupDeleteFuture<'a> {
            Box::pin(async { Err(AdminGroupWriteError::Internal) })
        }
    }

    struct RecordingRefresher {
        calls: AtomicUsize,
        fail: bool,
    }

    impl RecordingRefresher {
        fn new(fail: bool) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                fail,
            }
        }
    }

    impl GroupPricingRuntimeRefresher for RecordingRefresher {
        fn refresh<'a>(&'a self) -> GroupPricingRuntimeRefreshFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                if self.fail {
                    Err(GroupPricingRuntimeRefreshError)
                } else {
                    Ok(())
                }
            })
        }
    }

    #[tokio::test]
    async fn runtime_refresh_runs_after_each_successful_mutation() {
        let refresher = Arc::new(RecordingRefresher::new(false));
        let writer = RuntimeRefreshingAdminGroupWriter::new(
            Arc::new(SuccessfulGroupWriter),
            refresher.clone(),
        );
        let principal = admin_principal();
        let group_id = GroupId::new(1).unwrap();

        writer.create(principal, create_command()).await.unwrap();
        writer
            .update(principal, group_id, update_command())
            .await
            .unwrap();
        writer.delete(principal, group_id).await.unwrap();

        assert_eq!(refresher.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn runtime_refresh_failure_is_returned_after_commit() {
        let refresher = Arc::new(RecordingRefresher::new(true));
        let writer = RuntimeRefreshingAdminGroupWriter::new(
            Arc::new(SuccessfulGroupWriter),
            refresher.clone(),
        );

        assert_eq!(
            writer
                .create(admin_principal(), create_command())
                .await
                .unwrap_err(),
            AdminGroupWriteError::RuntimeRefreshFailed
        );
        assert_eq!(refresher.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_persistence_does_not_refresh_runtime() {
        let refresher = Arc::new(RecordingRefresher::new(false));
        let writer =
            RuntimeRefreshingAdminGroupWriter::new(Arc::new(FailingGroupWriter), refresher.clone());

        assert_eq!(
            writer
                .create(admin_principal(), create_command())
                .await
                .unwrap_err(),
            AdminGroupWriteError::Internal
        );
        assert_eq!(refresher.calls.load(Ordering::SeqCst), 0);
    }

    fn admin_principal() -> SessionPrincipal {
        SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::Admin)
    }

    fn create_command() -> AdminGroupCreateCommand {
        AdminGroupCreateCommand::new(
            "standard".to_owned(),
            "Standard".to_owned(),
            1_000_000,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            serde_json::json!({}),
        )
        .unwrap()
    }

    fn update_command() -> AdminGroupUpdateCommand {
        AdminGroupUpdateCommand::new(
            "standard".to_owned(),
            "Standard".to_owned(),
            1_000_000,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            serde_json::json!({}),
        )
        .unwrap()
    }

    fn test_group() -> AdminGroup {
        AdminGroup::from_parts(
            GroupId::new(1).unwrap(),
            "standard".to_owned(),
            "Standard".to_owned(),
            1_000_000,
            None,
            false,
            None,
            None,
            None,
            crate::AdminGroupWindow::from_parts(0, 0, 1),
            crate::AdminGroupWindow::from_parts(0, 0, 1),
            crate::AdminGroupWindow::from_parts(0, 0, 1),
            None,
            None,
            serde_json::json!({}),
        )
    }
}
