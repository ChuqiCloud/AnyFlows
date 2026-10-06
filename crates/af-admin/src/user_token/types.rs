use std::fmt;

use af_db::{MAX_USER_TOKEN_PAGE_SIZE, UserTokenRecord, UserTokenWriteRecord};
use af_domain::{PLAYGROUND_TOKEN_NAME, TokenId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    IssuedApiKey, PresentedApiKey,
    token_write::{
        validate_ip_allowlist, validate_model_limits, validate_name, validate_non_negative,
    },
};

/// 用户 Key 列表默认页大小。
pub const DEFAULT_USER_TOKEN_PAGE_SIZE: usize = 50;
/// 每个用户允许保留的未软删除 API Key 数量。
pub const MAX_USER_TOKENS_PER_USER: usize = af_db::MAX_USER_TOKENS_PER_USER;

/// 用户 API 使用的稳定 Key 状态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserTokenStatus {
    /// Key 已启用。
    Enabled,
    /// Key 已禁用。
    Disabled,
}

impl UserTokenStatus {
    fn from_database(value: i16) -> Result<Self, UserTokenError> {
        match value {
            1 => Ok(Self::Enabled),
            2 => Ok(Self::Disabled),
            _ => Err(UserTokenError::Internal),
        }
    }

    const fn database_value(self) -> i16 {
        match self {
            Self::Enabled => 1,
            Self::Disabled => 2,
        }
    }
}

/// 普通用户可见的非敏感 Key 快照。
pub struct UserToken {
    token_id: TokenId,
    key_prefix: String,
    name: String,
    status: UserTokenStatus,
    remain_quota: i64,
    unlimited_quota: bool,
    used_quota: i64,
    expired_at: Option<i64>,
    model_limits: Option<Vec<String>>,
    allow_ips: Option<Vec<String>>,
    created_at: i64,
    updated_at: i64,
}

impl UserToken {
    /// 组合已校验的用户 Key 字段，供边界适配器和测试实现使用。
    #[allow(clippy::too_many_arguments, reason = "字段与稳定用户 Key 响应一一对应")]
    #[must_use]
    pub fn from_parts(
        token_id: TokenId,
        key_prefix: String,
        name: String,
        status: UserTokenStatus,
        remain_quota: i64,
        unlimited_quota: bool,
        used_quota: i64,
        expired_at: Option<i64>,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
        created_at: i64,
        updated_at: i64,
    ) -> Self {
        Self {
            token_id,
            key_prefix,
            name,
            status,
            remain_quota,
            unlimited_quota,
            used_quota,
            expired_at,
            model_limits,
            allow_ips,
            created_at,
            updated_at,
        }
    }

    pub(super) fn from_record(record: UserTokenRecord) -> Result<Self, UserTokenError> {
        Ok(Self {
            token_id: record.token_id(),
            key_prefix: record.key_prefix().to_owned(),
            name: record.name().to_owned(),
            status: UserTokenStatus::from_database(record.status())?,
            remain_quota: record.remain_quota(),
            unlimited_quota: record.unlimited_quota(),
            used_quota: record.used_quota(),
            expired_at: record.expired_at(),
            model_limits: record.model_limits().map(|values| values.to_vec()),
            allow_ips: record.allow_ips().map(|values| values.to_vec()),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
        })
    }

    /// 返回 Key 标识。
    #[must_use]
    pub const fn token_id(&self) -> TokenId {
        self.token_id
    }

    /// 返回不可鉴权的展示前缀。
    #[must_use]
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    /// 返回 Key 名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回 Key 状态。
    #[must_use]
    pub const fn status(&self) -> UserTokenStatus {
        self.status
    }

    /// 返回有限 Key 的剩余额度。
    #[must_use]
    pub const fn remain_quota(&self) -> i64 {
        self.remain_quota
    }

    /// 返回是否跳过 Key 自身额度上限。
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

    /// 返回创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// 返回最近更新时间的 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

impl fmt::Debug for UserToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserToken(<redacted>)")
    }
}

/// 已校验的用户 Key 列表查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserTokenListQuery {
    after: Option<TokenId>,
    limit: usize,
}

impl UserTokenListQuery {
    /// 校验单调 ID 游标和页大小。
    pub fn new(after: Option<TokenId>, limit: usize) -> Result<Self, UserTokenError> {
        if !(1..=MAX_USER_TOKEN_PAGE_SIZE).contains(&limit) {
            return Err(UserTokenError::InvalidInput);
        }
        Ok(Self { after, limit })
    }

    /// 返回上一页末尾 Key 标识。
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

impl Default for UserTokenListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_USER_TOKEN_PAGE_SIZE,
        }
    }
}

/// 一页用户 Key 响应。
pub struct UserTokenPage {
    tokens: Vec<UserToken>,
    next_cursor: Option<TokenId>,
}

impl UserTokenPage {
    /// 组装 Key 列表与下一游标。
    #[must_use]
    pub fn from_parts(tokens: Vec<UserToken>, next_cursor: Option<TokenId>) -> Self {
        Self {
            tokens,
            next_cursor,
        }
    }

