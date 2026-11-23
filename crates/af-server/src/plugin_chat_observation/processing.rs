//! Transport-independent response stages. WebSocket responses are normalized
//! to the same SSE stream by the transport dispatcher before reaching here.
use super::*;
use af_adapter::{AdaptorError, ResponseMode, UpstreamBody, UpstreamBodyStream};
use af_domain::Protocol;
use af_protocol::{CanonicalStreamEvent, ContentBlock, ContentDelta};
use futures_util::{StreamExt, stream};

fn protocol_error() -> af_domain::UpstreamError {
    af_domain::UpstreamError::ProtocolError
}

impl DatabasePluginProviderRelayHook {
    async fn process_stage(
        &self,
        request: &ProviderRelayRequest,
        stage: RequestProcessingStage,
        status: Option<u16>,
        body: Value,
    ) -> Result<Value, af_domain::UpstreamError> {
        let targets = self.processor.targets_for_request().await;
        if targets.is_empty() {
            return Ok(body);
        }
        let mut input = RequestProcessingInput {
            stage,
            request_id: request.request_id.clone(),
            format: "relay".into(),
            path: Url::parse(&request.target)
                .map_err(|_| protocol_error())?
                .path()
                .into(),
            model: Some(request.model.clone()),
            upstream_model: Some(request.model.clone()),
            headers: Default::default(),
            body,
            response_status: status,
            response_body: None,
            usage: None,
        };
        // Large payloads remain under the normal Relay limits; plugins see a summary.
        let summary = serde_json::to_vec(&input.body).is_ok_and(|value| value.len() > 48 * 1024);
        let original = input.body.clone();
        if summary {
            input.body = json!({"payloadOmitted":true});
        }
        let usage = original.get("usage").cloned();
        input.usage = usage.clone();
        let deadline = Instant::now() + OBSERVER_REQUEST_BUDGET;
        for target in targets.into_iter().take(OBSERVER_MAX_PLUGINS_PER_REQUEST) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| protocol_error())?
                .as_millis() as u64;
            let output = tokio::time::timeout_at(
                deadline,
                self.runtime.process_request_stage_with_deadline(
                    &self.repository,
                    &target.plugin_id,
                    target.generation,
                    &request.request_id,
                    now_ms.saturating_add(remaining.as_millis() as u64),
                    input.clone(),
                ),
            )
            .await
            .map_err(|_| protocol_error())?
            .map_err(|_| protocol_error())?;
            if output.is_terminal() {
                return Err(protocol_error());
            }
            output.apply_to(&mut input).map_err(|_| protocol_error())?;
        }
        if summary {
            return Ok(original);
        }
        // Request processors may edit content, but cannot change billing facts.
        if let Some(usage) = usage {
            input.body["usage"] = usage;
        }
        Ok(input.body)
    }

    async fn provider_value(
        &self,
        request: &ProviderRelayRequest,
        status: u16,
        body: Value,
    ) -> Result<Value, af_domain::UpstreamError> {
        let Some(binding) = &request.binding else {
            return Ok(body);
        };
        let target = ProviderTarget::from(binding);
        let invocation = self.invocation(&target, &request.model, "provider-response");
        match self
            .runtime
            .invoke_provider(
                &self.repository,
                &binding.plugin_id,
                binding.generation,
                &invocation,
                binding.capability_version,
                PluginProviderOperation::Response {
                    status,
                    response: body,
                },
            )
            .await
            .map_err(|_| protocol_error())?
        {
            PluginProviderResult::Response { response } => Ok(response),
            _ => Err(protocol_error()),
        }
    }

    pub(super) async fn process_outbound(
        &self,
        mut request: ProviderRelayRequest,
    ) -> Result<ProviderRelayRequest, af_domain::UpstreamError> {
        if let Some(body) = request
            .body
            .as_ref()
            .and_then(|body| serde_json::from_slice::<Value>(body).ok())
        {
            let processed = self
                .process_stage(
                    &request,
                    RequestProcessingStage::Outbound,
                    None,
                    body.clone(),
                )
                .await?;
            // Native relay preserves the verified bytes when processing leaves the JSON intact.
            if processed != body {
                request.body = Some(Bytes::from(
                    serde_json::to_vec(&processed).map_err(|_| protocol_error())?,
                ));
            }
        }
        Ok(request)
    }

    pub(super) async fn process_response(
        &self,
        request: &ProviderRelayRequest,
        mut response: UpstreamResponse,
    ) -> Result<UpstreamResponse, af_domain::UpstreamError> {
        if request.binding.is_none() && self.processor.targets_for_request().await.is_empty() {
            return Ok(response);
        }
        let status = response.status();
        let mut headers = response.headers().clone();
        let is_sse = headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"))
            });
        if is_sse && response.full_body().is_some() {
            let body = response
                .into_body()
                .into_bytes_with_limit(request.response_body_limit)
                .await
                .map_err(|_| protocol_error())?;
            headers.remove("content-length");
            headers.remove("content-encoding");
            response = UpstreamResponse::stream(
                status,
                headers.clone(),
                stream::once(async move { Ok::<_, AdaptorError>(body) }),
            )
            .map_err(|_| protocol_error())?;
        }
        if let Some(bytes) = response.full_body() {
            let Ok(body) = serde_json::from_slice(bytes) else {
                return Ok(response);
            };
            let body = if request.session.is_some() {
                body
            } else {
                self.provider_value(request, status.as_u16(), body).await?
            };
            let body = self
                .process_stage(
                    request,
                    RequestProcessingStage::Egress,
                    Some(status.as_u16()),
                    body,
                )
                .await?;
            let body = self
                .process_stage(
                    request,
                    RequestProcessingStage::Reply,
                    Some(status.as_u16()),
                    body,
                )
                .await?;
            let _ = self
                .process_stage(
                    request,
                    RequestProcessingStage::Report,
                    Some(status.as_u16()),
                    json!({"completed":true,"usage":body.get("usage")}),
                )
                .await;
            if request.operation == af_domain::Operation::Chat
                && request.upstream_protocol == Protocol::OpenAiChat
                && request.response_mode == ResponseMode::Stream
            {
                let chunks = chat_json_to_sse(&body, request.response_body_limit)?;
                headers.remove("content-length");
                headers.remove("content-encoding");
                headers.insert(
                    "content-type",
                    HeaderValue::from_static("text/event-stream; charset=utf-8"),
                );
                headers.insert("cache-control", HeaderValue::from_static("no-cache"));
                let stream = stream::iter(chunks.into_iter().map(Ok::<_, AdaptorError>));
                return UpstreamResponse::stream(status, headers, stream)
                    .map_err(|_| protocol_error());
            }
            headers.remove("content-length");
            headers.remove("content-encoding");
            return UpstreamResponse::full_with_limit(
                status,
                headers,
                Bytes::from(serde_json::to_vec(&body).map_err(|_| protocol_error())?),
                request.response_body_limit,
            )
            .map_err(|_| protocol_error());
        }
        if !is_sse {
            if request.operation == af_domain::Operation::Chat
                && request.upstream_protocol == Protocol::OpenAiChat
            {
                let bytes = response
                    .into_body()
                    .into_bytes_with_limit(request.response_body_limit)
                    .await
                    .map_err(|_| protocol_error())?;
                let Ok(original) = serde_json::from_slice::<Value>(&bytes) else {
                    return UpstreamResponse::full_with_limit(
                        status,
                        headers,
                        bytes,
                        request.response_body_limit,
                    )
                    .map_err(|_| protocol_error());
                };
                let body = self
                    .provider_value(request, status.as_u16(), original)
                    .await?;
                let body = self
                    .process_stage(
                        request,
                        RequestProcessingStage::Egress,
                        Some(status.as_u16()),
                        body,
                    )
                    .await?;
                let body = self
                    .process_stage(
                        request,
                        RequestProcessingStage::Reply,
                        Some(status.as_u16()),
                        body,
                    )
                    .await?;
                let _ = self
                    .process_stage(
                        request,
                        RequestProcessingStage::Report,
                        Some(status.as_u16()),
                        json!({"completed":true,"usage":body.get("usage")}),
                    )
                    .await;
                headers.remove("content-length");
                headers.remove("content-encoding");
                if request.response_mode == ResponseMode::Stream {
                    let chunks = chat_json_to_sse(&body, request.response_body_limit)?;
                    headers.insert(
                        "content-type",
                        HeaderValue::from_static("text/event-stream; charset=utf-8"),
                    );
                    headers.insert("cache-control", HeaderValue::from_static("no-cache"));
                    let stream = stream::iter(chunks.into_iter().map(Ok::<_, AdaptorError>));
                    return UpstreamResponse::stream(status, headers, stream)
                        .map_err(|_| protocol_error());
                }
                headers.insert(
                    "content-type",
                    HeaderValue::from_static("application/json; charset=utf-8"),
                );
                return UpstreamResponse::full_with_limit(
                    status,
                    headers,
                    Bytes::from(serde_json::to_vec(&body).map_err(|_| protocol_error())?),
                    request.response_body_limit,
                )
                .map_err(|_| protocol_error());
            }
            return Ok(response);
        }
        let UpstreamBody::Stream(source) = response.into_body() else {
            return Err(protocol_error());
        };
        headers.remove("content-length");
        headers.remove("content-encoding");
        let state = ResponseStream {
            source,
            pending: Vec::new(),
            hook: self.clone(),
            request: request.clone(),
            status: status.as_u16(),
            done: false,
            reported: false,
            usage: None,
        };
        let stream = stream::unfold(state, |mut state| async move {
            if state.done {
                return None;
            }
            loop {
                if let Some((at, separator)) = event_end(&state.pending) {
                    let frame: Vec<_> = state.pending.drain(..at + separator).collect();
                    let result = state.project_frame(frame).await;
                    if result.is_err() {
                        state.done = true;
                    }
                    return Some((result, state));
                }
                match state.source.next().await {
                    Some(Ok(bytes)) => {
                        if state.pending.len().saturating_add(bytes.len()) > 256 * 1024 {
                            state.done = true;
                            return Some((Err(AdaptorError::ResponseBodyTooLarge), state));
                        }
                        state.pending.extend_from_slice(&bytes);
                    }
                    Some(Err(error)) => {
                        state.done = true;
                        return Some((Err(error), state));
                    }
                    None => {
                        state.done = true;
                        let _ = state
                            .hook
                            .process_stage(
                                &state.request,
                                RequestProcessingStage::Report,
                                Some(state.status),
                                json!({"completed":true,"usage":state.usage}),
                            )
                            .await;
                        state.reported = true;
                        if state.pending.is_empty() {
                            return None;
                        }
                        let remaining = std::mem::take(&mut state.pending);
                        let result = state.project_frame(remaining).await;
                        return Some((result, state));
                    }
                }
            }
        });
        let response = UpstreamResponse::stream(status, headers.clone(), stream)
            .map_err(|_| protocol_error())?;
        if request.operation == af_domain::Operation::Chat
            && request.upstream_protocol == Protocol::OpenAiChat
            && request.response_mode == ResponseMode::Full
        {
            let bytes = response
                .into_body()
                .into_bytes_with_limit(request.response_body_limit)
                .await
                .map_err(|_| protocol_error())?;
            let body = chat_sse_to_json(bytes).await?;
            headers.remove("content-length");
            headers.remove("content-encoding");
            headers.insert(
                "content-type",
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
            return UpstreamResponse::full_with_limit(
                status,
                headers,
                body,
                request.response_body_limit,
            )
            .map_err(|_| protocol_error());
        }
        Ok(response)
    }
}

