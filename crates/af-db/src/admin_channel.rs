use std::{fmt, str::FromStr as _, time::Duration};

use af_domain::{
    ChannelAutoBanRules, ChannelId, ChannelTimeout, ChannelType, ClientSimulationBodyProfile,
    ClientSimulationProfile, GroupId, Protocol, ResponsesCompactMode, ResponsesCompactProbeResult,
    Status,
};
use sea_orm::{
    ConnectionTrait, DbBackend, DbErr, JsonValue, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, ExprTrait, Func, Order, Query, SelectStatement, SimpleExpr},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    ChannelModelMappings, ChannelParameterOverrides, DatabasePool,
    ability_write::{AbilityWriteError, ChannelRoutingSnapshot, load_channel_routing_snapshots},
    channel_settings::{
        AUTO_BAN_RULES_KEY, CLIENT_SIMULATION_BODY_PROFILE_KEY, CLIENT_SIMULATION_PROFILE_KEY,
        RESPONSES_COMPACT_MODE_KEY, RESPONSES_COMPACT_MODEL_MAPPING_KEY,
        RESPONSES_COMPACT_PROBE_CHECKED_AT_KEY, RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY,
        RESPONSES_COMPACT_PROBE_RESULT_KEY, client_simulation_body_profile,
        client_simulation_profile, parse_auto_ban_rules_value, pool_mode, responses_compact_mode,
        responses_compact_model_mapping, responses_compact_probe_record,
        responses_websocket_enabled, validate_client_simulation_body_capability,
        validate_client_simulation_capability, validate_responses_compact_capability,
        validate_responses_compact_model_mapping, validate_responses_compact_probe_result,
        validate_responses_websocket_capability,
    },
    entity::{ChannelBaseUrl, channels},
};

/// 单页渠道或凭据查询允许返回的最大记录数。
pub const MAX_ADMIN_CHANNEL_PAGE_SIZE: usize = 100;
/// 管理响应允许返回的单个渠道 JSON 配置最大字节数。
pub const MAX_ADMIN_CHANNEL_JSON_BYTES: i64 = 64 * 1024;

/// 管理端可读取的非敏感渠道快照。
pub struct AdminChannelRecord {
    provider: Option<String>,
    channel_id: ChannelId,
    name: String,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    timeout: Option<ChannelTimeout>,
    status: Status,
    weight: i32,
    priority: i32,
    auto_ban: bool,
    auto_ban_rules: ChannelAutoBanRules,
    pool_mode: bool,
    client_simulation_profile: Option<ClientSimulationProfile>,
    client_simulation_body_profile: Option<ClientSimulationBodyProfile>,
    responses_websocket_enabled: bool,
    responses_compact_mode: ResponsesCompactMode,
    responses_compact_model_mapping: JsonValue,
    responses_compact_probe_result: ResponsesCompactProbeResult,
    responses_compact_probe_checked_at: Option<i64>,
    responses_compact_probe_http_status: Option<u16>,
    models: Vec<String>,
    group_ids: Vec<GroupId>,
    model_mapping: JsonValue,
    param_override: JsonValue,
    balance: Option<i64>,
    used_quota: i64,
    tag: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl AdminChannelRecord {
    pub(crate) fn try_from_model(
        model: channels::Model,
    ) -> Result<Self, AdminChannelRepositoryError> {
        let model_mapping_oversized = match serde_json::to_vec(&model.model_mapping) {
            Ok(encoded) => encoded.len() > MAX_ADMIN_CHANNEL_JSON_BYTES as usize,
            Err(_) => true,
        };
        let param_override_oversized = match serde_json::to_vec(&model.param_override) {
            Ok(encoded) => encoded.len() > MAX_ADMIN_CHANNEL_JSON_BYTES as usize,
            Err(_) => true,
        };
        let settings = model.settings.into_inner();
        let settings_oversized = match serde_json::to_vec(&settings) {
            Ok(encoded) => encoded.len() > MAX_ADMIN_CHANNEL_JSON_BYTES as usize,
            Err(_) => true,
        };
        let auto_ban_rules_value = if settings_oversized {
            None
        } else {
            settings.as_object().map(|object| {
                object
                    .get(AUTO_BAN_RULES_KEY)
                    .cloned()
                    .unwrap_or_else(|| JsonValue::Object(Default::default()))
            })
        };
        AdminChannelRow {
            provider: setting_value(&settings, "provider"),
            channel_id: model.id,
            name: model.name,
            channel_type: model.r#type,
            protocol: model.protocol,
            base_url: model.base_url.map(|value| value.as_str().to_owned()),
            timeout_secs: model.timeout_secs,
            status: model.status,
            weight: model.weight,
            priority: model.priority,
            auto_ban: model.auto_ban,
            auto_ban_rules: auto_ban_rules_value,
            pool_mode_state: if settings_oversized {
                -1
            } else {
                pool_mode(&settings).map(i64::from).unwrap_or(-1)
            },
            client_simulation_profile: setting_value(&settings, CLIENT_SIMULATION_PROFILE_KEY),
            client_simulation_body_profile: setting_value(
                &settings,
                CLIENT_SIMULATION_BODY_PROFILE_KEY,
            ),
            models: Vec::new(),
            group_ids: Vec::new(),
            model_mapping: Some(model.model_mapping),
            model_mapping_oversized,
            param_override: Some(model.param_override),
            param_override_oversized,
            responses_websocket_state: if settings_oversized {
                -1
            } else {
                responses_websocket_enabled(&settings)
                    .map(i64::from)
                    .unwrap_or(-1)
            },
            responses_compact_mode: setting_value(&settings, RESPONSES_COMPACT_MODE_KEY),
            responses_compact_model_mapping: setting_value(
                &settings,
                RESPONSES_COMPACT_MODEL_MAPPING_KEY,
            ),
            responses_compact_probe_result: setting_value(
                &settings,
                RESPONSES_COMPACT_PROBE_RESULT_KEY,
            ),
            responses_compact_probe_checked_at: setting_value(
                &settings,
                RESPONSES_COMPACT_PROBE_CHECKED_AT_KEY,
            ),
            responses_compact_probe_http_status: setting_value(
                &settings,
                RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY,
            ),
            settings_oversized,
            balance: model.balance,
            used_quota: model.used_quota,
            tag: model.tag,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
        .validate()
    }

    /// 返回独立于通信适配器的厂商标识。
    #[must_use]
    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    /// 返回渠道主键。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回渠道名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回渠道适配器类型。
    #[must_use]
    pub const fn channel_type(&self) -> ChannelType {
        self.channel_type
    }

    /// 返回渠道使用的上游协议族。
    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// 返回不含凭据、查询串和片段的基础地址。
    #[must_use]
    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    /// 返回可选渠道超时；空值表示使用对应场景的服务默认值。
    #[must_use]
    pub const fn timeout(&self) -> Option<ChannelTimeout> {
        self.timeout
    }

    /// 返回渠道运行状态。
    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }

    /// 返回渠道加权随机权重。
    #[must_use]
    pub const fn weight(&self) -> i32 {
        self.weight
    }

    /// 返回渠道优先级。
    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }

