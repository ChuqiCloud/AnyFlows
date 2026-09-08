use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::response_wire::{
    AssistantRoleWire, MessageObjectWire, OutputTokensDetailsWire, RefusalStopDetailsWire,
    ServerToolUsageWire, ServiceTierWire, StopReasonWire, ToolCallerWire, UsageWire,
};

/// Anthropic Messages 的单个流式 data 事件。
#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub(super) enum StreamEventWire {
    /// 消息身份与初始累计用量。
    #[serde(rename = "message_start")]
    MessageStart { message: MessageStartWire },
    /// 内容块开始。
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u32,
        content_block: ContentBlockStartWire,
    },
    /// 内容块增量。
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: u32,
        delta: ContentBlockDeltaWire,
    },
    /// 内容块结束。
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: u32 },
    /// 消息停止原因与最终累计用量。
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDeltaWire,
        usage: MessageDeltaUsageWire,
    },
    /// 消息完整结束。
    #[serde(rename = "message_stop")]
    MessageStop,
    /// 上游保活事件。
    #[serde(rename = "ping")]
    Ping,
    /// HTTP 成功提交后出现的流内错误。
    #[serde(rename = "error")]
    Error { error: StreamErrorWire },
}

/// `message_start` 中尚未产生内容的消息快照。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MessageStartWire {
    pub(super) id: String,
    #[serde(rename = "type")]
    pub(super) _kind: MessageObjectWire,
    #[serde(rename = "role")]
    pub(super) _role: AssistantRoleWire,
    pub(super) content: Vec<Value>,
    pub(super) model: String,
    #[serde(default)]
    pub(super) container: Option<Value>,
    #[serde(default)]
    pub(super) stop_reason: Option<StopReasonWire>,
    #[serde(default)]
    pub(super) stop_sequence: Option<String>,
    #[serde(default)]
    pub(super) stop_details: Option<RefusalStopDetailsWire>,
    pub(super) usage: UsageWire,
}

/// 流式内容块开始时允许的闭合块类型。
#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub(super) enum ContentBlockStartWire {
    /// 文本块。
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(default)]
        citations: Option<Vec<Value>>,
    },
    /// 带连续会话签名的思考块。
    #[serde(rename = "thinking")]
    Thinking { thinking: String, signature: String },
    /// 客户端声明的直接工具调用。
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
        #[serde(default, rename = "caller")]
        _caller: Option<ToolCallerWire>,
    },
}

/// 流式内容块增量。
#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub(super) enum ContentBlockDeltaWire {
    /// 文本增量。
    #[serde(rename = "text_delta")]
    Text { text: String },
    /// 思考文本增量。
    #[serde(rename = "thinking_delta")]
    Thinking { thinking: String },
    /// 思考签名增量。
    #[serde(rename = "signature_delta")]
    Signature { signature: String },
    /// 工具参数的未完成 JSON 增量。
    #[serde(rename = "input_json_delta")]
    InputJson { partial_json: String },
}

/// `message_delta.delta` 的停止状态。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MessageDeltaWire {
    #[serde(default)]
    pub(super) container: Option<Value>,
    #[serde(default)]
    pub(super) stop_details: Option<RefusalStopDetailsWire>,
    pub(super) stop_reason: StopReasonWire,
    #[serde(default)]
    pub(super) stop_sequence: Option<String>,
}

/// `message_delta.usage` 的最终累计快照。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MessageDeltaUsageWire {
    #[serde(default)]
    pub(super) input_tokens: Option<i64>,
    #[serde(default)]
    pub(super) cache_creation_input_tokens: Option<i64>,
    #[serde(default)]
    pub(super) cache_read_input_tokens: Option<i64>,
    pub(super) output_tokens: i64,
    #[serde(default)]
    pub(super) output_tokens_details: Option<OutputTokensDetailsWire>,
    #[serde(default)]
    pub(super) server_tool_use: Option<ServerToolUsageWire>,
    #[serde(default)]
    pub(super) service_tier: Option<ServiceTierWire>,
}

/// Anthropic 流内错误；正文只用于结构校验，不进入 Canonical 或 Debug。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamErrorWire {
    #[serde(rename = "type")]
    pub(super) kind: StreamErrorTypeWire,
    #[serde(rename = "message")]
    pub(super) _message: String,
}

/// Anthropic API 当前公开的错误分类。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StreamErrorTypeWire {
    InvalidRequestError,
    AuthenticationError,
    BillingError,
    PermissionError,
    NotFoundError,
    RequestTooLarge,
    RateLimitError,
    GatewayTimeout,
    ApiError,
    OverloadedError,
}

/// 编码后的 `message_start` 事件。
#[derive(Serialize)]
pub(super) struct EncodedMessageStart<'a> {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) message: EncodedMessage<'a>,
}

/// 编码后的初始消息快照。
#[derive(Serialize)]
pub(super) struct EncodedMessage<'a> {
    pub(super) id: &'a str,
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) role: &'static str,
    pub(super) content: Vec<()>,
    pub(super) model: &'a str,
    pub(super) container: Option<()>,
    pub(super) stop_reason: Option<()>,
    pub(super) stop_sequence: Option<()>,
    pub(super) stop_details: Option<()>,
    pub(super) usage: Value,
}

/// 编码后的内容块开始事件。
#[derive(Serialize)]
pub(super) struct EncodedContentBlockStart<'a> {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) index: u32,
    pub(super) content_block: EncodedContentBlock<'a>,
}

/// 编码后的内容块起始快照。
#[derive(Serialize)]
#[serde(tag = "type")]
pub(super) enum EncodedContentBlock<'a> {
    #[serde(rename = "text")]
    Text {
        text: &'static str,
        citations: Option<()>,
    },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: &'static str,
        signature: &'static str,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: &'a str,
        name: &'a str,
        input: Value,
        caller: EncodedDirectCaller,
    },
}

/// 编码后的直接工具调用来源。
#[derive(Serialize)]
pub(super) struct EncodedDirectCaller {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
}

/// 编码后的内容块增量事件。
#[derive(Serialize)]
pub(super) struct EncodedContentBlockDelta<'a> {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) index: u32,
    pub(super) delta: EncodedContentDelta<'a>,
}

/// 编码后的内容块增量。
#[derive(Serialize)]
#[serde(tag = "type")]
pub(super) enum EncodedContentDelta<'a> {
    #[serde(rename = "text_delta")]
    Text { text: &'a str },
    #[serde(rename = "thinking_delta")]
    Thinking { thinking: &'a str },
    #[serde(rename = "signature_delta")]
    Signature { signature: &'a str },
    #[serde(rename = "input_json_delta")]
    InputJson { partial_json: &'a str },
}

/// 编码后的内容块停止事件。
#[derive(Serialize)]
pub(super) struct EncodedContentBlockStop {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) index: u32,
}

/// 编码后的消息状态增量。
#[derive(Serialize)]
pub(super) struct EncodedMessageDelta<'a> {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) delta: EncodedMessageDeltaState<'a>,
    pub(super) usage: Value,
}

/// 编码后的停止原因。
#[derive(Serialize)]
pub(super) struct EncodedMessageDeltaState<'a> {
    pub(super) container: Option<()>,
    pub(super) stop_details: Option<()>,
    pub(super) stop_reason: &'static str,
    pub(super) stop_sequence: Option<&'a str>,
}

/// 不携带额外字段的终止或保活事件。
#[derive(Serialize)]
pub(super) struct EncodedSimpleEvent {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
}