fn chat_json_to_sse(
    body: &Value,
    response_body_limit: usize,
) -> Result<Vec<Bytes>, af_domain::UpstreamError> {
    let json = serde_json::to_vec(body).map_err(|_| protocol_error())?;
    let response = af_protocol::openai_chat::parse_response(&json).map_err(|_| protocol_error())?;
    let metadata = response
        .raw_passthrough()
        .map(|raw| {
            raw.fields_for_protocol(Protocol::OpenAiChat)
                .cloned()
                .map_err(|_| protocol_error())
        })
        .transpose()?
        .unwrap_or_default();
    let created = response.created_at.ok_or_else(protocol_error)?;
    let mut encoder = af_protocol::openai_chat::OpenAiChatStreamEncoder::new(
        response.id.clone(),
        response.model.clone(),
        created,
    )
    .map_err(|_| protocol_error())?;
    let mut chunks = Vec::new();
    let mut total_bytes = 0_usize;
    {
        let mut emit = |event| -> Result<(), af_domain::UpstreamError> {
            let bytes = encoder.encode(event).map_err(|_| protocol_error())?;
            if !bytes.is_empty() {
                total_bytes = total_bytes
                    .checked_add(bytes.len())
                    .filter(|size| *size <= response_body_limit)
                    .ok_or_else(protocol_error)?;
                chunks.push(Bytes::from(bytes));
            }
            Ok(())
        };
        for choice in response.choices {
            emit(CanonicalStreamEvent::MessageStart {
                choice_index: choice.index,
                role: af_domain::Role::Assistant,
            })?;
            if let Some(logprobs) = choice.logprobs {
                emit(CanonicalStreamEvent::LogprobsDelta {
                    choice_index: choice.index,
                    logprobs,
                })?;
            }
            let mut content_index = 0_u32;
            let mut tool_index = 0_u32;
            for block in choice.message.content {
                match block {
                    ContentBlock::Text(text) => emit(CanonicalStreamEvent::ContentDelta {
                        choice_index: choice.index,
                        content_index,
                        delta: ContentDelta::Text(text),
                    })?,
                    ContentBlock::Thinking { text, signature } => {
                        emit(CanonicalStreamEvent::ReasoningDelta {
                            choice_index: choice.index,
                            content_index,
                            text,
                            signature,
                        })?;
                    }
                    ContentBlock::ToolUse {
                        id,
                        name,
                        input,
                        signature,
                    } => {
                        if signature.is_some() {
                            return Err(protocol_error());
                        }
                        emit(CanonicalStreamEvent::ToolCallStart {
                            choice_index: choice.index,
                            tool_index,
                            id,
                            name,
                        })?;
                        let arguments =
                            serde_json::to_string(&input).map_err(|_| protocol_error())?;
                        emit(CanonicalStreamEvent::ToolCallArgsDelta {
                            choice_index: choice.index,
                            tool_index,
                            partial_json: arguments,
                        })?;
                        emit(CanonicalStreamEvent::ToolCallEnd {
                            choice_index: choice.index,
                            tool_index,
                        })?;
                        tool_index = tool_index.checked_add(1).ok_or_else(protocol_error)?;
                    }
                    _ => return Err(protocol_error()),
                }
                content_index = content_index.checked_add(1).ok_or_else(protocol_error)?;
            }
            emit(CanonicalStreamEvent::Finish {
                choice_index: choice.index,
                reason: choice.finish_reason,
                stop_sequence: choice.stop_sequence,
            })?;
        }
        if let Some(usage) = response.usage {
            emit(CanonicalStreamEvent::Usage(usage))?;
        }
        emit(CanonicalStreamEvent::StreamEnd)?;
    }
    if !metadata.is_empty() {
        for chunk in &mut chunks {
            let text = std::str::from_utf8(chunk).map_err(|_| protocol_error())?;
            let Some(data) = text
                .strip_prefix("data: ")
                .and_then(|value| value.strip_suffix("\n\n"))
            else {
                continue;
            };
            if data == "[DONE]" {
                continue;
            }
            let mut value: Value = serde_json::from_str(data).map_err(|_| protocol_error())?;
            let object = value.as_object_mut().ok_or_else(protocol_error)?;
            for (name, metadata_value) in &metadata {
                object.insert(name.clone(), metadata_value.clone());
            }
            *chunk = Bytes::from(format!(
                "data: {}\n\n",
                serde_json::to_string(&value).map_err(|_| protocol_error())?
            ));
        }
    }
    if total_bytes > response_body_limit {
        return Err(protocol_error());
    }
    chunks.iter().try_fold(0_usize, |total, chunk| {
        total
            .checked_add(chunk.len())
            .filter(|size| *size <= response_body_limit)
            .ok_or_else(protocol_error)
    })?;
    Ok(chunks)
}

