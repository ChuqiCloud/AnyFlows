use crate::scheduler_runtime::ChannelModelMappings;
use af_domain::{
    ChannelAutoBanRules, ChannelType, ClientSimulationBodyProfile, ClientSimulationProfile,
    Protocol, ResponsesCompactMode, ResponsesCompactProbeResult,
};
use serde_json::{Map, Value, json};

/// 渠道 settings 中由运行时独占管理的 Responses WebSocket 能力键。
pub(crate) const RESPONSES_WEBSOCKET_ENABLED_KEY: &str = "responses_websocket_enabled";
/// 渠道 settings 中控制 Responses Compact 的三态能力键。
pub(crate) const RESPONSES_COMPACT_MODE_KEY: &str = "responses_compact_mode";
/// 渠道 settings 中仅供 `/responses/compact` 使用的上游模型映射键。
pub(crate) const RESPONSES_COMPACT_MODEL_MAPPING_KEY: &str = "compact_model_mapping";
/// 渠道 settings 中由运行时独占维护的 Compact 确定性探测结论键。
pub(crate) const RESPONSES_COMPACT_PROBE_RESULT_KEY: &str = "responses_compact_probe_result";
/// 渠道 settings 中由运行时独占维护的 Compact 最近探测时间键。
pub(crate) const RESPONSES_COMPACT_PROBE_CHECKED_AT_KEY: &str =
    "responses_compact_probe_checked_at";
/// 渠道 settings 中由运行时独占维护的 Compact 最近 HTTP 状态键。
pub(crate) const RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY: &str =
    "responses_compact_probe_http_status";
/// 渠道 settings 中由管理与运行时共同维护的自动禁用规则键。
pub(crate) const AUTO_BAN_RULES_KEY: &str = "auto_ban_rules";
/// 渠道 settings 中由管理与运行时共同维护的外部账号池模式键。
pub(crate) const POOL_MODE_KEY: &str = "pool_mode";
/// 渠道 settings 中显式选择的客户端仿真档案；缺失表示关闭。
pub(crate) const CLIENT_SIMULATION_PROFILE_KEY: &str = "client_simulation_profile";
/// 渠道 settings 中显式选择的客户端仿真正文档案；缺失表示关闭。
pub(crate) const CLIENT_SIMULATION_BODY_PROFILE_KEY: &str = "client_simulation_body_profile";

const AUTO_BAN_STATUS_CODES_KEY: &str = "status_codes";
const AUTO_BAN_KEYWORDS_KEY: &str = "keywords";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ChannelSettingsError;

/// 从渠道 settings 读取并验证的 Compact 探测事实。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResponsesCompactProbeRecord {
    result: ResponsesCompactProbeResult,
    checked_at: i64,
    http_status: Option<u16>,
}

impl ResponsesCompactProbeRecord {
    /// 返回最近一次确定性探测结论。
    pub(crate) const fn result(self) -> ResponsesCompactProbeResult {
        self.result
    }

    /// 返回最近一次确定性探测开始时的 UTC Unix 毫秒。
    pub(crate) const fn checked_at(self) -> i64 {
        self.checked_at
    }

    /// 返回最近一次响应的受控 HTTP 状态；传输失败不会产生本记录。
    pub(crate) const fn http_status(self) -> Option<u16> {
        self.http_status
    }
}

/// 从敏感 settings 中读取非敏感能力投影；缺失表示默认关闭。
pub(crate) fn responses_websocket_enabled(settings: &Value) -> Result<bool, ChannelSettingsError> {
    let object = settings.as_object().ok_or(ChannelSettingsError)?;
    match object.get(RESPONSES_WEBSOCKET_ENABLED_KEY) {
        None => Ok(false),
        Some(Value::Bool(enabled)) => Ok(*enabled),
        Some(_) => Err(ChannelSettingsError),
    }
}

