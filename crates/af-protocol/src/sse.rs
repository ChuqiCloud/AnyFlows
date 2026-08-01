use std::{error::Error, fmt, mem, str};

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// 默认单个 SSE 事件最大 8 MiB，可覆盖大文本和 JSON 转义膨胀。
pub const DEFAULT_MAX_SSE_EVENT_BYTES: usize = 8 * 1024 * 1024;
/// 单个 SSE 事件的协议硬上限，与当前协议正文上限保持一致。
pub const MAX_SSE_EVENT_BYTES: usize = 32 * 1024 * 1024;

/// 增量 SSE framing 错误；不保留原始行、事件名或数据。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SseParseError {
    /// 调用方提供的单事件上限为零或超过协议硬上限。
    InvalidLimit,
    /// 当前 SSE 事件超过配置的字节上限。
    EventTooLarge,
    /// SSE 文本不是合法 UTF-8。
    InvalidUtf8,
    /// 传输结束时仍有未用空行封闭的事件。
    TruncatedEvent,
    /// parser 已因前一次错误进入终止状态。
    ParserFailed,
    /// parser 已完成，不能继续接收字节。
    ParserClosed,
}

impl fmt::Display for SseParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidLimit => "SSE 事件上限无效",
            Self::EventTooLarge => "SSE 事件超过大小限制",
            Self::InvalidUtf8 => "SSE 事件不是有效 UTF-8",
            Self::TruncatedEvent => "SSE 流在事件结束前截断",
            Self::ParserFailed => "SSE parser 已失败",
            Self::ParserClosed => "SSE parser 已结束",
        };
        formatter.write_str(message)
    }
}

impl Error for SseParseError {}

/// 已完成 framing 的 SSE 事件；Debug 只暴露结构，不输出事件名或 data。
pub struct SseEvent {
    event_name: Option<String>,
    data: Vec<u8>,
    comment: bool,
}

impl SseEvent {
    fn message(event_name: Option<String>, data: Vec<u8>) -> Self {
        Self {
            event_name,
            data,
            comment: false,
        }
    }

    fn comment() -> Self {
        Self {
            event_name: None,
            data: Vec::new(),
            comment: true,
        }
    }

    /// 返回显式 `event:` 名称；缺失时按 SSE 默认 `message` 处理。
    #[must_use]
    pub fn event_name(&self) -> Option<&str> {
        self.event_name.as_deref()
    }

    /// 返回多行 `data:` 按 SSE 规则用换行拼接后的字节。
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// 返回该事件是否由仅含注释的心跳帧产生。
    #[must_use]
    pub const fn is_comment(&self) -> bool {
        self.comment
    }
}

impl fmt::Debug for SseEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SseEvent")
            .field("has_event_name", &self.event_name.is_some())
            .field("data_bytes", &self.data.len())
            .field("comment", &self.comment)
            .finish()
    }
}

/// 有界增量 SSE parser，支持 LF、CRLF、CR、跨 chunk 行与多行 data。
///
/// 单次错误后 parser 进入终止状态，调用方必须丢弃，避免继续消费部分事件状态。
pub struct SseParser {
    max_event_bytes: usize,
    frame_bytes: usize,
    line: Vec<u8>,
    data: Vec<u8>,
    event_name: Option<String>,
    saw_data: bool,
    saw_comment: bool,
    skip_lf: bool,
    bom_checked: bool,
    failed: bool,
    closed: bool,
}

impl SseParser {
    /// 使用默认 8 MiB 单事件上限创建 parser。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用调用方收紧后的单事件上限创建 parser。
    pub fn with_max_event_bytes(max_event_bytes: usize) -> Result<Self, SseParseError> {
        if max_event_bytes == 0 || max_event_bytes > MAX_SSE_EVENT_BYTES {
            return Err(SseParseError::InvalidLimit);
        }
        Ok(Self {
            max_event_bytes,
            frame_bytes: 0,
            line: Vec::new(),
            data: Vec::new(),
            event_name: None,
            saw_data: false,
            saw_comment: false,
            skip_lf: false,
            bom_checked: false,
            failed: false,
            closed: false,
        })
    }