    /// 返回渠道是否允许自动禁用。
    #[must_use]
    pub const fn auto_ban(&self) -> bool {
        self.auto_ban
    }

    /// 返回已校验的渠道自动禁用状态码与关键词规则。
    #[must_use]
    pub const fn auto_ban_rules(&self) -> &ChannelAutoBanRules {
        &self.auto_ban_rules
    }

    /// 返回渠道是否把上游账号健康交由外部池管理。
    #[must_use]
    pub const fn pool_mode(&self) -> bool {
        self.pool_mode
    }

    /// 返回渠道显式选择的版本化客户端仿真档案。
    #[must_use]
    pub const fn client_simulation_profile(&self) -> Option<ClientSimulationProfile> {
        self.client_simulation_profile
    }

    /// 返回渠道显式选择的版本化客户端仿真正文档案。
    #[must_use]
    pub const fn client_simulation_body_profile(&self) -> Option<ClientSimulationBodyProfile> {
        self.client_simulation_body_profile
    }

    /// 返回渠道是否显式开启原生 Responses WebSocket。
    #[must_use]
    pub const fn responses_websocket_enabled(&self) -> bool {
        self.responses_websocket_enabled
    }

    /// 返回 Compact 三态能力策略。
    #[must_use]
    pub const fn responses_compact_mode(&self) -> ResponsesCompactMode {
        self.responses_compact_mode
    }

    /// 返回 Compact 专属模型映射对象。
    #[must_use]
    pub const fn responses_compact_model_mapping(&self) -> &JsonValue {
        &self.responses_compact_model_mapping
    }

    /// 返回最近一次确定性 Compact 探测结论。
    #[must_use]
    pub const fn responses_compact_probe_result(&self) -> ResponsesCompactProbeResult {
        self.responses_compact_probe_result
    }

    /// 返回最近一次确定性 Compact 探测时间（Unix 毫秒）。
    #[must_use]
    pub const fn responses_compact_probe_checked_at(&self) -> Option<i64> {
        self.responses_compact_probe_checked_at
    }

    /// 返回最近一次 Compact 探测的受控 HTTP 状态。
    #[must_use]
    pub const fn responses_compact_probe_http_status(&self) -> Option<u16> {
        self.responses_compact_probe_http_status
    }

    /// 返回按 Canonical 原文排序的渠道模型集合。
    #[must_use]
    pub fn models(&self) -> &[String] {
        &self.models
    }

    /// 返回按正整数 ID 排序的渠道分组集合。
    #[must_use]
    pub fn group_ids(&self) -> &[GroupId] {
        &self.group_ids
    }

    /// 返回请求模型到上游模型的映射。
    #[must_use]
    pub const fn model_mapping(&self) -> &JsonValue {
        &self.model_mapping
    }

