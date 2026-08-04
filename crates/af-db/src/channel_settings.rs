use af_domain::{ChannelType, Protocol};
use serde_json::Value;

/// 渠道 settings 中由运行时独占管理的 Responses WebSocket 能力键。
pub(crate) const RESPONSES_WEBSOCKET_ENABLED_KEY: &str = "responses_websocket_enabled";

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
}