async fn chat_sse_to_json(bytes: Bytes) -> Result<Bytes, af_domain::UpstreamError> {
    let identity = chat_stream_identity(&bytes)?;
    let mut response =
        af_relay::collect_conversation_response(Protocol::OpenAiChat, UpstreamBody::Full(bytes))
            .await?;
    response.id = identity.id;
    response.model = identity.model;
    response.created_at = Some(identity.created);
    let mut body =
        af_protocol::openai_chat::build_response(&response).map_err(|_| protocol_error())?;
    if let Some(service_tier) = identity.service_tier {
        body["service_tier"] = Value::String(service_tier);
    }
    if let Some(system_fingerprint) = identity.system_fingerprint {
        body["system_fingerprint"] = Value::String(system_fingerprint);
    }
    let body_bytes = serde_json::to_vec(&body).map_err(|_| protocol_error())?;
    af_protocol::openai_chat::parse_response(&body_bytes).map_err(|_| protocol_error())?;
    Ok(Bytes::from(body_bytes))
}

struct ChatStreamIdentity {
    id: String,
    model: String,
    created: i64,
    service_tier: Option<String>,
    system_fingerprint: Option<String>,
}

fn chat_stream_identity(bytes: &[u8]) -> Result<ChatStreamIdentity, af_domain::UpstreamError> {
    let text = std::str::from_utf8(bytes).map_err(|_| protocol_error())?;
    let mut identity: Option<ChatStreamIdentity> = None;
    for frame in text.split("\n\n").flat_map(|frame| frame.split("\r\n\r\n")) {
        let data = frame
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let value: Value = serde_json::from_str(&data).map_err(|_| protocol_error())?;
        let current = ChatStreamIdentity {
            id: value
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(protocol_error)?
                .to_owned(),
            model: value
                .get("model")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(protocol_error)?
                .to_owned(),
            created: value
                .get("created")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 0)
                .ok_or_else(protocol_error)?,
            service_tier: value
                .get("service_tier")
                .and_then(Value::as_str)
                .map(str::to_owned),
            system_fingerprint: value
                .get("system_fingerprint")
                .and_then(Value::as_str)
                .map(str::to_owned),
        };
        if let Some(previous) = identity.as_mut() {
            let id_compatible = previous.id == current.id
                || previous.id == "chatcmpl-wb"
                || current.id == "chatcmpl-wb";
            if !id_compatible
                || previous.model != current.model
                || previous.created != current.created
                || previous.service_tier != current.service_tier
                || previous.system_fingerprint != current.system_fingerprint
            {
                return Err(protocol_error());
            }
            if previous.id == "chatcmpl-wb" && current.id != "chatcmpl-wb" {
                previous.id.clone_from(&current.id);
            }
        } else {
            identity = Some(current);
        }
    }
    identity.ok_or_else(protocol_error)
}

