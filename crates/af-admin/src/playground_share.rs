use std::{
    fmt,
    future::Future,
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    PlaygroundShareRepository, PlaygroundShareRepositoryError, PlaygroundShareRevokeOutcome,
    PlaygroundShareWrite,
};
use thiserror::Error;

use crate::playground_share_snapshot::sessions_from_snapshot;
use crate::{
    IssuedPlaygroundShareToken, PlaygroundShareCreateCommand, PlaygroundShareInputError,
    PlaygroundShareSession, PresentedPlaygroundShareToken, SessionPrincipal,
};

/// 创建成功后只返回一次的分享令牌与服务器时间边界。
pub struct IssuedPlaygroundShare {
    token: PresentedPlaygroundShareToken,
    created_at: i64,
    expires_at: i64,
}

impl IssuedPlaygroundShare {
    /// 消费签发结果并返回响应所需的令牌和时间边界。
    #[must_use]
    pub fn into_parts(self) -> (PresentedPlaygroundShareToken, i64, i64) {
        (self.token, self.created_at, self.expires_at)
    }
}

impl fmt::Debug for IssuedPlaygroundShare {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IssuedPlaygroundShare(<redacted>)")
    }
}

/// 游客可读取的只读分享视图。
pub struct PlaygroundShareView {
    sessions: Vec<PlaygroundShareSession>,
    created_at: i64,
    expires_at: i64,
}

impl PlaygroundShareView {
    /// 消费视图并返回公开会话及时间边界。
    #[must_use]
    pub fn into_parts(self) -> (Vec<PlaygroundShareSession>, i64, i64) {
        (self.sessions, self.created_at, self.expires_at)
    }
}

impl fmt::Debug for PlaygroundShareView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundShareView")
            .field("session_count", &self.sessions.len())
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// 分享应用服务的稳定错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlaygroundShareError {
    /// TTL、模型、消息或容量不满足公开契约。
    #[error("Playground 分享参数无效")]
    InvalidInput,
    /// 会话用户在写入前已经失效。
    #[error("Playground 分享会话无效")]
    InvalidSession,
    /// 当前用户的有效分享已经达到上限。
    #[error("Playground 有效分享已达上限")]
    LimitReached,
    /// 令牌未知、失效、撤销或不属于当前用户。
    #[error("Playground 分享不存在")]
    NotFound,
    /// 随机源、数据库或持久化快照发生内部故障。
    #[error("Playground 分享内部失败")]
    Internal,
}

/// 创建分享 Future。
pub type PlaygroundShareCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<IssuedPlaygroundShare, PlaygroundShareError>> + Send + 'a>>;
/// 公开读取分享 Future。
pub type PlaygroundShareReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PlaygroundShareView, PlaygroundShareError>> + Send + 'a>>;
/// 撤销分享 Future。
pub type PlaygroundShareRevokeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), PlaygroundShareError>> + Send + 'a>>;

/// Playground 只读分享应用端口。
pub trait PlaygroundShareService: Send + Sync {
    /// 为当前登录用户创建有界不可变快照。
    fn create(
        &self,
        principal: SessionPrincipal,
        command: PlaygroundShareCreateCommand,
    ) -> PlaygroundShareCreateFuture<'_>;

    /// 按高熵令牌公开读取有效快照。
    fn read(&self, token: String) -> PlaygroundShareReadFuture<'_>;

    /// 仅允许创建者撤销有效快照。
    fn revoke(&self, principal: SessionPrincipal, token: String)
    -> PlaygroundShareRevokeFuture<'_>;
}

/// 使用数据库仓储实现的 Playground 分享服务。
pub struct DatabasePlaygroundShareService {
    repository: PlaygroundShareRepository,
}

impl DatabasePlaygroundShareService {
    /// 绑定已配置截止时间和容量边界的分享仓储。
    #[must_use]
    pub const fn new(repository: PlaygroundShareRepository) -> Self {
        Self { repository }
    }
}

impl PlaygroundShareService for DatabasePlaygroundShareService {
    fn create(
        &self,
        principal: SessionPrincipal,
        command: PlaygroundShareCreateCommand,
    ) -> PlaygroundShareCreateFuture<'_> {
        Box::pin(async move {
            let ttl = command.ttl();
            let snapshot = command.into_snapshot().map_err(map_input_error)?;
            let token = IssuedPlaygroundShareToken::generate()
                .map_err(|_| PlaygroundShareError::Internal)?;
            let expires_at = current_timestamp()?
                .checked_add(ttl.seconds())
                .and_then(|value| i64::try_from(value).ok())
                .ok_or(PlaygroundShareError::Internal)?;
            let created = self
                .repository
                .create(PlaygroundShareWrite::new(
                    principal.user_id(),
                    token.digest().as_str().to_owned(),
                    snapshot,
                    expires_at,
                ))
                .await
                .map_err(map_repository_error)?;
            Ok(IssuedPlaygroundShare {
                token: token.into_token(),
                created_at: created.created_at(),
                expires_at: created.expires_at(),
            })
        })
    }

    fn read(&self, token: String) -> PlaygroundShareReadFuture<'_> {
        Box::pin(async move {
            let token = PresentedPlaygroundShareToken::parse_owned(token)
                .map_err(|_| PlaygroundShareError::NotFound)?;
            let digest = token.digest();
            let record = self
                .repository
                .find_active(digest.as_str())
                .await
                .map_err(map_repository_error)?
                .ok_or(PlaygroundShareError::NotFound)?;
            let (snapshot, created_at, expires_at) = record.into_parts();
            let sessions =
                sessions_from_snapshot(snapshot).map_err(|_| PlaygroundShareError::Internal)?;
            Ok(PlaygroundShareView {
                sessions,
                created_at,
                expires_at,
            })
        })
    }

    fn revoke(
        &self,
        principal: SessionPrincipal,
        token: String,
    ) -> PlaygroundShareRevokeFuture<'_> {
        Box::pin(async move {
            let token = PresentedPlaygroundShareToken::parse_owned(token)
                .map_err(|_| PlaygroundShareError::NotFound)?;
            let digest = token.digest();
            match self
                .repository
                .revoke(principal.user_id(), digest.as_str())
                .await
                .map_err(map_repository_error)?
            {
                PlaygroundShareRevokeOutcome::Revoked => Ok(()),
                PlaygroundShareRevokeOutcome::NotFound => Err(PlaygroundShareError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabasePlaygroundShareService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabasePlaygroundShareService(<redacted>)")
    }
}

fn current_timestamp() -> Result<u64, PlaygroundShareError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| PlaygroundShareError::Internal)
}

fn map_input_error(_error: PlaygroundShareInputError) -> PlaygroundShareError {
    PlaygroundShareError::InvalidInput
}

fn map_repository_error(error: PlaygroundShareRepositoryError) -> PlaygroundShareError {
    match error {
        PlaygroundShareRepositoryError::OwnerUnavailable => PlaygroundShareError::InvalidSession,
        PlaygroundShareRepositoryError::LimitReached => PlaygroundShareError::LimitReached,
        PlaygroundShareRepositoryError::InvalidInput
        | PlaygroundShareRepositoryError::TokenConflict
        | PlaygroundShareRepositoryError::Query
        | PlaygroundShareRepositoryError::Timeout
        | PlaygroundShareRepositoryError::Invariant => PlaygroundShareError::Internal,
    }
}
