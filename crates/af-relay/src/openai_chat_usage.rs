use std::{collections::BTreeMap, fmt};

use af_protocol::{
    CanonicalRequest, CanonicalResponse, CanonicalStreamEvent, ContentBlock, ContentDelta, Message,
    TokenCount, ToolChoice, Usage, UsageDetails, UsageSemantics, UsageSource,
    openai_chat::MAX_OUTPUT_TOKENS,
};
use thiserror::Error;
use tiktoken_rs::{
    CoreBPE, bpe_for_tokenizer, cl100k_base_singleton, o200k_base_singleton,
    tokenizer::get_tokenizer,
};
use tokio::sync::oneshot;

const MESSAGE_OVERHEAD_TOKENS: i64 = 3;
const REPLY_PRIMING_TOKENS: i64 = 3;
const NAMED_FIELD_OVERHEAD_TOKENS: i64 = 1;
const TOOL_OVERHEAD_TOKENS: i64 = 8;
const ENCRYPTED_REASONING_ITEM_OVERHEAD_TOKENS: i64 = 16;
const MAX_ESTIMATED_OUTPUT_BYTES: usize = 12 * 1024 * 1024;

/// OpenAI Chat usage 无法可靠得到时的闭合原因。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UsageResolutionError {
    /// 流未收到完整 `[DONE]` 终点。
    #[error("流在 usage 确认前中断")]
    Interrupted,
    /// 请求或响应含本地令牌器没有可靠公式的媒体内容。
    #[error("当前内容无法使用本地令牌器可靠估算")]
    UnsupportedContent,
    /// 估算状态违反已建模的流事件顺序。
    #[error("usage 估算状态无效")]
    InvalidSequence,
    /// 字节数或令牌数超过受支持的整数边界。
    #[error("usage 估算超过数值边界")]
    Overflow,
    /// 结构化内容无法转换为稳定估算输入。
    #[error("usage 估算输入无法编码")]
    EstimationFailed,
    /// Responses 远程续接上下文无法在预扣前确定输入令牌数。
    #[error("远程续接上下文无法安全预估")]
    StatefulContextUnsupported,
}

/// 流式响应完成后可消费一次的规范化 usage 结果。
pub struct OpenAiChatUsageHandle {
    receiver: oneshot::Receiver<Result<Usage, UsageResolutionError>>,
}

impl OpenAiChatUsageHandle {
    /// 等待流完整结束，并返回上游 usage 或本地估算结果。
    pub async fn resolve(self) -> Result<Usage, UsageResolutionError> {
        self.receiver
            .await
            .unwrap_or(Err(UsageResolutionError::Interrupted))
    }
}

impl fmt::Debug for OpenAiChatUsageHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OpenAiChatUsageHandle(<待完成>)")
    }
}

pub(crate) type UsageSender = oneshot::Sender<Result<Usage, UsageResolutionError>>;

pub(crate) fn usage_channel() -> (UsageSender, OpenAiChatUsageHandle) {
    let (sender, receiver) = oneshot::channel();
    (sender, OpenAiChatUsageHandle { receiver })
}

/// 在不接触计费会话的前提下归一上游 usage，或对受支持内容执行本地估算。
pub(crate) struct OpenAiChatUsageEstimator {
    counter: TokenCounter,
    input_tokens: Result<TokenCount, UsageResolutionError>,
    output: OutputAccumulator,
    upstream_usage: Option<Usage>,
}

/// 按请求边界估算一次 Chat 调用的最大规范化 usage。
///
/// 输入令牌复用当前有界 tokenizer；未声明输出上限时使用协议硬上限，避免把缺失参数
/// 静默当成零额度或免费请求。该函数只做纯计算，不执行定价、预扣或任何持久化 IO。
pub fn estimate_openai_chat_request_upper_bound(
    request: &CanonicalRequest,
) -> Result<Usage, UsageResolutionError> {
    estimate_openai_request_upper_bound(request)
}