    /// 返回非敏感请求参数覆盖配置。
    #[must_use]
    pub const fn param_override(&self) -> &JsonValue {
        &self.param_override
    }

    /// 返回最近一次探测到的上游余额。
    #[must_use]
    pub const fn balance(&self) -> Option<i64> {
        self.balance
    }

    /// 返回渠道累计消耗额度。
    #[must_use]
    pub const fn used_quota(&self) -> i64 {
        self.used_quota
    }

    /// 返回可选的批量管理标签。
    #[must_use]
    pub fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
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

    pub(crate) fn with_routing(mut self, routing: ChannelRoutingSnapshot) -> Self {
        (self.models, self.group_ids) = routing.into_parts();
        self
    }
}

impl fmt::Debug for AdminChannelRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminChannelRecord(<redacted>)")
    }
}

/// 一页有界渠道结果。
pub struct AdminChannelPageRecord {
    channels: Vec<AdminChannelRecord>,
    next_cursor: Option<ChannelId>,
}

impl AdminChannelPageRecord {
    /// 消费页面并返回渠道记录和下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<AdminChannelRecord>, Option<ChannelId>) {
        (self.channels, self.next_cursor)
    }
}

impl fmt::Debug for AdminChannelPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminChannelPageRecord(<redacted>)")
    }
}

/// 渠道详情查询结果。
pub enum AdminChannelLookupOutcome {
    /// 找到当前未软删除渠道。
    Found(Box<AdminChannelRecord>),
    /// 渠道不存在或已经软删除。
    NotFound,
}

impl fmt::Debug for AdminChannelLookupOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found(_) => formatter.write_str("AdminChannelLookupOutcome::Found(<redacted>)"),
            Self::NotFound => formatter.write_str("AdminChannelLookupOutcome::NotFound"),
        }
    }
}

/// 管理渠道仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminChannelRepositoryConfigError {
    /// 零超时无法形成有效的数据库查询截止时间。
    #[error("管理渠道查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 管理渠道与凭据仓储内部错误；不携带配置、凭据或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminChannelRepositoryError {
    /// 获取连接或执行查询失败。
    #[error("管理渠道数据库查询失败")]
    Query,
    /// 查询超过配置的硬截止时间。
    #[error("管理渠道数据库查询超时")]
    Timeout,
    /// 查询输入或持久化结果违反不变量。
    #[error("管理渠道持久化状态损坏")]
    Invariant,
}

/// 管理端渠道与其凭据共用的只读仓储。
#[derive(Clone)]
pub struct AdminChannelRepository {
    pub(super) pool: DatabasePool,
    pub(super) lookup_timeout: Duration,
}

impl AdminChannelRepository {
    /// 使用共享数据库连接池和单次查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, AdminChannelRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(AdminChannelRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按单调渠道 ID 游标读取一页未软删除渠道。
    pub async fn list(
        &self,
        after: Option<ChannelId>,
        limit: usize,
    ) -> Result<AdminChannelPageRecord, AdminChannelRepositoryError> {
        if !(1..=MAX_ADMIN_CHANNEL_PAGE_SIZE).contains(&limit) {
            return Err(record_internal_error(
                AdminChannelRepositoryError::Invariant,
            ));
        }
        let mut results = self
            .query_with_timeout(channel_list_query(self.database_backend(), after, limit))
            .await?;
        let has_more = results.len() > limit;
        if has_more {
            results.truncate(limit);
        }
        let mut channels = results
            .iter()
            .map(AdminChannelRow::try_from_query_result)
            .map(|row| row.map_err(|_| record_internal_error(AdminChannelRepositoryError::Query)))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(AdminChannelRow::validate)
            .collect::<Result<Vec<_>, _>>()?;
        self.attach_routing(&mut channels).await?;
        let next_cursor = has_more
            .then(|| channels.last().map(AdminChannelRecord::channel_id))
            .flatten();
        Ok(AdminChannelPageRecord {
            channels,
            next_cursor,
        })
    }

    /// 按稳定渠道 ID 查询当前未软删除渠道。
    pub async fn get(
        &self,
        channel_id: ChannelId,
    ) -> Result<AdminChannelLookupOutcome, AdminChannelRepositoryError> {
        let mut results = self
            .query_with_timeout(channel_detail_query(self.database_backend(), channel_id))
            .await?;
        match results.len() {
            0 => Ok(AdminChannelLookupOutcome::NotFound),
            1 => {
                let row =
                    AdminChannelRow::try_from_query_result(&results.pop().ok_or_else(|| {
                        record_internal_error(AdminChannelRepositoryError::Invariant)
                    })?)
                    .map_err(|_| record_internal_error(AdminChannelRepositoryError::Query))?;
                let mut channel = row.validate()?;
                self.attach_routing(std::slice::from_mut(&mut channel))
                    .await?;
                Ok(AdminChannelLookupOutcome::Found(Box::new(channel)))
            }
            _ => Err(record_internal_error(
                AdminChannelRepositoryError::Invariant,
            )),
        }
    }

    pub(super) fn database_backend(&self) -> DbBackend {
        self.pool.connection().get_database_backend()
    }

