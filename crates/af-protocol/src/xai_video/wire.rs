use serde::{Deserialize, Deserializer, de::Error as _};

/// 区分字段缺失与显式空值；xAI 视频协议中的显式 `null` 一律拒绝。
#[derive(Default)]
pub(super) enum Field<T> {
    #[default]
    Missing,
    Value(T),
}

pub(super) fn deserialize_field<'de, D, T>(deserializer: D) -> Result<Field<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Field::Value)
        .ok_or_else(|| D::Error::custom("字段不得为 null"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct XaiVideoGenerationRequestWire {
    pub(super) model: String,
    pub(super) prompt: String,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) duration: Field<u8>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) aspect_ratio: Field<String>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) resolution: Field<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct XaiVideoSubmissionWire {
    pub(super) request_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct XaiVideoPollWire {
    pub(super) status: XaiVideoStatusWire,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) video: Field<XaiVideoOutputWire>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) model: Field<String>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) error: Field<XaiVideoErrorWire>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum XaiVideoStatusWire {
    Pending,
    Done,
    Expired,
    Failed,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct XaiVideoOutputWire {
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) url: Field<String>,
    pub(super) duration: u8,
    pub(super) respect_moderation: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct XaiVideoErrorWire {
    pub(super) code: XaiVideoErrorCodeWire,
    pub(super) message: String,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum XaiVideoErrorCodeWire {
    InvalidArgument,
    PermissionDenied,
    FailedPrecondition,
    ServiceUnavailable,
    InternalError,
}
