use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::response_wire::{CompletionUsageWire, FinishReasonWire, ServiceTierWire};

/// OpenAI Chat Completions 流式 chunk。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChatStreamChunkWire {
    pub(super) id: String,
    /// NewAPI 兼容字段；流式身份仍以 OpenAI `id` 为准。
    #[serde(default)]
    pub(super) request_id: Option<String>,
    pub(super) object: ChatStreamObjectWire,
    pub(super) created: i64,
    pub(super) model: String,
    pub(super) choices: Vec<StreamChoiceWire>,
    #[serde(default)]
    pub(super) service_tier: Option<ServiceTierWire>,
    #[serde(default)]
    pub(super) system_fingerprint: Option<String>,
    #[serde(default)]
    pub(super) usage: Option<CompletionUsageWire>,
    #[serde(default)]
    pub(super) moderation: Option<Value>,
}

#[derive(Deserialize)]
pub(super) enum ChatStreamObjectWire {
    #[serde(rename = "chat.completion.chunk")]
    ChatCompletionChunk,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamChoiceWire {
    pub(super) index: u32,
    #[serde(default)]
    pub(super) delta: StreamDeltaWire,
    #[serde(default)]
    pub(super) finish_reason: Option<FinishReasonWire>,
    #[serde(default)]
    pub(super) logprobs: Option<Value>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamDeltaWire {
    #[serde(default)]
    pub(super) role: Option<StreamRoleWire>,
    #[serde(default)]
    pub(super) content: Option<String>,
    #[serde(default)]
    pub(super) reasoning_content: Option<String>,
    #[serde(default)]
    pub(super) reasoning: Option<String>,
    #[serde(default)]
    pub(super) tool_calls: Option<Vec<StreamToolCallWire>>,
    #[serde(default)]
    pub(super) refusal: Option<Value>,
    #[serde(default)]
    pub(super) audio: Option<Value>,
    #[serde(default)]
    pub(super) function_call: Option<Value>,
}

#[derive(Deserialize)]
pub(super) enum StreamRoleWire {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamToolCallWire {
    pub(super) index: u32,
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default, rename = "type")]
    pub(super) kind: Option<StreamToolTypeWire>,
    #[serde(default)]
    pub(super) function: Option<StreamFunctionWire>,
}

#[derive(Deserialize)]
pub(super) enum StreamToolTypeWire {
    #[serde(rename = "function")]
    Function,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamFunctionWire {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) arguments: Option<String>,
}

#[derive(Serialize)]
pub(super) struct EncodedChunk<'a> {
    pub(super) id: &'a str,
    pub(super) object: &'static str,
    pub(super) created: i64,
    pub(super) model: &'a str,
    pub(super) choices: Vec<EncodedChoice<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) usage: Option<Option<EncodedUsage>>,
}

#[derive(Serialize)]
pub(super) struct EncodedChoice<'a> {
    pub(super) index: u32,
    pub(super) delta: EncodedDelta<'a>,
    pub(super) logprobs: Option<()>,
    pub(super) finish_reason: Option<&'static str>,
}

#[derive(Default, Serialize)]
pub(super) struct EncodedDelta<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) content: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reasoning_content: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) tool_calls: Vec<EncodedToolCall<'a>>,
}

#[derive(Serialize)]
pub(super) struct EncodedToolCall<'a> {
    pub(super) index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub(super) kind: Option<&'static str>,
    pub(super) function: EncodedFunction<'a>,
}

#[derive(Serialize)]
pub(super) struct EncodedFunction<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) arguments: Option<&'a str>,
}

#[derive(Serialize)]
pub(super) struct EncodedUsage {
    pub(super) prompt_tokens: i64,
    pub(super) completion_tokens: i64,
    pub(super) total_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) prompt_tokens_details: Option<EncodedPromptDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) completion_tokens_details: Option<EncodedCompletionDetails>,
}

#[derive(Serialize)]
pub(super) struct EncodedPromptDetails {
    pub(super) cached_tokens: i64,
    pub(super) audio_tokens: i64,
}

#[derive(Serialize)]
pub(super) struct EncodedCompletionDetails {
    pub(super) reasoning_tokens: i64,
    pub(super) audio_tokens: i64,
}