fn event_end(bytes: &[u8]) -> Option<(usize, usize)> {
    bytes
        .windows(2)
        .position(|bytes| bytes == b"\n\n")
        .map(|at| (at, 2))
        .into_iter()
        .chain(
            bytes
                .windows(4)
                .position(|bytes| bytes == b"\r\n\r\n")
                .map(|at| (at, 4)),
        )
        .min_by_key(|(at, _)| *at)
}

struct ResponseStream {
    source: UpstreamBodyStream,
    pending: Vec<u8>,
    hook: DatabasePluginProviderRelayHook,
    request: ProviderRelayRequest,
    status: u16,
    done: bool,
    reported: bool,
    usage: Option<Value>,
}
impl ResponseStream {
    async fn project_frame(&mut self, frame: Vec<u8>) -> Result<Bytes, AdaptorError> {
        let text = std::str::from_utf8(&frame).map_err(|_| AdaptorError::ResponseBodyTooLarge)?;
        let data = text
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            return Ok(Bytes::from(frame));
        }
        let original: Value =
            serde_json::from_str(&data).map_err(|_| AdaptorError::ResponseBodyTooLarge)?;
        let body = self
            .hook
            .provider_value(&self.request, self.status, original.clone())
            .await
            .map_err(|_| AdaptorError::ResponseBodyTooLarge)?;
        let body = self
            .hook
            .process_stage(
                &self.request,
                RequestProcessingStage::Egress,
                Some(self.status),
                body,
            )
            .await
            .map_err(|_| AdaptorError::ResponseBodyTooLarge)?;
        let body = self
            .hook
            .process_stage(
                &self.request,
                RequestProcessingStage::Reply,
                Some(self.status),
                body,
            )
            .await
            .map_err(|_| AdaptorError::ResponseBodyTooLarge)?;
        if let Some(usage) = body.get("usage").or_else(|| {
            body.get("response")
                .and_then(|response| response.get("usage"))
        }) {
            self.usage = Some(usage.clone());
        }
        if body == original {
            return Ok(Bytes::from(frame));
        }
        let mut result = String::new();
        for line in text
            .lines()
            .filter(|line| !line.starts_with("data:") && !line.is_empty())
        {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str("data: ");
        result.push_str(
            &serde_json::to_string(&body).map_err(|_| AdaptorError::ResponseBodyTooLarge)?,
        );
        result.push_str("\n\n");
        Ok(Bytes::from(result))
    }
}