/// 按 Canonical 请求边界估算 Chat 或 Responses 调用的最大 usage。
pub fn estimate_openai_request_upper_bound(
    request: &CanonicalRequest,
) -> Result<Usage, UsageResolutionError> {
    if request.operation == af_domain::Operation::Responses
        && (request.continuation.previous_response_id().is_some()
            || request.continuation.conversation_id().is_some())
    {
        return Err(UsageResolutionError::StatefulContextUnsupported);
    }
    let estimator = OpenAiChatUsageEstimator::new(request);
    let input_tokens = estimator.input_tokens?;
    let output_tokens = request
        .sampling
        .max_output_tokens()
        .unwrap_or_else(|| TokenCount::new(MAX_OUTPUT_TOKENS).expect("协议输出上限必须有效"));
    Usage::new(
        input_tokens,
        output_tokens,
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Estimated,
        UsageSemantics::Inclusive,
    )
    .map_err(|_| UsageResolutionError::Overflow)
}

/// 使用模型对应的 OpenAI tokenizer 估算一组纯文本字段的令牌总数。
///
/// 未知兼容模型会取 `cl100k` 与 `o200k` 的较大值，降低第三方模型名导致少计费的风险。
pub fn estimate_openai_text_tokens<'a>(
    model: &str,
    texts: impl IntoIterator<Item = &'a str>,
) -> Result<TokenCount, UsageResolutionError> {
    let counter = TokenCounter::for_model(model);
    let mut total = 0_i64;
    for text in texts {
        checked_add(&mut total, counter.count(text)?)?;
    }
    TokenCount::new(total).map_err(|_| UsageResolutionError::Overflow)
}

impl OpenAiChatUsageEstimator {
    pub(crate) fn new(request: &CanonicalRequest) -> Self {
        let counter = TokenCounter::for_model(&request.model);
        let input_tokens = estimate_input_tokens(request, counter);
        Self {
            counter,
            input_tokens,
            output: OutputAccumulator::default(),
            upstream_usage: None,
        }
    }

    pub(crate) const fn has_upstream_usage(&self) -> bool {
        self.upstream_usage.is_some()
    }

    /// 返回流起点可公开的受控本地输入用量，不用零值代替未知输入。
    pub(crate) fn initial_usage(&self) -> Result<Usage, UsageResolutionError> {
        Usage::new(
            self.input_tokens?,
            TokenCount::ZERO,
            UsageDetails::new(
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
            ),
            UsageSource::Estimated,
            UsageSemantics::Inclusive,
        )
        .map_err(|_| UsageResolutionError::Overflow)
    }

    pub(crate) fn observe(&mut self, event: &CanonicalStreamEvent) {
        match event {
            CanonicalStreamEvent::MessageStart { choice_index, .. } => {
                self.output.start_choice(*choice_index);
            }
            CanonicalStreamEvent::ContentDelta {
                choice_index,
                delta,
                ..
            } => self.output.push_content(*choice_index, delta),
            CanonicalStreamEvent::ReasoningDelta {
                choice_index, text, ..
            } => self.output.push_reasoning(*choice_index, text),
            CanonicalStreamEvent::ToolCallStart {
                choice_index,
                tool_index,
                id,
                name,
            } => self.output.start_tool(*choice_index, *tool_index, id, name),
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index,
                tool_index,
                partial_json,
            } => self
                .output
                .push_tool_arguments(*choice_index, *tool_index, partial_json),
            CanonicalStreamEvent::Usage(usage) => {
                if self.upstream_usage.is_some() {
                    self.output.fail(UsageResolutionError::InvalidSequence);
                } else {
                    self.upstream_usage = Some(*usage);
                }
            }
            CanonicalStreamEvent::ToolCallEnd { .. }
            | CanonicalStreamEvent::ToolCallSignature { .. }
            | CanonicalStreamEvent::CompactionStart { .. }
            | CanonicalStreamEvent::CompactionEnd { .. }
            | CanonicalStreamEvent::Finish { .. }
            | CanonicalStreamEvent::PromptBlocked
            | CanonicalStreamEvent::Ping
            | CanonicalStreamEvent::StreamEnd => {}
            CanonicalStreamEvent::Error(_) => {
                self.output.fail(UsageResolutionError::Interrupted);
            }
        }
    }

    pub(crate) fn resolve(&self) -> Result<Usage, UsageResolutionError> {
        if let Some(usage) = self.upstream_usage {
            return Ok(usage);
        }
        self.resolve_estimated()
    }

    /// 忽略上游快照并按本地计数器闭合 usage，供要求流起点 usage 的目标协议使用。
    pub(crate) fn resolve_estimated(&self) -> Result<Usage, UsageResolutionError> {
        estimate_usage(self.input_tokens, &self.output, self.counter)
    }

    pub(crate) fn resolve_response(
        mut self,
        response: &CanonicalResponse,
    ) -> Result<Usage, UsageResolutionError> {
        if let Some(usage) = response.usage {
            return Ok(usage);
        }
        for choice in &response.choices {
            self.output.start_choice(choice.index);
            self.output
                .push_response_message(choice.index, &choice.message);
        }
        self.resolve()
    }
}

