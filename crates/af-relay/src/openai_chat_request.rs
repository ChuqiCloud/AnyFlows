use af_adapter::Bytes;
use af_domain::{AfError, Protocol};
use af_protocol::{
    CanonicalRequest, CanonicalRequestEnvelope, StreamOptions, openai_chat::build_request,
};

/// 已决定正文来源的 OpenAI Chat 上游请求。
pub(crate) struct PreparedOpenAiChatRequest {
    pub(crate) canonical: CanonicalRequest,
    pub(crate) body: Bytes,
    pub(crate) include_usage: bool,
}

/// 在不削弱协议校验的前提下选择原字节直通或 Canonical 重构。
pub(crate) fn prepare_openai_chat_request(
    request: CanonicalRequestEnvelope,
) -> Result<PreparedOpenAiChatRequest, AfError> {
    let include_usage = request.canonical().stream_options.include_usage();
    let requires_usage_injection = request.canonical().stream && !include_usage;

    if !requires_usage_injection {
        match request.try_into_same_protocol(Protocol::OpenAiChat) {
            Ok(passthrough) => {
                let (canonical, body) = passthrough.into_parts();
                return Ok(PreparedOpenAiChatRequest {
                    canonical,
                    body,
                    include_usage,
                });
            }
            Err(request) => return rebuild_request(request, include_usage),
        }
    }

    rebuild_request(request, include_usage)
}

fn rebuild_request(
    request: CanonicalRequestEnvelope,
    include_usage: bool,
) -> Result<PreparedOpenAiChatRequest, AfError> {
    let mut canonical = request.into_canonical();
    if canonical.stream {
        // 内部结算始终需要上游 usage；客户端可见性仍使用原始 include_usage。
        canonical.stream_options = StreamOptions::new(true);
    }
    let value = build_request(&canonical).map_err(|_| AfError::InvalidRequest)?;
    let body = serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| AfError::Internal)?;
    Ok(PreparedOpenAiChatRequest {
        canonical,
        body,
        include_usage,
    })
}

#[cfg(test)]
mod tests {
    use af_protocol::{anthropic, openai_chat};

    use super::*;

    #[test]
    fn validated_same_protocol_body_is_preserved_exactly() {
        let body = Bytes::from_static(
            b"{\n  \"model\": \"gpt-test\",\n  \"messages\": [{\"role\":\"user\",\"content\":\"hello\"}]\n}",
        );
        let request = openai_chat::parse_request_envelope(body.clone()).unwrap();

        let prepared = prepare_openai_chat_request(request).unwrap();

        assert_eq!(prepared.body, body);
        assert!(!prepared.canonical.stream);
    }

    #[test]
    fn cross_protocol_and_canonical_only_requests_are_rebuilt() {
        let anthropic_body = Bytes::from_static(
            br#"{"model":"gpt-test","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
        );
        let cross_protocol = anthropic::parse_request_envelope(anthropic_body.clone()).unwrap();
        let prepared = prepare_openai_chat_request(cross_protocol).unwrap();
        assert_ne!(prepared.body, anthropic_body);
        assert!(
            serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap()["messages"]
                .is_array()
        );

        let canonical_only = CanonicalRequestEnvelope::from_canonical(prepared.canonical);
        let rebuilt = prepare_openai_chat_request(canonical_only).unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&rebuilt.body).is_ok());
    }

    #[test]
    fn stream_without_client_usage_is_rebuilt_for_internal_settlement() {
        let body = Bytes::from_static(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hello"}],"stream":true}"#,
        );
        let request = openai_chat::parse_request_envelope(body.clone()).unwrap();

        let prepared = prepare_openai_chat_request(request).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();

        assert_ne!(prepared.body, body);
        assert!(!prepared.include_usage);
        assert_eq!(value["stream_options"]["include_usage"], true);
    }

    #[test]
    fn stream_with_explicit_usage_keeps_the_validated_body() {
        let body = Bytes::from_static(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_usage":true}}"#,
        );
        let request = openai_chat::parse_request_envelope(body.clone()).unwrap();

        let prepared = prepare_openai_chat_request(request).unwrap();

        assert_eq!(prepared.body, body);
        assert!(prepared.include_usage);
    }
}