/// 从渠道 settings 读取 Compact 策略；缺失时保持探测优先的安全默认值。
pub(crate) fn responses_compact_mode(
    settings: &Value,
) -> Result<ResponsesCompactMode, ChannelSettingsError> {
    let object = settings.as_object().ok_or(ChannelSettingsError)?;
    match object.get(RESPONSES_COMPACT_MODE_KEY) {
        None => Ok(ResponsesCompactMode::Auto),
        Some(Value::String(mode)) => mode.parse().map_err(|_| ChannelSettingsError),
        Some(_) => Err(ChannelSettingsError),
    }
}

/// 读取 Compact 专属模型映射；缺失时表示沿用普通渠道映射后的模型名。
pub(crate) fn responses_compact_model_mapping(
    settings: &Value,
) -> Result<ChannelModelMappings, ChannelSettingsError> {
    let object = settings.as_object().ok_or(ChannelSettingsError)?;
    object.get(RESPONSES_COMPACT_MODEL_MAPPING_KEY).map_or_else(
        || Ok(ChannelModelMappings::default()),
        |value| ChannelModelMappings::parse(value).map_err(|_| ChannelSettingsError),
    )
}

/// 读取 Compact 探测事实；旧记录或显式未知状态统一视为尚未探测。
pub(crate) fn responses_compact_probe_record(
    settings: &Value,
) -> Result<Option<ResponsesCompactProbeRecord>, ChannelSettingsError> {
    let object = settings.as_object().ok_or(ChannelSettingsError)?;
    let result = match object.get(RESPONSES_COMPACT_PROBE_RESULT_KEY) {
        None => {
            if object.contains_key(RESPONSES_COMPACT_PROBE_CHECKED_AT_KEY)
                || object.contains_key(RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY)
            {
                return Err(ChannelSettingsError);
            }
            return Ok(None);
        }
        Some(Value::String(result)) => result
            .parse::<ResponsesCompactProbeResult>()
            .map_err(|_| ChannelSettingsError)?,
        Some(_) => return Err(ChannelSettingsError),
    };
    if result == ResponsesCompactProbeResult::Unknown {
        if object.contains_key(RESPONSES_COMPACT_PROBE_CHECKED_AT_KEY)
            || object.contains_key(RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY)
        {
            return Err(ChannelSettingsError);
        }
        return Ok(None);
    }
    let checked_at = object
        .get(RESPONSES_COMPACT_PROBE_CHECKED_AT_KEY)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or(ChannelSettingsError)?;
    let http_status = object
        .get(RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY)
        .map(parse_http_status)
        .transpose()?;
    Ok(Some(ResponsesCompactProbeRecord {
        result,
        checked_at,
        http_status,
    }))
}

/// 返回 Compact 的闭合探测结论；缺少记录时保持失败关闭的未知状态。
pub(crate) fn responses_compact_probe_result(
    settings: &Value,
) -> Result<ResponsesCompactProbeResult, ChannelSettingsError> {
    responses_compact_probe_record(settings).map(|record| {
        record.map_or(ResponsesCompactProbeResult::Unknown, |record| {
            record.result()
        })
    })
}

/// Compact 专属映射只允许挂在原生 OpenAI Responses 渠道上。
pub(crate) fn validate_responses_compact_model_mapping(
    channel_type: ChannelType,
    protocol: Protocol,
    mapping: &ChannelModelMappings,
) -> Result<(), ChannelSettingsError> {
    if !mapping.is_empty()
        && (channel_type != ChannelType::OpenAi || protocol != Protocol::OpenAiResponses)
    {
        return Err(ChannelSettingsError);
    }
    Ok(())
}

/// 从敏感 settings 中读取池模式；缺失表示继续维护渠道级本地健康状态。
pub(crate) fn pool_mode(settings: &Value) -> Result<bool, ChannelSettingsError> {
    let object = settings.as_object().ok_or(ChannelSettingsError)?;
    match object.get(POOL_MODE_KEY) {
        None => Ok(false),
        Some(Value::Bool(enabled)) => Ok(*enabled),
        Some(_) => Err(ChannelSettingsError),
    }
}