    pub(super) async fn query_with_timeout(
        &self,
        query: SelectStatement,
    ) -> Result<Vec<QueryResult>, AdminChannelRepositoryError> {
        let connection = self.pool.connection();
        let statement = connection.get_database_backend().build(&query);
        let operation = connection
            .query_all(statement)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.lookup_timeout, operation).await {
            Ok(Ok(results)) => Ok(results),
            Ok(Err(_)) => Err(record_internal_error(AdminChannelRepositoryError::Query)),
            Err(_) => Err(record_internal_error(AdminChannelRepositoryError::Timeout)),
        }
    }

    async fn attach_routing(
        &self,
        channels: &mut [AdminChannelRecord],
    ) -> Result<(), AdminChannelRepositoryError> {
        let channel_ids = channels
            .iter()
            .map(AdminChannelRecord::channel_id)
            .collect::<Vec<_>>();
        let operation = load_channel_routing_snapshots(self.pool.connection(), &channel_ids)
            .with_subscriber(NoSubscriber::default());
        let mut routing = match timeout(self.lookup_timeout, operation).await {
            Ok(Ok(routing)) => routing,
            Ok(Err(error)) => return Err(map_ability_read_error(error)),
            Err(_) => return Err(record_internal_error(AdminChannelRepositoryError::Timeout)),
        };
        for channel in channels {
            let snapshot = routing
                .remove(&channel.channel_id())
                .ok_or_else(internal_invariant)?;
            let (models, group_ids) = snapshot.into_parts();
            channel.models = models;
            channel.group_ids = group_ids;
        }
        if routing.is_empty() {
            Ok(())
        } else {
            Err(internal_invariant())
        }
    }
}

impl fmt::Debug for AdminChannelRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminChannelRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

struct AdminChannelRow {
    provider: Option<JsonValue>,
    channel_id: i64,
    name: String,
    channel_type: String,
    protocol: String,
    base_url: Option<String>,
    timeout_secs: Option<i32>,
    status: i16,
    weight: i32,
    priority: i32,
    auto_ban: bool,
    auto_ban_rules: Option<JsonValue>,
    pool_mode_state: i64,
    client_simulation_profile: Option<JsonValue>,
    client_simulation_body_profile: Option<JsonValue>,
    models: Vec<String>,
    group_ids: Vec<GroupId>,
    model_mapping: Option<JsonValue>,
    model_mapping_oversized: bool,
    param_override: Option<JsonValue>,
    param_override_oversized: bool,
    responses_websocket_state: i64,
    responses_compact_mode: Option<JsonValue>,
    responses_compact_model_mapping: Option<JsonValue>,
    responses_compact_probe_result: Option<JsonValue>,
    responses_compact_probe_checked_at: Option<JsonValue>,
    responses_compact_probe_http_status: Option<JsonValue>,
    settings_oversized: bool,
    balance: Option<i64>,
    used_quota: i64,
    tag: Option<String>,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
}

