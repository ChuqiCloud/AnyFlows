use std::fmt;

use af_db::AdminCredentialRecord;
use af_domain::{ChannelId, CredentialId, CredentialKind};
use serde::{Deserialize, Serialize};

use crate::{AdminChannelReadError, AdminRoutingStatus, DEFAULT_ADMIN_CHANNEL_PAGE_SIZE};

/// 已校验的管理凭据列表查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminCredentialListQuery {
    after: Option<CredentialId>,
    limit: usize,
}

impl AdminCredentialListQuery {
    /// 校验单调 ID 游标和固定页大小边界。
    pub fn new(after: Option<CredentialId>, limit: usize) -> Result<Self, AdminChannelReadError> {
        if !(1..=af_db::MAX_ADMIN_CHANNEL_PAGE_SIZE).contains(&limit) {
            return Err(AdminChannelReadError::InvalidPagination);
        }
        Ok(Self { after, limit })
    }

    /// 返回上一页最后一个凭据 ID。
    #[must_use]
    pub const fn after(self) -> Option<CredentialId> {
        self.after
    }

    /// 返回本页最大记录数。
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for AdminCredentialListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ADMIN_CHANNEL_PAGE_SIZE,
        }
    }
}

/// 同渠道多凭据的稳定选择模式。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminCredentialMultiKeyMode {
    /// 每次从可调度凭据中随机选择。
    Random,
    /// 按凭据顺序轮询选择。
    RoundRobin,
}

impl AdminCredentialMultiKeyMode {
    fn from_database(value: i16) -> Result<Self, AdminChannelReadError> {
        match value {
            1 => Ok(Self::Random),
            2 => Ok(Self::RoundRobin),
            _ => Err(AdminChannelReadError::Internal),
        }
    }
}

/// 管理 API 沿用原有名称暴露领域层的稳定额度维度。
pub use af_domain::CredentialQuotaDimension as AdminCredentialQuotaDimension;

/// 管理 API 可读取的非敏感凭据元数据。
pub struct AdminCredential {
    credential_id: CredentialId,
    channel_id: ChannelId,
    kind: CredentialKind,
    status: AdminRoutingStatus,
    multi_key_mode: Option<AdminCredentialMultiKeyMode>,
    priority: i32,
    weight: i32,
    concurrency: Option<i32>,
    load_factor_micros: Option<i64>,
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    rate_limited_at: Option<i64>,
    rate_limit_reset_at: Option<i64>,
    overload_until: Option<i64>,
    temp_unschedulable_until: Option<i64>,
    shared_auth_cooling: bool,
    session_window_start: Option<i64>,
    session_window_end: Option<i64>,
    parent_id: Option<CredentialId>,
    quota_dimension: AdminCredentialQuotaDimension,
    proxy_id: Option<i64>,
    oauth_provider: Option<String>,
    oauth_token_pending: bool,
    oauth_account_key: Option<String>,
    oauth_project_id: Option<String>,
    oauth_revision: i64,
    last_used_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

impl AdminCredential {
    /// 组合已经完成持久化校验的凭据元数据，供测试适配器使用。
    #[allow(clippy::too_many_arguments, reason = "字段与稳定管理 API 响应一一对应")]
    #[must_use]
    pub fn from_parts(
        credential_id: CredentialId,
        channel_id: ChannelId,
        kind: CredentialKind,
        status: AdminRoutingStatus,
        multi_key_mode: Option<AdminCredentialMultiKeyMode>,
        priority: i32,
        weight: i32,
        concurrency: Option<i32>,
        load_factor_micros: Option<i64>,
        rate_multiplier_micros: Option<i64>,
        schedulable: bool,
        rate_limited_at: Option<i64>,
        rate_limit_reset_at: Option<i64>,
        overload_until: Option<i64>,
        temp_unschedulable_until: Option<i64>,
        shared_auth_cooling: bool,
        session_window_start: Option<i64>,
        session_window_end: Option<i64>,
        parent_id: Option<CredentialId>,
        quota_dimension: AdminCredentialQuotaDimension,
        proxy_id: Option<i64>,
        oauth_provider: Option<String>,
        oauth_token_pending: bool,
        oauth_account_key: Option<String>,
        oauth_project_id: Option<String>,
        oauth_revision: i64,
        last_used_at: Option<i64>,
        created_at: i64,
        updated_at: i64,
    ) -> Self {
        Self {
            credential_id,
            channel_id,
            kind,
            status,
            multi_key_mode,
            priority,
            weight,
            concurrency,
            load_factor_micros,
            rate_multiplier_micros,
            schedulable,
            rate_limited_at,
            rate_limit_reset_at,
            overload_until,
            temp_unschedulable_until,
            shared_auth_cooling,
            session_window_start,
            session_window_end,
            parent_id,
            quota_dimension,
            proxy_id,
            oauth_provider,
            oauth_token_pending,
            oauth_account_key,
            oauth_project_id,
            oauth_revision,
            last_used_at,
            created_at,
            updated_at,
        }
    }

