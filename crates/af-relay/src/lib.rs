//! 端到端转发生命周期与故障转移编排。

mod anthropic;
mod anthropic_request;
mod anthropic_stream;
mod attempt_gate;
mod chat_response;
mod chat_stream_encoder;
mod cohere_rerank;
mod diagnostic;
mod error;
mod gemini;
mod gemini_request;
mod gemini_stream;
mod generation_stream;
mod jina_rerank;
mod openai_audio;
mod openai_chat;
mod openai_chat_request;
mod openai_chat_stream;
mod openai_chat_usage;
mod openai_embeddings;
mod openai_images;
mod openai_responses;
mod openai_responses_compact;
mod openai_responses_request;
mod openai_responses_stream;
mod openai_speech;
mod rerank;
mod state_machine;
mod video_task;

pub use anthropic::{RelayAnthropicOutcome, relay_anthropic, relay_anthropic_with_report};
pub use anthropic_request::{AnthropicRequestOverrideError, AnthropicRequestOverrides};
pub use anthropic_stream::AnthropicMessagesStream;
pub use attempt_gate::{
    RelayAttemptGate, RelayAttemptGateError, RelayAttemptGateFuture, RelayAttemptPermit,
    RelayAttemptReleaseFuture,
};
pub use chat_response::ChatResponse;
pub use cohere_rerank::build_cohere_rerank_candidate_request;
pub use diagnostic::{
    RelayAttemptDiagnostic, RelayDiagnosticInput, RelayDiagnosticPolicy, RelayDiagnosticSnapshot,
};
pub use error::{RelayBuildError, RelayError};
pub use gemini::{RelayGeminiOutcome, relay_gemini, relay_gemini_with_report};
pub use gemini_request::{GeminiRequestOverrideError, GeminiRequestOverrides};
pub use gemini_stream::GeminiGenerateContentStream;
pub use generation_stream::{
    GenerationCompletionFuture, GenerationCompletionHook, GenerationStream,
};
pub use jina_rerank::{
    JinaRerankResponse, RelayJinaRerankOutcome, build_jina_rerank_candidate_request,
    relay_jina_rerank, relay_jina_rerank_with_report,
};
pub use openai_audio::{
    AudioTranscriptionResponse, MAX_AUDIO_MULTIPART_BODY_BYTES, RelayOpenAiAudioOutcome,
    encode_openai_audio_request, relay_openai_audio, relay_openai_audio_with_report,
};
pub use openai_chat::{
    RelayOpenAiChatOutcome, RelayService, relay_openai_chat, relay_openai_chat_with_report,
};
pub use openai_chat_request::{OpenAiChatRequestOverrideError, OpenAiChatRequestOverrides};
pub use openai_chat_stream::OpenAiChatStream;
pub use openai_chat_usage::{
    OpenAiChatUsageHandle, UsageResolutionError, estimate_openai_chat_request_upper_bound,
    estimate_openai_request_upper_bound, estimate_openai_text_tokens,
};
pub use openai_embeddings::{
    EmbeddingResponse, RelayOpenAiEmbeddingsOutcome, relay_openai_embeddings,
    relay_openai_embeddings_with_report,
};
pub use openai_images::{
    ImageResponse, RelayOpenAiImagesOutcome, relay_openai_images, relay_openai_images_with_report,
};
pub use openai_responses::{
    RelayOpenAiResponsesOutcome, relay_openai_responses, relay_openai_responses_with_report,
};
pub use openai_responses_compact::{
    RelayOpenAiResponsesCompactOutcome, ResponsesCompactionResponse,
    relay_openai_responses_compact, relay_openai_responses_compact_with_report,
};
pub use openai_responses_request::{
    OpenAiResponsesRequestOverrideError, OpenAiResponsesRequestOverrides,
};
pub use openai_responses_stream::OpenAiResponsesStream;
pub use openai_speech::{
    RelayOpenAiSpeechOutcome, SpeechResponse, encode_openai_speech_request, relay_openai_speech,
    relay_openai_speech_with_report,
};
pub use rerank::{RelayRerankOutcome, RerankResponse, relay_rerank_with_report};
pub use state_machine::{
    RelayAttemptFailure, RelayAttemptReport, RelayCandidate, RelayCandidateRequest,
    RelayClientSimulationAttempt, RelayExecutionError, RelayRequest, RelayResponse, RelayState,
    RelayStateMachine,
};
pub use video_task::{
    VideoTaskPollOutcome, VideoTaskSubmissionCandidate, VideoTaskSubmissionDisposition,
    VideoTaskSubmissionError, VideoTaskSubmissionOutcome, VideoTaskTarget, relay_video_task_poll,
    relay_video_task_submission,
};
