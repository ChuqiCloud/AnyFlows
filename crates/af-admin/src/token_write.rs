use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminTokenCreateRecord, AdminTokenDeleteOutcome, AdminTokenMutationOutcome,
    AdminTokenRepository, AdminTokenWriteRecord, AdminTokenWriteRepositoryError,
};
use af_domain::{GroupId, IpCidr, TokenId, TokenModelPolicy, UserId};
use thiserror::Error;

use crate::{
    AdminToken, AdminTokenReadError, AdminTokenStatus, IssuedApiKey, PresentedApiKey,
    SessionPrincipal, SessionRole,
};

/// 单个令牌 IP 白名单允许的最大条目数。
pub const MAX_ADMIN_TOKEN_IP_ALLOWLIST_COUNT: usize = 64;
/// 单条令牌 IP/CIDR 文本允许的最大 UTF-8 字节数。
pub const MAX_ADMIN_TOKEN_IP_ALLOWLIST_ITEM_BYTES: usize = 64;
/// 单个令牌 IP 白名单允许的原始文本总字节数。
pub const MAX_ADMIN_TOKEN_IP_ALLOWLIST_TEXT_BYTES: usize = 4_096;

/// 管理员签发令牌时使用的完整配置。
pub struct AdminTokenCreateCommand {
    fields: AdminTokenWriteFields,
}

impl AdminTokenCreateCommand {
    /// 校验所属用户、令牌配置、白名单和全部非负限额。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端令牌写入契约一一对应"
    )]
    pub fn new(
        user_id: UserId,
        name: String,
        status: AdminTokenStatus,
        group_id: Option<GroupId>,
        remain_quota: i64,
        unlimited_quota: bool,
        expired_at: Option<i64>,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
        cross_group_retry: bool,
        rate_limit_5h: Option<i64>,
        rate_limit_1d: Option<i64>,
        rate_limit_7d: Option<i64>,
        max_requests: Option<i64>,
    ) -> Result<Self, AdminTokenWriteError> {
        Ok(Self {
            fields: AdminTokenWriteFields::new(
                user_id,
                name,
                status,
                group_id,
                remain_quota,
                unlimited_quota,
                expired_at,
                model_limits,
                allow_ips,
                cross_group_retry,
                rate_limit_5h,
                rate_limit_1d,
                rate_limit_7d,
                max_requests,
            )?,
        })
    }

    fn into_parts(self) -> (UserId, AdminTokenWriteRecord) {
        (self.fields.user_id, self.fields.into_record())
    }
}

impl fmt::Debug for AdminTokenCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminTokenCreateCommand(<redacted>)")
    }
}

/// 管理员完整更新令牌时使用的配置；用户 ID 用于确认归属，不允许转移令牌。
pub struct AdminTokenUpdateCommand {
    fields: AdminTokenWriteFields,
}

impl AdminTokenUpdateCommand {
    /// 校验完整更新边界；密钥和累计使用状态不属于可写字段。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端令牌写入契约一一对应"
    )]
    pub fn new(
        user_id: UserId,
        name: String,
        status: AdminTokenStatus,
        group_id: Option<GroupId>,
        remain_quota: i64,
        unlimited_quota: bool,
        expired_at: Option<i64>,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
        cross_group_retry: bool,
        rate_limit_5h: Option<i64>,
        rate_limit_1d: Option<i64>,
        rate_limit_7d: Option<i64>,
        max_requests: Option<i64>,
    ) -> Result<Self, AdminTokenWriteError> {
        Ok(Self {
            fields: AdminTokenWriteFields::new(
                user_id,
                name,
                status,
                group_id,
                remain_quota,
                unlimited_quota,
                expired_at,
                model_limits,
                allow_ips,
                cross_group_retry,
                rate_limit_5h,
                rate_limit_1d,
                rate_limit_7d,
                max_requests,
            )?,
        })
    }

    fn into_parts(self) -> (UserId, AdminTokenWriteRecord) {
        (self.fields.user_id, self.fields.into_record())
    }
}

