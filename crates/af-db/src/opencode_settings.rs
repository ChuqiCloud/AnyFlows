use af_domain::{ChannelType, OpenCodeEntry};
use serde_json::Value;

/// 读取受控入口。非 OpenCode 渠道不能携带服务专属设置。
pub fn channel_opencode_entry(
    channel_type: ChannelType,
    settings: &Value,
) -> Result<Option<OpenCodeEntry>, crate::AdminChannelWriteRepositoryError> {
    channel_opencode_full_compatibility(channel_type, settings)?;
    let value = settings.get("opencode_entry");
    if channel_type != ChannelType::OpenCode {
        return if value.is_some() {
            Err(crate::AdminChannelWriteRepositoryError::InvalidInput)
        } else {
            Ok(None)
        };
    }
    let entry = value.map_or(Ok(OpenCodeEntry::default()), |value| {
        serde_json::from_value(value.clone())
            .map_err(|_| crate::AdminChannelWriteRepositoryError::InvalidInput)
    })?;
    Ok(Some(entry))
}

/// 完整工具兼容仅允许在 OpenCode 渠道显式开启，旧配置保持关闭。
pub fn channel_opencode_full_compatibility(
    channel_type: ChannelType,
    settings: &Value,
) -> Result<bool, crate::AdminChannelWriteRepositoryError> {
    match settings.get("opencode_full_compatibility") {
        None => Ok(false),
        Some(Value::Bool(enabled)) if channel_type == ChannelType::OpenCode => Ok(*enabled),
        Some(_) => Err(crate::AdminChannelWriteRepositoryError::InvalidInput),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compatibility_requires_explicit_boolean_on_opencode_only() {
        assert!(
            !channel_opencode_full_compatibility(ChannelType::OpenCode, &serde_json::json!({}))
                .unwrap()
        );
        assert!(
            channel_opencode_full_compatibility(
                ChannelType::OpenCode,
                &serde_json::json!({"opencode_full_compatibility":true})
            )
            .unwrap()
        );
        for value in [
            serde_json::json!(null),
            serde_json::json!("true"),
            serde_json::json!(1),
        ] {
            assert!(
                channel_opencode_entry(
                    ChannelType::OpenCode,
                    &serde_json::json!({"opencode_full_compatibility":value})
                )
                .is_err()
            );
        }
        for value in [true, false] {
            assert!(
                channel_opencode_entry(
                    ChannelType::OpenAi,
                    &serde_json::json!({"opencode_full_compatibility":value})
                )
                .is_err()
            );
        }
    }
    #[test]
    fn entry_selection_rejects_arbitrary_urls_and_other_channels() {
        assert_eq!(
            channel_opencode_entry(ChannelType::OpenCode, &serde_json::json!({})).unwrap(),
            Some(OpenCodeEntry::InferenceV1)
        );
        assert_eq!(
            channel_opencode_entry(
                ChannelType::OpenCode,
                &serde_json::json!({"opencode_entry":"zen_v1"})
            )
            .unwrap(),
            Some(OpenCodeEntry::ZenV1)
        );
        for value in [
            serde_json::json!("https://example.com"),
            serde_json::json!(null),
            serde_json::json!({}),
        ] {
            assert!(
                channel_opencode_entry(
                    ChannelType::OpenCode,
                    &serde_json::json!({"opencode_entry": value})
                )
                .is_err()
            );
        }
        assert!(
            channel_opencode_entry(
                ChannelType::OpenAi,
                &serde_json::json!({"opencode_entry":"inference_v1"})
            )
            .is_err()
        );
    }
}
