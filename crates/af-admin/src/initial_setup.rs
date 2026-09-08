use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    InitialSetupOutcome as RepositorySetupOutcome, InitialSetupRecord, InitialSetupRepository,
    InitialSetupRepositoryError, InitialSetupStatus as RepositorySetupStatus,
};
use thiserror::Error;

use crate::GroupPricingRuntimeRefresher;

/// 首次安装管理员密码的最小 UTF-8 字节数。
pub const MIN_INITIAL_ADMIN_PASSWORD_BYTES: usize = 12;
/// 首次安装管理员密码的最大 UTF-8 字节数。
pub const MAX_INITIAL_ADMIN_PASSWORD_BYTES: usize = 128;

/// 首次安装对外状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialSetupStatus {
    /// 当前数据库允许创建首个管理员。
    Required,
    /// 当前数据库已经完成或越过首次安装阶段。
    Complete,
}

/// 首次安装管理员输入；调试输出始终隐藏用户名和密码。
pub struct InitialSetupCommand {
    username: String,
    password: String,
}

impl InitialSetupCommand {
    /// 校验首次安装公开输入边界。
    pub fn new(username: String, password: String) -> Result<Self, InitialSetupError> {
        if username.is_empty()
            || username.len() > 64
            || username.trim() != username
            || username.chars().any(char::is_control)
            || !(MIN_INITIAL_ADMIN_PASSWORD_BYTES..=MAX_INITIAL_ADMIN_PASSWORD_BYTES)
                .contains(&password.len())
            || password.chars().any(char::is_control)
        {
            return Err(InitialSetupError::InvalidInput);
        }
        Ok(Self { username, password })
    }

    fn into_record(self) -> InitialSetupRecord {
        InitialSetupRecord::new(self.username, self.password)
    }
}

impl fmt::Debug for InitialSetupCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InitialSetupCommand(<redacted>)")
    }
}

/// 首次安装失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InitialSetupError {
    /// 用户名或密码违反公开输入边界。
    #[error("首次安装参数无效")]
    InvalidInput,
    /// 系统已经完成安装，或另一个请求先完成了安装。
    #[error("系统已经完成首次安装")]
    Conflict,
    /// 数据库、随机源或持久化状态发生内部故障。
    #[error("首次安装内部失败")]
    Internal,
}

/// 首次安装状态查询 Future。
pub type InitialSetupStatusFuture<'a> =
    Pin<Box<dyn Future<Output = Result<InitialSetupStatus, InitialSetupError>> + Send + 'a>>;

/// 首次安装写入 Future。
pub type InitialSetupFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), InitialSetupError>> + Send + 'a>>;

/// 首次安装应用端口；实现必须同时保证失败关闭状态和一次性写入。
pub trait InitialSetup: Send + Sync {
    /// 读取当前是否仍需执行首次安装。
    fn status(&self) -> InitialSetupStatusFuture<'_>;

    /// 一次性创建默认分组和首个管理员。
    fn initialize(&self, command: InitialSetupCommand) -> InitialSetupFuture<'_>;
}

/// 在首次安装事务提交后同步刷新运行时分组计费快照。
pub struct RuntimeRefreshingInitialSetup {
    setup: Arc<dyn InitialSetup>,
    refresher: Arc<dyn GroupPricingRuntimeRefresher>,
}

impl RuntimeRefreshingInitialSetup {
    /// 包装首次安装端口，保持数据库事务与运行时缓存发布职责分离。
    #[must_use]
    pub fn new(
        setup: Arc<dyn InitialSetup>,
        refresher: Arc<dyn GroupPricingRuntimeRefresher>,
    ) -> Self {
        Self { setup, refresher }
    }
}

impl InitialSetup for RuntimeRefreshingInitialSetup {
    fn status(&self) -> InitialSetupStatusFuture<'_> {
        self.setup.status()
    }

    fn initialize(&self, command: InitialSetupCommand) -> InitialSetupFuture<'_> {
        Box::pin(async move {
            self.setup.initialize(command).await?;
            self.refresher
                .refresh()
                .await
                .map_err(|_| InitialSetupError::Internal)
        })
    }
}

