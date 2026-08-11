use std::fmt;

use af_adapter::Bytes;
use af_domain::{AfError, Protocol, UpstreamError};
use af_protocol::{CanonicalResponse, Usage, anthropic, gemini};

use crate::{GenerationStream, OpenAiChatUsageHandle, UsageResolutionError};

/// 已按客户端协议完成编码的完整或流式 Chat 响应。
pub enum ChatResponse {
    /// 完整解析通过后按客户端协议编码的 JSON。
    Full {
        /// 写入下游的响应正文。
        body: Bytes,
        /// 上游 usage 或本地受控估算结果。
        usage: Result<Usage, UsageResolutionError>,
    },
    /// 从上游协议逐事件转换到客户端协议的 SSE。
    Stream {
        /// 写入下游的 Canonical 重编码流。
        body: Box<dyn GenerationStream>,
        /// 流完整结束后只可消费一次的 usage 结果。
        usage: OpenAiChatUsageHandle,
    },
}

impl fmt::Debug for ChatResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full { body, usage: _ } => formatter
                .debug_struct("ChatResponse")
                .field("mode", &"full")
                .field("body_bytes", &body.len())
                .finish(),
            Self::Stream { body: _, usage: _ } => formatter
                .debug_struct("ChatResponse")
                .field("mode", &"stream")
                .finish(),
        }
    }
}

/// 把已经验证的 OpenAI 上游完整响应编码为指定客户端协议。
pub(crate) fn encode_full_response(
    protocol: Protocol,
    original_body: Bytes,
    response: &CanonicalResponse,
    usage: Result<Usage, UsageResolutionError>,
    client_model: &str,
    request_id: &str,
) -> Result<ChatResponse, AfError> {
    if protocol == Protocol::OpenAiChat {
        return Ok(ChatResponse::Full {
            body: original_body,
            usage,
        });
    }
    let usage = usage.map_err(|_| AfError::from(UpstreamError::ProtocolError))?;
    // 跨协议响应只保留 Canonical 语义，丢弃 OpenAI 同源 raw，并回显客户端模型。
    let response_id = match protocol {
        Protocol::Anthropic => format!("msg_{request_id}"),
        Protocol::Gemini => format!("response-{request_id}"),
        Protocol::OpenAiChat
        | Protocol::OpenAiResponses
        | Protocol::OpenAiEmbeddings
        | Protocol::OpenAiImages
        | Protocol::OpenAiAudio
        | Protocol::OpenAiSpeech
        | Protocol::JinaRerank
        | Protocol::CohereRerank
        | Protocol::XaiVideo => {
            return Err(AfError::Internal);
        }
    };
    let response = CanonicalResponse::new(
        response.operation,
        response_id,
        client_model.to_owned(),
        response.created_at,
        response.choices.clone(),
        Some(usage),
    );
    let value = match protocol {
        Protocol::Anthropic => anthropic::build_response(&response)
            .map_err(|_| AfError::from(UpstreamError::ProtocolError))?,
        Protocol::Gemini => gemini::build_response(&response)
            .map_err(|_| AfError::from(UpstreamError::ProtocolError))?,
        Protocol::OpenAiChat
        | Protocol::OpenAiResponses
        | Protocol::OpenAiEmbeddings
        | Protocol::OpenAiImages
        | Protocol::OpenAiAudio
        | Protocol::OpenAiSpeech
        | Protocol::JinaRerank
        | Protocol::CohereRerank
        | Protocol::XaiVideo => {
            unreachable!("协议分支已在上方闭合")
        }
    };
    let body = serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| AfError::Internal)?;
    Ok(ChatResponse::Full {
        body,
        usage: Ok(usage),
    })
}

#[cfg(test)]
mod tests {
    use af_protocol::openai_chat;
    use serde_json::Value;

    use super::*;

    #[test]
    fn openai_response_is_rebuilt_as_anthropic_without_upstream_identity() {
        let original = Bytes::from_static(
            br#"{"id":"upstream-private-id","object":"chat.completion","created":1700000000,"model":"upstream-private-model","choices":[{"index":0,"message":{"role":"assistant","content":"answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3},"service_tier":"default"}"#,
        );
        let canonical = openai_chat::parse_response(&original).unwrap();
        let usage = canonical.usage.unwrap();

        let ChatResponse::Full {
            body,
            usage: result,
        } = encode_full_response(
            Protocol::Anthropic,
            original,
            &canonical,
            Ok(usage),
            "public-model",
            "request-1",
        )
        .unwrap()
        else {
            panic!("非流式响应必须保持完整响应模式");
        };

        assert_eq!(result.unwrap(), usage);
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["id"], "msg_request-1");
        assert_eq!(value["type"], "message");
        assert_eq!(value["model"], "public-model");
        assert_eq!(value["content"][0]["text"], "answer");
        assert_eq!(value["usage"]["service_tier"], Value::Null);
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(!text.contains("upstream-private-id"));
        assert!(!text.contains("upstream-private-model"));
        assert!(!text.contains("\"service_tier\":\"default\""));
    }

    #[test]
    fn openai_response_is_rebuilt_as_gemini_without_upstream_identity() {
        let original = Bytes::from_static(
            br#"{"id":"upstream-private-id","object":"chat.completion","created":1700000000,"model":"upstream-private-model","choices":[{"index":0,"message":{"role":"assistant","content":"answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}}"#,
        );
        let canonical = openai_chat::parse_response(&original).unwrap();
        let usage = canonical.usage.unwrap();

        let ChatResponse::Full {
            body,
            usage: result,
        } = encode_full_response(
            Protocol::Gemini,
            original,
            &canonical,
            Ok(usage),
            "public-model",
            "request-1",
        )
        .unwrap()
        else {
            panic!("非流式响应必须保持完整响应模式");
        };

        assert_eq!(result.unwrap(), usage);
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["responseId"], "response-request-1");
        assert_eq!(value["modelVersion"], "public-model");
        assert_eq!(value["candidates"][0]["content"]["role"], "model");
        assert_eq!(
            value["candidates"][0]["content"]["parts"][0]["text"],
            "answer"
        );
        assert_eq!(value["usageMetadata"]["promptTokenCount"], 2);
        assert_eq!(value["usageMetadata"]["candidatesTokenCount"], 1);
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(!text.contains("upstream-private-id"));
        assert!(!text.contains("upstream-private-model"));
    }
}
