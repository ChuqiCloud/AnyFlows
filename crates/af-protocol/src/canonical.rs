use std::{error::Error, fmt};

use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, Value};

use crate::ResponsesCompactionItem;
use crate::request::{
    Attachment, ReasoningConfig, RequestContinuation, RequestMetadata, Sampling, StreamOptions,
    ToolChoice, ToolDef,
};

/// 一次请求的协议无关中间表示。
///
/// `system` 与 `developer` 指令统一放在 `messages` 中，以保留原始消息顺序。
/// 尚未建模的字段只能通过绑定来源协议的 [`RawPassthrough`] 携带。
///
/// Canonical IR 不是任何厂商的 wire DTO，不直接实现 Serde：
///
/// ```compile_fail
/// use af_domain::Operation;
/// use af_protocol::CanonicalRequest;
///
/// let request = CanonicalRequest::new(Operation::Chat, "model".to_owned(), vec![], false);
/// serde_json::to_value(request).unwrap();
/// ```
///
/// ```compile_fail
/// use af_protocol::CanonicalRequest;
///
/// let _: CanonicalRequest = serde_json::from_value(serde_json::json!({})).unwrap();
/// ```
#[derive(Clone, PartialEq)]
pub struct CanonicalRequest {
    /// 请求执行的操作类型。
    pub operation: Operation,
    /// 客户端请求的模型名；渠道映射前不得改写。
    pub model: String,
    /// 按原始顺序归一化的消息。
    pub messages: Vec<Message>,
    /// 客户端声明的可调用工具。
    pub tools: Vec<ToolDef>,
    /// 模型选择工具的策略。
    pub tool_choice: ToolChoice,
    /// 协议无关的推理参数。
    pub reasoning: Option<ReasoningConfig>,
    /// 协议无关的采样参数。
    pub sampling: Sampling,
    /// 客户端是否请求流式响应。
    pub stream: bool,
    /// 客户端对流末 usage 等可选数据的偏好。
    pub stream_options: StreamOptions,
    /// 未内联到消息内容中的多模态附件。
    pub attachments: Vec<Attachment>,
    /// 用于会话粘性等网关能力的请求元数据。
    pub metadata: RequestMetadata,
    /// 跨请求延续响应、会话和提示缓存的引用。
    pub continuation: RequestContinuation,
    raw_passthrough: Option<RawPassthrough>,
}

impl CanonicalRequest {
    /// 构造不包含协议私有字段的 Canonical 请求。
    #[must_use]
    pub fn new(operation: Operation, model: String, messages: Vec<Message>, stream: bool) -> Self {
        Self {
            operation,
            model,
            messages,
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            reasoning: None,
            sampling: Sampling::EMPTY,
            stream,
            stream_options: StreamOptions::EMPTY,
            attachments: Vec::new(),
            metadata: RequestMetadata::EMPTY,
            continuation: RequestContinuation::EMPTY,
            raw_passthrough: None,
        }
        .with_validated_raw_passthrough(None)
    }

    /// 由协议解析器附加已完成边界校验的私有字段。
    pub(crate) fn with_validated_raw_passthrough(
        mut self,
        raw_passthrough: Option<(Protocol, Map<String, Value>)>,
    ) -> Self {
        self.raw_passthrough = raw_passthrough.map(|(source_protocol, fields)| {
            RawPassthrough::new_validated(source_protocol, fields)
        });
        self
    }

    /// 返回绑定来源协议的未归一化字段。
    #[must_use]
    pub const fn raw_passthrough(&self) -> Option<&RawPassthrough> {
        self.raw_passthrough.as_ref()
    }

    /// 用已校验的消息序列替换正文，并取消私有字段直通资格。
    ///
    /// 正文语义发生变化后必须由目标协议重新编码，不能复用来源 JSON 的私有字段。
    pub(crate) fn with_rewritten_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self.raw_passthrough = None;
        self
    }
}

impl fmt::Debug for CanonicalRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalRequest")
            .field("operation", &self.operation)
            .field("model", &"<已脱敏>")
            .field("message_count", &self.messages.len())
            .field("tool_count", &self.tools.len())
            .field("tool_choice", &self.tool_choice)
            .field("reasoning", &self.reasoning)
            .field("sampling", &self.sampling)
            .field("stream", &self.stream)
            .field("stream_options", &self.stream_options)
            .field("attachment_count", &self.attachments.len())
            .field("metadata", &self.metadata)
            .field("continuation", &self.continuation)
            .field("raw_passthrough", &self.raw_passthrough)
            .finish()
    }
}

/// 一条按角色分类的 Canonical 消息。
#[derive(Clone, PartialEq)]
pub struct Message {
    /// 消息在指令层级中的角色。
    pub role: Role,
    /// 按原始顺序保存的内容块。
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// 构造一条 Canonical 消息。
    #[must_use]
    pub fn new(role: Role, content: Vec<ContentBlock>) -> Self {
        Self { role, content }
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Message")
            .field("role", &self.role)
            .field("content_count", &self.content.len())
            .finish()
    }
}

