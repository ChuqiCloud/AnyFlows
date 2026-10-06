use std::{error::Error, fmt};

use af_domain::{ClientSimulationBodyProfile, Operation, Role};

use crate::{CanonicalRequestEnvelope, ContentBlock, Message};

/// 受信 UTC 时钟冻结后的公历日期；不接受时区、时间或用户提供的文本。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UtcDate {
    year: u16,
    month: u8,
    day: u8,
}

impl UtcDate {
    /// 创建经过范围校验的 UTC 公历日期。
    pub const fn new(
        year: u16,
        month: u8,
        day: u8,
    ) -> Result<Self, ClientSimulationBodyPatchError> {
        if year == 0 || month == 0 || month > 12 || day == 0 || day > days_in_month(year, month) {
            return Err(ClientSimulationBodyPatchError::InvalidUtcDate);
        }
        Ok(Self { year, month, day })
    }

    /// 返回日期所属年份。
    #[must_use]
    pub const fn year(self) -> u16 {
        self.year
    }

    /// 返回日期所属月份。
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    /// 返回日期所属日。
    #[must_use]
    pub const fn day(self) -> u8 {
        self.day
    }

    fn instruction(self) -> String {
        format!(
            "Current date: {:04}-{:02}-{:02}.",
            self.year, self.month, self.day
        )
    }
}

/// 版本化正文档案的纯 Canonical 补丁；不接触 Header、URL、凭据或计费对象。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientSimulationBodyPatch {
    profile: ClientSimulationBodyProfile,
}

impl ClientSimulationBodyPatch {
    /// 创建一个闭合正文档案补丁。
    #[must_use]
    pub const fn new(profile: ClientSimulationBodyProfile) -> Self {
        Self { profile }
    }

    /// 返回本补丁的版本化档案标识。
    #[must_use]
    pub const fn profile(self) -> ClientSimulationBodyProfile {
        self.profile
    }

    /// 在允许的 Anthropic system 形状上应用一次冻结日期，并永久丢弃原始正文资格。
    pub fn apply(
        self,
        request: CanonicalRequestEnvelope,
        date: UtcDate,
    ) -> Result<CanonicalRequestEnvelope, ClientSimulationBodyPatchError> {
        if request.canonical().operation != Operation::Chat
            || !request.canonical().continuation.is_empty()
            || request.canonical().raw_passthrough().is_some()
        {
            return Err(ClientSimulationBodyPatchError::UnsupportedRequestShape);
        }

        let messages = rewrite_messages(&request.canonical().messages, date)?;
        let canonical = request
            .canonical()
            .clone()
            .with_rewritten_messages(messages);
        Ok(request.with_rebuilt_canonical(canonical))
    }
}

/// 受控正文补丁的稳定失败分类；不包含日期、提示词或渠道标识。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientSimulationBodyPatchError {
    /// 受信时钟未能提供有效的 UTC 日期。
    InvalidUtcDate,
    /// 请求不是可精确重建的 Anthropic Chat 形状。
    UnsupportedRequestShape,
}

impl fmt::Display for ClientSimulationBodyPatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtcDate => formatter.write_str("UTC 日期无效"),
            Self::UnsupportedRequestShape => formatter.write_str("客户端仿真正文请求形状不受支持"),
        }
    }
}

impl Error for ClientSimulationBodyPatchError {}

