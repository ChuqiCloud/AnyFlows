use af_domain::{ChannelType, OpenCodeEntry};
use serde_json::Value;

/// 读取受控入口。非 OpenCode 渠道不能携带服务专属设置。
pub fn channel_opencode_entry(
    channel_type: ChannelType,
    settings: &Value,
) -> Result<Option<OpenCodeEntry>, crate::AdminChannelWriteRepositoryError> {
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

#[cfg(test)]
mod tests {
    use super::*;
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
