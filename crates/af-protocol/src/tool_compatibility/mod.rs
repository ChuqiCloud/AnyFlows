//! 受控工具补充的纯协议边界。调用方负责冻结路由、预扣、总超时和发送。

mod names;
#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, error::Error, fmt};

use af_domain::{Operation, Role};

use crate::{
    CanonicalRequestEnvelope, CanonicalStreamEvent as Event, ContentBlock, ContentDelta,
    FinishReason, Message, Sampling, TokenCount, ToolChoice, ToolDef, Usage, UsageSource,
};
use names::ToolNames;

const MAX_ARGUMENT_BYTES: usize = 1_048_576;
const UNAVAILABLE: &str = "Tool unavailable: this API session cannot execute this tool. Do not call it again. Continue using the conversation and the caller's available tools.";

/// 受控工具会话的一步；续答只交付协议请求，不会发起网络请求或执行工具。
#[derive(Debug)]
pub enum ToolCompatibilityStep {
    /// 可立即送入下游编码器的事件；隐藏工具事件不包含在内。
    Forward(Vec<Event>),
    /// 第一轮仅调用不可用工具；使用原候选发送且只允许发送这一次续答。
    Continue {
        request: Box<CanonicalRequestEnvelope>,
        usage: Usage,
    },
    /// 最终合法终态；事件包含最终 Finish、合并用量和 StreamEnd。
    Complete {
        events: Vec<Event>,
        usage: Option<Usage>,
    },
}

/// 一次请求的工具映射和最多两轮的响应状态。不得跨调用方或请求复用。
///
/// 注入定义只能由程序注册的实现提供。本组件不提供配置入口，也不执行工具。
/// 生产启用前，调用方必须为准备后的请求及一次续答预扣，并固定凭据和总超时。
pub struct ToolCompatibilitySession {
    request: CanonicalRequestEnvelope,
    names: ToolNames,
    round: Round,
    round_index: usize,
    closed: bool,
    remaining: TokenCount,
    observed: [Option<Usage>; 2],
}

impl ToolCompatibilitySession {
    /// 补充注册工具并冻结映射，保留客户端模型别名并取消源正文直通资格。
    /// 私有参数和不透明续答无法安全改写时明确拒绝。
    pub fn prepare(
        request: CanonicalRequestEnvelope,
        injected: Vec<ToolDef>,
    ) -> Result<Self, ToolCompatibilityError> {
        let canonical = request.canonical();
        if !matches!(canonical.operation, Operation::Chat | Operation::Responses)
            || request.native_only()
            || (canonical.raw_passthrough().is_some() && request.source_context().is_none())
            || canonical.continuation.has_server_managed_state()
        {
            return Err(ToolCompatibilityError::UnsupportedRequest);
        }
        let remaining = canonical
            .sampling
            .max_output_tokens()
            .filter(|budget| *budget > TokenCount::ZERO)
            .ok_or(ToolCompatibilityError::MissingOutputBudget)?;
        let (request, names) = ToolNames::prepare(request, injected)?;
        Ok(Self {
            request,
            names,
            round: Round::default(),
            round_index: 0,
            closed: false,
            remaining,
            observed: [None, None],
        })
    }

    /// 返回第一轮重建请求，用于预估实际上下文和上游编码。
    #[must_use]
    pub const fn request(&self) -> &CanonicalRequestEnvelope {
        &self.request
    }

    /// 保留已知的逐轮原始用量，供失败尝试记录使用；缺失值不替换为零。
    #[must_use]
    pub const fn observed_usage(&self) -> &[Option<Usage>; 2] {
        &self.observed
    }

    /// 消费一个已解码事件。正文、推理和真实工具立即输出；失败后会话永久关闭。
    pub fn push(&mut self, event: Event) -> Result<ToolCompatibilityStep, ToolCompatibilityError> {
        if self.closed {
            return Err(ToolCompatibilityError::Closed);
        }
        let result = self.process(event);
        if result.is_err() {
            self.closed = true;
        }
        result
    }

    /// 传输断流、取消或超时必须由调用方封闭；不会产生成功终态或续答。
    pub fn abort(&mut self) {
        self.closed = true;
    }