impl fmt::Debug for AdminTokenUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminTokenUpdateCommand(<redacted>)")
    }
}

struct AdminTokenWriteFields {
    user_id: UserId,
    name: String,
    status: AdminTokenStatus,
    group_id: Option<GroupId>,
    remain_quota: i64,
    unlimited_quota: bool,
    expired_at: Option<i64>,
    model_limits: Option<Vec<String>>,
    allow_ips: Option<Vec<String>>,
    cross_group_retry: bool,
    rate_limit_5h: Option<i64>,
    rate_limit_1d: Option<i64>,
    rate_limit_7d: Option<i64>,
    max_requests: Option<i64>,
}

impl AdminTokenWriteFields {
    #[allow(
        clippy::too_many_arguments,
        reason = "仅在统一校验入口组装完整令牌字段"
    )]
    fn new(
        user_id: UserId,
        name: String,
        status: AdminTokenStatus,
        group_id: Option<GroupId>,
        remain_quota: i64,
        unlimited_quota: bool,
        expired_at: Option<i64>,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
        cross_group_retry: bool,
        rate_limit_5h: Option<i64>,
        rate_limit_1d: Option<i64>,
        rate_limit_7d: Option<i64>,
        max_requests: Option<i64>,
    ) -> Result<Self, AdminTokenWriteError> {
        validate_name(&name)?;
        validate_non_negative(remain_quota)?;
        if expired_at.is_some_and(|value| value < 0)
            || [rate_limit_5h, rate_limit_1d, rate_limit_7d, max_requests]
                .into_iter()
                .flatten()
                .any(|value| value < 0)
        {
            return Err(AdminTokenWriteError::InvalidInput);
        }
        validate_model_limits(model_limits.as_deref())?;
        validate_ip_allowlist(allow_ips.as_deref())?;
        Ok(Self {
            user_id,
            name,
            status,
            group_id,
            remain_quota,
            unlimited_quota,
            expired_at,
            model_limits,
            allow_ips,
            cross_group_retry,
            rate_limit_5h,
            rate_limit_1d,
            rate_limit_7d,
            max_requests,
        })
    }

    fn into_record(self) -> AdminTokenWriteRecord {
        AdminTokenWriteRecord::new(
            self.name,
            match self.status {
                AdminTokenStatus::Enabled => 1,
                AdminTokenStatus::Disabled => 2,
            },
            self.group_id,
            self.remain_quota,
            self.unlimited_quota,
            self.expired_at,
            self.model_limits,
            self.allow_ips,
            self.cross_group_retry,
            self.rate_limit_5h,
            self.rate_limit_1d,
            self.rate_limit_7d,
            self.max_requests,
        )
    }
}

/// 一次性令牌签发结果；完整密钥只能由 HTTP 签发响应显式读取。
pub struct IssuedAdminToken {
    token: AdminToken,
    api_key: IssuedApiKey,
}

impl IssuedAdminToken {
    /// 组合已持久化快照与一次性签发材料，供受控适配器和测试实现使用。
    #[must_use]
    pub fn from_parts(token: AdminToken, api_key: IssuedApiKey) -> Self {
        Self { token, api_key }
    }

    /// 返回已持久化的非敏感令牌快照。
    #[must_use]
    pub const fn token(&self) -> &AdminToken {
        &self.token
    }

    /// 返回仅允许在本次签发响应展示的完整 API Key。
    #[must_use]
    pub const fn api_key(&self) -> &PresentedApiKey {
        self.api_key.key()
    }
}

impl fmt::Debug for IssuedAdminToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IssuedAdminToken(<redacted>)")
    }
}

