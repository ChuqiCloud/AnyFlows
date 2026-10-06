use std::{future::Future, pin::Pin};

use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal};
use af_protocol::CanonicalAudioSpeechRequest;
use af_relay::SpeechResponse;

/// Audio Speech 业务服务的一次异步调用结果。
pub type SpeechServiceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SpeechResponse, AfError>> + Send + 'a>>;

/// HTTP 层依赖的对象安全 Audio Speech 服务端口。
pub trait SpeechService: Send + Sync {
    /// 转发一次非 SSE 文本转语音请求，并在内部闭合真实时长与最终计费。
    fn synthesize<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalAudioSpeechRequest,
        request_id: &'a str,
    ) -> SpeechServiceFuture<'a>;
}