fn rewrite_messages(
    messages: &[Message],
    date: UtcDate,
) -> Result<Vec<Message>, ClientSimulationBodyPatchError> {
    if messages
        .iter()
        .skip(1)
        .any(|message| message.role == Role::System)
        || messages
            .iter()
            .any(|message| message.role == Role::Developer)
    {
        return Err(ClientSimulationBodyPatchError::UnsupportedRequestShape);
    }

    let date_message = Message::new(Role::System, vec![ContentBlock::Text(date.instruction())]);
    match messages.first() {
        None => Ok(vec![date_message]),
        Some(Message {
            role: Role::System,
            content,
        }) if matches!(content.as_slice(), [ContentBlock::Text(_)]) => {
            let mut rewritten = Vec::with_capacity(messages.len() + 1);
            rewritten.push(date_message);
            rewritten.extend_from_slice(messages);
            Ok(rewritten)
        }
        Some(Message {
            role: Role::System, ..
        }) => Err(ClientSimulationBodyPatchError::UnsupportedRequestShape),
        Some(_) => {
            let mut rewritten = Vec::with_capacity(messages.len() + 1);
            rewritten.push(date_message);
            rewritten.extend_from_slice(messages);
            Ok(rewritten)
        }
    }
}

const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use af_domain::{Operation, Protocol, Role};
    use bytes::Bytes;

    use super::*;
    use crate::{CanonicalRequest, SameProtocolDecision, anthropic};

    fn patch() -> ClientSimulationBodyPatch {
        ClientSimulationBodyPatch::new(ClientSimulationBodyProfile::AnthropicCliSystemDateV1)
    }

    #[test]
    fn inserts_frozen_date_before_single_text_system_instruction() {
        let request = anthropic::parse_request_envelope(Bytes::from_static(
            br#"{"model":"claude-test","system":"private-system","messages":[{"role":"user","content":"hello"}],"max_tokens":32}"#,
        ))
        .unwrap();

        let patched = patch()
            .apply(request, UtcDate::new(2026, 9, 3).unwrap())
            .unwrap();
        assert_eq!(patched.source_protocol(), None);
        let messages = &patched.canonical().messages;
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(
            messages[0].content,
            vec![ContentBlock::Text("Current date: 2026-09-03.".to_owned())]
        );
        assert_eq!(messages[1].role, Role::System);
        assert_eq!(messages[2].role, Role::User);
        assert!(matches!(
            patched.into_same_protocol(Protocol::Anthropic),
            SameProtocolDecision::Rebuild(_)
        ));
    }

    #[test]
    fn adds_system_date_when_no_system_instruction_exists() {
        let request = CanonicalRequest::new(
            Operation::Chat,
            "claude-test".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("hello".to_owned())],
            )],
            false,
        )
        .into();

        let patched = patch()
            .apply(request, UtcDate::new(2028, 2, 29).unwrap())
            .unwrap();
        assert_eq!(patched.canonical().messages.len(), 2);
        assert_eq!(patched.canonical().messages[0].role, Role::System);
    }

    #[test]
    fn rejects_unsafe_system_shapes_and_continuation() {
        let multiple_system = CanonicalRequest::new(
            Operation::Chat,
            "claude-test".to_owned(),
            vec![
                Message::new(Role::System, vec![ContentBlock::Text("one".to_owned())]),
                Message::new(Role::System, vec![ContentBlock::Text("two".to_owned())]),
            ],
            false,
        )
        .into();
        assert_eq!(
            patch().apply(multiple_system, UtcDate::new(2026, 1, 1).unwrap()),
            Err(ClientSimulationBodyPatchError::UnsupportedRequestShape)
        );

        let mut continuation =
            CanonicalRequest::new(Operation::Chat, "claude-test".to_owned(), Vec::new(), false);
        continuation.continuation =
            crate::RequestContinuation::new(Some("private".to_owned()), None, None);
        assert_eq!(
            patch().apply(continuation.into(), UtcDate::new(2026, 1, 1).unwrap()),
            Err(ClientSimulationBodyPatchError::UnsupportedRequestShape)
        );
    }

    #[test]
    fn utc_date_rejects_invalid_calendar_values() {
        for invalid in [(0, 1, 1), (2026, 0, 1), (2026, 2, 29), (2026, 13, 1)] {
            assert_eq!(
                UtcDate::new(invalid.0, invalid.1, invalid.2),
                Err(ClientSimulationBodyPatchError::InvalidUtcDate)
            );
        }
    }
}