/// 从渠道 settings 读取可选客户端仿真档案；缺失表示默认关闭。
pub(crate) fn client_simulation_profile(
    settings: &Value,
) -> Result<Option<ClientSimulationProfile>, ChannelSettingsError> {
    let object = settings.as_object().ok_or(ChannelSettingsError)?;
    match object.get(CLIENT_SIMULATION_PROFILE_KEY) {
        None => Ok(None),
        Some(Value::String(profile)) => profile.parse().map(Some).map_err(|_| ChannelSettingsError),
        Some(_) => Err(ChannelSettingsError),
    }
}

/// 从渠道 settings 读取可选客户端仿真正文档案；缺失表示默认关闭。
pub(crate) fn client_simulation_body_profile(
    settings: &Value,
) -> Result<Option<ClientSimulationBodyProfile>, ChannelSettingsError> {
    let object = settings.as_object().ok_or(ChannelSettingsError)?;
    match object.get(CLIENT_SIMULATION_BODY_PROFILE_KEY) {
        None => Ok(None),
        Some(Value::String(profile)) => profile.parse().map(Some).map_err(|_| ChannelSettingsError),
        Some(_) => Err(ChannelSettingsError),
    }
}

/// 从敏感 settings 中读取已验证的非敏感自动禁用规则；缺失表示空规则集。
pub(crate) fn auto_ban_rules(
    settings: &Value,
) -> Result<ChannelAutoBanRules, ChannelSettingsError> {
    let object = settings.as_object().ok_or(ChannelSettingsError)?;
    parse_auto_ban_rules_value(object.get(AUTO_BAN_RULES_KEY))
}

/// 解析管理查询或运行时投影取得的单个规则对象。
pub(crate) fn parse_auto_ban_rules_value(
    value: Option<&Value>,
) -> Result<ChannelAutoBanRules, ChannelSettingsError> {
    let Some(value) = value else {
        return Ok(ChannelAutoBanRules::default());
    };
    let object = value.as_object().ok_or(ChannelSettingsError)?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            AUTO_BAN_STATUS_CODES_KEY | AUTO_BAN_KEYWORDS_KEY
        )
    }) {
        return Err(ChannelSettingsError);
    }
    let status_codes = parse_status_codes(object)?;
    let keywords = parse_keywords(object)?;
    ChannelAutoBanRules::new(status_codes, keywords).map_err(|_| ChannelSettingsError)
}

/// 验证该能力只在已经完成生产装配的原生 Responses 渠道开启。
pub(crate) fn validate_responses_websocket_capability(
    channel_type: ChannelType,
    protocol: Protocol,
    enabled: bool,
) -> Result<(), ChannelSettingsError> {
    if enabled && (channel_type != ChannelType::OpenAi || protocol != Protocol::OpenAiResponses) {
        return Err(ChannelSettingsError);
    }
    Ok(())
}

/// 首版仿真档案只允许挂在原生 Anthropic 渠道；OAuth 凭据在运行时继续复验。
pub(crate) fn validate_client_simulation_capability(
    channel_type: ChannelType,
    protocol: Protocol,
    profile: Option<ClientSimulationProfile>,
) -> Result<(), ChannelSettingsError> {
    if profile.is_some()
        && (channel_type != ChannelType::Anthropic || protocol != Protocol::Anthropic)
    {
        return Err(ChannelSettingsError);
    }
    Ok(())
}

/// 正文档案只能附加在对应的受控 Header 档案上，避免出现半配置的正文改写。
pub(crate) fn validate_client_simulation_body_capability(
    channel_type: ChannelType,
    protocol: Protocol,
    profile: Option<ClientSimulationProfile>,
    body_profile: Option<ClientSimulationBodyProfile>,
) -> Result<(), ChannelSettingsError> {
    if body_profile.is_some()
        && (channel_type != ChannelType::Anthropic
            || protocol != Protocol::Anthropic
            || profile != Some(ClientSimulationProfile::AnthropicCliHeadersV1))
    {
        return Err(ChannelSettingsError);
    }
    Ok(())
}

