//! OpenAI Audio Speech 公开网关契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use utoipa::openapi::{KnownFormat, ObjectBuilder, RefOr, SchemaFormat, Type, schema::Schema};
use utoipa::{OpenApi, PartialSchema, ToSchema};

/// AnyFlows 当前生产支持的非 SSE Speech 请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AudioSpeechRequest)]
struct AudioSpeechRequestSchema {
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    #[schema(min_length = 1, max_length = 4096)]
    input: String,
    voice: AudioSpeechVoiceSchema,
    /// 最多 65536 个 UTF-8 字节；多字节字符会更早触发运行时字节限制。
    #[schema(max_length = 65536)]
    instructions: Option<String>,
    response_format: Option<AudioSpeechOutputFormatSchema>,
    #[schema(minimum = 0.25, maximum = 4.0, default = 1.0)]
    speed: Option<f64>,
    stream_format: Option<AudioSpeechStreamFormatSchema>,
}

/// Speech 内建声音名称或 custom voice 引用。
#[derive(Deserialize, ToSchema)]
#[serde(untagged)]
#[schema(as = AudioSpeechVoice)]
enum AudioSpeechVoiceSchema {
    Named(String),
    Custom(AudioSpeechCustomVoiceSchema),
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AudioSpeechCustomVoice)]
struct AudioSpeechCustomVoiceSchema {
    #[schema(min_length = 1, max_length = 512)]
    id: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
#[schema(as = AudioSpeechOutputFormat)]
enum AudioSpeechOutputFormatSchema {
    Mp3,
    Opus,
    Aac,
    Flac,
    Wav,
    Pcm,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
#[schema(as = AudioSpeechStreamFormat)]
enum AudioSpeechStreamFormatSchema {
    Audio,
}

/// 由 `response_format` 决定具体 MIME 的完整二进制音频。
struct AudioSpeechBinarySchema;

impl PartialSchema for AudioSpeechBinarySchema {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Binary)))
            .into()
    }
}

impl ToSchema for AudioSpeechBinarySchema {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("AudioSpeechBinary")
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AudioSpeechErrorBody)]
struct AudioSpeechErrorBodySchema {
    code: String,
    message: String,
    #[schema(required = true)]
    param: Option<String>,
    #[serde(rename = "type")]
    error_type: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AudioSpeechError)]
struct AudioSpeechErrorSchema {
    error: AudioSpeechErrorBodySchema,
}

#[utoipa::path(
    post,
    path = "/v1/audio/speech",
    operation_id = "synthesizeSpeech",
    tag = "OpenAI Audio",
    summary = "将文本合成为完整音频",
    description = "仅接受缺失或 audio 流格式；SSE 当前失败关闭。响应 MIME 由 response_format 决定，支持 mp3、opus、aac、flac、wav 与 pcm。",
    request_body = AudioSpeechRequestSchema,
    responses(
        (status = 200, description = "经过格式、签名和大小校验的完整音频", content(
            (AudioSpeechBinarySchema = "audio/mpeg"),
            (AudioSpeechBinarySchema = "audio/ogg"),
            (AudioSpeechBinarySchema = "audio/aac"),
            (AudioSpeechBinarySchema = "audio/flac"),
            (AudioSpeechBinarySchema = "audio/wav"),
            (AudioSpeechBinarySchema = "audio/pcm")
        )),
        (status = 400, description = "请求字段、字符数或格式无效", body = AudioSpeechErrorSchema),
        (status = 401, description = "API Key 无效", body = AudioSpeechErrorSchema),
        (status = 404, description = "模型不存在、不可用或不在令牌允许范围内", body = AudioSpeechErrorSchema),
        (status = 429, description = "额度、并发或请求频率受限", body = AudioSpeechErrorSchema),
        (status = 500, description = "音频时长或计费事实无法可信闭合", body = AudioSpeechErrorSchema),
        (status = 503, description = "没有可用的 Speech 候选", body = AudioSpeechErrorSchema)
    ),
    security(("apiKeyAuth" = []))
)]
fn synthesize_speech() {}

#[derive(OpenApi)]
#[openapi(
    paths(synthesize_speech),
    components(schemas(
        AudioSpeechRequestSchema,
        AudioSpeechVoiceSchema,
        AudioSpeechCustomVoiceSchema,
        AudioSpeechOutputFormatSchema,
        AudioSpeechStreamFormatSchema,
        AudioSpeechBinarySchema,
        AudioSpeechErrorBodySchema,
        AudioSpeechErrorSchema
    ))
)]
struct SpeechApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    SpeechApi::openapi()
}