    fn process(&mut self, event: Event) -> Result<ToolCompatibilityStep, ToolCompatibilityError> {
        use ToolCompatibilityError as E;
        let mut output = Vec::new();
        match event {
            Event::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            } if !self.round.started => self.round.started = true,
            Event::Usage(usage) => {
                let previous = self.observed[self.round_index];
                self.observed[self.round_index] = Some(usage);
                if usage.output_tokens() > self.remaining
                    || previous.is_some_and(|old| old.output_tokens() > usage.output_tokens())
                {
                    return Err(E::OutputBudgetExceeded);
                }
            }
            Event::Ping => output.push(Event::Ping),
            Event::StreamEnd => return self.end_round(),
            Event::ContentDelta {
                choice_index: 0,
                content_index,
                delta,
            } => {
                self.round.require_open()?;
                if !matches!(&delta, ContentDelta::Text(text) if text.is_empty()) {
                    self.round.make_visible(&mut output)?;
                    output.push(Event::ContentDelta {
                        choice_index: 0,
                        content_index,
                        delta,
                    });
                }
            }
            Event::ReasoningDelta {
                choice_index: 0,
                content_index,
                text,
                signature,
            } => {
                self.round.require_open()?;
                if !text.is_empty() || signature.is_some() {
                    self.round.make_visible(&mut output)?;
                    output.push(Event::ReasoningDelta {
                        choice_index: 0,
                        content_index,
                        text,
                        signature,
                    });
                }
            }
            Event::LogprobsDelta {
                choice_index: 0,
                logprobs,
            } => {
                self.round.require_open()?;
                // 隐藏工具调用的概率明细可能暴露内部工具参数，不转发到客户端。
                if !self.round.hidden {
                    self.round.make_visible(&mut output)?;
                    output.push(Event::LogprobsDelta {
                        choice_index: 0,
                        logprobs,
                    });
                }
            }
            Event::ToolCallStart {
                choice_index: 0,
                tool_index,
                id,
                name,
            } => {
                self.round.require_open()?;
                let hidden = self.names.is_injected(&name);
                if self.round.tools.len() >= 128
                    || id.is_empty()
                    || id.len() > 512
                    || self.round.tools.contains_key(&tool_index)
                    || self.round.tools.values().any(|tool| tool.id == id)
                    || self.names.is_historical_id(&id)
                {
                    return Err(E::InvalidOutput);
                }
                if hidden {
                    if self.round_index != 0 {
                        return Err(E::RepeatedUnavailableTool);
                    }
                    if self.round.visible {
                        return Err(E::HiddenToolAfterOutput);
                    }
                    if matches!(
                        self.request.canonical().tool_choice,
                        ToolChoice::None | ToolChoice::Named { .. }
                    ) {
                        return Err(E::InvalidOutput);
                    }
                    self.round.hidden = true;
                } else {
                    match &self.request.canonical().tool_choice {
                        ToolChoice::None => return Err(E::InvalidOutput),
                        ToolChoice::Named { name: required } if required != &name => {
                            return Err(E::InvalidOutput);
                        }
                        _ => {}
                    }
                    let original = self.names.caller_name(&name).ok_or(E::UnknownTool)?;
                    self.round.make_visible(&mut output)?;
                    output.push(Event::ToolCallStart {
                        choice_index: 0,
                        tool_index,
                        id: id.clone(),
                        name: original.to_owned(),
                    });
                }
                self.round.tools.insert(
                    tool_index,
                    Tool {
                        id,
                        name,
                        arguments: String::new(),
                        signature: None,
                        hidden,
                        ended: false,
                    },
                );
            }
            Event::ToolCallArgsDelta {
                choice_index: 0,
                tool_index,
                partial_json,
            } => {
                self.round.require_open()?;
                self.round.argument_bytes = self
                    .round
                    .argument_bytes
                    .checked_add(partial_json.len())
                    .filter(|bytes| *bytes <= MAX_ARGUMENT_BYTES)
                    .ok_or(E::ArgumentLimit)?;
                let tool = self.round.open_tool(tool_index)?;
                tool.arguments.push_str(&partial_json);
                if !tool.hidden {
                    output.push(Event::ToolCallArgsDelta {
                        choice_index: 0,
                        tool_index,
                        partial_json,
                    });
                }
            }
            Event::ToolCallSignature {
                choice_index: 0,
                tool_index,
                signature,
            } => {
                self.round.require_open()?;
                self.round.argument_bytes = self
                    .round
                    .argument_bytes
                    .checked_add(signature.len())
                    .filter(|bytes| *bytes <= MAX_ARGUMENT_BYTES)
                    .ok_or(E::ArgumentLimit)?;
                let tool = self.round.open_tool(tool_index)?;
                if tool.signature.is_some() {
                    return Err(E::InvalidOutput);
                }
                // 真实工具已改名时，签名不再具有可证明的完整性。
                if !tool.hidden && self.names.caller_name(&tool.name) != Some(tool.name.as_str()) {
                    return Err(E::UnsupportedSignature);
                }
                tool.signature = Some(signature.clone());
                if !tool.hidden {
                    output.push(Event::ToolCallSignature {
                        choice_index: 0,
                        tool_index,
                        signature,
                    });
                }
            }
            Event::ToolCallEnd {
                choice_index: 0,
                tool_index,
            } => {
                self.round.require_open()?;
                let tool = self.round.open_tool(tool_index)?;
                tool.input()?;
                tool.ended = true;
                if !tool.hidden {
                    output.push(Event::ToolCallEnd {
                        choice_index: 0,
                        tool_index,
                    });
                }
            }
            Event::Finish {
                choice_index: 0,
                reason,
                stop_sequence,
            } => {
                self.round.require_open()?;
                if self.round.tools.values().any(|tool| !tool.ended)
                    || (reason == FinishReason::ToolCalls) == self.round.tools.is_empty()
                    || (stop_sequence.is_some() && reason != FinishReason::Stop)
                {
                    return Err(E::InvalidOutput);
                }
                self.round.finish = Some((reason, stop_sequence));
            }
            Event::Error(_) => return Err(E::UpstreamFailure),
            // 多候选和专属能力不能经过工具补充后被压缩或丢弃。
            _ => return Err(E::InvalidOutput),
        }
        Ok(ToolCompatibilityStep::Forward(output))
    }

    fn end_round(&mut self) -> Result<ToolCompatibilityStep, ToolCompatibilityError> {
        use ToolCompatibilityError as E;
        let (reason, stop_sequence) = self.round.finish.clone().ok_or(E::InvalidOutput)?;
        let usage = self.observed[self.round_index];
        if self.round.hidden {
            let usage = usage
                .filter(|usage| {
                    usage.source() == UsageSource::Upstream
                        && usage.output_tokens() > TokenCount::ZERO
                })
                .ok_or(E::MissingUsage)?;
            let remaining = self.remaining.get() - usage.output_tokens().get();
            if remaining <= 0 {
                return Err(E::OutputBudgetExceeded);
            }
            self.remaining = TokenCount::new(remaining).map_err(|_| E::OutputBudgetExceeded)?;
            let request = self.continuation_request()?;
            for tool in self.round.tools.values() {
                self.names.remember_id(tool.id.clone());
            }
            self.round = Round::default();
            self.round_index = 1;
            return Ok(ToolCompatibilityStep::Continue {
                request: Box::new(request),
                usage,
            });
        }
        if !self.round.visible && reason != FinishReason::ContentFilter {
            return Err(E::InvalidOutput);
        }
        if self.round.tools.is_empty()
            && reason != FinishReason::ContentFilter
            && matches!(
                self.request.canonical().tool_choice,
                ToolChoice::Required | ToolChoice::Named { .. }
            )
        {
            return Err(E::InvalidOutput);
        }
        let usage = match (self.observed[0], usage, self.round_index) {
            (Some(first), Some(second), 1) => {
                Some(first.checked_add(second).map_err(|_| E::UsageOverflow)?)
            }
            (_, None, 1) => return Err(E::MissingUsage),
            (_, usage, _) => usage,
        };
        let mut events = Vec::new();
        if !self.round.visible {
            events.push(Event::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            });
        }
        events.push(Event::Finish {
            choice_index: 0,
            reason,
            stop_sequence,
        });
        if let Some(usage) = usage {
            events.push(Event::Usage(usage));
        }
        events.push(Event::StreamEnd);
        self.closed = true;
        Ok(ToolCompatibilityStep::Complete { events, usage })
    }

    fn continuation_request(&self) -> Result<CanonicalRequestEnvelope, ToolCompatibilityError> {
        let mut canonical = self.request.canonical().clone();
        let calls = self
            .round
            .tools
            .values()
            .map(|tool| {
                Ok(ContentBlock::ToolUse {
                    id: tool.id.clone(),
                    name: tool.name.clone(),
                    input: tool.input()?,
                    signature: tool.signature.clone(),
                })
            })
            .collect::<Result<Vec<_>, ToolCompatibilityError>>()?;
        canonical
            .messages
            .push(Message::new(Role::Assistant, calls));
        for tool in self.round.tools.values() {
            canonical.messages.push(Message::new(
                Role::Tool,
                vec![ContentBlock::ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: vec![ContentBlock::Text(UNAVAILABLE.to_owned())],
                    structured_content: None,
                    // 文本明确表达不可执行；不制造 Chat/Responses 不支持的错误标志。
                    is_error: false,
                }],
            ));
        }
        canonical.sampling = Sampling::new(
            canonical.sampling.temperature(),
            canonical.sampling.top_p(),
            Some(self.remaining),
            canonical.sampling.stop_sequences().to_vec(),
        )
        .map_err(|_| ToolCompatibilityError::UnsupportedRequest)?;
        Ok(self.request.clone().with_rebuilt_canonical(canonical))
    }
}