    /// 返回凭据标识。
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.credential_id
    }
    /// 返回所属渠道标识。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }
    /// 返回凭据类型。
    #[must_use]
    pub const fn kind(&self) -> CredentialKind {
        self.kind
    }
    /// 返回凭据运行状态。
    #[must_use]
    pub const fn status(&self) -> AdminRoutingStatus {
        self.status
    }
    /// 返回同渠道多凭据选择模式。
    #[must_use]
    pub const fn multi_key_mode(&self) -> Option<AdminCredentialMultiKeyMode> {
        self.multi_key_mode
    }
    /// 返回凭据调度优先级。
    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }
    /// 返回凭据调度权重。
    #[must_use]
    pub const fn weight(&self) -> i32 {
        self.weight
    }
    /// 返回账号级并发限制。
    #[must_use]
    pub const fn concurrency(&self) -> Option<i32> {
        self.concurrency
    }
    /// 返回百万分比负载系数。
    #[must_use]
    pub const fn load_factor_micros(&self) -> Option<i64> {
        self.load_factor_micros
    }
    /// 返回百万分比上游成本倍率。
    #[must_use]
    pub const fn rate_multiplier_micros(&self) -> Option<i64> {
        self.rate_multiplier_micros
    }
    /// 返回是否参与调度。
    #[must_use]
    pub const fn schedulable(&self) -> bool {
        self.schedulable
    }
    /// 返回最近限流时间。
    #[must_use]
    pub const fn rate_limited_at(&self) -> Option<i64> {
        self.rate_limited_at
    }
    /// 返回限流恢复时间。
    #[must_use]
    pub const fn rate_limit_reset_at(&self) -> Option<i64> {
        self.rate_limit_reset_at
    }
    /// 返回过载冷却截止时间。
    #[must_use]
    pub const fn overload_until(&self) -> Option<i64> {
        self.overload_until
    }
    /// 返回临时不可调度截止时间。
    #[must_use]
    pub const fn temp_unschedulable_until(&self) -> Option<i64> {
        self.temp_unschedulable_until
    }

    /// 判断当前凭据能否作为可用的 Spark 影子母凭据。
    #[must_use]
    pub fn blocks_spark_shadow(&self, now_seconds: i64) -> bool {
        self.kind != CredentialKind::Oauth
            || self.status != AdminRoutingStatus::Enabled
            || self.oauth_token_pending
            || self.parent_id.is_some()
            || self.quota_dimension != AdminCredentialQuotaDimension::Global
            || (self.shared_auth_cooling
                && self
                    .temp_unschedulable_until
                    .is_some_and(|until| until > now_seconds))
    }
    /// 返回订阅额度窗口起点。
    #[must_use]
    pub const fn session_window_start(&self) -> Option<i64> {
        self.session_window_start
    }
    /// 返回订阅额度窗口终点。
    #[must_use]
    pub const fn session_window_end(&self) -> Option<i64> {
        self.session_window_end
    }
    /// 返回同渠道影子账号母凭据。
    #[must_use]
    pub const fn parent_id(&self) -> Option<CredentialId> {
        self.parent_id
    }
    /// 返回影子账号配额维度。
    #[must_use]
    pub const fn quota_dimension(&self) -> AdminCredentialQuotaDimension {
        self.quota_dimension
    }
    /// 返回专属代理标识。
    #[must_use]
    pub const fn proxy_id(&self) -> Option<i64> {
        self.proxy_id
    }
    /// 返回 OAuth 提供方标识。
    #[must_use]
    pub fn oauth_provider(&self) -> Option<&str> {
        self.oauth_provider.as_deref()
    }

    /// 返回 OAuth token 是否仍等待首次授权交换。
    #[must_use]
    pub const fn oauth_token_pending(&self) -> bool {
        self.oauth_token_pending
    }
    /// 返回 OAuth 账号的非令牌业务标识。
    #[must_use]
    pub fn oauth_account_key(&self) -> Option<&str> {
        self.oauth_account_key.as_deref()
    }
    /// 返回 OAuth 项目标识。
    #[must_use]
    pub fn oauth_project_id(&self) -> Option<&str> {
        self.oauth_project_id.as_deref()
    }
    /// 返回仅由 OAuth token 持久化推进的非敏感版本号。
    #[must_use]
    pub const fn oauth_revision(&self) -> i64 {
        self.oauth_revision
    }
    /// 返回最近调度时间。
    #[must_use]
    pub const fn last_used_at(&self) -> Option<i64> {
        self.last_used_at
    }
    /// 返回创建时间。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    /// 返回最后更新时间。
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }

    pub(crate) fn from_record(
        record: AdminCredentialRecord,
    ) -> Result<Self, AdminChannelReadError> {
        Ok(Self::from_parts(
            record.credential_id(),
            record.channel_id(),
            record.kind(),
            AdminRoutingStatus::from_domain(record.status()),
            record
                .multi_key_mode()
                .map(AdminCredentialMultiKeyMode::from_database)
                .transpose()?,
            record.priority(),
            record.weight(),
            record.concurrency(),
            record.load_factor_micros(),
            record.rate_multiplier_micros(),
            record.schedulable(),
            record.rate_limited_at(),
            record.rate_limit_reset_at(),
            record.overload_until(),
            record.temp_unschedulable_until(),
            record.shared_auth_cooling(),
            record.session_window_start(),
            record.session_window_end(),
            record.parent_id(),
            record.quota_dimension(),
            record.proxy_id(),
            record.oauth_provider().map(str::to_owned),
            record.oauth_token_pending(),
            record.oauth_account_key().map(str::to_owned),
            record.oauth_project_id().map(str::to_owned),
            record.oauth_revision(),
            record.last_used_at(),
            record.created_at(),
            record.updated_at(),
        ))
    }
}

