use serde::{Deserialize, Serialize};

/// 固定服务入口。新增入口须同时更新适配器，不能配置任意地址。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenCodeEntry {
    #[default]
    InferenceV1,
    ZenV1,
}

impl OpenCodeEntry {
    pub const fn root(self) -> &'static str {
        match self {
            Self::InferenceV1 => "https://opencode.ai/inference",
            Self::ZenV1 => "https://opencode.ai/zen",
        }
    }
}
