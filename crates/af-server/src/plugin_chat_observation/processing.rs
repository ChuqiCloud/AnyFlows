//! Transport-independent response stages. WebSocket responses are normalized
//! to the same SSE stream by the transport dispatcher before reaching here.
use super::*;
use af_adapter::{AdaptorError, UpstreamBody, UpstreamBodyStream};
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
        response: UpstreamResponse,
    ) -> Result<UpstreamResponse, af_domain::UpstreamError> {
        if request.binding.is_none() && self.processor.targets_for_request().await.is_empty() {
            return Ok(response);
        }
        let status = response.status();
        let mut headers = response.headers().clone();
        if let Some(bytes) = response.full_body() {
            let Ok(body) = serde_json::from_slice(bytes) else {
                return Ok(response);
            };
            let body = self.provider_value(request, status.as_u16(), body).await?;
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
            return UpstreamResponse::full_with_limit(
                status,
                headers,
                Bytes::from(serde_json::to_vec(&body).map_err(|_| protocol_error())?),
                request.response_body_limit,
            )
            .map_err(|_| protocol_error());
        }
        if !headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"))
        {
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
        UpstreamResponse::stream(status, headers, stream).map_err(|_| protocol_error())
    }
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
