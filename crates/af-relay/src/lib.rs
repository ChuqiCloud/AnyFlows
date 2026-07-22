//! 端到端转发生命周期与故障转移编排。

mod error;
mod openai_chat;
mod openai_chat_stream;
mod state_machine;

pub use error::{RelayBuildError, RelayError};
pub use openai_chat::{OpenAiChatResponse, RelayService};
pub use openai_chat_stream::OpenAiChatStream;
pub use state_machine::{
    RelayCandidate, RelayRequest, RelayResponse, RelayState, RelayStateMachine,
};