impl AdminChannelRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            provider: result.try_get("", "provider")?,
            channel_id: result.try_get("", "channel_id")?,
            name: result.try_get("", "name")?,
            channel_type: result.try_get("", "channel_type")?,
            protocol: result.try_get("", "protocol")?,
            base_url: result.try_get("", "base_url")?,
            timeout_secs: result.try_get("", "timeout_secs")?,
            status: result.try_get("", "status")?,
            weight: result.try_get("", "weight")?,
            priority: result.try_get("", "priority")?,
            auto_ban: result.try_get("", "auto_ban")?,
            auto_ban_rules: result.try_get("", "auto_ban_rules")?,
            pool_mode_state: result.try_get("", "pool_mode")?,
            client_simulation_profile: result.try_get("", "client_simulation_profile")?,
            client_simulation_body_profile: result.try_get("", "client_simulation_body_profile")?,
            models: Vec::new(),
            group_ids: Vec::new(),
            model_mapping: result.try_get("", "model_mapping")?,
            model_mapping_oversized: result.try_get("", "model_mapping_oversized")?,
            param_override: result.try_get("", "param_override")?,
            param_override_oversized: result.try_get("", "param_override_oversized")?,
            responses_websocket_state: result.try_get("", "responses_websocket_enabled")?,
            responses_compact_mode: result.try_get("", "responses_compact_mode")?,
            responses_compact_model_mapping: result
                .try_get("", "responses_compact_model_mapping")?,
            responses_compact_probe_result: result.try_get("", "responses_compact_probe_result")?,
            responses_compact_probe_checked_at: result
                .try_get("", "responses_compact_probe_checked_at")?,
            responses_compact_probe_http_status: result
                .try_get("", "responses_compact_probe_http_status")?,
            settings_oversized: result.try_get("", "settings_oversized")?,
            balance: result.try_get("", "balance")?,
            used_quota: result.try_get("", "used_quota")?,
            tag: result.try_get("", "tag")?,
            created_at: result.try_get("", "created_at")?,
            updated_at: result.try_get("", "updated_at")?,
        })
    }

    fn validate(self) -> Result<AdminChannelRecord, AdminChannelRepositoryError> {
        let provider = match self.provider {
            None => None,
            Some(JsonValue::String(value)) if valid_text(&value, 64) && value.trim() == value => {
                Some(value)
            }
            Some(_) => return Err(internal_invariant()),
        };
        let channel_id = ChannelId::new(self.channel_id).map_err(|_| internal_invariant())?;
        let channel_type =
            ChannelType::from_str(&self.channel_type).map_err(|_| internal_invariant())?;
        let protocol = Protocol::from_str(&self.protocol).map_err(|_| internal_invariant())?;
        let status = Status::try_from(self.status).map_err(|_| internal_invariant())?;
        let base_url = self
            .base_url
            .map(|value| {
                ChannelBaseUrl::parse(&value)
                    .map(|validated| validated.as_str().to_owned())
                    .map_err(|_| internal_invariant())
            })
            .transpose()?;
        let timeout = self
            .timeout_secs
            .map(|value| {
                <u64 as std::convert::TryFrom<i32>>::try_from(value)
                    .ok()
                    .and_then(|value| ChannelTimeout::new(value).ok())
                    .ok_or_else(internal_invariant)
            })
            .transpose()?;
        let model_mapping =
            validated_json_object(self.model_mapping, self.model_mapping_oversized, true)?;
        ChannelModelMappings::parse(&model_mapping).map_err(|_| internal_invariant())?;
        let param_override =
            validated_json_object(self.param_override, self.param_override_oversized, false)?;
        ChannelParameterOverrides::parse(&param_override).map_err(|_| internal_invariant())?;
        let responses_websocket_enabled = match self.responses_websocket_state {
            0 => false,
            1 => true,
            _ => return Err(internal_invariant()),
        };
        if self.settings_oversized {
            return Err(internal_invariant());
        }
        let settings = compact_settings_projection(
            self.client_simulation_profile,
            self.client_simulation_body_profile,
            self.responses_compact_mode,
            self.responses_compact_model_mapping,
            self.responses_compact_probe_result,
            self.responses_compact_probe_checked_at,
            self.responses_compact_probe_http_status,
        );
        let responses_compact_mode =
            responses_compact_mode(&settings).map_err(|_| internal_invariant())?;
        let client_simulation_profile =
            client_simulation_profile(&settings).map_err(|_| internal_invariant())?;
        let client_simulation_body_profile =
            client_simulation_body_profile(&settings).map_err(|_| internal_invariant())?;
        validate_client_simulation_capability(channel_type, protocol, client_simulation_profile)
            .map_err(|_| internal_invariant())?;
        validate_client_simulation_body_capability(
            channel_type,
            protocol,
            client_simulation_profile,
            client_simulation_body_profile,
        )
        .map_err(|_| internal_invariant())?;
        let responses_compact_model_mapping =
            responses_compact_model_mapping(&settings).map_err(|_| internal_invariant())?;
        validate_responses_compact_capability(channel_type, protocol, responses_compact_mode)
            .map_err(|_| internal_invariant())?;
        validate_responses_compact_model_mapping(
            channel_type,
            protocol,
            &responses_compact_model_mapping,
        )
        .map_err(|_| internal_invariant())?;
        let responses_compact_model_mapping = responses_compact_model_mapping.to_projection_json();
        let responses_compact_probe =
            responses_compact_probe_record(&settings).map_err(|_| internal_invariant())?;
        let responses_compact_probe_result = responses_compact_probe
            .map_or(ResponsesCompactProbeResult::Unknown, |record| {
                record.result()
            });
        validate_responses_compact_probe_result(
            channel_type,
            protocol,
            responses_compact_probe_result,
        )
        .map_err(|_| internal_invariant())?;
        let responses_compact_probe_checked_at =
            responses_compact_probe.map(|record| record.checked_at());
        let responses_compact_probe_http_status =
            responses_compact_probe.and_then(|record| record.http_status());
        let auto_ban_rules_value = self
            .auto_ban_rules
            .as_ref()
            .ok_or_else(internal_invariant)?;
        let auto_ban_rules = parse_auto_ban_rules_value(Some(auto_ban_rules_value))
            .map_err(|_| internal_invariant())?;
        let pool_mode = match self.pool_mode_state {
            0 => false,
            1 => true,
            _ => return Err(internal_invariant()),
        };
        validate_responses_websocket_capability(
            channel_type,
            protocol,
            responses_websocket_enabled,
        )
        .map_err(|_| internal_invariant())?;
        let created_at = self.created_at.unix_timestamp();
        let updated_at = self.updated_at.unix_timestamp();
        if !valid_text(&self.name, 128)
            || self.weight < 0
            || self.used_quota < 0
            || self
                .tag
                .as_deref()
                .is_some_and(|value| !valid_text(value, 64))
            || created_at < 0
            || updated_at < created_at
        {
            return Err(internal_invariant());
        }
        Ok(AdminChannelRecord {
            provider,
            channel_id,
            name: self.name,
            channel_type,
            protocol,
            base_url,
            timeout,
            status,
            weight: self.weight,
            priority: self.priority,
            auto_ban: self.auto_ban,
            auto_ban_rules,
            pool_mode,
            client_simulation_profile,
            client_simulation_body_profile,
            responses_websocket_enabled,
            responses_compact_mode,
            responses_compact_model_mapping,
            responses_compact_probe_result,
            responses_compact_probe_checked_at,
            responses_compact_probe_http_status,
            models: self.models,
            group_ids: self.group_ids,
            model_mapping,
            param_override,
            balance: self.balance,
            used_quota: self.used_quota,
            tag: self.tag,
            created_at,
            updated_at,
        })
    }
}

