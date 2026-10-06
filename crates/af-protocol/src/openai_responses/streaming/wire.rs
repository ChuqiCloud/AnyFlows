use serde::Deserialize;
use serde_json::{Map, Value};

use super::super::response_wire::{OutputContentWire, ResponseOutputItemWire, SummaryTextWire};

/// 当前切片支持的 Responses 语义流事件。
#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub(super) enum StreamEventWire {
    #[serde(rename = "response.created")]
    ResponseCreated {
        response: Value,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.in_progress")]
    ResponseInProgress {
        response: Value,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        output_index: u32,
        item: ResponseOutputItemWire,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        output_index: u32,
        item: ResponseOutputItemWire,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: OutputContentWire,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: OutputContentWire,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        item_id: String,
        output_index: u32,
        content_index: u32,
        delta: String,
        #[serde(default)]
        logprobs: Vec<Value>,
        #[serde(default, rename = "obfuscation")]
        _obfuscation: Option<String>,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
        text: String,
        #[serde(default)]
        logprobs: Vec<Value>,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionArgumentsDelta {
        item_id: String,
        output_index: u32,
        delta: String,
        #[serde(default, rename = "obfuscation")]
        _obfuscation: Option<String>,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionArgumentsDone {
        item_id: String,
        output_index: u32,
        arguments: String,
        name: String,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.reasoning_summary_part.added")]
    ReasoningSummaryPartAdded {
        item_id: String,
        output_index: u32,
        summary_index: u32,
        part: SummaryTextWire,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.reasoning_summary_part.done")]
    ReasoningSummaryPartDone {
        item_id: String,
        output_index: u32,
        summary_index: u32,
        part: SummaryTextWire,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta {
        item_id: String,
        output_index: u32,
        summary_index: u32,
        delta: String,
        #[serde(default, rename = "obfuscation")]
        _obfuscation: Option<String>,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.reasoning_summary_text.done")]
    ReasoningSummaryTextDone {
        item_id: String,
        output_index: u32,
        summary_index: u32,
        text: String,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.completed")]
    ResponseCompleted {
        response: Value,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.incomplete")]
    ResponseIncomplete {
        response: Value,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.failed")]
    ResponseFailed {
        response: Value,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        code: Option<String>,
        #[serde(rename = "message")]
        _message: String,
        #[serde(default, rename = "param")]
        _param: Option<String>,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
}

impl StreamEventWire {
    /// 返回供应商提供的可选事件序号。
    pub(super) const fn sequence_number(&self) -> Option<u64> {
        match self {
            Self::ResponseCreated {
                sequence_number, ..
            }
            | Self::ResponseInProgress {
                sequence_number, ..
            }
            | Self::OutputItemAdded {
                sequence_number, ..
            }
            | Self::OutputItemDone {
                sequence_number, ..
            }
            | Self::ContentPartAdded {
                sequence_number, ..
            }
            | Self::ContentPartDone {
                sequence_number, ..
            }
            | Self::OutputTextDelta {
                sequence_number, ..
            }
            | Self::OutputTextDone {
                sequence_number, ..
            }
            | Self::FunctionArgumentsDelta {
                sequence_number, ..
            }
            | Self::FunctionArgumentsDone {
                sequence_number, ..
            }
            | Self::ReasoningSummaryPartAdded {
                sequence_number, ..
            }
            | Self::ReasoningSummaryPartDone {
                sequence_number, ..
            }
            | Self::ReasoningSummaryTextDelta {
                sequence_number, ..
            }
            | Self::ReasoningSummaryTextDone {
                sequence_number, ..
            }
            | Self::ResponseCompleted {
                sequence_number, ..
            }
            | Self::ResponseIncomplete {
                sequence_number, ..
            }
            | Self::ResponseFailed {
                sequence_number, ..
            }
            | Self::Error {
                sequence_number, ..
            } => *sequence_number,
        }
    }
}

/// 判断事件类型是否属于当前明确支持的闭合集合。
pub(super) fn is_supported_event_type(kind: &str) -> bool {
    matches!(
        kind,
        "response.created"
            | "response.in_progress"
            | "response.output_item.added"
            | "response.output_item.done"
            | "response.content_part.added"
            | "response.content_part.done"
            | "response.output_text.delta"
            | "response.output_text.done"
            | "response.function_call_arguments.delta"
            | "response.function_call_arguments.done"
            | "response.reasoning_summary_part.added"
            | "response.reasoning_summary_part.done"
            | "response.reasoning_summary_text.delta"
            | "response.reasoning_summary_text.done"
            | "response.completed"
            | "response.incomplete"
            | "response.failed"
            | "error"
    )
}

/// 允许忽略但不改变响应生命周期的供应商元数据事件。
pub(super) fn is_ignored_metadata_event_type(kind: &str) -> bool {
    matches!(kind, "codex.rate_limits")
}

/// 已知但尚未进入 Canonical 的事件必须按能力不支持失败关闭。
pub(super) fn is_known_unsupported_event_type(kind: &str) -> bool {
    kind.starts_with("response.") && !is_supported_event_type(kind)
}

/// 在反序列化前识别无法无损表达的 Item 与内容块类型。
pub(super) fn uses_known_unsupported_payload(kind: &str, value: &Value) -> bool {
    let object = value.as_object();
    match kind {
        "response.output_item.added" | "response.output_item.done" => object
            .and_then(|event| event.get("item"))
            .and_then(Value::as_object)
            .and_then(|item| item.get("type"))
            .and_then(Value::as_str)
            .is_some_and(|item_kind| {
                !matches!(
                    item_kind,
                    "message" | "function_call" | "reasoning" | "compaction"
                )
            }),
        "response.content_part.added" | "response.content_part.done" => object
            .and_then(|event| event.get("part"))
            .and_then(Value::as_object)
            .and_then(|part| part.get("type"))
            .and_then(Value::as_str)
            .is_some_and(|part_kind| part_kind != "output_text"),
        _ => false,
    }
}

/// 尚未分配官方事件序号的出站语义事件。
pub(super) struct PendingEvent {
    kind: &'static str,
    fields: Map<String, Value>,
}

impl PendingEvent {
    /// 使用固定事件类型与已构造字段创建出站事件。
    pub(super) fn new<const N: usize>(kind: &'static str, fields: [(&str, Value); N]) -> Self {
        Self {
            kind,
            fields: Map::from_iter(
                fields
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value)),
            ),
        }
    }

    /// 返回用于 SSE `event:` 行的官方事件类型。
    pub(super) const fn kind(&self) -> &'static str {
        self.kind
    }

    /// 注入严格单调的序号并构造完整 data JSON。
    pub(super) fn into_value(mut self, sequence_number: u64) -> Value {
        self.fields
            .insert("type".to_owned(), Value::String(self.kind.to_owned()));
        self.fields.insert(
            "sequence_number".to_owned(),
            Value::Number(sequence_number.into()),
        );
        Value::Object(self.fields)
    }
}
