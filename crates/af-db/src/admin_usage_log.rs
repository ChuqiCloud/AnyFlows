use std::{fmt, time::Duration};

use af_domain::{GroupId, TokenId, UserId};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, UsageLogVideoResolution, entity::usage_logs};

/// 单页用量日志查询允许返回的最大记录数。
pub const MAX_ADMIN_USAGE_LOG_PAGE_SIZE: usize = 100;
const MAX_AUDIO_DURATION_NANOSECONDS: i64 = 24 * 60 * 60 * 1_000_000_000;
const MAX_VIDEO_DURATION_SECONDS: i64 = 24 * 60 * 60;

/// 管理端读取的规范化 token 用量明细。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminUsageLogUsageRecord {
    input_tokens: i64,
    output_tokens: i64,
    cache_read: i64,
    cache_creation_5m: i64,
    cache_creation_1h: i64,
    reasoning_tokens: i64,
    audio_input_tokens: i64,
    audio_output_tokens: i64,
}

impl AdminUsageLogUsageRecord {
    /// 返回输入 token 数。
    #[must_use]
    pub const fn input_tokens(self) -> i64 {
        self.input_tokens
    }

    /// 返回输出 token 数。
    #[must_use]
    pub const fn output_tokens(self) -> i64 {
        self.output_tokens
    }

    /// 返回缓存读取 token 数。
    #[must_use]
    pub const fn cache_read(self) -> i64 {
        self.cache_read
    }

    /// 返回 5 分钟缓存创建 token 数。
    #[must_use]
    pub const fn cache_creation_5m(self) -> i64 {
        self.cache_creation_5m
    }

    /// 返回 1 小时缓存创建 token 数。
    #[must_use]
    pub const fn cache_creation_1h(self) -> i64 {
        self.cache_creation_1h
    }

    /// 返回推理 token 数。
    #[must_use]
    pub const fn reasoning_tokens(self) -> i64 {
        self.reasoning_tokens
    }

    /// 返回音频输入 token 数。
    #[must_use]
    pub const fn audio_input_tokens(self) -> i64 {
        self.audio_input_tokens
    }

    /// 返回音频输出 token 数。
    #[must_use]
    pub const fn audio_output_tokens(self) -> i64 {
        self.audio_output_tokens
    }
}

/// 管理端可读取的一条已确认用量事实。
pub struct AdminUsageLogRecord {
    id: i64,
    event_id: String,
    user_id: UserId,
    token_id: TokenId,
    group_id: GroupId,
    billing_mode: i16,
    usage: AdminUsageLogUsageRecord,
    usage_source: i16,
    usage_semantics: i16,
    audio_duration_nanoseconds: Option<i64>,
    video_duration_seconds: Option<i64>,
    video_resolution: Option<UsageLogVideoResolution>,
    quota: i64,
    created_at: i64,
}

impl AdminUsageLogRecord {
    /// 返回日志主键。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    /// 返回用于审计关联的计费事件标识。
    #[must_use]
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    /// 返回用户标识。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回令牌标识。
    #[must_use]
    pub const fn token_id(&self) -> TokenId {
        self.token_id
    }

    /// 返回实际计费分组标识。
    #[must_use]
    pub const fn group_id(&self) -> GroupId {
        self.group_id
    }

    /// 返回数据库计费模式码。
    #[must_use]
    pub const fn billing_mode(&self) -> i16 {
        self.billing_mode
    }

    /// 返回完整 token 用量明细。
    #[must_use]
    pub const fn usage(&self) -> AdminUsageLogUsageRecord {
        self.usage
    }

    /// 返回数据库用量来源码。
    #[must_use]
    pub const fn usage_source(&self) -> i16 {
        self.usage_source
    }

    /// 返回数据库用量口径码。
    #[must_use]
    pub const fn usage_semantics(&self) -> i16 {
        self.usage_semantics
    }

    /// 返回可选的音频时长纳秒事实；非 Audio 请求保持缺失。
    #[must_use]
    pub const fn audio_duration_nanoseconds(&self) -> Option<i64> {
        self.audio_duration_nanoseconds
    }

    /// 返回可选的视频真实时长秒数；非视频请求或缺少可信事实时保持缺失。
    #[must_use]
    pub const fn video_duration_seconds(&self) -> Option<i64> {
        self.video_duration_seconds
    }

    /// 返回可选的视频分辨率档位；请求未明确提供时保持缺失。
    #[must_use]
    pub const fn video_resolution(&self) -> Option<UsageLogVideoResolution> {
        self.video_resolution
    }

    /// 返回最终扣减额度单位数。
    #[must_use]
    pub const fn quota(&self) -> i64 {
        self.quota
    }

