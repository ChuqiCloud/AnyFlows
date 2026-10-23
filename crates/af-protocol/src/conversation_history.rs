//! 将代理回传的工具历史整理为四种对话协议都能表达的调用/结果轮次。

use std::collections::{HashMap, HashSet};

use af_domain::{Protocol, Role};

use crate::{CanonicalRequest, ContentBlock, Message};

#[cfg(test)]
#[path = "conversation_history_tests.rs"]
mod tests;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct Position {
    message: usize,
    block: usize,
}

struct Call {
    position: Position,
    id: String,
    result: Option<usize>,
}

struct ResultEntry {
    position: Position,
    id: String,
    call: Option<usize>,
}

/// 关联显式工具 ID、补齐可唯一恢复的 ID，并将结果放回对应调用之后。
///
/// 无法确定关联的历史保留为明确的普通内容，不补造工具执行或成功结果。
/// 原生 Responses 远程会话可引用本地历史中不存在的调用。来源供应商签名
/// 仍由信封的来源上下文保留，本函数不会将不透明签名写入可见历史文本。
pub(crate) fn normalize_history(
    request: &mut CanonicalRequest,
    target: Protocol,
) -> Result<(), ()> {
    if !matches!(
        target,
        Protocol::OpenAiChat | Protocol::OpenAiResponses | Protocol::Anthropic | Protocol::Gemini
    ) {
        return Err(());
    }
    let remote_results =
        target == Protocol::OpenAiResponses && request.continuation.has_server_managed_state();
    let (mut calls, mut results) = index_history(&request.messages);
    associate_results(&mut calls, &mut results);
    let normalized_ids = assign_call_ids(&calls, &results);
    let call_positions: HashMap<_, _> = calls
        .iter()
        .enumerate()
        .map(|(index, call)| (call.position, index))
        .collect();
    let result_positions: HashMap<_, _> = results
        .iter()
        .enumerate()
        .map(|(index, result)| (result.position, index))
        .collect();
    let mut output = Vec::with_capacity(request.messages.len());
    let mut remote_ids = HashSet::new();
    let local_ids: HashSet<_> = calls.iter().map(|call| call.id.as_str()).collect();
    for (message_index, message) in request.messages.iter().enumerate() {
        let role = if message.role == Role::Tool {
            Role::User
        } else {
            message.role
        };
        let mut blocks = Vec::new();
        let mut pending = Vec::new();
        let mut previous_was_result = false;
        for (block_index, block) in message.content.iter().enumerate() {
            let position = Position {
                message: message_index,
                block: block_index,
            };
            if matches!(block, ContentBlock::CacheControl(_)) {
                if previous_was_result {
                    continue;
                }
            } else {
                previous_was_result = matches!(block, ContentBlock::ToolResult { .. });
            }
            if let Some(&result_index) = result_positions.get(&position) {
                let result = &results[result_index];
                if result.call.is_some() {
                    continue;
                }
                flush_blocks(
                    &mut output,
                    role,
                    &mut blocks,
                    &mut pending,
                    &calls,
                    &results,
                    &normalized_ids,
                    &request.messages,
                );
                if remote_results
                    && valid_id(&result.id)
                    && !local_ids.contains(result.id.as_str())
                    && remote_ids.insert(result.id.clone())
                {
                    output.push(Message::new(
                        Role::Tool,
                        result_blocks(result, &request.messages),
                    ));
                } else {
                    output.push(Message::new(Role::User, historical_result(block)?));
                    append_result_cache_hints(&mut output, result, &request.messages);
                }
                continue;
            }
            if let Some(&call_index) = call_positions.get(&position)
                && calls[call_index].result.is_some()
            {
                let mut block = block.clone();
                let ContentBlock::ToolUse { id, .. } = &mut block else {
                    unreachable!("工具索引只包含调用块");
                };
                *id = normalized_ids[call_index].clone();
                blocks.push(block);
                pending.push(call_index);
                continue;
            }
            if !pending.is_empty() && !matches!(block, ContentBlock::CacheControl(_)) {
                flush_blocks(
                    &mut output,
                    role,
                    &mut blocks,
                    &mut pending,
                    &calls,
                    &results,
                    &normalized_ids,
                    &request.messages,
                );
            }
            blocks.push(match block {
                ContentBlock::ToolUse { .. } => historical_call(block)?,
                _ => block.clone(),
            });
        }
        flush_blocks(
            &mut output,
            role,
            &mut blocks,
            &mut pending,
            &calls,
            &results,
            &normalized_ids,
            &request.messages,
        );
    }
    if target == Protocol::Anthropic {
        output = merge_adjacent_roles(output);
    }
    request.messages = output;
    Ok(())
}

