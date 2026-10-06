use std::fmt;

use af_db::AdminChannelRecord;
use af_domain::{
    ChannelAutoBanRules, ChannelId, ChannelTimeout, ChannelType, ClientSimulationBodyProfile,
    ClientSimulationProfile, GroupId, Protocol, ResponsesCompactMode, ResponsesCompactProbeResult,
    Status,
};
use serde::{Deserialize, Serialize};

use crate::AdminChannelReadError;

/// 管理渠道和凭据列表默认页大小。
pub const DEFAULT_ADMIN_CHANNEL_PAGE_SIZE: usize = 50;

/// 已校验的管理渠道列表查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminChannelListQuery {
    after: Option<ChannelId>,
    limit: usize,
}

impl AdminChannelListQuery {
    /// 校验单调 ID 游标和固定页大小边界。
    pub fn new(after: Option<ChannelId>, limit: usize) -> Result<Self, AdminChannelReadError> {
        if !(1..=af_db::MAX_ADMIN_CHANNEL_PAGE_SIZE).contains(&limit) {
            return Err(AdminChannelReadError::InvalidPagination);
        }
        Ok(Self { after, limit })
    }

    /// 返回上一页最后一个渠道 ID。
    #[must_use]
    pub const fn after(self) -> Option<ChannelId> {
        self.after
    }
    /// 返回本页最大记录数。
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for AdminChannelListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ADMIN_CHANNEL_PAGE_SIZE,
        }
    }
}

/// 管理 API 公开的渠道与凭据运行状态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminRoutingStatus {
    /// 已启用并允许参与服务。
    Enabled,
    /// 管理员手动禁用或尚未验证。
    Disabled,
    /// 因上游故障被系统自动禁用。
    AutoDisabled,
}

impl AdminRoutingStatus {
    pub(crate) fn from_domain(value: Status) -> Self {
        match value {
            Status::Enabled => Self::Enabled,
            Status::Disabled => Self::Disabled,
            Status::AutoDisabled => Self::AutoDisabled,
        }
    }
}