impl fmt::Debug for OpenAiChatUsageEstimator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiChatUsageEstimator")
            .field("input_estimate_ready", &self.input_tokens.is_ok())
            .field("choice_count", &self.output.choices.len())
            .field("has_upstream_usage", &self.upstream_usage.is_some())
            .finish()
    }
}

#[derive(Clone, Copy)]
enum TokenCounter {
    Exact(&'static CoreBPE),
    Conservative {
        cl100k: &'static CoreBPE,
        o200k: &'static CoreBPE,
    },
}

impl TokenCounter {
    fn for_model(model: &str) -> Self {
        if let Some(tokenizer) = get_tokenizer(model)
            && let Ok(bpe) = bpe_for_tokenizer(tokenizer)
        {
            return Self::Exact(bpe);
        }
        // 兼容渠道的模型名可能不在 tiktoken 映射表中；取两代编码的较大值降低少扣风险。
        Self::Conservative {
            cl100k: cl100k_base_singleton(),
            o200k: o200k_base_singleton(),
        }
    }

    fn count(self, text: &str) -> Result<i64, UsageResolutionError> {
        if text.is_empty() {
            return Ok(0);
        }
        let tokens = match self {
            Self::Exact(bpe) => bpe.count_ordinary(text),
            Self::Conservative { cl100k, o200k } => {
                cl100k.count_ordinary(text).max(o200k.count_ordinary(text))
            }
        };
        i64::try_from(tokens).map_err(|_| UsageResolutionError::Overflow)
    }
}

fn estimate_input_tokens(
    request: &CanonicalRequest,
    counter: TokenCounter,
) -> Result<TokenCount, UsageResolutionError> {
    let mut total = REPLY_PRIMING_TOKENS;
    let allow_encrypted_reasoning = request.operation == af_domain::Operation::Responses;
    for message in &request.messages {
        checked_add(&mut total, MESSAGE_OVERHEAD_TOKENS)?;
        checked_add(&mut total, counter.count(message.role.as_str())?)?;
        for block in &message.content {
            add_input_block(&mut total, block, counter, allow_encrypted_reasoning)?;
        }
    }
    for tool in &request.tools {
        checked_add(&mut total, TOOL_OVERHEAD_TOKENS)?;
        checked_add(&mut total, counter.count(&tool.name)?)?;
        if let Some(description) = &tool.description {
            checked_add(&mut total, counter.count(description)?)?;
        }
        let schema = serde_json::to_string(&tool.input_schema)
            .map_err(|_| UsageResolutionError::EstimationFailed)?;
        checked_add(&mut total, counter.count(&schema)?)?;
    }
    if let ToolChoice::Named { name } = &request.tool_choice {
        checked_add(&mut total, counter.count(name)?)?;
    }
    TokenCount::new(total).map_err(|_| UsageResolutionError::Overflow)
}

fn add_input_block(
    total: &mut i64,
    block: &ContentBlock,
    counter: TokenCounter,
    allow_encrypted_reasoning: bool,
) -> Result<(), UsageResolutionError> {
    match block {
        ContentBlock::Text(text) => checked_add(total, counter.count(text)?),
        ContentBlock::Image { .. } | ContentBlock::Audio { .. } => {
            Err(UsageResolutionError::UnsupportedContent)
        }
        ContentBlock::ToolUse {
            id,
            name,
            input,
            signature,
        } => {
            if signature.is_some() {
                return Err(UsageResolutionError::UnsupportedContent);
            }
            checked_add(total, TOOL_OVERHEAD_TOKENS)?;
            checked_add(total, counter.count(id)?)?;
            checked_add(total, counter.count(name)?)?;
            let arguments =
                serde_json::to_string(input).map_err(|_| UsageResolutionError::EstimationFailed)?;
            checked_add(total, counter.count(&arguments)?)
        }
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            structured_content,
            is_error: _,
        } => {
            if structured_content.is_some() {
                return Err(UsageResolutionError::UnsupportedContent);
            }
            checked_add(total, NAMED_FIELD_OVERHEAD_TOKENS)?;
            checked_add(total, counter.count(tool_use_id)?)?;
            for nested in content {
                add_input_block(total, nested, counter, allow_encrypted_reasoning)?;
            }
            Ok(())
        }
        ContentBlock::Thinking {
            text,
            signature: Some(signature),
        } if allow_encrypted_reasoning && !signature.is_empty() => {
            // 加密推理内容是不透明字节串，按一字节一 Token 预扣可避免 tokenizer 低估。
            let signature_tokens =
                i64::try_from(signature.len()).map_err(|_| UsageResolutionError::Overflow)?;
            checked_add(total, ENCRYPTED_REASONING_ITEM_OVERHEAD_TOKENS)?;
            checked_add(total, signature_tokens)?;
            checked_add(total, counter.count(text)?)
        }
        ContentBlock::Thinking { .. }
        | ContentBlock::CacheControl(_)
        | ContentBlock::Compaction(_) => Err(UsageResolutionError::UnsupportedContent),
    }
}

