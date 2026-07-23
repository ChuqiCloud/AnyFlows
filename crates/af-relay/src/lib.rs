//! 端到端转发生命周期与故障转移编排。

mod error;
mod openai_chat;
mod openai_chat_stream;
mod openai_chat_usage;
mod state_machine;

pub use error::{RelayBuildError, RelayError};
pub use openai_chat::{OpenAiChatResponse, RelayService};
pub use openai_chat_stream::{
    OpenAiChatCompletionFuture, OpenAiChatCompletionHook, OpenAiChatStream,
};
pub use openai_chat_usage::{
    OpenAiChatUsageHandle, UsageResolutionError, estimate_openai_chat_request_upper_bound,
};
pub use state_machine::{
    RelayCandidate, RelayRequest, RelayResponse, RelayState, RelayStateMachine,
};
