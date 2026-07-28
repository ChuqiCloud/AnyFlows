mod decode;
mod encode;
mod error;
mod wire;

pub use crate::sse::{
    DEFAULT_MAX_SSE_EVENT_BYTES, MAX_SSE_EVENT_BYTES, SseEvent, SseParseError, SseParser,
};
pub use decode::OpenAiChatStreamDecoder;
pub use encode::OpenAiChatStreamEncoder;
pub use error::{EncodeStreamError, ParseStreamError};

#[cfg(test)]
mod tests;
