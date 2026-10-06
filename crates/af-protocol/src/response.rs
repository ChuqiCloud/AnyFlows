use std::fmt;

use af_domain::{Operation, Protocol};
use serde_json::{Map, Value};

use crate::{FinishReason, Message, RawPassthrough, Usage};

/// 一次模型响应的协议无关中间表示。
///
/// 响应元数据和候选结果保持独立，避免把多候选响应压缩成单条消息。
/// Canonical 响应不是厂商 wire DTO，不直接实现 Serde。
///
/// ```compile_fail
/// use af_domain::Operation;
/// use af_protocol::CanonicalResponse;
///
/// let response = CanonicalResponse::new(
///     Operation::Chat,
///     "id".to_owned(),
///     "model".to_owned(),
///     None,
///     vec![],
///     None,
/// );
/// serde_json::to_value(response).unwrap();
/// ```
#[derive(Clone, PartialEq)]
pub struct CanonicalResponse {
    /// 响应对应的操作类型。
    pub operation: Operation,
    /// 上游返回的响应标识。
    pub id: String,
    /// 实际生成响应的模型名。
    pub model: String,
    /// 上游创建时间的 Unix 秒时间戳；协议未提供时保持 `None`。
    pub created_at: Option<i64>,
    /// 按上游顺序保存的候选结果。
    pub choices: Vec<ResponseChoice>,
    /// 上游返回的用量；缺失时保持 `None`。
    pub usage: Option<Usage>,
    raw_passthrough: Option<RawPassthrough>,
}

impl CanonicalResponse {
    /// 构造不包含协议私有字段的 Canonical 响应。
    #[must_use]
    pub fn new(
        operation: Operation,
        id: String,
        model: String,
        created_at: Option<i64>,
        choices: Vec<ResponseChoice>,
        usage: Option<Usage>,
    ) -> Self {
        Self {
            operation,
            id,
            model,
            created_at,
            choices,
            usage,
            raw_passthrough: None,
        }
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
}

impl fmt::Debug for CanonicalResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalResponse")
            .field("operation", &self.operation)
            .field("id", &"<已脱敏>")
            .field("model", &"<已脱敏>")
            .field("created_at", &self.created_at)
            .field("choice_count", &self.choices.len())
            .field("usage", &self.usage)
            .field("raw_passthrough", &self.raw_passthrough)
            .finish()
    }
}

/// 响应中的单个候选结果。
#[derive(Clone, PartialEq)]
pub struct ResponseChoice {
    /// 上游提供的候选索引，不假定连续或有序。
    pub index: u32,
    /// 候选生成的助手消息。
    pub message: Message,
    /// 上游声明的结束原因。
    pub finish_reason: FinishReason,
    /// 实际命中的停止序列；仅在协议明确返回时存在。
    pub stop_sequence: Option<String>,
}

impl ResponseChoice {
    /// 构造一个响应候选结果。
    #[must_use]
    pub const fn new(index: u32, message: Message, finish_reason: FinishReason) -> Self {
        Self {
            index,
            message,
            finish_reason,
            stop_sequence: None,
        }
    }

    /// 附加协议明确返回的停止序列。
    #[must_use]
    pub fn with_stop_sequence(mut self, stop_sequence: Option<String>) -> Self {
        self.stop_sequence = stop_sequence;
        self
    }
}

impl fmt::Debug for ResponseChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponseChoice")
            .field("index", &self.index)
            .field("message", &self.message)
            .field("finish_reason", &self.finish_reason)
            .field(
                "stop_sequence",
                &self.stop_sequence.as_ref().map(|_| "<已脱敏>"),
            )
            .finish()
    }
}