fn estimate_usage(
    input_tokens: Result<TokenCount, UsageResolutionError>,
    output: &OutputAccumulator,
    counter: TokenCounter,
) -> Result<Usage, UsageResolutionError> {
    let input_tokens = input_tokens?;
    let (output_tokens, reasoning_tokens) = output.estimate(counter)?;
    Usage::new(
        input_tokens,
        output_tokens,
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            reasoning_tokens,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Estimated,
        UsageSemantics::Inclusive,
    )
    .map_err(|_| UsageResolutionError::Overflow)
}

#[derive(Default)]
struct OutputAccumulator {
    choices: BTreeMap<u32, OutputChoice>,
    bytes: usize,
    error: Option<UsageResolutionError>,
}

impl OutputAccumulator {
    fn start_choice(&mut self, choice_index: u32) {
        if self
            .choices
            .insert(choice_index, OutputChoice::default())
            .is_some()
        {
            self.fail(UsageResolutionError::InvalidSequence);
        }
    }

    fn push_content(&mut self, choice_index: u32, delta: &ContentDelta) {
        let ContentDelta::Text(text) = delta else {
            self.fail(UsageResolutionError::UnsupportedContent);
            return;
        };
        self.push_text(choice_index, text);
    }

    fn push_text(&mut self, choice_index: u32, text: &str) {
        if self.reserve(text.len()).is_err() {
            return;
        }
        let Some(choice) = self.choices.get_mut(&choice_index) else {
            self.fail(UsageResolutionError::InvalidSequence);
            return;
        };
        choice.text.push_str(text);
    }

    fn push_reasoning(&mut self, choice_index: u32, text: &str) {
        if self.reserve(text.len()).is_err() {
            return;
        }
        let Some(choice) = self.choices.get_mut(&choice_index) else {
            self.fail(UsageResolutionError::InvalidSequence);
            return;
        };
        choice.reasoning.push_str(text);
    }

    fn start_tool(&mut self, choice_index: u32, tool_index: u32, id: &str, name: &str) {
        if self.reserve(id.len().saturating_add(name.len())).is_err() {
            return;
        }
        let Some(choice) = self.choices.get_mut(&choice_index) else {
            self.fail(UsageResolutionError::InvalidSequence);
            return;
        };
        if choice
            .tools
            .insert(
                tool_index,
                OutputTool {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    arguments: String::new(),
                },
            )
            .is_some()
        {
            self.fail(UsageResolutionError::InvalidSequence);
        }
    }