/// 管理令牌写入失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminTokenWriteError {
    /// 请求字段、用户归属或分组引用无效。
    #[error("管理令牌写入参数无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("管理令牌写入权限不足")]
    Forbidden,
    /// 令牌不存在或已经软删除。
    #[error("管理令牌不存在")]
    NotFound,
    /// 用户现有未软删除令牌已经达到统一容量上限。
    #[error("用户 API Key 数量已达上限")]
    LimitReached,
    /// 随机数、数据库或持久化状态发生内部故障。
    #[error("管理令牌写入内部失败")]
    Internal,
}

/// 管理令牌签发调用的对象安全 Future。
pub type AdminTokenCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<IssuedAdminToken, AdminTokenWriteError>> + Send + 'a>>;
/// 管理令牌更新调用的对象安全 Future。
pub type AdminTokenUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminToken, AdminTokenWriteError>> + Send + 'a>>;
/// 管理令牌删除调用的对象安全 Future。
pub type AdminTokenDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminTokenWriteError>> + Send + 'a>>;

/// 管理令牌写入应用端口；角色校验必须在签发密钥和访问仓储前完成。
pub trait AdminTokenWriter: Send + Sync {
    /// 签发令牌，完整 API Key 只随本次结果返回。
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminTokenCreateCommand,
    ) -> AdminTokenCreateFuture<'a>;

    /// 完整更新令牌配置，不轮换密钥或重置累计状态。
    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        token_id: TokenId,
        command: AdminTokenUpdateCommand,
    ) -> AdminTokenUpdateFuture<'a>;

    /// 软删除令牌，阻止后续鉴权但保留历史审计关联。
    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        token_id: TokenId,
    ) -> AdminTokenDeleteFuture<'a>;
}

/// 使用数据库仓储实现管理员令牌写入。
pub struct DatabaseAdminTokenWriter {
    repository: AdminTokenRepository,
}

impl DatabaseAdminTokenWriter {
    /// 绑定已经配置查询和写入截止时间的令牌仓储。
    #[must_use]
    pub const fn new(repository: AdminTokenRepository) -> Self {
        Self { repository }
    }
}

impl AdminTokenWriter for DatabaseAdminTokenWriter {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminTokenCreateCommand,
    ) -> AdminTokenCreateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let issued = IssuedApiKey::generate().map_err(|_| AdminTokenWriteError::Internal)?;
            let (user_id, fields) = command.into_parts();
            let record = AdminTokenCreateRecord::new(
                user_id,
                issued.digest().as_str().to_owned(),
                issued.display_prefix().as_str().to_owned(),
                fields,
            );
            let token = self
                .repository
                .create_token(record)
                .await
                .map_err(map_repository_error)
                .and_then(map_record)?;
            Ok(IssuedAdminToken {
                token,
                api_key: issued,
            })
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        token_id: TokenId,
        command: AdminTokenUpdateCommand,
    ) -> AdminTokenUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let (expected_user_id, fields) = command.into_parts();
            match self
                .repository
                .update_token(token_id, expected_user_id, fields)
                .await
                .map_err(map_repository_error)?
            {
                AdminTokenMutationOutcome::Mutated(record) => map_record(*record),
                AdminTokenMutationOutcome::NotFound => Err(AdminTokenWriteError::NotFound),
            }
        })
    }

    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        token_id: TokenId,
    ) -> AdminTokenDeleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .delete_token(token_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminTokenDeleteOutcome::Deleted => Ok(()),
                AdminTokenDeleteOutcome::NotFound => Err(AdminTokenWriteError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminTokenWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminTokenWriter(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminTokenWriteError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminTokenWriteError::Forbidden)
    }
}

fn map_repository_error(error: AdminTokenWriteRepositoryError) -> AdminTokenWriteError {
    match error {
        AdminTokenWriteRepositoryError::InvalidInput
        | AdminTokenWriteRepositoryError::InvalidReference => AdminTokenWriteError::InvalidInput,
        AdminTokenWriteRepositoryError::LimitReached => AdminTokenWriteError::LimitReached,
        AdminTokenWriteRepositoryError::Query
        | AdminTokenWriteRepositoryError::Timeout
        | AdminTokenWriteRepositoryError::Invariant => AdminTokenWriteError::Internal,
    }
}

