//! 为目标 wire 整理已校验内容；工具执行结果与可见文本均须完整保留。

use af_domain::{Protocol, Role};
use serde_json::{Value, json};

use crate::{CanonicalRequest, ContentBlock};

#[cfg(test)]
#[path = "conversation_content_tests.rs"]
mod tests;

/// 在签名来源处理之后、工具轮次关联之前调用。
///
/// Chat 和 Responses 的 wire 将推理、文本和工具调用分开表示，按类别稳定归并
/// 可避免来源消息中合法交错的块触发目标构造器的顺序限制。其他目标保留原顺序。
pub(crate) fn normalize_content(
    request: &mut CanonicalRequest,
    target: Protocol,
) -> Result<(), ()> {
    if !matches!(
        target,
        Protocol::OpenAiChat | Protocol::OpenAiResponses | Protocol::Anthropic | Protocol::Gemini
    ) {
        return Err(());
    }
    for message in &mut request.messages {
        if message.role == Role::Assistant
            && matches!(target, Protocol::OpenAiChat | Protocol::OpenAiResponses)
        {
            normalize_assistant(&mut message.content, target)?;
        }
        if target == Protocol::Gemini {
            normalize_gemini_results(&mut message.content)?;
        }
    }
    Ok(())
}

fn normalize_assistant(blocks: &mut Vec<ContentBlock>, target: Protocol) -> Result<(), ()> {
    // 供应商签名由来源证明处理，本模块不能把签名转成可见推理或合并不同签名。
    if target == Protocol::OpenAiChat
        && blocks.iter().any(|block| {
            matches!(
                block,
                ContentBlock::Thinking {
                    signature: Some(_),
                    ..
                }
            )
        })
    {
        return Err(());
    }
    if blocks
        .iter()
        .any(|block| matches!(block, ContentBlock::Compaction(_)))
    {
        return if blocks.len() == 1 && target == Protocol::OpenAiResponses {
            Ok(())
        } else {
            Err(())
        };
    }
    let mut reasoning = Vec::new();
    let mut content = Vec::new();
    let mut calls = Vec::new();
    for block in std::mem::take(blocks) {
        match block {
            ContentBlock::Thinking { .. } => reasoning.push(block),
            ContentBlock::ToolUse { .. } => calls.push(block),
            _ => content.push(block),
        }
    }
    if target == Protocol::OpenAiChat && reasoning.len() > 1 {
        let mut merged = String::new();
        for block in reasoning {
            let ContentBlock::Thinking { text, .. } = block else {
                unreachable!("推理分组仅包含推理块");
            };
            if !merged.is_empty() && !text.is_empty() {
                merged.push('\n');
            }
            merged.push_str(&text);
        }
        reasoning = vec![ContentBlock::Thinking {
            text: merged,
            signature: None,
        }];
    }
    reasoning.append(&mut content);
    reasoning.append(&mut calls);
    *blocks = reasoning;
    Ok(())
}

fn normalize_gemini_results(blocks: &mut [ContentBlock]) -> Result<(), ()> {
    for block in blocks {
        let ContentBlock::ToolResult {
            content,
            structured_content,
            is_error,
            ..
        } = block
        else {
            continue;
        };
        let native_object = structured_content.as_ref().and_then(Value::as_object);
        if content.is_empty()
            && native_object.is_some_and(|object| object.contains_key("error") == *is_error)
        {
            continue;
        }
        if structured_content.is_none()
            && (matches!(content.as_slice(), [ContentBlock::Text(_)])
                || content.is_empty() && !*is_error)
        {
            continue;
        }
        let mut texts = Vec::with_capacity(content.len());
        for part in content.iter() {
            let ContentBlock::Text(text) = part else {
                // 未实现的媒体结果不能降成说明文字冒充已处理的附件。
                return Err(());
            };
            texts.push(text.clone());
        }
        let mut payload = serde_json::Map::new();
        if let Some(value) = structured_content.take() {
            payload.insert("structured_content".to_owned(), value);
        }
        if !texts.is_empty() {
            payload.insert("content".to_owned(), json!(texts));
        }
        let key = if *is_error { "error" } else { "result" };
        *structured_content = Some(json!({key: payload}));
        content.clear();
    }
    Ok(())
}