fn channel_list_query(
    database_backend: DbBackend,
    after: Option<ChannelId>,
    limit: usize,
) -> SelectStatement {
    let mut query = channel_base_query(database_backend);
    query
        .and_where(Expr::col((channels::Entity, channels::Column::DeletedAt)).is_null())
        .order_by((channels::Entity, channels::Column::Id), Order::Asc)
        .limit((limit + 1) as u64);
    if let Some(after) = after {
        query.and_where(Expr::col((channels::Entity, channels::Column::Id)).gt(after.get()));
    }
    query.to_owned()
}

fn channel_detail_query(database_backend: DbBackend, channel_id: ChannelId) -> SelectStatement {
    channel_base_query(database_backend)
        .and_where(Expr::col((channels::Entity, channels::Column::Id)).eq(channel_id.get()))
        .and_where(Expr::col((channels::Entity, channels::Column::DeletedAt)).is_null())
        .limit(2)
        .to_owned()
}

pub(crate) fn channel_base_query(database_backend: DbBackend) -> SelectStatement {
    let model_mapping_length =
        channel_json_length(database_backend, channels::Column::ModelMapping);
    let param_override_length =
        channel_json_length(database_backend, channels::Column::ParamOverride);
    let settings_length = channel_json_length(database_backend, channels::Column::Settings);
    let model_mapping_column = || Expr::col((channels::Entity, channels::Column::ModelMapping));
    let param_override_column = || Expr::col((channels::Entity, channels::Column::ParamOverride));
    let bounded_setting = |key| {
        Expr::case(
            settings_length.clone().lte(MAX_ADMIN_CHANNEL_JSON_BYTES),
            channel_setting_projection(database_backend, key),
        )
        .finally(Expr::value(Option::<JsonValue>::None))
    };

    Query::select()
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Id)),
            Alias::new("channel_id"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Name)),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Type)),
            Alias::new("channel_type"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Protocol)),
            Alias::new("protocol"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::BaseUrl)),
            Alias::new("base_url"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::TimeoutSecs)),
            Alias::new("timeout_secs"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Status)),
            Alias::new("status"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Weight)),
            Alias::new("weight"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Priority)),
            Alias::new("priority"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::AutoBan)),
            Alias::new("auto_ban"),
        )
        .expr_as(
            Expr::case(
                model_mapping_length
                    .clone()
                    .lte(MAX_ADMIN_CHANNEL_JSON_BYTES),
                model_mapping_column(),
            )
            .finally(Expr::value(Option::<JsonValue>::None)),
            Alias::new("model_mapping"),
        )
        .expr_as(
            Expr::case(model_mapping_length.gt(MAX_ADMIN_CHANNEL_JSON_BYTES), true).finally(false),
            Alias::new("model_mapping_oversized"),
        )
        .expr_as(
            Expr::case(
                param_override_length
                    .clone()
                    .lte(MAX_ADMIN_CHANNEL_JSON_BYTES),
                param_override_column(),
            )
            .finally(Expr::value(Option::<JsonValue>::None)),
            Alias::new("param_override"),
        )
        .expr_as(
            Expr::case(param_override_length.gt(MAX_ADMIN_CHANNEL_JSON_BYTES), true).finally(false),
            Alias::new("param_override_oversized"),
        )
        .expr_as(
            channel_responses_websocket_projection(database_backend),
            Alias::new("responses_websocket_enabled"),
        )
        .expr_as(
            bounded_setting(CLIENT_SIMULATION_PROFILE_KEY),
            Alias::new("client_simulation_profile"),
        )
        .expr_as(bounded_setting("provider"), Alias::new("provider"))
        .expr_as(
            bounded_setting(CLIENT_SIMULATION_BODY_PROFILE_KEY),
            Alias::new("client_simulation_body_profile"),
        )
        .expr_as(
            bounded_setting(RESPONSES_COMPACT_MODE_KEY),
            Alias::new("responses_compact_mode"),
        )
        .expr_as(
            bounded_setting(RESPONSES_COMPACT_MODEL_MAPPING_KEY),
            Alias::new("responses_compact_model_mapping"),
        )
        .expr_as(
            bounded_setting(RESPONSES_COMPACT_PROBE_RESULT_KEY),
            Alias::new("responses_compact_probe_result"),
        )
        .expr_as(
            bounded_setting(RESPONSES_COMPACT_PROBE_CHECKED_AT_KEY),
            Alias::new("responses_compact_probe_checked_at"),
        )
        .expr_as(
            bounded_setting(RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY),
            Alias::new("responses_compact_probe_http_status"),
        )
        .expr_as(
            Expr::case(settings_length.gt(MAX_ADMIN_CHANNEL_JSON_BYTES), true).finally(false),
            Alias::new("settings_oversized"),
        )
        .expr_as(
            channel_auto_ban_rules_projection(database_backend),
            Alias::new("auto_ban_rules"),
        )
        .expr_as(
            channel_pool_mode_projection(database_backend),
            Alias::new("pool_mode"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Balance)),
            Alias::new("balance"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::UsedQuota)),
            Alias::new("used_quota"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Tag)),
            Alias::new("tag"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::CreatedAt)),
            Alias::new("created_at"),
        )
        .expr_as(
            Expr::col((channels::Entity, channels::Column::UpdatedAt)),
            Alias::new("updated_at"),
        )
        .from(channels::Entity)
        .to_owned()
}