fn index_history(messages: &[Message]) -> (Vec<Call>, Vec<ResultEntry>) {
    let mut calls = Vec::new();
    let mut results = Vec::new();
    for (message, value) in messages.iter().enumerate() {
        for (block, content) in value.content.iter().enumerate() {
            let position = Position { message, block };
            match content {
                ContentBlock::ToolUse { id, .. } if value.role == Role::Assistant => {
                    calls.push(Call {
                        position,
                        id: id.clone(),
                        result: None,
                    });
                }
                ContentBlock::ToolResult { tool_use_id, .. } => {
                    results.push(ResultEntry {
                        position,
                        id: tool_use_id.clone(),
                        call: None,
                    });
                }
                _ => {}
            }
        }
    }
    (calls, results)
}

fn associate_results(calls: &mut [Call], results: &mut [ResultEntry]) {
    let mut explicit_calls: HashMap<String, (usize, usize)> = HashMap::new();
    let mut explicit_results: HashMap<String, usize> = HashMap::new();
    for (index, call) in calls.iter().enumerate() {
        if !call.id.is_empty() {
            let entry = explicit_calls.entry(call.id.clone()).or_insert((0, index));
            entry.0 += 1;
        }
    }
    for result in results.iter() {
        if !result.id.is_empty() {
            *explicit_results.entry(result.id.clone()).or_default() += 1;
        }
    }
    for (result_index, result) in results.iter_mut().enumerate() {
        if let Some(&(1, call_index)) = explicit_calls.get(&result.id)
            && explicit_results.get(&result.id) == Some(&1)
        {
            calls[call_index].result = Some(result_index);
            result.call = Some(call_index);
        }
    }

    // 每个缺失 ID 只能在最近的调用消息内恢复。按位置双向扫描候选，
    // 避免长工具历史为每个结果反复扫描整个历史。
    let mut groups: HashMap<usize, (Vec<usize>, Vec<usize>)> = HashMap::new();
    for (call_index, call) in calls.iter().enumerate() {
        if call.result.is_none() {
            groups
                .entry(call.position.message)
                .or_default()
                .0
                .push(call_index);
        }
    }
    let mut preceding_calls = 0;
    for (result_index, result) in results.iter().enumerate() {
        while preceding_calls < calls.len()
            && comes_before(calls[preceding_calls].position, result.position)
        {
            preceding_calls += 1;
        }
        if result.call.is_some() || preceding_calls == 0 {
            continue;
        }
        if let Some(group) = groups.get_mut(&calls[preceding_calls - 1].position.message) {
            group.1.push(result_index);
        }
    }
    for (call_indices, result_indices) in groups.values() {
        let mut candidates = Vec::with_capacity(result_indices.len());
        let mut call_cursor = 0;
        let mut empty_calls = 0;
        let mut last_empty_call = None;
        for &result_index in result_indices {
            let result = &results[result_index];
            while call_cursor < call_indices.len()
                && comes_before(calls[call_indices[call_cursor]].position, result.position)
            {
                let call_index = call_indices[call_cursor];
                if calls[call_index].id.is_empty() {
                    empty_calls += 1;
                    last_empty_call = Some(call_index);
                }
                call_cursor += 1;
            }
            candidates.push(if result.id.is_empty() && call_cursor == 1 {
                Some(call_indices[0])
            } else if !result.id.is_empty() && empty_calls == 1 {
                last_empty_call
            } else {
                None
            });
        }
        let mut eligible_results = HashMap::new();
        let mut result_cursor = result_indices.len();
        let mut total_results = 0;
        let mut empty_results = 0;
        for &call_index in call_indices.iter().rev() {
            while result_cursor > 0
                && comes_before(
                    calls[call_index].position,
                    results[result_indices[result_cursor - 1]].position,
                )
            {
                result_cursor -= 1;
                total_results += 1;
                if results[result_indices[result_cursor]].id.is_empty() {
                    empty_results += 1;
                }
            }
            eligible_results.insert(
                call_index,
                if calls[call_index].id.is_empty() {
                    total_results
                } else {
                    empty_results
                },
            );
        }
        for (&result_index, candidate) in result_indices.iter().zip(candidates) {
            if let Some(call_index) = candidate
                && eligible_results.get(&call_index) == Some(&1)
            {
                calls[call_index].result = Some(result_index);
                results[result_index].call = Some(call_index);
            }
        }
    }
}