    /// 返回当前页 Key。
    #[must_use]
    pub fn tokens(&self) -> &[UserToken] {
        &self.tokens
    }

    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<TokenId> {
        self.next_cursor
    }
}

/// 普通用户创建或更新 Key 的命令。
pub struct UserTokenWriteCommand {
    name: String,
    status: UserTokenStatus,
    remain_quota: i64,
    unlimited_quota: bool,
    expired_at: Option<i64>,
    model_limits: Option<Vec<String>>,
    allow_ips: Option<Vec<String>>,
}

impl UserTokenWriteCommand {
    /// 校验名称、额度、有效期以及模型/IP 白名单。
    #[allow(clippy::too_many_arguments, reason = "字段与用户 Key 写入契约一一对应")]
    pub fn new(
        name: String,
        status: UserTokenStatus,
        remain_quota: i64,
        unlimited_quota: bool,
        expired_at: Option<i64>,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
    ) -> Result<Self, UserTokenError> {
        validate_name(&name).map_err(|_| UserTokenError::InvalidInput)?;
        if name == PLAYGROUND_TOKEN_NAME {
            return Err(UserTokenError::InvalidInput);
        }
        validate_non_negative(remain_quota).map_err(|_| UserTokenError::InvalidInput)?;
        if expired_at.is_some_and(|value| value < 0) {
            return Err(UserTokenError::InvalidInput);
        }
        validate_model_limits(model_limits.as_deref()).map_err(|_| UserTokenError::InvalidInput)?;
        validate_ip_allowlist(allow_ips.as_deref()).map_err(|_| UserTokenError::InvalidInput)?;
        Ok(Self {
            name,
            status,
            remain_quota,
            unlimited_quota,
            expired_at,
            model_limits,
            allow_ips,
        })
    }

    pub(super) fn into_record(self) -> UserTokenWriteRecord {
        UserTokenWriteRecord::new(
            self.name,
            self.status.database_value(),
            self.remain_quota,
            self.unlimited_quota,
            self.expired_at,
            self.model_limits,
            self.allow_ips,
        )
    }
}

impl fmt::Debug for UserTokenWriteCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTokenWriteCommand(<redacted>)")
    }
}

/// 一次性 Key 签发结果。
pub struct IssuedUserToken {
    token: UserToken,
    api_key: IssuedApiKey,
}

impl IssuedUserToken {
    /// 组合已持久化快照与一次性完整 Key。
    #[must_use]
    pub fn from_parts(token: UserToken, api_key: IssuedApiKey) -> Self {
        Self { token, api_key }
    }

    /// 返回非敏感 Key 快照。
    #[must_use]
    pub const fn token(&self) -> &UserToken {
        &self.token
    }

    /// 返回只允许本次签发响应展示的完整 Key。
    #[must_use]
    pub const fn api_key(&self) -> &PresentedApiKey {
        self.api_key.key()
    }
}

impl fmt::Debug for IssuedUserToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IssuedUserToken(<redacted>)")
    }
}

/// 用户 Key 应用服务稳定错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserTokenError {
    /// 请求字段或分页参数无效。
    #[error("用户 API Key 参数无效")]
    InvalidInput,
    /// 当前登录用户在写入前已经失效。
    #[error("用户 API Key 登录状态无效")]
    InvalidSession,
    /// 当前用户的未软删除 Key 已达上限。
    #[error("用户 API Key 数量已达上限")]
    LimitReached,
    /// Key 不存在、已经软删除或不属于当前用户。
    #[error("用户 API Key 不存在")]
    NotFound,
    /// 随机数、数据库或持久化状态发生内部故障。
    #[error("用户 API Key 内部失败")]
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_command_reuses_closed_policy_validation_and_redacts_values() {
        let command = UserTokenWriteCommand::new(
            "personal".to_owned(),
            UserTokenStatus::Enabled,
            100,
            false,
            Some(1_900_000_000),
            Some(vec!["gpt-5".to_owned()]),
            Some(vec!["192.0.2.0/24".to_owned()]),
        )
        .unwrap();
        assert_eq!(format!("{command:?}"), "UserTokenWriteCommand(<redacted>)");
        assert_eq!(
            UserTokenWriteCommand::new(
                " bad ".to_owned(),
                UserTokenStatus::Enabled,
                0,
                false,
                None,
                None,
                None,
            )
            .unwrap_err(),
            UserTokenError::InvalidInput
        );
        assert_eq!(
            UserTokenListQuery::new(None, MAX_USER_TOKEN_PAGE_SIZE + 1),
            Err(UserTokenError::InvalidInput)
        );
        assert_eq!(
            UserTokenWriteCommand::new(
                PLAYGROUND_TOKEN_NAME.to_owned(),
                UserTokenStatus::Enabled,
                0,
                true,
                None,
                None,
                None,
            )
            .unwrap_err(),
            UserTokenError::InvalidInput
        );
    }
}