fn channel_setting_projection(database_backend: DbBackend, key: &str) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(format!(
            r#"CASE WHEN jsonb_typeof("channels"."settings") <> 'object' THEN NULL ELSE "channels"."settings" -> '{key}' END"#,
        )),
        DbBackend::MySql => Expr::cust(format!(
            "CASE WHEN JSON_TYPE(`channels`.`settings`) <> 'OBJECT' THEN NULL ELSE JSON_EXTRACT(`channels`.`settings`, '$.{key}') END",
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            r#"CASE WHEN json_type("channels"."settings") <> 'object' THEN NULL WHEN json_type("channels"."settings", '$.{key}') IS NULL THEN NULL ELSE json_quote(json_extract("channels"."settings", '$.{key}')) END"#,
        )),
    }
}

fn setting_value(settings: &JsonValue, key: &str) -> Option<JsonValue> {
    settings
        .as_object()
        .and_then(|object| object.get(key))
        .cloned()
}

fn compact_settings_projection(
    client_simulation_profile: Option<JsonValue>,
    client_simulation_body_profile: Option<JsonValue>,
    mode: Option<JsonValue>,
    model_mapping: Option<JsonValue>,
    probe_result: Option<JsonValue>,
    probe_checked_at: Option<JsonValue>,
    probe_http_status: Option<JsonValue>,
) -> JsonValue {
    let mut settings = serde_json::Map::new();
    for (key, value) in [
        (CLIENT_SIMULATION_PROFILE_KEY, client_simulation_profile),
        (
            CLIENT_SIMULATION_BODY_PROFILE_KEY,
            client_simulation_body_profile,
        ),
        (RESPONSES_COMPACT_MODE_KEY, mode),
        (RESPONSES_COMPACT_MODEL_MAPPING_KEY, model_mapping),
        (RESPONSES_COMPACT_PROBE_RESULT_KEY, probe_result),
        (RESPONSES_COMPACT_PROBE_CHECKED_AT_KEY, probe_checked_at),
        (RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY, probe_http_status),
    ] {
        if let Some(value) = value {
            settings.insert(key.to_owned(), value);
        }
    }
    JsonValue::Object(settings)
}

fn channel_json_length(database_backend: DbBackend, column: channels::Column) -> SimpleExpr {
    let value = Expr::col((channels::Entity, column));
    match database_backend {
        DbBackend::Postgres => Func::cust(Alias::new("OCTET_LENGTH"))
            .arg(value.cast_as(Alias::new("TEXT")))
            .into(),
        DbBackend::MySql => Func::cust(Alias::new("OCTET_LENGTH"))
            .arg(value.cast_as(Alias::new("CHAR")))
            .into(),
        DbBackend::Sqlite => Func::char_length(value.cast_as(Alias::new("BLOB"))).into(),
    }
}

// PostgreSQL 将 CASE 中的 -1/0/1 推断为 INT4；显式转为 BIGINT 以匹配 i64 解码。
fn channel_responses_websocket_projection(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#"CAST(CASE WHEN jsonb_typeof("channels"."settings") <> 'object' THEN -1 WHEN NOT ("channels"."settings" ? 'responses_websocket_enabled') THEN 0 WHEN jsonb_typeof("channels"."settings" -> 'responses_websocket_enabled') <> 'boolean' THEN -1 WHEN ("channels"."settings" ->> 'responses_websocket_enabled')::boolean THEN 1 ELSE 0 END AS BIGINT)"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CASE WHEN JSON_TYPE(`channels`.`settings`) <> 'OBJECT' THEN -1 WHEN JSON_CONTAINS_PATH(`channels`.`settings`, 'one', '$.responses_websocket_enabled') = 0 THEN 0 WHEN JSON_TYPE(JSON_EXTRACT(`channels`.`settings`, '$.responses_websocket_enabled')) <> 'BOOLEAN' THEN -1 WHEN JSON_UNQUOTE(JSON_EXTRACT(`channels`.`settings`, '$.responses_websocket_enabled')) = 'true' THEN 1 ELSE 0 END",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"CASE WHEN json_type("channels"."settings") <> 'object' THEN -1 WHEN json_type("channels"."settings", '$.responses_websocket_enabled') IS NULL THEN 0 WHEN json_type("channels"."settings", '$.responses_websocket_enabled') NOT IN ('true', 'false') THEN -1 WHEN json_extract("channels"."settings", '$.responses_websocket_enabled') = 1 THEN 1 ELSE 0 END"#,
        ),
    }
}