fn map_record(record: af_db::AdminTokenRecord) -> Result<AdminToken, AdminTokenWriteError> {
    AdminToken::from_record(record).map_err(map_read_error)
}

fn map_read_error(error: AdminTokenReadError) -> AdminTokenWriteError {
    let _ = error;
    AdminTokenWriteError::Internal
}

pub(crate) fn validate_name(name: &str) -> Result<(), AdminTokenWriteError> {
    if name.is_empty()
        || name.len() > 128
        || name.trim() != name
        || name.chars().any(char::is_control)
    {
        return Err(AdminTokenWriteError::InvalidInput);
    }
    Ok(())
}

pub(crate) fn validate_non_negative(value: i64) -> Result<(), AdminTokenWriteError> {
    if value < 0 {
        return Err(AdminTokenWriteError::InvalidInput);
    }
    Ok(())
}

pub(crate) fn validate_model_limits(
    model_limits: Option<&[String]>,
) -> Result<(), AdminTokenWriteError> {
    let Some(model_limits) = model_limits else {
        return Ok(());
    };
    TokenModelPolicy::try_from_allowlist(model_limits.to_vec())
        .map(|_| ())
        .map_err(|_| AdminTokenWriteError::InvalidInput)
}

pub(crate) fn validate_ip_allowlist(
    allow_ips: Option<&[String]>,
) -> Result<(), AdminTokenWriteError> {
    let Some(allow_ips) = allow_ips else {
        return Ok(());
    };
    if allow_ips.is_empty() || allow_ips.len() > MAX_ADMIN_TOKEN_IP_ALLOWLIST_COUNT {
        return Err(AdminTokenWriteError::InvalidInput);
    }
    let mut total_text_bytes = 0_usize;
    for entry in allow_ips {
        total_text_bytes = total_text_bytes
            .checked_add(entry.len())
            .filter(|total| *total <= MAX_ADMIN_TOKEN_IP_ALLOWLIST_TEXT_BYTES)
            .ok_or(AdminTokenWriteError::InvalidInput)?;
        if entry.is_empty()
            || entry.len() > MAX_ADMIN_TOKEN_IP_ALLOWLIST_ITEM_BYTES
            || entry.parse::<IpCidr>().is_err()
        {
            return Err(AdminTokenWriteError::InvalidInput);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(
        name: &str,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
    ) -> Result<AdminTokenCreateCommand, AdminTokenWriteError> {
        AdminTokenCreateCommand::new(
            UserId::new(1).unwrap(),
            name.to_owned(),
            AdminTokenStatus::Enabled,
            None,
            100,
            false,
            None,
            model_limits,
            allow_ips,
            true,
            Some(10),
            Some(20),
            Some(30),
            Some(40),
        )
    }

    #[test]
    fn commands_validate_configuration_and_redact_allowlists() {
        let valid_command = command(
            "primary",
            Some(vec!["gpt-5.5".to_owned(), "gpt-5.5".to_owned()]),
            Some(vec!["192.0.2.0/24".to_owned()]),
        )
        .unwrap();
        assert_eq!(
            format!("{valid_command:?}"),
            "AdminTokenCreateCommand(<redacted>)"
        );
        assert!(!format!("{valid_command:?}").contains("gpt-5.5"));

        assert_eq!(
            command(" bad ", None, None).unwrap_err(),
            AdminTokenWriteError::InvalidInput
        );
        assert_eq!(
            command("valid", Some(Vec::new()), None).unwrap_err(),
            AdminTokenWriteError::InvalidInput
        );
        assert_eq!(
            command("valid", None, Some(vec!["invalid-ip".to_owned()])).unwrap_err(),
            AdminTokenWriteError::InvalidInput
        );
    }

    #[test]
    fn normal_user_is_rejected_before_key_generation() {
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminTokenWriteError::Forbidden)
        );
    }
}
