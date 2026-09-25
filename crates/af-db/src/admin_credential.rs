use std::{fmt, str::FromStr as _};

use af_domain::{ChannelId, CredentialId, CredentialKind, CredentialQuotaDimension, Status};
use sea_orm::{
    ColumnTrait, Condition, DbErr, EntityTrait, QueryFilter, QueryOrder, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Order, Query, SelectStatement},
};
use tokio::time::timeout;

use crate::{
    AdminChannelRepository, AdminChannelRepositoryError, EncryptedCredentialEnvelope,
    MAX_ADMIN_CHANNEL_PAGE_SIZE,
    admin_channel::{internal_invariant, record_internal_error, valid_text},
    entity::{channels, credentials},
};

/// 管理员导出 OAuth 账号池时使用的密文视图；明文只在上层解密后短暂存在。
pub struct AdminCredentialSecretRecord {
    credential: AdminCredentialRecord,
    envelope: EncryptedCredentialEnvelope,
}

impl AdminCredentialSecretRecord {
    #[must_use]
    pub fn credential(&self) -> &AdminCredentialRecord {
        &self.credential
    }

    #[must_use]
    pub fn envelope(&self) -> &EncryptedCredentialEnvelope {
        &self.envelope
    }
}

/// 管理端可读取的非敏感凭据元数据。
pub struct AdminCredentialRecord {
    credential_id: CredentialId,
    channel_id: ChannelId,
    kind: CredentialKind,
    status: Status,
    multi_key_mode: Option<i16>,
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
    quota_dimension: CredentialQuotaDimension,
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

impl AdminCredentialRecord {
    pub(crate) fn try_from_model(
        model: credentials::Model,
    ) -> Result<Self, AdminChannelRepositoryError> {
        AdminCredentialRow {
            credential_id: model.id,
            channel_id: model.channel_id,
            kind: model.kind,
            status: model.status,
            multi_key_mode: model.multi_key_mode,
            priority: model.priority,
            weight: model.weight,
            concurrency: model.concurrency,
            load_factor_micros: model.load_factor_micros,
            rate_multiplier_micros: model.rate_multiplier_micros,
            schedulable: model.schedulable,
            rate_limited_at: model.rate_limited_at,
            rate_limit_reset_at: model.rate_limit_reset_at,
            overload_until: model.overload_until,
            temp_unschedulable_until: model.temp_unschedulable_until,
            temp_unschedulable_reason: model.temp_unschedulable_reason,
            session_window_start: model.session_window_start,
            session_window_end: model.session_window_end,
            parent_id: model.parent_id,
            quota_dimension: model.quota_dimension,
            proxy_id: model.proxy_id,
            oauth_provider: model.oauth_provider,
            oauth_token_pending: model.oauth_token_pending,
            oauth_account_key: model.oauth_account_key,
            oauth_project_id: model.oauth_project_id,
            oauth_revision: model.oauth_revision,
            last_used_at: model.last_used_at,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
        .validate()
    }

    /// 返回凭据主键。
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.credential_id
    }

    /// 返回凭据所属渠道。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回凭据类型；不包含任何凭据内容。
    #[must_use]
    pub const fn kind(&self) -> CredentialKind {
        self.kind
    }

    /// 返回凭据运行状态。
    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }

    /// 返回同渠道多凭据选择模式的稳定数值。
    #[must_use]
    pub const fn multi_key_mode(&self) -> Option<i16> {
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

    /// 返回可选的账号级并发限制。
    #[must_use]
    pub const fn concurrency(&self) -> Option<i32> {
        self.concurrency
    }

    /// 返回百万分比负载系数。
    #[must_use]
    pub const fn load_factor_micros(&self) -> Option<i64> {
        self.load_factor_micros
    }

    /// 返回百万分比上游成本倍率；该值不参与用户扣费。
    #[must_use]
    pub const fn rate_multiplier_micros(&self) -> Option<i64> {
        self.rate_multiplier_micros
    }

    /// 返回凭据是否允许进入调度。
    #[must_use]
    pub const fn schedulable(&self) -> bool {
        self.schedulable
    }

    /// 返回最近一次限流时间的 Unix 秒数。
    #[must_use]
    pub const fn rate_limited_at(&self) -> Option<i64> {
        self.rate_limited_at
    }

    /// 返回限流恢复时间的 Unix 秒数。
    #[must_use]
    pub const fn rate_limit_reset_at(&self) -> Option<i64> {
        self.rate_limit_reset_at
    }

    /// 返回过载冷却截止时间的 Unix 秒数。
    #[must_use]
    pub const fn overload_until(&self) -> Option<i64> {
        self.overload_until
    }

    /// 返回临时不可调度截止时间的 Unix 秒数。
    #[must_use]
    pub const fn temp_unschedulable_until(&self) -> Option<i64> {
        self.temp_unschedulable_until
    }

    /// 返回临时不可调度原因是否属于母子共享的认证健康域。
    #[must_use]
    pub const fn shared_auth_cooling(&self) -> bool {
        self.shared_auth_cooling
    }

    /// 返回订阅额度窗口起点的 Unix 秒数。
    #[must_use]
    pub const fn session_window_start(&self) -> Option<i64> {
        self.session_window_start
    }

    /// 返回订阅额度窗口终点的 Unix 秒数。
    #[must_use]
    pub const fn session_window_end(&self) -> Option<i64> {
        self.session_window_end
    }

    /// 返回同渠道影子账号的母凭据。
    #[must_use]
    pub const fn parent_id(&self) -> Option<CredentialId> {
        self.parent_id
    }

    /// 返回影子账号配额维度。
    #[must_use]
    pub const fn quota_dimension(&self) -> CredentialQuotaDimension {
        self.quota_dimension
    }

    /// 返回可选的专属代理标识。
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

    /// 返回最近一次调度时间的 Unix 秒数。
    #[must_use]
    pub const fn last_used_at(&self) -> Option<i64> {
        self.last_used_at
    }

    /// 返回创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// 返回最后更新时间的 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

impl fmt::Debug for AdminCredentialRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialRecord(<redacted>)")
    }
}

/// 一页有界凭据结果。
pub struct AdminCredentialPageRecord {
    credentials: Vec<AdminCredentialRecord>,
    next_cursor: Option<CredentialId>,
}

impl AdminCredentialPageRecord {
    /// 消费页面并返回凭据记录和下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<AdminCredentialRecord>, Option<CredentialId>) {
        (self.credentials, self.next_cursor)
    }
}

impl fmt::Debug for AdminCredentialPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialPageRecord(<redacted>)")
    }
}

/// 渠道凭据列表查询结果。
pub enum AdminCredentialPageOutcome {
    /// 渠道存在，并返回其中一页未软删除凭据。
    Found(AdminCredentialPageRecord),
    /// 父渠道不存在或已经软删除。
    ChannelNotFound,
}

impl fmt::Debug for AdminCredentialPageOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found(_) => formatter.write_str("AdminCredentialPageOutcome::Found(<redacted>)"),
            Self::ChannelNotFound => {
                formatter.write_str("AdminCredentialPageOutcome::ChannelNotFound")
            }
        }
    }
}

/// 单个渠道凭据查询结果。
pub enum AdminCredentialLookupOutcome {
    /// 找到指定渠道下的未软删除凭据。
    Found(Box<AdminCredentialRecord>),
    /// 父渠道不存在或已经软删除。
    ChannelNotFound,
    /// 凭据不存在、已软删除或不属于指定渠道。
    CredentialNotFound,
}

impl fmt::Debug for AdminCredentialLookupOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found(_) => {
                formatter.write_str("AdminCredentialLookupOutcome::Found(<redacted>)")
            }
            Self::ChannelNotFound => {
                formatter.write_str("AdminCredentialLookupOutcome::ChannelNotFound")
            }
            Self::CredentialNotFound => {
                formatter.write_str("AdminCredentialLookupOutcome::CredentialNotFound")
            }
        }
    }
}