impl Drop for ResponseStream {
    fn drop(&mut self) {
        if self.reported {
            return;
        }
        let (hook, request, status, usage) = (
            self.hook.clone(),
            self.request.clone(),
            self.status,
            self.usage.clone(),
        );
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = hook
                    .process_stage(
                        &request,
                        RequestProcessingStage::Report,
                        Some(status),
                        json!({"completed":false,"usage":usage}),
                    )
                    .await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn chat_response_converts_between_full_and_streaming_without_losing_usage() {
        let original = json!({
            "id": "chatcmpl-provider-test",
            "object": "chat.completion",
            "created": 1_700_000_000_i64,
            "model": "provider-model",
            "service_tier": "default",
            "system_fingerprint": "fp_provider_test",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "hello"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 2, "completion_tokens": 1, "total_tokens": 3}
        });
        let chunks = chat_json_to_sse(&original, 32 * 1024 * 1024).unwrap();
        let bytes = Bytes::from(
            chunks
                .into_iter()
                .flat_map(|chunk| chunk.to_vec())
                .collect::<Vec<_>>(),
        );
        let converted: Value =
            serde_json::from_slice(&chat_sse_to_json(bytes).await.unwrap()).unwrap();
        assert_eq!(converted["model"], original["model"]);
        assert_eq!(converted["created"], original["created"]);
        assert_eq!(converted["service_tier"], original["service_tier"]);
        assert_eq!(
            converted["system_fingerprint"],
            original["system_fingerprint"]
        );
        assert_eq!(converted["choices"][0]["message"]["content"], "hello");
        assert_eq!(converted["usage"]["total_tokens"], 3);
    }

    #[test]
    fn chat_stream_conversion_enforces_the_configured_response_limit() {
        let body = json!({
            "id": "chatcmpl-limit-test",
            "object": "chat.completion",
            "created": 1_700_000_000_i64,
            "model": "provider-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "hello"},
                "finish_reason": "stop"
            }]
        });
        assert!(chat_json_to_sse(&body, 1).is_err());
    }

    #[test]
    fn provider_stream_mode_updates_body_and_drops_full_request_stream_options() {
        let mut body = json!({
            "model": "model-a",
            "stream": true,
            "stream_options": {"include_usage": true}
        });
        super::super::set_chat_stream_mode(&mut body, ResponseMode::Full).unwrap();
        assert_eq!(body["stream"], false);
        assert!(body.get("stream_options").is_none());
    }

    #[test]
    fn provider_stream_mode_sets_an_accept_header_matching_the_upstream_format() {
        let mut headers = af_adapter::HeaderMap::new();
        super::super::set_chat_accept_header(&mut headers, ResponseMode::Stream);
        assert_eq!(headers.get("accept").unwrap(), "text/event-stream");
        super::super::set_chat_accept_header(&mut headers, ResponseMode::Full);
        assert_eq!(headers.get("accept").unwrap(), "application/json");
    }
}
