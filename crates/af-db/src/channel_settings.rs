use af_domain::{ChannelAutoBanRules, ChannelType, Protocol};
use serde_json::{Map, Value, json};

/// 渠道 settings 中由运行时独占管理的 Responses WebSocket 能力键。
pub(crate) const RESPONSES_WEBSOCKET_ENABLED_KEY: &str = "responses_websocket_enabled";
/// 渠道 settings 中由管理与运行时共同维护的自动禁用规则键。
pub(crate) const AUTO_BAN_RULES_KEY: &str = "auto_ban_rules";

const AUTO_BAN_STATUS_CODES_KEY: &str = "status_codes";
const AUTO_BAN_KEYWORDS_KEY: &str = "keywords";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ChannelSettingsError;

/// 从敏感 settings 中读取非敏感能力投影；缺失表示默认关闭。
pub(crate) fn responses_websocket_enabled(settings: &Value) -> Result<bool, ChannelSettingsError> {
    let object = settings.as_object().ok_or(ChannelSettingsError)?;
    match object.get(RESPONSES_WEBSOCKET_ENABLED_KEY) {
        None => Ok(false),
        Some(Value::Bool(enabled)) => Ok(*enabled),
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
    fn controlled_write_preserves_unknown_settings() {
        let mut settings = json!({"private_extension": {"secret": true}});
        set_responses_websocket_enabled(&mut settings, true).unwrap();
        assert!(responses_websocket_enabled(&settings).unwrap());
        assert_eq!(settings["private_extension"]["secret"], true);
        set_responses_websocket_enabled(&mut settings, false).unwrap();
        assert!(!responses_websocket_enabled(&settings).unwrap());
        assert_eq!(settings["private_extension"]["secret"], true);
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