impl AdminChannelRepository {
    /// 读取指定渠道的全部有效 OAuth 密文，供管理员导出使用。
    pub async fn list_oauth_credential_secrets(
        &self,
        channel_id: ChannelId,
    ) -> Result<Option<Vec<AdminCredentialSecretRecord>>, AdminChannelRepositoryError> {
        if !self.channel_exists(channel_id).await? {
            return Ok(None);
        }
        let query = credentials::Entity::find()
            .filter(credentials::Column::ChannelId.eq(channel_id.get()))
            .filter(credentials::Column::Kind.eq(CredentialKind::Oauth.as_str()))
            .filter(
                Condition::any()
                    .add(credentials::Column::OauthProvider.eq("codex"))
                    .add(credentials::Column::OauthProvider.is_null()),
            )
            .filter(credentials::Column::QuotaDimension.eq("global"))
            .filter(credentials::Column::OauthTokenPending.eq(false))
            .filter(credentials::Column::DeletedAt.is_null())
            .order_by_asc(credentials::Column::Id);
        let operation = query.all(self.pool.connection());
        let models = match timeout(self.lookup_timeout, operation).await {
            Ok(Ok(models)) => models,
            Ok(Err(_)) => return Err(record_internal_error(AdminChannelRepositoryError::Query)),
            Err(_) => return Err(record_internal_error(AdminChannelRepositoryError::Timeout)),
        };
        models
            .into_iter()
            .map(|model| {
                let credential = AdminCredentialRecord::try_from_model(model.clone())?;
                let (key_id, nonce, ciphertext) = model
                    .secret
                    .envelope_parts()
                    .map_err(|_| internal_invariant())?;
                let envelope = EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
                    .map_err(|_| internal_invariant())?;
                Ok(AdminCredentialSecretRecord {
                    credential,
                    envelope,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    /// 按凭据 ID 游标读取指定渠道下的一页非敏感凭据元数据。
    pub async fn list_credentials(
        &self,
        channel_id: ChannelId,
        after: Option<CredentialId>,
        limit: usize,
    ) -> Result<AdminCredentialPageOutcome, AdminChannelRepositoryError> {
        if !(1..=MAX_ADMIN_CHANNEL_PAGE_SIZE).contains(&limit) {
            return Err(internal_invariant());
        }
        if !self.channel_exists(channel_id).await? {
            return Ok(AdminCredentialPageOutcome::ChannelNotFound);
        }
        let mut results = self
            .query_with_timeout(credential_list_query(channel_id, after, limit))
            .await?;
        let has_more = results.len() > limit;
        if has_more {
            results.truncate(limit);
        }
        let credentials = results
            .iter()
            .map(AdminCredentialRow::try_from_query_result)
            .map(|row| row.map_err(|_| internal_invariant()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(AdminCredentialRow::validate)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| credentials.last().map(AdminCredentialRecord::credential_id))
            .flatten();
        Ok(AdminCredentialPageOutcome::Found(
            AdminCredentialPageRecord {
                credentials,
                next_cursor,
            },
        ))
    }

    /// 查询指定渠道下的单个凭据，父渠道与凭据缺失使用独立结果。
    pub async fn get_credential(
        &self,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> Result<AdminCredentialLookupOutcome, AdminChannelRepositoryError> {
        if !self.channel_exists(channel_id).await? {
            return Ok(AdminCredentialLookupOutcome::ChannelNotFound);
        }
        let mut results = self
            .query_with_timeout(credential_detail_query(channel_id, credential_id))
            .await?;
        match results.len() {
            0 => Ok(AdminCredentialLookupOutcome::CredentialNotFound),
            1 => {
                let row = AdminCredentialRow::try_from_query_result(
                    &results.pop().ok_or_else(internal_invariant)?,
                )
                .map_err(|_| internal_invariant())?;
                Ok(AdminCredentialLookupOutcome::Found(Box::new(
                    row.validate()?,
                )))
            }
            _ => Err(internal_invariant()),
        }
    }

    async fn channel_exists(
        &self,
        channel_id: ChannelId,
    ) -> Result<bool, AdminChannelRepositoryError> {
        let results = self
            .query_with_timeout(channel_exists_query(channel_id))
            .await?;
        match results.len() {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(internal_invariant()),
        }
    }
}

struct AdminCredentialRow {
    credential_id: i64,
    channel_id: i64,
    kind: String,
    status: i16,
    multi_key_mode: Option<i16>,
    priority: i32,
    weight: i32,
    concurrency: Option<i32>,
    load_factor_micros: Option<i64>,
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    rate_limited_at: Option<TimeDateTimeWithTimeZone>,
    rate_limit_reset_at: Option<TimeDateTimeWithTimeZone>,
    overload_until: Option<TimeDateTimeWithTimeZone>,
    temp_unschedulable_until: Option<TimeDateTimeWithTimeZone>,
    temp_unschedulable_reason: Option<String>,
    session_window_start: Option<TimeDateTimeWithTimeZone>,
    session_window_end: Option<TimeDateTimeWithTimeZone>,
    parent_id: Option<i64>,
    quota_dimension: String,
    proxy_id: Option<i64>,
    oauth_provider: Option<String>,
    oauth_token_pending: bool,
    oauth_account_key: Option<String>,
    oauth_project_id: Option<String>,
    oauth_revision: i64,
    last_used_at: Option<TimeDateTimeWithTimeZone>,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
}

impl AdminCredentialRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            credential_id: result.try_get("", "credential_id")?,
            channel_id: result.try_get("", "channel_id")?,
            kind: result.try_get("", "kind")?,
            status: result.try_get("", "status")?,
            multi_key_mode: result.try_get("", "multi_key_mode")?,
            priority: result.try_get("", "priority")?,
            weight: result.try_get("", "weight")?,
            concurrency: result.try_get("", "concurrency")?,
            load_factor_micros: result.try_get("", "load_factor_micros")?,
            rate_multiplier_micros: result.try_get("", "rate_multiplier_micros")?,
            schedulable: result.try_get("", "schedulable")?,
            rate_limited_at: result.try_get("", "rate_limited_at")?,
            rate_limit_reset_at: result.try_get("", "rate_limit_reset_at")?,
            overload_until: result.try_get("", "overload_until")?,
            temp_unschedulable_until: result.try_get("", "temp_unschedulable_until")?,
            temp_unschedulable_reason: result.try_get("", "temp_unschedulable_reason")?,
            session_window_start: result.try_get("", "session_window_start")?,
            session_window_end: result.try_get("", "session_window_end")?,
            parent_id: result.try_get("", "parent_id")?,
            quota_dimension: result.try_get("", "quota_dimension")?,
            proxy_id: result.try_get("", "proxy_id")?,
            oauth_provider: result.try_get("", "oauth_provider")?,
            oauth_token_pending: result.try_get("", "oauth_token_pending")?,
            oauth_account_key: result.try_get("", "oauth_account_key")?,
            oauth_project_id: result.try_get("", "oauth_project_id")?,
            oauth_revision: result.try_get("", "oauth_revision")?,
            last_used_at: result.try_get("", "last_used_at")?,
            created_at: result.try_get("", "created_at")?,
            updated_at: result.try_get("", "updated_at")?,
        })
    }

    fn validate(self) -> Result<AdminCredentialRecord, AdminChannelRepositoryError> {
        let credential_id =
            CredentialId::new(self.credential_id).map_err(|_| internal_invariant())?;
        let channel_id = ChannelId::new(self.channel_id).map_err(|_| internal_invariant())?;
        let kind = CredentialKind::from_str(&self.kind).map_err(|_| internal_invariant())?;
        let status = Status::try_from(self.status).map_err(|_| internal_invariant())?;
        let parent_id = self
            .parent_id
            .map(CredentialId::new)
            .transpose()
            .map_err(|_| internal_invariant())?;
        let quota_dimension = CredentialQuotaDimension::from_str(&self.quota_dimension)
            .map_err(|_| internal_invariant())?;
        let created_at = self.created_at.unix_timestamp();
        let updated_at = self.updated_at.unix_timestamp();
        let rate_limited_at = unix_timestamp(self.rate_limited_at)?;
        let rate_limit_reset_at = unix_timestamp(self.rate_limit_reset_at)?;
        let overload_until = unix_timestamp(self.overload_until)?;
        let temp_unschedulable_until = unix_timestamp(self.temp_unschedulable_until)?;
        // quota_exhausted 只属于当前额度维度；未知旧原因按共享故障失败关闭。
        let shared_auth_cooling = !matches!(
            self.temp_unschedulable_reason.as_deref(),
            None | Some("quota_exhausted")
        );
        let session_window_start = unix_timestamp(self.session_window_start)?;
        let session_window_end = unix_timestamp(self.session_window_end)?;
        let last_used_at = unix_timestamp(self.last_used_at)?;
        if self
            .multi_key_mode
            .is_some_and(|value| !matches!(value, 1 | 2))
            || self.weight < 0
            || self.concurrency.is_some_and(|value| value < 0)
            || self.load_factor_micros.is_some_and(|value| value < 0)
            || self.rate_multiplier_micros.is_some_and(|value| value < 0)
            || parent_id == Some(credential_id)
            || self.proxy_id.is_some_and(|value| value <= 0)
            || !valid_optional_text(self.oauth_provider.as_deref(), 64)
            || !valid_optional_text(self.oauth_account_key.as_deref(), 255)
            || !valid_optional_text(self.oauth_project_id.as_deref(), 255)
            || (self.oauth_token_pending
                && (kind != CredentialKind::Oauth || self.oauth_provider.is_none()))
            || self.oauth_revision < 0
            || created_at < 0
            || updated_at < created_at
            || session_window_start.is_some() != session_window_end.is_some()
        {
            return Err(internal_invariant());
        }
        Ok(AdminCredentialRecord {
            credential_id,
            channel_id,
            kind,
            status,
            multi_key_mode: self.multi_key_mode,
            priority: self.priority,
            weight: self.weight,
            concurrency: self.concurrency,
            load_factor_micros: self.load_factor_micros,
            rate_multiplier_micros: self.rate_multiplier_micros,
            schedulable: self.schedulable,
            rate_limited_at,
            rate_limit_reset_at,
            overload_until,
            temp_unschedulable_until,
            shared_auth_cooling,
            session_window_start,
            session_window_end,
            parent_id,
            quota_dimension,
            proxy_id: self.proxy_id,
            oauth_provider: self.oauth_provider,
            oauth_token_pending: self.oauth_token_pending,
            oauth_account_key: self.oauth_account_key,
            oauth_project_id: self.oauth_project_id,
            oauth_revision: self.oauth_revision,
            last_used_at,
            created_at,
            updated_at,
        })
    }
}

fn credential_list_query(
    channel_id: ChannelId,
    after: Option<CredentialId>,
    limit: usize,
) -> SelectStatement {
    let mut query = credential_base_query();
    query
        .and_where(
            Expr::col((credentials::Entity, credentials::Column::ChannelId)).eq(channel_id.get()),
        )
        .and_where(Expr::col((credentials::Entity, credentials::Column::DeletedAt)).is_null())
        .and_where(Expr::col((channels::Entity, channels::Column::DeletedAt)).is_null())
        .order_by((credentials::Entity, credentials::Column::Id), Order::Asc)
        .limit((limit + 1) as u64);
    if let Some(after) = after {
        query.and_where(Expr::col((credentials::Entity, credentials::Column::Id)).gt(after.get()));
    }
    query.to_owned()
}

fn credential_detail_query(channel_id: ChannelId, credential_id: CredentialId) -> SelectStatement {
    credential_base_query()
        .and_where(
            Expr::col((credentials::Entity, credentials::Column::Id)).eq(credential_id.get()),
        )
        .and_where(
            Expr::col((credentials::Entity, credentials::Column::ChannelId)).eq(channel_id.get()),
        )
        .and_where(Expr::col((credentials::Entity, credentials::Column::DeletedAt)).is_null())
        .and_where(Expr::col((channels::Entity, channels::Column::DeletedAt)).is_null())
        .limit(2)
        .to_owned()
}

fn channel_exists_query(channel_id: ChannelId) -> SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Id)),
            Alias::new("channel_id"),
        )
        .from(channels::Entity)
        .and_where(Expr::col((channels::Entity, channels::Column::Id)).eq(channel_id.get()))
        .and_where(Expr::col((channels::Entity, channels::Column::DeletedAt)).is_null())
        .limit(2)
        .to_owned()
}