    /// 返回创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    pub(super) fn try_from_model(
        model: usage_logs::Model,
    ) -> Result<Self, AdminUsageLogRepositoryError> {
        let token_values = [
            model.input_tokens,
            model.output_tokens,
            model.cache_read,
            model.cache_creation_5m,
            model.cache_creation_1h,
            model.reasoning_tokens,
            model.audio_input_tokens,
            model.audio_output_tokens,
        ];
        let video_resolution = model
            .video_resolution
            .and_then(UsageLogVideoResolution::from_database);
        if model.id <= 0
            || model.event_type != 1
            || !matches!(model.billing_mode, 1..=3)
            || !matches!(model.usage_source, 1 | 2)
            || !matches!(model.usage_semantics, 1 | 2)
            || model.quota < 0
            || model
                .audio_duration_nanoseconds
                .is_some_and(|value| !(0..=MAX_AUDIO_DURATION_NANOSECONDS).contains(&value))
            || model
                .video_duration_seconds
                .is_some_and(|value| !(1..=MAX_VIDEO_DURATION_SECONDS).contains(&value))
            || (model.video_resolution.is_some() && video_resolution.is_none())
            || token_values.into_iter().any(|value| value < 0)
        {
            return Err(record_internal_error(
                AdminUsageLogRepositoryError::Invariant,
            ));
        }
        Ok(Self {
            id: model.id,
            event_id: model.event_id.as_str().to_owned(),
            user_id: UserId::new(model.user_id)
                .map_err(|_| record_internal_error(AdminUsageLogRepositoryError::Invariant))?,
            token_id: TokenId::new(model.token_id)
                .map_err(|_| record_internal_error(AdminUsageLogRepositoryError::Invariant))?,
            group_id: GroupId::new(model.group_id)
                .map_err(|_| record_internal_error(AdminUsageLogRepositoryError::Invariant))?,
            billing_mode: model.billing_mode,
            usage: AdminUsageLogUsageRecord {
                input_tokens: model.input_tokens,
                output_tokens: model.output_tokens,
                cache_read: model.cache_read,
                cache_creation_5m: model.cache_creation_5m,
                cache_creation_1h: model.cache_creation_1h,
                reasoning_tokens: model.reasoning_tokens,
                audio_input_tokens: model.audio_input_tokens,
                audio_output_tokens: model.audio_output_tokens,
            },
            usage_source: model.usage_source,
            usage_semantics: model.usage_semantics,
            audio_duration_nanoseconds: model.audio_duration_nanoseconds,
            video_duration_seconds: model.video_duration_seconds,
            video_resolution,
            quota: model.quota,
            created_at: model.created_at.unix_timestamp(),
        })
    }
}

impl fmt::Debug for AdminUsageLogRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUsageLogRecord(<redacted>)")
    }
}

/// 一页倒序排列的用量日志结果。
pub struct AdminUsageLogPageRecord {
    logs: Vec<AdminUsageLogRecord>,
    next_cursor: Option<i64>,
}

impl AdminUsageLogPageRecord {
    /// 消费页面并返回日志记录和下一页游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<AdminUsageLogRecord>, Option<i64>) {
        (self.logs, self.next_cursor)
    }
}

impl fmt::Debug for AdminUsageLogPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUsageLogPageRecord(<redacted>)")
    }
}

/// 管理用量日志仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminUsageLogRepositoryConfigError {
    /// 零超时无法形成有效的数据库查询截止时间。
    #[error("管理用量日志查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 管理用量日志仓储错误；不携带事件、主体、用量或额度。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminUsageLogRepositoryError {
    /// 获取连接或执行查询失败。
    #[error("管理用量日志数据库查询失败")]
    Query,
    /// 查询超过配置的硬截止时间。
    #[error("管理用量日志数据库查询超时")]
    Timeout,
    /// 查询参数或持久化结果违反不变量。
    #[error("管理用量日志持久化状态损坏")]
    Invariant,
}

/// 管理用量日志的只读仓储。
#[derive(Clone)]
pub struct AdminUsageLogRepository {
    pool: DatabasePool,
    lookup_timeout: Duration,
}

impl AdminUsageLogRepository {
    /// 使用共享数据库连接池和单次查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, AdminUsageLogRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(AdminUsageLogRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按日志 ID 从新到旧读取一页已确认用量事实。
    pub async fn list(
        &self,
        before: Option<i64>,
        limit: usize,
    ) -> Result<AdminUsageLogPageRecord, AdminUsageLogRepositoryError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_ADMIN_USAGE_LOG_PAGE_SIZE).contains(&limit)
        {
            return Err(record_internal_error(
                AdminUsageLogRepositoryError::Invariant,
            ));
        }
        let operation = self
            .query(before, limit)
            .with_subscriber(NoSubscriber::default());
        let mut models = match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result?,
            Err(_) => return Err(record_internal_error(AdminUsageLogRepositoryError::Timeout)),
        };
        let has_more = models.len() > limit;
        if has_more {
            models.truncate(limit);
        }
        let logs = models
            .into_iter()
            .map(AdminUsageLogRecord::try_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| logs.last().map(AdminUsageLogRecord::id))
            .flatten();
        Ok(AdminUsageLogPageRecord { logs, next_cursor })
    }

    async fn query(
        &self,
        before: Option<i64>,
        limit: usize,
    ) -> Result<Vec<usage_logs::Model>, AdminUsageLogRepositoryError> {
        let mut query = usage_logs::Entity::find()
            .order_by_desc(usage_logs::Column::Id)
            .limit((limit + 1) as u64);
        if let Some(before) = before {
            query = query.filter(usage_logs::Column::Id.lt(before));
        }
        query
            .all(self.pool.connection())
            .await
            .map_err(|_| record_internal_error(AdminUsageLogRepositoryError::Query))
    }
}

impl fmt::Debug for AdminUsageLogRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminUsageLogRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

/// 只记录闭合内部分类，避免事件、主体、用量与额度进入日志。
fn record_internal_error(error: AdminUsageLogRepositoryError) -> AdminUsageLogRepositoryError {
    let error_kind = match error {
        AdminUsageLogRepositoryError::Query => "admin_usage_log_query",
        AdminUsageLogRepositoryError::Timeout => "admin_usage_log_timeout",
        AdminUsageLogRepositoryError::Invariant => "admin_usage_log_invariant",
    };
    tracing::error!(
        target: "af_db::admin_usage_log",
        error_kind,
        "管理用量日志仓储发生内部错误"
    );
    error
}