/// 管理 API 可读取的非敏感渠道快照。
pub struct AdminChannel {
    provider: Option<String>,
    channel_id: ChannelId,
    name: String,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    timeout: Option<ChannelTimeout>,
    status: AdminRoutingStatus,
    weight: i32,
    priority: i32,
    auto_ban: bool,
    auto_ban_rules: ChannelAutoBanRules,
    pool_mode: bool,
    client_simulation_profile: Option<ClientSimulationProfile>,
    client_simulation_body_profile: Option<ClientSimulationBodyProfile>,
    responses_websocket_enabled: bool,
    responses_compact_mode: ResponsesCompactMode,
    responses_compact_model_mapping: serde_json::Value,
    responses_compact_probe_result: ResponsesCompactProbeResult,
    responses_compact_probe_checked_at: Option<i64>,
    responses_compact_probe_http_status: Option<u16>,
    models: Vec<String>,
    group_ids: Vec<GroupId>,
    model_mapping: serde_json::Value,
    param_override: serde_json::Value,
    balance: Option<i64>,
    used_quota: i64,
    tag: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl AdminChannel {
    /// 组合已经完成持久化校验的渠道字段，供测试适配器使用。
    #[allow(clippy::too_many_arguments, reason = "字段与稳定管理 API 响应一一对应")]
    #[must_use]
    pub fn from_parts(
        channel_id: ChannelId,
        name: String,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        timeout: Option<ChannelTimeout>,
        status: AdminRoutingStatus,
        weight: i32,
        priority: i32,
        auto_ban: bool,
        models: Vec<String>,
        group_ids: Vec<GroupId>,
        model_mapping: serde_json::Value,
        param_override: serde_json::Value,
        balance: Option<i64>,
        used_quota: i64,
        tag: Option<String>,
        created_at: i64,
        updated_at: i64,
    ) -> Self {
        Self {
            channel_id,
            name,
            channel_type,
            protocol,
            base_url,
            timeout,
            status,
            weight,
            priority,
            auto_ban,
            auto_ban_rules: ChannelAutoBanRules::default(),
            pool_mode: false,
            provider: None,
            client_simulation_profile: None,
            client_simulation_body_profile: None,
            responses_websocket_enabled: false,
            responses_compact_mode: ResponsesCompactMode::Auto,
            responses_compact_model_mapping: serde_json::json!({}),
            responses_compact_probe_result: ResponsesCompactProbeResult::Unknown,
            responses_compact_probe_checked_at: None,
            responses_compact_probe_http_status: None,
            models,
            group_ids,
            model_mapping,
            param_override,
            balance,
            used_quota,
            tag,
            created_at,
            updated_at,
        }
    }

    /// 返回渠道标识。
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
    /// 返回渠道上游协议族。
    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }
    /// 返回安全校验后的可选基础地址。
    #[must_use]
    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }
    /// 返回渠道级读取与完整请求超时；空值表示使用对应场景的服务默认值。
    #[must_use]
    pub const fn timeout(&self) -> Option<ChannelTimeout> {
        self.timeout
    }
    /// 返回渠道运行状态。
    #[must_use]
    pub const fn status(&self) -> AdminRoutingStatus {
        self.status
    }
    /// 返回渠道调度权重。
    #[must_use]
    pub const fn weight(&self) -> i32 {
        self.weight
    }
    /// 返回渠道优先级。
    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }
    /// 返回是否允许自动禁用。
    #[must_use]
    pub const fn auto_ban(&self) -> bool {
        self.auto_ban
    }

    /// 返回已校验的自动禁用状态码与关键词规则。
    #[must_use]
    pub const fn auto_ban_rules(&self) -> &ChannelAutoBanRules {
        &self.auto_ban_rules
    }
    /// 返回是否启用外部账号池模式。
    #[must_use]
    pub const fn pool_mode(&self) -> bool {
        self.pool_mode
    }

    /// 返回独立于通信适配器的厂商标识。
    #[must_use]
    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    /// 返回显式选择的版本化客户端仿真档案。
    #[must_use]
    pub const fn client_simulation_profile(&self) -> Option<ClientSimulationProfile> {
        self.client_simulation_profile
    }

    /// 返回显式选择的版本化客户端仿真正文档案。
    #[must_use]
    pub const fn client_simulation_body_profile(&self) -> Option<ClientSimulationBodyProfile> {
        self.client_simulation_body_profile
    }
    /// 返回是否显式开启原生 Responses WebSocket。
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
    pub const fn responses_compact_model_mapping(&self) -> &serde_json::Value {
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
    /// 返回渠道声明的 Canonical 模型集合。
    #[must_use]
    pub fn models(&self) -> &[String] {
        &self.models
    }
    /// 返回渠道关联的有效分组集合。
    #[must_use]
    pub fn group_ids(&self) -> &[GroupId] {
        &self.group_ids
    }
    /// 返回模型映射对象。
    #[must_use]
    pub const fn model_mapping(&self) -> &serde_json::Value {
        &self.model_mapping
    }
    /// 返回非敏感参数覆盖对象。
    #[must_use]
    pub const fn param_override(&self) -> &serde_json::Value {
        &self.param_override
    }
    /// 返回最近探测余额。
    #[must_use]
    pub const fn balance(&self) -> Option<i64> {
        self.balance
    }
    /// 返回累计消耗额度。
    #[must_use]
    pub const fn used_quota(&self) -> i64 {
        self.used_quota
    }
    /// 返回批量管理标签。
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

    pub(crate) fn from_record(record: AdminChannelRecord) -> Self {
        Self::from_parts(
            record.channel_id(),
            record.name().to_owned(),
            record.channel_type(),
            record.protocol(),
            record.base_url().map(str::to_owned),
            record.timeout(),
            AdminRoutingStatus::from_domain(record.status()),
            record.weight(),
            record.priority(),
            record.auto_ban(),
            record.models().to_vec(),
            record.group_ids().to_vec(),
            record.model_mapping().clone(),
            record.param_override().clone(),
            record.balance(),
            record.used_quota(),
            record.tag().map(str::to_owned),
            record.created_at(),
            record.updated_at(),
        )
        .with_auto_ban_rules(record.auto_ban_rules().clone())
        .with_pool_mode(record.pool_mode())
        .with_provider(record.provider().map(str::to_owned))
        .with_client_simulation_profile(record.client_simulation_profile())
        .with_client_simulation_body_profile(record.client_simulation_body_profile())
        .with_responses_websocket_enabled(record.responses_websocket_enabled())
        .with_responses_compact_configuration(
            record.responses_compact_mode(),
            record.responses_compact_model_mapping().clone(),
            record.responses_compact_probe_result(),
            record.responses_compact_probe_checked_at(),
            record.responses_compact_probe_http_status(),
        )
    }

    fn with_responses_websocket_enabled(mut self, enabled: bool) -> Self {
        self.responses_websocket_enabled = enabled;
        self
    }

    fn with_responses_compact_configuration(
        mut self,
        mode: ResponsesCompactMode,
        model_mapping: serde_json::Value,
        probe_result: ResponsesCompactProbeResult,
        probe_checked_at: Option<i64>,
        probe_http_status: Option<u16>,
    ) -> Self {
        self.responses_compact_mode = mode;
        self.responses_compact_model_mapping = model_mapping;
        self.responses_compact_probe_result = probe_result;
        self.responses_compact_probe_checked_at = probe_checked_at;
        self.responses_compact_probe_http_status = probe_http_status;
        self
    }

    fn with_auto_ban_rules(mut self, rules: ChannelAutoBanRules) -> Self {
        self.auto_ban_rules = rules;
        self
    }

    fn with_pool_mode(mut self, enabled: bool) -> Self {
        self.pool_mode = enabled;
        self
    }

    fn with_provider(mut self, provider: Option<String>) -> Self {
        self.provider = provider;
        self
    }

    fn with_client_simulation_profile(mut self, profile: Option<ClientSimulationProfile>) -> Self {
        self.client_simulation_profile = profile;
        self
    }

    fn with_client_simulation_body_profile(
        mut self,
        profile: Option<ClientSimulationBodyProfile>,
    ) -> Self {
        self.client_simulation_body_profile = profile;
        self
    }
}