/// 强制开启 Compact 时只允许原生 OpenAI Responses 渠道，其他策略保持兼容但不启用。
pub(crate) fn validate_responses_compact_capability(
    channel_type: ChannelType,
    protocol: Protocol,
    mode: ResponsesCompactMode,
) -> Result<(), ChannelSettingsError> {
    if mode == ResponsesCompactMode::ForceOn
        && (channel_type != ChannelType::OpenAi || protocol != Protocol::OpenAiResponses)
    {
        return Err(ChannelSettingsError);
    }
    Ok(())
}

/// 确定性 Compact 探测事实只允许挂在原生 OpenAI Responses 渠道上。
pub(crate) fn validate_responses_compact_probe_result(
    channel_type: ChannelType,
    protocol: Protocol,
    result: ResponsesCompactProbeResult,
) -> Result<(), ChannelSettingsError> {
    if result != ResponsesCompactProbeResult::Unknown
        && (channel_type != ChannelType::OpenAi || protocol != Protocol::OpenAiResponses)
    {
        return Err(ChannelSettingsError);
    }
    Ok(())
}

/// 在保留未知敏感设置的前提下写入受控能力；关闭时移除默认值。
pub(crate) fn set_responses_websocket_enabled(
    settings: &mut Value,
    enabled: bool,
) -> Result<(), ChannelSettingsError> {
    let object = settings.as_object_mut().ok_or(ChannelSettingsError)?;
    if enabled {
        object.insert(
            RESPONSES_WEBSOCKET_ENABLED_KEY.to_owned(),
            Value::Bool(true),
        );
    } else {
        object.remove(RESPONSES_WEBSOCKET_ENABLED_KEY);
    }
    Ok(())
}

/// 写入 Compact 策略，默认的 auto 不落库以兼容旧渠道记录。
pub(crate) fn set_responses_compact_mode(
    settings: &mut Value,
    mode: ResponsesCompactMode,
) -> Result<(), ChannelSettingsError> {
    let object = settings.as_object_mut().ok_or(ChannelSettingsError)?;
    if mode == ResponsesCompactMode::Auto {
        object.remove(RESPONSES_COMPACT_MODE_KEY);
    } else {
        object.insert(
            RESPONSES_COMPACT_MODE_KEY.to_owned(),
            Value::String(mode.as_str().to_owned()),
        );
    }
    Ok(())
}

/// 写入 Compact 专属模型映射；空映射移除默认键并保留其他敏感设置。
pub(crate) fn set_responses_compact_model_mapping(
    settings: &mut Value,
    mapping: &ChannelModelMappings,
) -> Result<(), ChannelSettingsError> {
    let object = settings.as_object_mut().ok_or(ChannelSettingsError)?;
    if mapping.is_empty() {
        object.remove(RESPONSES_COMPACT_MODEL_MAPPING_KEY);
    } else {
        object.insert(
            RESPONSES_COMPACT_MODEL_MAPPING_KEY.to_owned(),
            mapping.to_projection_json(),
        );
    }
    Ok(())
}