pub(crate) fn credential_base_query() -> SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::Id)),
            Alias::new("credential_id"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::ChannelId)),
            Alias::new("channel_id"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::Kind)),
            Alias::new("kind"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::Status)),
            Alias::new("status"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::MultiKeyMode)),
            Alias::new("multi_key_mode"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::Priority)),
            Alias::new("priority"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::Weight)),
            Alias::new("weight"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::Concurrency)),
            Alias::new("concurrency"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::LoadFactorMicros)),
            Alias::new("load_factor_micros"),
        )
        .expr_as(
            Expr::col((
                credentials::Entity,
                credentials::Column::RateMultiplierMicros,
            )),
            Alias::new("rate_multiplier_micros"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::Schedulable)),
            Alias::new("schedulable"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::RateLimitedAt)),
            Alias::new("rate_limited_at"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::RateLimitResetAt)),
            Alias::new("rate_limit_reset_at"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::OverloadUntil)),
            Alias::new("overload_until"),
        )
        .expr_as(
            Expr::col((
                credentials::Entity,
                credentials::Column::TempUnschedulableUntil,
            )),
            Alias::new("temp_unschedulable_until"),
        )
        .expr_as(
            Expr::col((
                credentials::Entity,
                credentials::Column::TempUnschedulableReason,
            )),
            Alias::new("temp_unschedulable_reason"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::SessionWindowStart)),
            Alias::new("session_window_start"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::SessionWindowEnd)),
            Alias::new("session_window_end"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::ParentId)),
            Alias::new("parent_id"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::QuotaDimension)),
            Alias::new("quota_dimension"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::ProxyId)),
            Alias::new("proxy_id"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::OauthProvider)),
            Alias::new("oauth_provider"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::OauthTokenPending)),
            Alias::new("oauth_token_pending"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::OauthAccountKey)),
            Alias::new("oauth_account_key"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::OauthProjectId)),
            Alias::new("oauth_project_id"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::OauthRevision)),
            Alias::new("oauth_revision"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::LastUsedAt)),
            Alias::new("last_used_at"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::CreatedAt)),
            Alias::new("created_at"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::UpdatedAt)),
            Alias::new("updated_at"),
        )
        .from(credentials::Entity)
        .inner_join(
            channels::Entity,
            Expr::col((credentials::Entity, credentials::Column::ChannelId))
                .equals((channels::Entity, channels::Column::Id)),
        )
        .to_owned()
}

fn unix_timestamp(
    value: Option<TimeDateTimeWithTimeZone>,
) -> Result<Option<i64>, AdminChannelRepositoryError> {
    value
        .map(|value| value.unix_timestamp())
        .map(|value| (value >= 0).then_some(value).ok_or_else(internal_invariant))
        .transpose()
}

fn valid_optional_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_none_or(|value| valid_text(value, maximum_bytes))
}