impl fmt::Debug for AdminCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredential(<redacted>)")
    }
}

/// 一页管理凭据响应。
pub struct AdminCredentialPage {
    credentials: Vec<AdminCredential>,
    next_cursor: Option<CredentialId>,
}

impl AdminCredentialPage {
    /// 组合凭据列表和可选下一游标。
    #[must_use]
    pub fn from_parts(
        credentials: Vec<AdminCredential>,
        next_cursor: Option<CredentialId>,
    ) -> Self {
        Self {
            credentials,
            next_cursor,
        }
    }
    /// 返回当前页凭据。
    #[must_use]
    pub fn credentials(&self) -> &[AdminCredential] {
        &self.credentials
    }
    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<CredentialId> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminCredentialPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialPage(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oauth_parent(temp_until: Option<i64>, shared_auth_cooling: bool) -> AdminCredential {
        AdminCredential::from_parts(
            CredentialId::new(1).unwrap(),
            ChannelId::new(1).unwrap(),
            CredentialKind::Oauth,
            AdminRoutingStatus::Enabled,
            None,
            0,
            1,
            Some(1),
            None,
            None,
            true,
            None,
            None,
            None,
            temp_until,
            shared_auth_cooling,
            None,
            None,
            None,
            AdminCredentialQuotaDimension::Global,
            None,
            Some("codex".to_owned()),
            false,
            None,
            None,
            0,
            None,
            1,
            1,
        )
    }

    #[test]
    fn spark_shadow_blocking_ignores_local_quota_and_expires_auth_cooling() {
        assert!(!oauth_parent(Some(2_000), false).blocks_spark_shadow(1_000));
        assert!(oauth_parent(Some(2_000), true).blocks_spark_shadow(1_000));
        assert!(!oauth_parent(Some(2_000), true).blocks_spark_shadow(2_000));
    }
}