/// 图片或音频内容的外部来源。
#[derive(Clone, PartialEq)]
pub enum MediaSource {
    /// 由协议边界校验过的 URL。
    Url(String),
    /// 不含 data URL 前缀的 base64 内容。
    Base64(String),
}

impl fmt::Debug for MediaSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(_) => formatter.write_str("Url(<已脱敏>)"),
            Self::Base64(_) => formatter.write_str("Base64(<已脱敏>)"),
        }
    }
}

/// 协议无关的提示缓存断点。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CacheHint {
    /// 缓存有效期为五分钟。
    Ephemeral5Minutes,
    /// 缓存有效期为一小时。
    Ephemeral1Hour,
}

/// 消息中的协议无关内容块。
#[derive(Clone, PartialEq)]
pub enum ContentBlock {
    /// 文本内容。
    Text(String),
    /// 图片内容。
    Image {
        /// 图片来源。
        source: MediaSource,
        /// 已校验的可选媒体类型；远程 URL 通常不提供该信息。
        mime_type: Option<String>,
    },
    /// 音频内容。
    Audio {
        /// 音频来源。
        source: MediaSource,
        /// 已校验的媒体类型。
        mime_type: String,
    },
    /// 模型发起的工具调用。
    ToolUse {
        /// 当前调用在消息内的稳定标识。
        id: String,
        /// 工具名称。
        name: String,
        /// 已解析的工具参数。
        input: Value,
        /// 供应商用于延续推理上下文的不透明签名。
        signature: Option<String>,
    },
    /// 工具执行结果。
    ToolResult {
        /// 对应工具调用的稳定标识。
        tool_use_id: String,
        /// 工具返回的内容块。
        content: Vec<ContentBlock>,
        /// 无法等价表示为普通内容块的结构化 JSON 结果。
        structured_content: Option<Value>,
        /// 工具是否以错误结束。
        is_error: bool,
    },
    /// 模型的推理内容及可选签名。
    Thinking {
        /// 推理文本。
        text: String,
        /// 厂商提供的推理签名。
        signature: Option<String>,
    },
    /// 在当前位置声明提示缓存断点。
    CacheControl(CacheHint),
    /// OpenAI Responses 专用的不透明上下文压缩 Item。
    Compaction(ResponsesCompactionItem),
}

impl fmt::Debug for ContentBlock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(_) => formatter.write_str("Text(<已脱敏>)"),
            Self::Image { .. } => formatter.write_str("Image(<已脱敏>)"),
            Self::Audio { .. } => formatter.write_str("Audio(<已脱敏>)"),
            Self::ToolUse { .. } => formatter.write_str("ToolUse(<已脱敏>)"),
            Self::ToolResult {
                content,
                structured_content,
                is_error,
                ..
            } => formatter
                .debug_struct("ToolResult")
                .field("tool_use_id", &"<已脱敏>")
                .field("content_count", &content.len())
                .field("has_structured_content", &structured_content.is_some())
                .field("is_error", is_error)
                .finish(),
            Self::Thinking { .. } => formatter.write_str("Thinking(<已脱敏>)"),
            Self::CacheControl(hint) => formatter.debug_tuple("CacheControl").field(hint).finish(),
            Self::Compaction(_) => formatter.write_str("Compaction(<已脱敏>)"),
        }
    }
}

/// 与来源协议绑定的未归一化字段集合。
///
/// 只有协议解析器完成键名、层级、大小和冲突校验后才能构造；出站转换不得把它
/// 盲目发送到其他协议，也不得静默丢弃无法消费的字段。
#[derive(Clone, PartialEq)]
pub struct RawPassthrough {
    source_protocol: Protocol,
    fields: Map<String, Value>,
}

impl RawPassthrough {
    /// 创建已由协议边界完成校验的同源字段集合。
    pub(crate) fn new_validated(source_protocol: Protocol, fields: Map<String, Value>) -> Self {
        Self {
            source_protocol,
            fields,
        }
    }

    /// 返回这些字段所属的入站协议。
    #[must_use]
    pub const fn source_protocol(&self) -> Protocol {
        self.source_protocol
    }

    /// 判断私有字段集合是否为空。
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// 仅向同源协议返回保持原始 JSON 结构的私有字段。
    ///
    /// 跨协议转换必须先把字段建模为 Canonical 语义，禁止直接读取后盲目合并。
    pub fn fields_for_protocol(
        &self,
        target_protocol: Protocol,
    ) -> Result<&Map<String, Value>, RawPassthroughError> {
        if target_protocol == self.source_protocol {
            Ok(&self.fields)
        } else {
            Err(RawPassthroughError::ProtocolMismatch)
        }
    }
}

impl fmt::Debug for RawPassthrough {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RawPassthrough")
            .field("source_protocol", &self.source_protocol)
            .field("field_count", &self.fields.len())
            .finish()
    }
}

/// 未归一化字段的访问错误，不保留字段名或字段值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawPassthroughError {
    /// 出站协议与字段来源协议不一致。
    ProtocolMismatch,
}

impl fmt::Display for RawPassthroughError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProtocolMismatch => formatter.write_str("未归一化字段不得跨协议透传"),
        }
    }
}

impl Error for RawPassthroughError {}