    fn push_tool_arguments(&mut self, choice_index: u32, tool_index: u32, partial_json: &str) {
        if self.reserve(partial_json.len()).is_err() {
            return;
        }
        let Some(tool) = self
            .choices
            .get_mut(&choice_index)
            .and_then(|choice| choice.tools.get_mut(&tool_index))
        else {
            self.fail(UsageResolutionError::InvalidSequence);
            return;
        };
        tool.arguments.push_str(partial_json);
    }

    fn push_response_message(&mut self, choice_index: u32, message: &Message) {
        for block in &message.content {
            match block {
                ContentBlock::Text(text) => {
                    self.push_text(choice_index, text);
                }
                ContentBlock::Thinking { text, .. } => {
                    self.push_reasoning(choice_index, text);
                }
                ContentBlock::ToolUse {
                    id,
                    name,
                    input,
                    signature,
                } => {
                    if signature.is_some() {
                        self.fail(UsageResolutionError::UnsupportedContent);
                        continue;
                    }
                    let tool_index = self
                        .choices
                        .get(&choice_index)
                        .and_then(|choice| u32::try_from(choice.tools.len()).ok());
                    let Some(tool_index) = tool_index else {
                        self.fail(UsageResolutionError::Overflow);
                        continue;
                    };
                    self.start_tool(choice_index, tool_index, id, name);
                    let arguments = match serde_json::to_string(input) {
                        Ok(arguments) => arguments,
                        Err(_) => {
                            self.fail(UsageResolutionError::EstimationFailed);
                            continue;
                        }
                    };
                    self.push_tool_arguments(choice_index, tool_index, &arguments);
                }
                ContentBlock::Image { .. }
                | ContentBlock::Audio { .. }
                | ContentBlock::ToolResult { .. }
                | ContentBlock::CacheControl(_)
                | ContentBlock::Compaction(_) => {
                    self.fail(UsageResolutionError::UnsupportedContent);
                }
            }
        }
    }

    fn reserve(&mut self, added: usize) -> Result<(), UsageResolutionError> {
        let next = self
            .bytes
            .checked_add(added)
            .ok_or(UsageResolutionError::Overflow)?;
        if next > MAX_ESTIMATED_OUTPUT_BYTES {
            self.fail(UsageResolutionError::Overflow);
            return Err(UsageResolutionError::Overflow);
        }
        self.bytes = next;
        Ok(())
    }

    fn fail(&mut self, error: UsageResolutionError) {
        self.error.get_or_insert(error);
    }

    fn estimate(
        &self,
        counter: TokenCounter,
    ) -> Result<(TokenCount, TokenCount), UsageResolutionError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let mut output = 0_i64;
        let mut reasoning = 0_i64;
        for choice in self.choices.values() {
            checked_add(&mut output, counter.count(&choice.text)?)?;
            let choice_reasoning = counter.count(&choice.reasoning)?;
            checked_add(&mut reasoning, choice_reasoning)?;
            checked_add(&mut output, choice_reasoning)?;
            for tool in choice.tools.values() {
                checked_add(&mut output, TOOL_OVERHEAD_TOKENS)?;
                checked_add(&mut output, counter.count(&tool.id)?)?;
                checked_add(&mut output, counter.count(&tool.name)?)?;
                checked_add(&mut output, counter.count(&tool.arguments)?)?;
            }
        }
        Ok((
            TokenCount::new(output).map_err(|_| UsageResolutionError::Overflow)?,
            TokenCount::new(reasoning).map_err(|_| UsageResolutionError::Overflow)?,
        ))
    }
}

#[derive(Default)]
struct OutputChoice {
    text: String,
    reasoning: String,
    tools: BTreeMap<u32, OutputTool>,
}

struct OutputTool {
    id: String,
    name: String,
    arguments: String,
}