impl fmt::Debug for AdminChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminChannel(<redacted>)")
    }
}

/// 一页管理渠道响应。
pub struct AdminChannelPage {
    channels: Vec<AdminChannel>,
    next_cursor: Option<ChannelId>,
}

impl AdminChannelPage {
    /// 组合渠道列表和可选下一游标。
    #[must_use]
    pub fn from_parts(channels: Vec<AdminChannel>, next_cursor: Option<ChannelId>) -> Self {
        Self {
            channels,
            next_cursor,
        }
    }
    /// 返回当前页渠道。
    #[must_use]
    pub fn channels(&self) -> &[AdminChannel] {
        &self.channels
    }
    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<ChannelId> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminChannelPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminChannelPage(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_debug_never_renders_configuration() {
        let channel = AdminChannel::from_parts(
            ChannelId::new(1).unwrap(),
            "private-channel".to_owned(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://private.example.com".to_owned()),
            Some(ChannelTimeout::new(60).unwrap()),
            AdminRoutingStatus::Enabled,
            10,
            20,
            true,
            vec!["private-model".to_owned()],
            vec![GroupId::new(1).unwrap()],
            serde_json::json!({"private-model": "upstream-model"}),
            serde_json::json!({"temperature": 0}),
            None,
            0,
            None,
            1,
            1,
        );
        assert_eq!(format!("{channel:?}"), "AdminChannel(<redacted>)");
    }
}