fn channel_pool_mode_projection(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#"CAST(CASE WHEN jsonb_typeof("channels"."settings") <> 'object' THEN -1 WHEN NOT ("channels"."settings" ? 'pool_mode') THEN 0 WHEN jsonb_typeof("channels"."settings" -> 'pool_mode') <> 'boolean' THEN -1 WHEN ("channels"."settings" ->> 'pool_mode')::boolean THEN 1 ELSE 0 END AS BIGINT)"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CASE WHEN JSON_TYPE(`channels`.`settings`) <> 'OBJECT' THEN -1 WHEN JSON_CONTAINS_PATH(`channels`.`settings`, 'one', '$.pool_mode') = 0 THEN 0 WHEN JSON_TYPE(JSON_EXTRACT(`channels`.`settings`, '$.pool_mode')) <> 'BOOLEAN' THEN -1 WHEN JSON_UNQUOTE(JSON_EXTRACT(`channels`.`settings`, '$.pool_mode')) = 'true' THEN 1 ELSE 0 END",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"CASE WHEN json_type("channels"."settings") <> 'object' THEN -1 WHEN json_type("channels"."settings", '$.pool_mode') IS NULL THEN 0 WHEN json_type("channels"."settings", '$.pool_mode') NOT IN ('true', 'false') THEN -1 WHEN json_extract("channels"."settings", '$.pool_mode') = 1 THEN 1 ELSE 0 END"#,
        ),
    }
}

fn channel_auto_ban_rules_projection(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#"CASE WHEN jsonb_typeof("channels"."settings") <> 'object' THEN NULL WHEN NOT ("channels"."settings" ? 'auto_ban_rules') THEN '{}'::jsonb ELSE "channels"."settings" -> 'auto_ban_rules' END"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CASE WHEN JSON_TYPE(`channels`.`settings`) <> 'OBJECT' THEN NULL WHEN JSON_CONTAINS_PATH(`channels`.`settings`, 'one', '$.auto_ban_rules') = 0 THEN JSON_OBJECT() ELSE JSON_EXTRACT(`channels`.`settings`, '$.auto_ban_rules') END",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"CASE WHEN json_type("channels"."settings") <> 'object' THEN NULL WHEN json_type("channels"."settings", '$.auto_ban_rules') IS NULL THEN json('{}') ELSE json_quote(json_extract("channels"."settings", '$.auto_ban_rules')) END"#,
        ),
    }
}

fn validated_json_object(
    value: Option<JsonValue>,
    oversized: bool,
    string_values_only: bool,
) -> Result<JsonValue, AdminChannelRepositoryError> {
    let Some(value) = value else {
        return Err(internal_invariant());
    };
    let JsonValue::Object(object) = &value else {
        return Err(internal_invariant());
    };
    if oversized
        || serde_json::to_vec(&value)
            .ok()
            .and_then(|encoded| i64::try_from(encoded.len()).ok())
            .is_none_or(|length| length > MAX_ADMIN_CHANNEL_JSON_BYTES)
        || (string_values_only
            && object.iter().any(|(key, value)| {
                !valid_text(key, 256) || value.as_str().is_none_or(|value| !valid_text(value, 256))
            }))
    {
        return Err(internal_invariant());
    }
    Ok(value)
}

pub(super) fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}

pub(super) fn internal_invariant() -> AdminChannelRepositoryError {
    record_internal_error(AdminChannelRepositoryError::Invariant)
}

fn map_ability_read_error(error: AbilityWriteError) -> AdminChannelRepositoryError {
    match error {
        AbilityWriteError::Query => record_internal_error(AdminChannelRepositoryError::Query),
        AbilityWriteError::InvalidInput
        | AbilityWriteError::InvalidReference
        | AbilityWriteError::CapacityExceeded
        | AbilityWriteError::Invariant => internal_invariant(),
    }
}

pub(super) fn record_internal_error(
    error: AdminChannelRepositoryError,
) -> AdminChannelRepositoryError {
    let error_kind = match error {
        AdminChannelRepositoryError::Query => "admin_channel_query",
        AdminChannelRepositoryError::Timeout => "admin_channel_timeout",
        AdminChannelRepositoryError::Invariant => "admin_channel_invariant",
    };
    tracing::error!(
        target: "af_db::admin_channel",
        error_kind,
        "管理渠道仓储发生内部错误"
    );
    error
}