fn checked_add(total: &mut i64, value: i64) -> Result<(), UsageResolutionError> {
    *total = total
        .checked_add(value)
        .ok_or(UsageResolutionError::Overflow)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use af_domain::{Operation, Role};
    use af_protocol::{FinishReason, MediaSource, ResponseChoice, StreamOptions, openai_responses};
    use serde_json::json;

    use super::*;

    fn text_request(model: &str) -> CanonicalRequest {
        CanonicalRequest::new(
            Operation::Chat,
            model.to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("hello usage".to_owned())],
            )],
            true,
        )
    }

    fn upstream_usage() -> Usage {
        Usage::new(
            TokenCount::new(11).unwrap(),
            TokenCount::new(7).unwrap(),
            UsageDetails::new(
                TokenCount::new(2).unwrap(),
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::new(3).unwrap(),
                TokenCount::ZERO,
                TokenCount::ZERO,
            ),
            UsageSource::Upstream,
            UsageSemantics::Inclusive,
        )
        .unwrap()
    }

    #[test]
    fn upstream_usage_wins_over_local_estimate() {
        let mut estimator = OpenAiChatUsageEstimator::new(&text_request("gpt-4o"));
        estimator.observe(&CanonicalStreamEvent::Usage(upstream_usage()));

        assert_eq!(estimator.resolve(), Ok(upstream_usage()));
    }

    #[test]
    fn request_upper_bound_uses_protocol_cap_when_output_is_unspecified() {
        let request = text_request("gpt-4o");
        let upper_bound = estimate_openai_chat_request_upper_bound(&request).unwrap();

        assert_eq!(upper_bound.input_tokens().get(), 9);
        assert_eq!(upper_bound.output_tokens().get(), MAX_OUTPUT_TOKENS);
        assert_eq!(upper_bound.source(), UsageSource::Estimated);
    }

    #[test]
    fn request_upper_bound_preserves_explicit_output_cap() {
        let mut request = text_request("gpt-4o");
        request.sampling =
            af_protocol::Sampling::new(None, None, Some(TokenCount::new(321).unwrap()), Vec::new())
                .unwrap();

        let upper_bound = estimate_openai_chat_request_upper_bound(&request).unwrap();
        assert_eq!(upper_bound.output_tokens().get(), 321);
    }

    #[test]
    fn text_fixture_pins_chat_overhead_and_tokenizer_result() {
        let mut estimator = OpenAiChatUsageEstimator::new(&text_request("gpt-4o"));
        estimator.observe(&CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        });
        estimator.observe(&CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("answer".to_owned()),
        });

        let usage = estimator.resolve().unwrap();
        assert_eq!(usage.input_tokens().get(), 9);
        assert_eq!(usage.output_tokens().get(), 1);
    }

    #[test]
    fn plain_text_estimator_uses_model_tokenizer_without_chat_overhead() {
        let tokens = estimate_openai_text_tokens("gpt-4o-mini-tts", ["hello", "calm"]).unwrap();
        assert_eq!(tokens.get(), 3);
    }

    #[test]
    fn text_reasoning_and_tools_produce_estimated_usage() {
        let mut request = text_request("gpt-4o");
        request.tools.push(af_protocol::ToolDef {
            name: "lookup".to_owned(),
            description: Some("look up a record".to_owned()),
            input_schema: json!({"type":"object","properties":{"id":{"type":"integer"}}}),
            strict: None,
        });
        request.stream_options = StreamOptions::new(true);
        let mut estimator = OpenAiChatUsageEstimator::new(&request);
        for event in [
            CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("answer".to_owned()),
            },
            CanonicalStreamEvent::ReasoningDelta {
                choice_index: 0,
                content_index: 1,
                text: "reason".to_owned(),
                signature: None,
            },
            CanonicalStreamEvent::ToolCallStart {
                choice_index: 0,
                tool_index: 0,
                id: "call-1".to_owned(),
                name: "lookup".to_owned(),
            },
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index: 0,
                tool_index: 0,
                partial_json: "{\"id\":1}".to_owned(),
            },
            CanonicalStreamEvent::ToolCallEnd {
                choice_index: 0,
                tool_index: 0,
            },
            CanonicalStreamEvent::Finish {
                choice_index: 0,
                reason: FinishReason::ToolCalls,
                stop_sequence: None,
            },
        ] {
            estimator.observe(&event);
        }

        let usage = estimator.resolve().unwrap();
        assert_eq!(usage.source(), UsageSource::Estimated);
        assert!(usage.input_tokens() > TokenCount::ZERO);
        assert!(usage.output_tokens() > usage.details().reasoning());
        assert!(usage.details().reasoning() > TokenCount::ZERO);
    }

    #[test]
    fn unknown_model_uses_conservative_supported_encodings() {
        let mut estimator = OpenAiChatUsageEstimator::new(&text_request("compatible-model"));
        estimator.observe(&CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        });
        estimator.observe(&CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("兼容模型输出".to_owned()),
        });

        let usage = estimator.resolve().unwrap();
        assert_eq!(usage.source(), UsageSource::Estimated);
        assert!(usage.checked_total_tokens().unwrap() > TokenCount::ZERO);
    }

    #[test]
    fn unsupported_media_never_fabricates_usage() {
        let request = CanonicalRequest::new(
            Operation::Chat,
            "gpt-4o".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Image {
                    source: MediaSource::Url("https://media.example/image.png".to_owned()),
                    mime_type: Some("image/png".to_owned()),
                }],
            )],
            true,
        );
        assert_eq!(
            estimate_openai_chat_request_upper_bound(&request),
            Err(UsageResolutionError::UnsupportedContent)
        );
    }

    #[test]
    fn responses_encrypted_reasoning_uses_conservative_input_upper_bound() {
        let signature = "encrypted-reasoning-canary";
        let request = openai_responses::parse_request(
            br#"{
                "model":"gpt-5.5",
                "input":[
                    {
                        "type":"reasoning",
                        "encrypted_content":"encrypted-reasoning-canary",
                        "summary":[{"type":"summary_text","text":"brief summary"}]
                    },
                    {"role":"user","content":[{"type":"input_text","text":"continue"}]}
                ],
                "include":["reasoning.encrypted_content"],
                "store":false
            }"#,
        )
        .unwrap();

        let upper_bound = estimate_openai_request_upper_bound(&request).unwrap();
        let encrypted_floor =
            i64::try_from(signature.len()).unwrap() + ENCRYPTED_REASONING_ITEM_OVERHEAD_TOKENS;
        assert!(upper_bound.input_tokens().get() > encrypted_floor);
        assert_eq!(upper_bound.source(), UsageSource::Estimated);
    }

    #[test]
    fn thinking_without_responses_encrypted_contract_remains_unsupported() {
        for (operation, signature) in [
            (Operation::Chat, Some("signed".to_owned())),
            (Operation::Chat, None),
            (Operation::Responses, None),
        ] {
            let request = CanonicalRequest::new(
                operation,
                "gpt-test".to_owned(),
                vec![Message::new(
                    Role::Assistant,
                    vec![ContentBlock::Thinking {
                        text: "reasoning".to_owned(),
                        signature,
                    }],
                )],
                false,
            );
            assert_eq!(
                estimate_openai_request_upper_bound(&request),
                Err(UsageResolutionError::UnsupportedContent)
            );
        }
    }

    #[test]
    fn non_streaming_response_prefers_upstream_and_estimates_when_missing() {
        let response_without_usage = CanonicalResponse::new(
            Operation::Chat,
            "response-id".to_owned(),
            "gpt-4o".to_owned(),
            Some(1),
            vec![ResponseChoice::new(
                0,
                Message::new(
                    Role::Assistant,
                    vec![ContentBlock::Text("answer".to_owned())],
                ),
                FinishReason::Stop,
            )],
            None,
        );
        let estimated = OpenAiChatUsageEstimator::new(&text_request("gpt-4o"))
            .resolve_response(&response_without_usage)
            .unwrap();
        assert_eq!(estimated.source(), UsageSource::Estimated);

        let response_with_usage = CanonicalResponse::new(
            Operation::Chat,
            "response-id".to_owned(),
            "gpt-4o".to_owned(),
            Some(1),
            response_without_usage.choices.clone(),
            Some(upstream_usage()),
        );
        assert_eq!(
            OpenAiChatUsageEstimator::new(&text_request("gpt-4o"))
                .resolve_response(&response_with_usage),
            Ok(upstream_usage())
        );
    }

    #[tokio::test]
    async fn dropped_sender_resolves_as_interrupted_without_payloads() {
        let (sender, handle) = usage_channel();
        drop(sender);

        assert_eq!(
            handle.resolve().await,
            Err(UsageResolutionError::Interrupted)
        );
        assert!(!format!("{:?}", usage_channel().1).contains("usage"));
    }
}
