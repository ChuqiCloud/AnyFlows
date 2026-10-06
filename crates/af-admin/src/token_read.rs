use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminTokenLookupOutcome, AdminTokenRecord, AdminTokenRepository, AdminTokenRepositoryError,
    MAX_ADMIN_TOKEN_PAGE_SIZE,
};
use af_domain::{GroupId, TokenId, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理令牌列表默认页大小。
pub const DEFAULT_ADMIN_TOKEN_PAGE_SIZE: usize = 50;

/// 已校验的管理令牌列表查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminTokenListQuery {
    after: Option<TokenId>,
    limit: usize,
}

impl AdminTokenListQuery {
    /// 校验单调 ID 游标和固定页大小边界。
    pub fn new(after: Option<TokenId>, limit: usize) -> Result<Self, AdminTokenReadError> {
        if !(1..=MAX_ADMIN_TOKEN_PAGE_SIZE).contains(&limit) {
            return Err(AdminTokenReadError::InvalidPagination);
        }
        Ok(Self { after, limit })
    }

    /// 返回上一页最后一个令牌 ID。
    #[must_use]
    pub const fn after(self) -> Option<TokenId> {
        self.after
    }

    /// 返回本页最大记录数。
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for AdminTokenListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ADMIN_TOKEN_PAGE_SIZE,
        }
    }
}

/// 管理 API 使用的稳定令牌状态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AdminTokenStatus {
    /// 令牌已启用。
    Enabled,
    /// 令牌已禁用。
    Disabled,
}

impl AdminTokenStatus {
    fn from_database(value: i16) -> Result<Self, AdminTokenReadError> {
        match value {
            1 => Ok(Self::Enabled),
            2 => Ok(Self::Disabled),
            _ => Err(AdminTokenReadError::Internal),
        }
    }
}

/// 管理 API 可读取的非敏感令牌快照。
pub struct AdminToken {
    token_id: TokenId,
    user_id: UserId,
    key_prefix: String,
    name: String,
    status: AdminTokenStatus,
    group_id: Option<GroupId>,
    remain_quota: i64,
    unlimited_quota: bool,
    used_quota: i64,
    expired_at: Option<i64>,
    model_limits: Option<Vec<String>>,
    allow_ips: Option<Vec<String>>,
    cross_group_retry: bool,
    rate_limit_5h: Option<i64>,
    rate_limit_1d: Option<i64>,
    rate_limit_7d: Option<i64>,
    usage_5h: i64,
    usage_1d: i64,
    usage_7d: i64,
    window_5h_start: i64,
    window_1d_start: i64,
    window_7d_start: i64,
    max_requests: Option<i64>,
    used_requests: i64,
}

impl AdminToken {
    /// 组合已经完成持久化校验的令牌字段，供仓储适配器和测试实现使用。
    #[allow(clippy::too_many_arguments, reason = "字段与稳定管理 API 响应一一对应")]
    #[must_use]
    pub fn from_parts(
        token_id: TokenId,
        user_id: UserId,
        key_prefix: String,
        name: String,
        status: AdminTokenStatus,
        group_id: Option<GroupId>,
        remain_quota: i64,
        unlimited_quota: bool,
        used_quota: i64,
        expired_at: Option<i64>,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
        cross_group_retry: bool,
        rate_limit_5h: Option<i64>,
        rate_limit_1d: Option<i64>,
        rate_limit_7d: Option<i64>,
        usage_5h: i64,
        usage_1d: i64,
        usage_7d: i64,
        window_5h_start: i64,
        window_1d_start: i64,
        window_7d_start: i64,
        max_requests: Option<i64>,
        used_requests: i64,
    ) -> Self {
        Self {
            token_id,
            user_id,
            key_prefix,
            name,
            status,
            group_id,
            remain_quota,
            unlimited_quota,
            used_quota,
            expired_at,
            model_limits,
            allow_ips,
            cross_group_retry,
            rate_limit_5h,
            rate_limit_1d,
            rate_limit_7d,
            usage_5h,
            usage_1d,
            usage_7d,
            window_5h_start,
            window_1d_start,
            window_7d_start,
            max_requests,
            used_requests,
        }
    }

    /// 返回令牌标识。
    #[must_use]
    pub const fn token_id(&self) -> TokenId {
        self.token_id
    }