fn comes_before(left: Position, right: Position) -> bool {
    (left.message, left.block) < (right.message, right.block)
}

fn assign_call_ids(calls: &[Call], results: &[ResultEntry]) -> Vec<String> {
    let reserved: HashSet<_> = calls
        .iter()
        .map(|call| call.id.as_str())
        .chain(results.iter().map(|result| result.id.as_str()))
        .filter(|id| valid_id(id))
        .collect();
    let mut used = HashSet::new();
    let mut next = 1;
    calls
        .iter()
        .map(|call| {
            let Some(result) = call.result.map(|index| &results[index]) else {
                return String::new();
            };
            let explicit = if valid_id(&call.id) {
                Some(&call.id)
            } else if valid_id(&result.id) {
                Some(&result.id)
            } else {
                None
            };
            if let Some(id) = explicit
                && used.insert(id.clone())
            {
                return id.clone();
            }
            loop {
                let id = format!("af_history_call_{next}");
                next += 1;
                if !reserved.contains(id.as_str()) && used.insert(id.clone()) {
                    return id;
                }
            }
        })
        .collect()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 256 && id.trim() == id && !id.chars().any(char::is_control)
}

#[allow(clippy::too_many_arguments)]
fn flush_blocks(
    output: &mut Vec<Message>,
    role: Role,
    blocks: &mut Vec<ContentBlock>,
    pending: &mut Vec<usize>,
    calls: &[Call],
    results: &[ResultEntry],
    ids: &[String],
    messages: &[Message],
) {
    if !blocks.is_empty() {
        output.push(Message::new(role, std::mem::take(blocks)));
    }
    for call_index in pending.drain(..) {
        let result = &results[calls[call_index].result.expect("只有配对调用进入结果轮次")];
        let mut blocks = result_blocks(result, messages);
        let ContentBlock::ToolResult { tool_use_id, .. } = &mut blocks[0] else {
            unreachable!("工具结果索引必须指向结果块");
        };
        *tool_use_id = ids[call_index].clone();
        output.push(Message::new(Role::Tool, blocks));
    }
}

fn result_blocks(result: &ResultEntry, messages: &[Message]) -> Vec<ContentBlock> {
    let message = &messages[result.position.message];
    let mut blocks = vec![message.content[result.position.block].clone()];
    blocks.extend(
        message.content[result.position.block + 1..]
            .iter()
            .take_while(|block| matches!(block, ContentBlock::CacheControl(_)))
            .cloned(),
    );
    blocks
}

fn append_result_cache_hints(output: &mut [Message], result: &ResultEntry, messages: &[Message]) {
    if let Some(last) = output.last_mut() {
        last.content
            .extend(result_blocks(result, messages).into_iter().skip(1));
    }
}

fn historical_call(block: &ContentBlock) -> Result<ContentBlock, ()> {
    let ContentBlock::ToolUse {
        id, name, input, ..
    } = block
    else {
        return Err(());
    };
    let detail = serde_json::json!({
        "call_id": id, "tool": name, "arguments": input,
        "history_state": "no associated result; execution is not confirmed",
    });
    Ok(ContentBlock::Text(format!(
        "[Historical tool call]\n{}",
        serde_json::to_string(&detail).map_err(|_| ())?
    )))
}

fn historical_result(block: &ContentBlock) -> Result<Vec<ContentBlock>, ()> {
    let ContentBlock::ToolResult {
        tool_use_id,
        content,
        structured_content,
        is_error,
    } = block
    else {
        return Err(());
    };
    let status = serde_json::json!({
        "call_id": tool_use_id,
        "history_state": "no associated call; execution is not confirmed",
        "reported_error": is_error,
    });
    let mut blocks = vec![ContentBlock::Text(format!(
        "[Historical tool result]\n{}",
        serde_json::to_string(&status).map_err(|_| ())?
    ))];
    blocks.extend(content.iter().cloned());
    if let Some(value) = structured_content {
        blocks.push(ContentBlock::Text(format!(
            "[Structured tool result]\n{}",
            serde_json::to_string(value).map_err(|_| ())?
        )));
    }
    Ok(blocks)
}

fn merge_adjacent_roles(messages: Vec<Message>) -> Vec<Message> {
    let mut output: Vec<Message> = Vec::with_capacity(messages.len());
    for message in messages {
        if message.role != Role::Tool
            && let Some(previous) = output.last_mut()
            && previous.role == message.role
        {
            previous.content.extend(message.content);
        } else {
            output.push(message);
        }
    }
    output
}
