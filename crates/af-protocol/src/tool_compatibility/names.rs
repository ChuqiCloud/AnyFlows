use std::collections::{BTreeMap, BTreeSet};

use crate::{CanonicalRequestEnvelope, ContentBlock, ToolChoice, ToolDef};

use super::ToolCompatibilityError as Error;

pub(super) struct ToolNames {
    aliases: BTreeMap<String, String>,
    callers: BTreeSet<String>,
    injected: BTreeSet<String>,
    historical_ids: BTreeSet<String>,
}

impl ToolNames {
    pub(super) fn prepare(
        request: CanonicalRequestEnvelope,
        injected: Vec<ToolDef>,
    ) -> Result<(CanonicalRequestEnvelope, Self), Error> {
        if injected.is_empty() || injected.len() > 32 || request.canonical().tools.len() > 96 {
            return Err(Error::InvalidToolSet);
        }
        let mut injected_names = BTreeSet::new();
        for tool in &injected {
            if !valid_name(&tool.name)
                || !tool.input_schema.is_object()
                || !injected_names.insert(tool.name.clone())
            {
                return Err(Error::InvalidToolSet);
            }
        }
        let mut callers = BTreeSet::new();
        for tool in &request.canonical().tools {
            if !valid_name(&tool.name) || !callers.insert(tool.name.clone()) {
                return Err(Error::InvalidToolSet);
            }
        }
        match &request.canonical().tool_choice {
            ToolChoice::Named { name } if !callers.contains(name) => {
                return Err(Error::UnsupportedRequest);
            }
            ToolChoice::Required if callers.is_empty() => return Err(Error::UnsupportedRequest),
            _ => {}
        }
        let mut occupied = callers.clone();
        let mut historical_ids = BTreeSet::new();
        for message in &request.canonical().messages {
            visit(&message.content, 0, &mut |block| {
                if let ContentBlock::ToolUse { id, name, .. } = block {
                    occupied.insert(name.clone());
                    historical_ids.insert(id.clone());
                }
                Ok(())
            })?;
        }
        // 历史工具即使已从当前目录移除，也不能被重新解释为注入工具。
        let collisions: Vec<_> = occupied.intersection(&injected_names).cloned().collect();
        occupied.extend(injected_names.iter().cloned());
        let mut aliases = BTreeMap::new();
        let mut forward = BTreeMap::new();
        let mut ordinal = 0_u32;
        for original in collisions {
            let alias = loop {
                let candidate = format!("af_client_{ordinal}");
                ordinal = ordinal.checked_add(1).ok_or(Error::InvalidToolSet)?;
                if occupied.insert(candidate.clone()) {
                    break candidate;
                }
            };
            aliases.insert(alias.clone(), original.clone());
            forward.insert(original, alias);
        }
        let mut canonical = request.canonical().clone();
        if canonical.tools.is_empty()
            && canonical.tool_choice == ToolChoice::None
            && request.source_context().is_some_and(|(protocol, body)| {
                serde_json::from_slice::<serde_json::Value>(body).is_ok_and(|body| {
                    let choice = if protocol == af_domain::Protocol::Gemini {
                        body.get("toolConfig")
                            .and_then(|config| config.get("functionCallingConfig"))
                            .and_then(|config| config.get("mode"))
                    } else {
                        body.get("tool_choice")
                            .or_else(|| body.get("function_call"))
                    };
                    choice.is_none_or(serde_json::Value::is_null)
                })
            })
        {
            // 协议解析器用 None 表示空目录缺省；工具注入后应恢复缺省 Auto。
            // 调用方明确声明的 none 仍然禁止任何工具替代。
            canonical.tool_choice = ToolChoice::Auto;
        }
        for message in &mut canonical.messages {
            rewrite(&mut message.content, 0, &forward)?;
        }
        for tool in &mut canonical.tools {
            if let Some(alias) = forward.get(&tool.name) {
                tool.name.clone_from(alias);
            }
        }
        if let ToolChoice::Named { name } = &mut canonical.tool_choice
            && let Some(alias) = forward.get(name)
        {
            name.clone_from(alias);
        }
        let callers = canonical
            .tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect();
        canonical.tools.extend(injected);
        Ok((
            request.with_rebuilt_canonical(canonical),
            Self {
                aliases,
                callers,
                injected: injected_names,
                historical_ids,
            },
        ))
    }

    pub(super) fn is_injected(&self, name: &str) -> bool {
        self.injected.contains(name)
    }

    pub(super) fn is_historical_id(&self, id: &str) -> bool {
        self.historical_ids.contains(id)
    }

    pub(super) fn remember_id(&mut self, id: String) {
        self.historical_ids.insert(id);
    }

    pub(super) fn caller_name<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        self.callers
            .contains(name)
            .then(|| self.aliases.get(name).map_or(name, String::as_str))
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn visit(
    blocks: &[ContentBlock],
    depth: u8,
    visitor: &mut impl FnMut(&ContentBlock) -> Result<(), Error>,
) -> Result<(), Error> {
    if depth > 16 {
        return Err(Error::UnsupportedRequest);
    }
    for block in blocks {
        visitor(block)?;
        if let ContentBlock::ToolResult { content, .. } = block {
            visit(content, depth + 1, visitor)?;
        }
    }
    Ok(())
}

fn rewrite(
    blocks: &mut [ContentBlock],
    depth: u8,
    aliases: &BTreeMap<String, String>,
) -> Result<(), Error> {
    if depth > 16 {
        return Err(Error::UnsupportedRequest);
    }
    for block in blocks {
        match block {
            ContentBlock::ToolUse {
                name, signature, ..
            } => {
                if let Some(alias) = aliases.get(name) {
                    // 签名可能绑定工具名称；不修改后假装签名仍然有效。
                    if signature.is_some() {
                        return Err(Error::UnsupportedRequest);
                    }
                    name.clone_from(alias);
                }
            }
            ContentBlock::ToolResult { content, .. } => rewrite(content, depth + 1, aliases)?,
            _ => {}
        }
    }
    Ok(())
}
