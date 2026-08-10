use std::{future::Future, pin::Pin};

use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal};
use af_protocol::CanonicalAudioTranscriptionRequest;
use af_relay::AudioTranscriptionResponse;

/// Audio 转录业务服务的一次异步调用结果。
pub type AudioServiceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AudioTranscriptionResponse, AfError>> + Send + 'a>>;

/// HTTP 层依赖的对象安全 Audio 转录服务端口。
///
/// 端口只接收已完成 multipart 解析、模型白名单校验和鉴权的不可变请求，不负责 HTTP wire。
pub trait AudioService: Send + Sync {
    /// 转发一次非流式文件转录请求，并在内部闭合时长探测与最终计费。
    fn transcribe<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalAudioTranscriptionRequest,
        request_id: &'a str,
    ) -> AudioServiceFuture<'a>;
}
