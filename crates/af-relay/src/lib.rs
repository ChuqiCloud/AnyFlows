//! 端到端转发生命周期与故障转移编排。

mod anthropic;
mod anthropic_request;
mod anthropic_stream;
mod attempt_gate;
mod chat_response;
mod chat_stream_encoder;
mod error;
mod gemini;
mod gemini_request;
mod gemini_stream;
mod generation_stream;
mod openai_chat;
mod openai_chat_request;
mod openai_chat_stream;
mod openai_chat_usage;
mod openai_embeddings;
mod openai_responses;
mod openai_responses_request;
mod openai_responses_stream;
mod state_machine;

pub use anthropic::{RelayAnthropicOutcome, relay_anthropic, relay_anthropic_with_report};
pub use anthropic_request::{AnthropicRequestOverrideError, AnthropicRequestOverrides};
pub use anthropic_stream::AnthropicMessagesStream;
pub use attempt_gate::{
    RelayAttemptGate, RelayAttemptGateError, RelayAttemptGateFuture, RelayAttemptPermit,
    RelayAttemptReleaseFuture,
};
pub use chat_response::ChatResponse;
pub use error::{RelayBuildError, RelayError};
pub use gemini::{RelayGeminiOutcome, relay_gemini, relay_gemini_with_report};
pub use gemini_request::{GeminiRequestOverrideError, GeminiRequestOverrides};
pub use gemini_stream::GeminiGenerateContentStream;
pub use generation_stream::{
    GenerationCompletionFuture, GenerationCompletionHook, GenerationStream,
};
pub use openai_chat::{
    RelayOpenAiChatOutcome, RelayService, relay_openai_chat, relay_openai_chat_with_report,
};
pub use openai_chat_request::{OpenAiChatRequestOverrideError, OpenAiChatRequestOverrides};
pub use openai_chat_stream::OpenAiChatStream;
pub use openai_chat_usage::{
    OpenAiChatUsageHandle, UsageResolutionError, estimate_openai_chat_request_upper_bound,
    estimate_openai_request_upper_bound,
};
pub use openai_embeddings::{
    EmbeddingResponse, RelayOpenAiEmbeddingsOutcome, relay_openai_embeddings,
    relay_openai_embeddings_with_report,
};
pub use openai_responses::{
    RelayOpenAiResponsesOutcome, relay_openai_responses, relay_openai_responses_with_report,
};
pub use openai_responses_request::{
    OpenAiResponsesRequestOverrideError, OpenAiResponsesRequestOverrides,
};
pub use openai_responses_stream::OpenAiResponsesStream;
pub use state_machine::{
    RelayAttemptFailure, RelayAttemptReport, RelayCandidate, RelayCandidateRequest,
    RelayExecutionError, RelayRequest, RelayResponse, RelayState, RelayStateMachine,
};