/// 写入一次确定性 Compact 探测事实；未知状态通过清理全部运行时字段表达。
pub(crate) fn set_responses_compact_probe_record(
    settings: &mut Value,
    result: ResponsesCompactProbeResult,
    checked_at: i64,
    http_status: Option<u16>,
) -> Result<(), ChannelSettingsError> {
    if result == ResponsesCompactProbeResult::Unknown {
        return clear_responses_compact_probe_record(settings);
    }
    if checked_at <= 0 || http_status.is_some_and(|status| !(100..=599).contains(&status)) {
        return Err(ChannelSettingsError);
    }
    let object = settings.as_object_mut().ok_or(ChannelSettingsError)?;
    object.insert(
        RESPONSES_COMPACT_PROBE_RESULT_KEY.to_owned(),
        Value::String(result.as_str().to_owned()),
    );
    object.insert(
        RESPONSES_COMPACT_PROBE_CHECKED_AT_KEY.to_owned(),
        Value::from(checked_at),
    );
    match http_status {
        Some(status) => {
            object.insert(
                RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY.to_owned(),
                Value::from(status),
            );
        }
        None => {
            object.remove(RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY);
        }
    }
    Ok(())
}

/// 清理运行时独占的 Compact 探测事实，供配置变更和未知状态复用。
pub(crate) fn clear_responses_compact_probe_record(
    settings: &mut Value,
) -> Result<(), ChannelSettingsError> {
    let object = settings.as_object_mut().ok_or(ChannelSettingsError)?;
    object.remove(RESPONSES_COMPACT_PROBE_RESULT_KEY);
    object.remove(RESPONSES_COMPACT_PROBE_CHECKED_AT_KEY);
    object.remove(RESPONSES_COMPACT_PROBE_HTTP_STATUS_KEY);
    Ok(())
}

/// 在保留未知敏感设置的前提下写入池模式；关闭时移除默认值。
pub(crate) fn set_pool_mode(
    settings: &mut Value,
    enabled: bool,
) -> Result<(), ChannelSettingsError> {
    let object = settings.as_object_mut().ok_or(ChannelSettingsError)?;
    if enabled {
        object.insert(POOL_MODE_KEY.to_owned(), Value::Bool(true));
    } else {
        object.remove(POOL_MODE_KEY);
    }
    Ok(())
}

/// 写入版本化仿真档案；关闭时移除键，保持默认路径无配置噪声。
pub(crate) fn set_client_simulation_profile(
    settings: &mut Value,
    profile: Option<ClientSimulationProfile>,
) -> Result<(), ChannelSettingsError> {
    let object = settings.as_object_mut().ok_or(ChannelSettingsError)?;
    match profile {
        Some(profile) => {
            object.insert(
                CLIENT_SIMULATION_PROFILE_KEY.to_owned(),
                Value::String(profile.as_str().to_owned()),
            );
        }
        None => {
            object.remove(CLIENT_SIMULATION_PROFILE_KEY);
        }
    }
    Ok(())
}

/// 写入版本化正文档案；关闭时移除键，保持默认路径无配置噪声。
pub(crate) fn set_client_simulation_body_profile(
    settings: &mut Value,
    profile: Option<ClientSimulationBodyProfile>,
) -> Result<(), ChannelSettingsError> {
    let object = settings.as_object_mut().ok_or(ChannelSettingsError)?;
    match profile {
        Some(profile) => {
            object.insert(
                CLIENT_SIMULATION_BODY_PROFILE_KEY.to_owned(),
                Value::String(profile.as_str().to_owned()),
            );
        }
        None => {
            object.remove(CLIENT_SIMULATION_BODY_PROFILE_KEY);
        }
    }
    Ok(())
}

/// 在保留未知敏感设置的前提下写入受控自动禁用规则；空规则集移除默认键。
pub(crate) fn set_auto_ban_rules(
    settings: &mut Value,
    rules: &ChannelAutoBanRules,
) -> Result<(), ChannelSettingsError> {
    let object = settings.as_object_mut().ok_or(ChannelSettingsError)?;
    if rules.is_empty() {
        object.remove(AUTO_BAN_RULES_KEY);
    } else {
        object.insert(
            AUTO_BAN_RULES_KEY.to_owned(),
            json!({
                "status_codes": rules
                    .server_statuses()
                    .iter()
                    .map(|status| status.get())
                    .collect::<Vec<_>>(),
                "keywords": rules.keywords(),
            }),
        );
    }
    Ok(())
}