impl fmt::Debug for RuntimeRefreshingInitialSetup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeRefreshingInitialSetup(<受控>)")
    }
}

/// 使用数据库仓储实现首次安装应用服务。
pub struct DatabaseInitialSetup {
    repository: InitialSetupRepository,
}

impl DatabaseInitialSetup {
    /// 绑定已经配置截止时间的首次安装仓储。
    #[must_use]
    pub const fn new(repository: InitialSetupRepository) -> Self {
        Self { repository }
    }
}

impl InitialSetup for DatabaseInitialSetup {
    fn status(&self) -> InitialSetupStatusFuture<'_> {
        Box::pin(async move {
            self.repository
                .status()
                .await
                .map(|status| match status {
                    RepositorySetupStatus::Required => InitialSetupStatus::Required,
                    RepositorySetupStatus::Complete => InitialSetupStatus::Complete,
                })
                .map_err(map_repository_error)
        })
    }

    fn initialize(&self, command: InitialSetupCommand) -> InitialSetupFuture<'_> {
        Box::pin(async move {
            match self
                .repository
                .initialize(command.into_record())
                .await
                .map_err(map_repository_error)?
            {
                RepositorySetupOutcome::Initialized { .. } => Ok(()),
                RepositorySetupOutcome::AlreadyInitialized => Err(InitialSetupError::Conflict),
            }
        })
    }
}

impl fmt::Debug for DatabaseInitialSetup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseInitialSetup(<redacted>)")
    }
}

fn map_repository_error(error: InitialSetupRepositoryError) -> InitialSetupError {
    let _ = error;
    InitialSetupError::Internal
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use crate::GroupPricingRuntimeRefreshFuture;

    struct SuccessfulSetup;

    impl InitialSetup for SuccessfulSetup {
        fn status(&self) -> InitialSetupStatusFuture<'_> {
            Box::pin(async { Ok(InitialSetupStatus::Complete) })
        }

        fn initialize(&self, _command: InitialSetupCommand) -> InitialSetupFuture<'_> {
            Box::pin(async { Ok(()) })
        }
    }

    struct RecordingRefresher {
        calls: AtomicUsize,
    }

    impl GroupPricingRuntimeRefresher for RecordingRefresher {
        fn refresh<'a>(&'a self) -> GroupPricingRuntimeRefreshFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    #[test]
    fn command_enforces_setup_specific_password_boundary() {
        assert_eq!(
            InitialSetupCommand::new("owner".to_owned(), "too-short".to_owned()).unwrap_err(),
            InitialSetupError::InvalidInput
        );
        assert_eq!(
            InitialSetupCommand::new(" owner".to_owned(), "long-enough-password".to_owned())
                .unwrap_err(),
            InitialSetupError::InvalidInput
        );
        assert!(
            InitialSetupCommand::new("owner".to_owned(), "long-enough-password".to_owned()).is_ok()
        );
    }

    #[test]
    fn command_debug_never_exposes_installation_input() {
        let command = InitialSetupCommand::new(
            "sensitive-owner".to_owned(),
            "sensitive-password".to_owned(),
        )
        .unwrap();
        let rendered = format!("{command:?}");
        assert_eq!(rendered, "InitialSetupCommand(<redacted>)");
        assert!(!rendered.contains("sensitive"));
    }

    #[tokio::test]
    async fn runtime_refresh_runs_after_successful_initialization() {
        let refresher = Arc::new(RecordingRefresher {
            calls: AtomicUsize::new(0),
        });
        let setup =
            RuntimeRefreshingInitialSetup::new(Arc::new(SuccessfulSetup), refresher.clone());

        setup
            .initialize(
                InitialSetupCommand::new("owner".to_owned(), "long-enough-password".to_owned())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(refresher.calls.load(Ordering::SeqCst), 1);
    }
}