impl fmt::Debug for ToolCompatibilitySession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ToolCompatibilitySession")
            .field("round", &self.round_index)
            .field("closed", &self.closed)
            .field("observed_usage", &self.observed)
            .finish()
    }
}

#[derive(Default)]
struct Round {
    started: bool,
    visible: bool,
    hidden: bool,
    tools: BTreeMap<u32, Tool>,
    argument_bytes: usize,
    finish: Option<(FinishReason, Option<String>)>,
}

impl Round {
    fn require_open(&self) -> Result<(), ToolCompatibilityError> {
        if !self.started || self.finish.is_some() {
            return Err(ToolCompatibilityError::InvalidOutput);
        }
        Ok(())
    }

    fn make_visible(&mut self, events: &mut Vec<Event>) -> Result<(), ToolCompatibilityError> {
        if self.hidden {
            return Err(ToolCompatibilityError::HiddenToolAfterOutput);
        }
        if !self.visible {
            events.push(Event::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            });
            self.visible = true;
        }
        Ok(())
    }

    fn open_tool(&mut self, index: u32) -> Result<&mut Tool, ToolCompatibilityError> {
        self.tools
            .get_mut(&index)
            .filter(|tool| !tool.ended)
            .ok_or(ToolCompatibilityError::InvalidOutput)
    }
}

struct Tool {
    id: String,
    name: String,
    arguments: String,
    signature: Option<String>,
    hidden: bool,
    ended: bool,
}

impl Tool {
    fn input(&self) -> Result<serde_json::Value, ToolCompatibilityError> {
        let value: serde_json::Value = serde_json::from_str(if self.arguments.is_empty() {
            "{}"
        } else {
            &self.arguments
        })
        .map_err(|_| ToolCompatibilityError::InvalidOutput)?;
        if !value.is_object() {
            return Err(ToolCompatibilityError::InvalidOutput);
        }
        Ok(value)
    }
}

/// 稳定错误分类，不包含工具名称、参数或上游正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCompatibilityError {
    UnsupportedRequest,
    InvalidToolSet,
    MissingOutputBudget,
    InvalidOutput,
    UnknownTool,
    HiddenToolAfterOutput,
    RepeatedUnavailableTool,
    UnsupportedSignature,
    ArgumentLimit,
    MissingUsage,
    OutputBudgetExceeded,
    UsageOverflow,
    UpstreamFailure,
    Closed,
}

impl fmt::Display for ToolCompatibilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "tool compatibility: {self:?}")
    }
}

impl Error for ToolCompatibilityError {}