    /// 返回所属用户标识。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回不可用于鉴权的展示前缀。
    #[must_use]
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    /// 返回令牌名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回令牌状态。
    #[must_use]
    pub const fn status(&self) -> AdminTokenStatus {
        self.status
    }

    /// 返回可选的强制绑定分组。
    #[must_use]
    pub const fn group_id(&self) -> Option<GroupId> {
        self.group_id
    }

    /// 返回有限令牌剩余额度。
    #[must_use]
    pub const fn remain_quota(&self) -> i64 {
        self.remain_quota
    }

    /// 返回是否跳过令牌级额度限制。
    #[must_use]
    pub const fn unlimited_quota(&self) -> bool {
        self.unlimited_quota
    }

    /// 返回累计已用额度。
    #[must_use]
    pub const fn used_quota(&self) -> i64 {
        self.used_quota
    }

    /// 返回可选过期时间的 Unix 秒数。
    #[must_use]
    pub const fn expired_at(&self) -> Option<i64> {
        self.expired_at
    }

    /// 返回可选模型白名单。
    #[must_use]
    pub fn model_limits(&self) -> Option<&[String]> {
        self.model_limits.as_deref()
    }

    /// 返回可选 IP/CIDR 白名单。
    #[must_use]
    pub fn allow_ips(&self) -> Option<&[String]> {
        self.allow_ips.as_deref()
    }

    /// 返回是否允许自动分组场景跨组重试。
    #[must_use]
    pub const fn cross_group_retry(&self) -> bool {
        self.cross_group_retry
    }

    /// 返回 5 小时窗口额度上限。
    #[must_use]
    pub const fn rate_limit_5h(&self) -> Option<i64> {
        self.rate_limit_5h
    }

    /// 返回 1 天窗口额度上限。
    #[must_use]
    pub const fn rate_limit_1d(&self) -> Option<i64> {
        self.rate_limit_1d
    }

    /// 返回 7 天窗口额度上限。
    #[must_use]
    pub const fn rate_limit_7d(&self) -> Option<i64> {
        self.rate_limit_7d
    }

    /// 返回 5 小时窗口已用额度。
    #[must_use]
    pub const fn usage_5h(&self) -> i64 {
        self.usage_5h
    }

    /// 返回 1 天窗口已用额度。
    #[must_use]
    pub const fn usage_1d(&self) -> i64 {
        self.usage_1d
    }

    /// 返回 7 天窗口已用额度。
    #[must_use]
    pub const fn usage_7d(&self) -> i64 {
        self.usage_7d
    }

    /// 返回 5 小时窗口起点的 Unix 秒数。
    #[must_use]
    pub const fn window_5h_start(&self) -> i64 {
        self.window_5h_start
    }

    /// 返回 1 天窗口起点的 Unix 秒数。
    #[must_use]
    pub const fn window_1d_start(&self) -> i64 {
        self.window_1d_start
    }

    /// 返回 7 天窗口起点的 Unix 秒数。
    #[must_use]
    pub const fn window_7d_start(&self) -> i64 {
        self.window_7d_start
    }

    /// 返回可选请求数上限。
    #[must_use]
    pub const fn max_requests(&self) -> Option<i64> {
        self.max_requests
    }

    /// 返回当前已用请求数。
    #[must_use]
    pub const fn used_requests(&self) -> i64 {
        self.used_requests
    }

    pub(super) fn from_record(record: AdminTokenRecord) -> Result<Self, AdminTokenReadError> {
        Ok(Self {
            token_id: record.token_id(),
            user_id: record.user_id(),
            key_prefix: record.key_prefix().to_owned(),
            name: record.name().to_owned(),
            status: AdminTokenStatus::from_database(record.status())?,
            group_id: record.group_id(),
            remain_quota: record.remain_quota(),
            unlimited_quota: record.unlimited_quota(),
            used_quota: record.used_quota(),
            expired_at: record.expired_at(),
            model_limits: record.model_limits().map(|values| values.to_vec()),
            allow_ips: record.allow_ips().map(|values| values.to_vec()),
            cross_group_retry: record.cross_group_retry(),
            rate_limit_5h: record.rate_limit_5h(),
            rate_limit_1d: record.rate_limit_1d(),
            rate_limit_7d: record.rate_limit_7d(),
            usage_5h: record.usage_5h(),
            usage_1d: record.usage_1d(),
            usage_7d: record.usage_7d(),
            window_5h_start: record.window_5h_start(),
            window_1d_start: record.window_1d_start(),
            window_7d_start: record.window_7d_start(),
            max_requests: record.max_requests(),
            used_requests: record.used_requests(),
        })
    }
}