    /// 推入任意字节分片，并返回本次新完成的 SSE 事件。
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, SseParseError> {
        if self.failed {
            return Err(SseParseError::ParserFailed);
        }
        if self.closed {
            return Err(SseParseError::ParserClosed);
        }

        let result = self.push_inner(chunk);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    /// 确认传输已结束；只有空闲边界才算完整，未封闭事件按截断处理。
    pub fn finish(&mut self) -> Result<(), SseParseError> {
        if self.failed {
            return Err(SseParseError::ParserFailed);
        }
        if self.closed {
            return Err(SseParseError::ParserClosed);
        }
        if self.frame_bytes != 0
            || !self.line.is_empty()
            || self.saw_data
            || self.saw_comment
            || self.event_name.is_some()
        {
            self.failed = true;
            return Err(SseParseError::TruncatedEvent);
        }
        self.closed = true;
        Ok(())
    }

    fn push_inner(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, SseParseError> {
        let mut events = Vec::new();
        for &byte in chunk {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }

            self.frame_bytes = self
                .frame_bytes
                .checked_add(1)
                .ok_or(SseParseError::EventTooLarge)?;
            if self.frame_bytes > self.max_event_bytes {
                return Err(SseParseError::EventTooLarge);
            }

            match byte {
                b'\r' => {
                    self.process_line(&mut events)?;
                    self.skip_lf = true;
                }
                b'\n' => self.process_line(&mut events)?,
                _ => self.line.push(byte),
            }
        }
        Ok(events)
    }

    fn process_line(&mut self, events: &mut Vec<SseEvent>) -> Result<(), SseParseError> {
        let mut line = mem::take(&mut self.line);
        if !self.bom_checked {
            self.bom_checked = true;
            // SSE 流首部允许携带 UTF-8 BOM，只剥离首行，避免吞掉后续数据。
            if line.starts_with(UTF8_BOM) {
                line.drain(..UTF8_BOM.len());
            }
        }
        if line.is_empty() {
            self.dispatch(events);
            return Ok(());
        }

        let line = str::from_utf8(&line).map_err(|_| SseParseError::InvalidUtf8)?;
        if line.starts_with(':') {
            self.saw_comment = true;
            return Ok(());
        }

        let (field, value) = line.split_once(':').map_or((line, ""), |(field, value)| {
            (field, value.strip_prefix(' ').unwrap_or(value))
        });
        match field {
            "data" => {
                if self.saw_data {
                    self.data.push(b'\n');
                }
                self.data.extend_from_slice(value.as_bytes());
                self.saw_data = true;
            }
            "event" => self.event_name = Some(value.to_owned()),
            // id/retry 与未知字段不参与协议数据解码，但仍计入事件预算。
            _ => {}
        }
        Ok(())
    }

    fn dispatch(&mut self, events: &mut Vec<SseEvent>) {
        if self.saw_data {
            events.push(SseEvent::message(
                self.event_name.take(),
                mem::take(&mut self.data),
            ));
        } else if self.saw_comment {
            events.push(SseEvent::comment());
        }
        self.frame_bytes = 0;
        self.line.clear();
        self.data.clear();
        self.event_name = None;
        self.saw_data = false;
        self.saw_comment = false;
    }
}

impl Default for SseParser {
    fn default() -> Self {
        Self::with_max_event_bytes(DEFAULT_MAX_SSE_EVENT_BYTES)
            .expect("默认 SSE 事件上限必须位于协议硬边界内")
    }
}

impl fmt::Debug for SseParser {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SseParser")
            .field("max_event_bytes", &self.max_event_bytes)
            .field("pending_bytes", &self.frame_bytes)
            .field("failed", &self.failed)
            .field("closed", &self.closed)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::SseParser;

    #[test]
    fn accepts_utf8_bom_split_before_first_event() {
        let chunks: [&[u8]; 4] = [
            b"\xEF",
            b"\xBB\xBFevent: message\n",
            b"data: hello\n",
            b"\n",
        ];
        let mut parser = SseParser::new();
        let mut events = Vec::new();
        for chunk in chunks {
            events.extend(parser.push(chunk).expect("SSE 分块必须可解析"));
        }
        parser.finish().expect("SSE 流必须正常结束");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name(), Some("message"));
        assert_eq!(events[0].data(), b"hello");
    }
}