fn parse_status_codes(object: &Map<String, Value>) -> Result<Vec<u16>, ChannelSettingsError> {
    object
        .get(AUTO_BAN_STATUS_CODES_KEY)
        .map_or(Ok(Vec::new()), |value| {
            value
                .as_array()
                .ok_or(ChannelSettingsError)?
                .iter()
                .map(|value| {
                    value
                        .as_u64()
                        .and_then(|value| u16::try_from(value).ok())
                        .ok_or(ChannelSettingsError)
                })
                .collect()
        })
}

fn parse_keywords(object: &Map<String, Value>) -> Result<Vec<String>, ChannelSettingsError> {
    object
        .get(AUTO_BAN_KEYWORDS_KEY)
        .map_or(Ok(Vec::new()), |value| {
            value
                .as_array()
                .ok_or(ChannelSettingsError)?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or(ChannelSettingsError)
                })
                .collect()
        })
}

fn parse_http_status(value: &Value) -> Result<u16, ChannelSettingsError> {
    value
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| (100..=599).contains(value))
        .ok_or(ChannelSettingsError)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn capability_defaults_off_and_rejects_non_boolean_values() {
        assert!(!responses_websocket_enabled(&json!({})).unwrap());
        assert!(
            responses_websocket_enabled(&json!({"responses_websocket_enabled": true})).unwrap()
        );
        assert!(
            responses_websocket_enabled(&json!({"responses_websocket_enabled": "true"})).is_err()
        );
    }

    #[test]
    fn compact_mode_defaults_to_auto_and_rejects_unknown_values() {
        assert_eq!(
            responses_compact_mode(&json!({})).unwrap(),
            ResponsesCompactMode::Auto
        );
        assert_eq!(
            responses_compact_mode(&json!({"responses_compact_mode": "force_on"})).unwrap(),
            ResponsesCompactMode::ForceOn
        );
        assert!(responses_compact_mode(&json!({"responses_compact_mode": "unknown"})).is_err());
        assert!(responses_compact_mode(&json!({"responses_compact_mode": true})).is_err());
    }

    #[test]
    fn compact_model_mapping_defaults_empty_and_rejects_non_native_channels() {
        assert!(
            responses_compact_model_mapping(&json!({}))
                .unwrap()
                .is_empty()
        );
        let mapping = responses_compact_model_mapping(&json!({
            "compact_model_mapping": {"gpt-public": "gpt-upstream"}
        }))
        .unwrap();
        assert_eq!(mapping.resolve("gpt-public"), Some("gpt-upstream"));
        assert!(
            validate_responses_compact_model_mapping(
                ChannelType::OpenAi,
                Protocol::OpenAiResponses,
                &mapping,
            )
            .is_ok()
        );
        assert!(
            validate_responses_compact_model_mapping(
                ChannelType::OpenAi,
                Protocol::OpenAiChat,
                &mapping,
            )
            .is_err()
        );
        assert!(
            responses_compact_model_mapping(&json!({
                "compact_model_mapping": {"gpt-public": 1}
            }))
            .is_err()
        );
    }

    #[test]
    fn compact_mode_validates_native_responses_and_preserves_unknown_settings() {
        assert!(
            validate_responses_compact_capability(
                ChannelType::OpenAi,
                Protocol::OpenAiResponses,
                ResponsesCompactMode::ForceOn,
            )
            .is_ok()
        );
        assert!(
            validate_responses_compact_capability(
                ChannelType::OpenAi,
                Protocol::OpenAiChat,
                ResponsesCompactMode::ForceOn,
            )
            .is_err()
        );
        assert!(
            validate_responses_compact_capability(
                ChannelType::Anthropic,
                Protocol::Anthropic,
                ResponsesCompactMode::Auto,
            )
            .is_ok()
        );

        let mut settings = json!({"private_extension": true});
        set_responses_compact_mode(&mut settings, ResponsesCompactMode::ForceOn).unwrap();
        assert_eq!(
            responses_compact_mode(&settings).unwrap(),
            ResponsesCompactMode::ForceOn
        );
        set_responses_compact_mode(&mut settings, ResponsesCompactMode::Auto).unwrap();
        assert_eq!(
            responses_compact_mode(&settings).unwrap(),
            ResponsesCompactMode::Auto
        );
        assert_eq!(settings["private_extension"], true);
    }

    #[test]
    fn compact_probe_record_is_closed_and_runtime_write_is_reversible() {
        assert_eq!(
            responses_compact_probe_result(&json!({})).unwrap(),
            ResponsesCompactProbeResult::Unknown
        );
        let mut settings = json!({"private_extension": true});
        set_responses_compact_probe_record(
            &mut settings,
            ResponsesCompactProbeResult::Supported,
            1_735_000_000_000,
            Some(200),
        )
        .unwrap();
        let record = responses_compact_probe_record(&settings).unwrap().unwrap();
        assert_eq!(record.result(), ResponsesCompactProbeResult::Supported);
        assert_eq!(record.checked_at(), 1_735_000_000_000);
        assert_eq!(record.http_status(), Some(200));
        assert_eq!(settings["private_extension"], true);

        clear_responses_compact_probe_record(&mut settings).unwrap();
        assert_eq!(
            responses_compact_probe_result(&settings).unwrap(),
            ResponsesCompactProbeResult::Unknown
        );
        for invalid in [
            json!({"responses_compact_probe_result": "supported"}),
            json!({
                "responses_compact_probe_result": "supported",
                "responses_compact_probe_checked_at": 1,
                "responses_compact_probe_http_status": 99
            }),
            json!({
                "responses_compact_probe_result": "unknown",
                "responses_compact_probe_checked_at": 1
            }),
        ] {
            assert!(responses_compact_probe_record(&invalid).is_err());
        }
        assert!(
            validate_responses_compact_probe_result(
                ChannelType::OpenAi,
                Protocol::OpenAiResponses,
                ResponsesCompactProbeResult::Supported,
            )
            .is_ok()
        );
        assert!(
            validate_responses_compact_probe_result(
                ChannelType::OpenAi,
                Protocol::OpenAiChat,
                ResponsesCompactProbeResult::Supported,
            )
            .is_err()
        );
    }

    #[test]
    fn controlled_write_preserves_unknown_settings() {
        let mut settings = json!({"private_extension": {"secret": true}});
        set_responses_websocket_enabled(&mut settings, true).unwrap();
        assert!(responses_websocket_enabled(&settings).unwrap());
        set_pool_mode(&mut settings, true).unwrap();
        assert!(pool_mode(&settings).unwrap());
        assert_eq!(settings["private_extension"]["secret"], true);
        set_responses_websocket_enabled(&mut settings, false).unwrap();
        set_pool_mode(&mut settings, false).unwrap();
        assert!(!responses_websocket_enabled(&settings).unwrap());
        assert!(!pool_mode(&settings).unwrap());
        assert_eq!(settings["private_extension"]["secret"], true);
    }

    #[test]
    fn pool_mode_defaults_off_and_rejects_non_boolean_values() {
        assert!(!pool_mode(&json!({})).unwrap());
        assert!(pool_mode(&json!({"pool_mode": true})).unwrap());
        assert!(pool_mode(&json!({"pool_mode": "true"})).is_err());
    }

    #[test]
    fn client_simulation_profile_defaults_off_and_preserves_unknown_settings() {
        assert_eq!(client_simulation_profile(&json!({})).unwrap(), None);
        let mut settings = json!({"private_extension": true});
        set_client_simulation_profile(
            &mut settings,
            Some(ClientSimulationProfile::AnthropicCliHeadersV1),
        )
        .unwrap();
        assert_eq!(
            client_simulation_profile(&settings).unwrap(),
            Some(ClientSimulationProfile::AnthropicCliHeadersV1)
        );
        assert_eq!(settings["private_extension"], true);
        set_client_simulation_profile(&mut settings, None).unwrap();
        assert_eq!(client_simulation_profile(&settings).unwrap(), None);
        assert_eq!(settings["private_extension"], true);

        for invalid in [
            json!({"client_simulation_profile": null}),
            json!({"client_simulation_profile": true}),
            json!({"client_simulation_profile": "unknown"}),
        ] {
            assert!(client_simulation_profile(&invalid).is_err());
        }
    }

    #[test]
    fn client_simulation_profile_is_limited_to_native_anthropic() {
        let profile = Some(ClientSimulationProfile::AnthropicCliHeadersV1);
        assert!(
            validate_client_simulation_capability(
                ChannelType::Anthropic,
                Protocol::Anthropic,
                profile,
            )
            .is_ok()
        );
        assert!(
            validate_client_simulation_capability(
                ChannelType::OpenAi,
                Protocol::Anthropic,
                profile,
            )
            .is_err()
        );
        assert!(
            validate_client_simulation_capability(
                ChannelType::Anthropic,
                Protocol::OpenAiChat,
                profile,
            )
            .is_err()
        );
        assert!(
            validate_client_simulation_capability(ChannelType::OpenAi, Protocol::OpenAiChat, None,)
                .is_ok()
        );
    }

    #[test]
    fn client_simulation_body_profile_requires_matching_header_profile() {
        assert_eq!(client_simulation_body_profile(&json!({})).unwrap(), None);
        let mut settings = json!({"private_extension": true});
        set_client_simulation_body_profile(
            &mut settings,
            Some(ClientSimulationBodyProfile::AnthropicCliSystemDateV1),
        )
        .unwrap();
        assert_eq!(
            client_simulation_body_profile(&settings).unwrap(),
            Some(ClientSimulationBodyProfile::AnthropicCliSystemDateV1)
        );
        assert!(
            validate_client_simulation_body_capability(
                ChannelType::Anthropic,
                Protocol::Anthropic,
                Some(ClientSimulationProfile::AnthropicCliHeadersV1),
                Some(ClientSimulationBodyProfile::AnthropicCliSystemDateV1),
            )
            .is_ok()
        );
        assert!(
            validate_client_simulation_body_capability(
                ChannelType::Anthropic,
                Protocol::Anthropic,
                None,
                Some(ClientSimulationBodyProfile::AnthropicCliSystemDateV1),
            )
            .is_err()
        );
        set_client_simulation_body_profile(&mut settings, None).unwrap();
        assert_eq!(client_simulation_body_profile(&settings).unwrap(), None);
    }

    #[test]
    fn auto_ban_rules_round_trip_without_exposing_unknown_settings() {
        let mut settings = json!({"private_extension": {"secret": true}});
        let rules = ChannelAutoBanRules::new(vec![503, 500], vec!["Workspace Disabled".to_owned()])
            .unwrap();

        set_auto_ban_rules(&mut settings, &rules).unwrap();
        assert_eq!(auto_ban_rules(&settings).unwrap(), rules);
        assert_eq!(settings["private_extension"]["secret"], true);

        set_auto_ban_rules(&mut settings, &ChannelAutoBanRules::default()).unwrap();
        assert!(auto_ban_rules(&settings).unwrap().is_empty());
        assert!(settings.get(AUTO_BAN_RULES_KEY).is_none());
    }

    #[test]
    fn auto_ban_rules_reject_unknown_fields_and_invalid_values() {
        for settings in [
            json!({"auto_ban_rules": {"status_codes": [499]}}),
            json!({"auto_ban_rules": {"keywords": ["line\nbreak"]}}),
            json!({"auto_ban_rules": {"unknown": []}}),
        ] {
            assert!(auto_ban_rules(&settings).is_err());
        }
    }
}