impl fmt::Debug for AdminToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminToken(<redacted>)")
    }
}

/// 一页管理令牌响应。
pub struct AdminTokenPage {
    tokens: Vec<AdminToken>,
    next_cursor: Option<TokenId>,
}

impl AdminTokenPage {
    /// 组装令牌列表和可选下一游标。
    #[must_use]
    pub fn from_parts(tokens: Vec<AdminToken>, next_cursor: Option<TokenId>) -> Self {
        Self {
            tokens,
            next_cursor,
        }
    }

    /// 返回当前页令牌。
    #[must_use]
    pub fn tokens(&self) -> &[AdminToken] {
        &self.tokens
    }

    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<TokenId> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminTokenPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminTokenPage(<redacted>)")
    }
}

/// 管理令牌读取失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminTokenReadError {
    /// 游标或页大小不满足公开接口。
    #[error("管理令牌分页参数无效")]
    InvalidPagination,
    /// 当前会话不是管理员。
    #[error("管理令牌读取权限不足")]
    Forbidden,
    /// 令牌不存在或已经软删除。
    #[error("管理令牌不存在")]
    NotFound,
    /// 数据库失败或持久化状态损坏。
    #[error("管理令牌读取内部失败")]
    Internal,
}

/// 管理令牌列表调用的对象安全 Future。
pub type AdminTokenListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminTokenPage, AdminTokenReadError>> + Send + 'a>>;

/// 管理令牌详情调用的对象安全 Future。
pub type AdminTokenGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminToken, AdminTokenReadError>> + Send + 'a>>;

/// 管理令牌只读应用端口；角色校验必须在进入仓储前完成。
pub trait AdminTokenReader: Send + Sync {
    /// 读取一页令牌。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminTokenListQuery,
    ) -> AdminTokenListFuture<'a>;

    /// 按令牌 ID 读取详情。
    fn get<'a>(&'a self, principal: SessionPrincipal, token_id: TokenId)
    -> AdminTokenGetFuture<'a>;
}

/// 使用数据库仓储实现管理员令牌读取。
pub struct DatabaseAdminTokenReader {
    repository: AdminTokenRepository,
}

impl DatabaseAdminTokenReader {
    /// 绑定已经配置查询截止时间的令牌仓储。
    #[must_use]
    pub const fn new(repository: AdminTokenRepository) -> Self {
        Self { repository }
    }
}

impl AdminTokenReader for DatabaseAdminTokenReader {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminTokenListQuery,
    ) -> AdminTokenListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .repository
                .list(query.after(), query.limit())
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page.into_parts();
            let tokens = records
                .into_iter()
                .map(AdminToken::from_record)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(AdminTokenPage::from_parts(tokens, next_cursor))
        })
    }

    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        token_id: TokenId,
    ) -> AdminTokenGetFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .get(token_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminTokenLookupOutcome::Found(record) => AdminToken::from_record(*record),
                AdminTokenLookupOutcome::NotFound => Err(AdminTokenReadError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminTokenReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminTokenReader(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminTokenReadError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminTokenReadError::Forbidden)
    }
}

fn map_repository_error(error: AdminTokenRepositoryError) -> AdminTokenReadError {
    let _ = error;
    AdminTokenReadError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_and_role_boundaries_are_closed() {
        assert_eq!(AdminTokenListQuery::default().limit(), 50);
        assert_eq!(
            AdminTokenListQuery::new(None, 0),
            Err(AdminTokenReadError::InvalidPagination)
        );
        assert_eq!(
            AdminTokenListQuery::new(None, MAX_ADMIN_TOKEN_PAGE_SIZE + 1),
            Err(AdminTokenReadError::InvalidPagination)
        );
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminTokenReadError::Forbidden)
        );
    }
}
