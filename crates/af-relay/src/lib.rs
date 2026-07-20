//! 端到端转发生命周期与故障转移编排。

mod error;
mod openai_chat;

pub use error::RelayBuildError;
pub use openai_chat::{OpenAiChatResponse, RelayService};
