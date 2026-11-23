use super::*;
use af_adapter::{TransportDispatchOutcome, UpstreamRequest};
use af_httpclient::{WebSocketConnection, WebSocketFrame, WebSocketTransportError};
use af_plugin_contract::{
    MAX_PROVIDER_SESSION_WEBSOCKET_AUTH_REQUEST_BYTES, MAX_PROVIDER_SESSION_WEBSOCKET_BATCH_FRAMES,
    MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES, PluginCredentialResult,
    PluginCredentialWebSocketFrame, PluginProviderOperation, PluginProviderResult,
    PluginProviderSessionAction,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;

const MAX_PROVIDER_SESSION_STEPS: u8 = 128;
const PROVIDER_SESSION_CLEANUP_TIMEOUT: Duration = Duration::from_millis(120);
// Keeps even worst-case JSON escaping of text frames below the 256 KiB action budget.
const MAX_PROVIDER_SESSION_WEBSOCKET_BATCH_BYTES: usize =
    MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES;

impl DatabasePluginProviderRelayHook {
    pub(super) async fn execute_provider_session(
        &self,
        request: &ProviderRelayRequest,
        session_context: ProviderRelaySessionContext<'_>,
    ) -> Result<TransportDispatchOutcome, af_domain::UpstreamError> {
        let session = request
            .session
            .as_ref()
            .ok_or(af_domain::UpstreamError::ProtocolError)?;
        let binding = request
            .binding
            .as_ref()
            .ok_or(af_domain::UpstreamError::ProtocolError)?;
        let mut cleanup = ProviderSessionCleanupGuard {
            hook: self.clone(),
            binding: binding.clone(),
            model: request.model.clone(),
            session_id: session.session_id.clone(),
            state: session.state.clone(),
            websocket: None,
            armed: true,
        };
        let timeout_ms = session.timeout_ms.clamp(1_000, 360_000);
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        let initial_action =
            serde_json::from_value::<PluginProviderSessionAction>(session.action.clone())
                .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        if initial_action.validate().is_err() {
            return Err(af_domain::UpstreamError::ProtocolError);
        }
        let (result, state) = self
            .run_provider_session_steps(
                request,
                &session_context,
                initial_action,
                deadline,
                &mut cleanup,
            )
            .await;
        let reason = if result.is_ok() {
            "completed"
        } else {
            "failed"
        };
        cleanup.state = state;
        cleanup.close(request, &session_context, reason).await;
        result
    }

    async fn run_provider_session_steps(
        &self,
        request: &ProviderRelayRequest,
        session_context: &ProviderRelaySessionContext<'_>,
        initial_action: PluginProviderSessionAction,
        deadline: Instant,
        cleanup: &mut ProviderSessionCleanupGuard,
    ) -> (
        Result<TransportDispatchOutcome, af_domain::UpstreamError>,
        Value,
    ) {
        let session = request.session.as_ref().expect("session checked by caller");
        let binding = request.binding.as_ref().expect("binding checked by caller");
        let target = ProviderTarget::from(binding);
        let mut action = initial_action;
        let mut state = session.state.clone();
        let mut last_response = None;
        for step in 0..=MAX_PROVIDER_SESSION_STEPS {
            if Instant::now() >= deadline {
                return (Err(provider_session_timeout()), state);
            }
            if let PluginProviderSessionAction::Complete { response } = &action {
                let output = match self.session_response(request, response.clone()) {
                    Ok(output) => output,
                    Err(error) => return (Err(error), state),
                };
                return (Ok(TransportDispatchOutcome::direct(output)), state);
            }
            if matches!(&action, PluginProviderSessionAction::Abort { .. })
                || step == MAX_PROVIDER_SESSION_STEPS
            {
                return (Err(af_domain::UpstreamError::ProtocolError), state);
            }
            let response = match action {
                PluginProviderSessionAction::Wait { delay_ms } => {
                    if tokio::time::timeout_at(
                        deadline,
                        tokio::time::sleep(Duration::from_millis(u64::from(delay_ms))),
                    )
                    .await
                    .is_err()
                    {
                        return (Err(provider_session_timeout()), state);
                    }
                    last_response
                        .clone()
                        .unwrap_or_else(|| json!({"kind": "waited", "delay_ms": delay_ms}))
                }
                action => match tokio::time::timeout_at(
                    deadline,
                    self.execute_provider_session_action(
                        request,
                        session_context,
                        action,
                        deadline,
                        &mut cleanup.websocket,
                    ),
                )
                .await
                {
                    Ok(Ok(response)) => response,
                    Ok(Err(error)) => return (Err(error), state),
                    Err(_) => return (Err(provider_session_timeout()), state),
                },
            };
            last_response = Some(response.clone());
            let invocation = self.invocation(&target, &request.model, "provider-session");
            let result = match tokio::time::timeout_at(
                deadline,
                self.runtime.invoke_provider(
                    &self.repository,
                    &binding.plugin_id,
                    binding.generation,
                    &invocation,
                    binding.capability_version,
                    PluginProviderOperation::SessionStep {
                        session_id: session.session_id.clone(),
                        state: state.clone(),
                        response: response.clone(),
                    },
                ),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => return (Err(af_domain::UpstreamError::ProtocolError), state),
                Err(_) => return (Err(provider_session_timeout()), state),
            };
            let PluginProviderResult::Session {
                session_id,
                state: next_state,
                action: next_action,
            } = result
            else {
                return (Err(af_domain::UpstreamError::ProtocolError), state);
            };
            if session_id != session.session_id {
                return (Err(af_domain::UpstreamError::ProtocolError), state);
            }
            if next_action.validate().is_err() {
                return (Err(af_domain::UpstreamError::ProtocolError), state);
            }
            state = next_state;
            cleanup.state = state.clone();
            action = next_action;
        }
        (Err(af_domain::UpstreamError::ProtocolError), state)
    }

    async fn execute_provider_session_action(
        &self,
        request: &ProviderRelayRequest,
        session_context: &ProviderRelaySessionContext<'_>,
        action: PluginProviderSessionAction,
        deadline: Instant,
        websocket: &mut Option<ActiveProviderWebSocket>,
    ) -> Result<Value, af_domain::UpstreamError> {
        match action {
            PluginProviderSessionAction::Request {
                method,
                path,
                endpoint_id,
                headers,
                body,
                stream: false,
            } => {
                let (current, endpoint) = if let Some(endpoint_id) = endpoint_id.as_deref() {
                    let session = request
                        .session
                        .as_ref()
                        .ok_or(af_domain::UpstreamError::ProtocolError)?;
                    let endpoint = session
                        .endpoints
                        .get(endpoint_id)
                        .ok_or(af_domain::UpstreamError::ProtocolError)?;
                    (
                        self.build_endpoint_session_projection(
                            request,
                            endpoint_id,
                            method,
                            &path,
                            headers,
                            body,
                        )?,
                        Some(endpoint),
                    )
                } else {
                    (
                        self.build_session_projection(request, method, path, headers, body)?,
                        None,
                    )
                };
                let current = self.sign_request(current, endpoint_id.as_deref()).await?;
                let upstream = if let Some(endpoint) = endpoint {
                    let upstream = self
                        .prepare_endpoint_session_upstream(&current, session_context)
                        .await?;
                    validate_prepared_provider_endpoint(endpoint, &upstream)?;
                    upstream
                } else {
                    self.prepare_session_upstream(&current, session_context)
                        .await?
                };
                self.send_session_request(upstream, session_context, deadline)
                    .await
            }
            PluginProviderSessionAction::Request { .. } => {
                Err(af_domain::UpstreamError::ProtocolError)
            }
            PluginProviderSessionAction::Wait { delay_ms } => {
                if tokio::time::timeout_at(
                    deadline,
                    tokio::time::sleep(Duration::from_millis(u64::from(delay_ms))),
                )
                .await
                .is_err()
                {
                    return Err(provider_session_timeout());
                }
                Ok(json!({"kind": "waited", "delay_ms": delay_ms}))
            }
            PluginProviderSessionAction::WebSocketConnect {
                endpoint_id,
                path,
                headers,
            } => {
                if let Some(mut previous) = websocket.take() {
                    let _ = tokio::time::timeout(
                        PROVIDER_SESSION_CLEANUP_TIMEOUT,
                        previous.connection.close(),
                    )
                    .await;
                }
                let session = request
                    .session
                    .as_ref()
                    .ok_or(af_domain::UpstreamError::ProtocolError)?;
                let endpoint = session
                    .endpoints
                    .get(&endpoint_id)
                    .ok_or(af_domain::UpstreamError::ProtocolError)?;
                if !endpoint.websocket_enabled || !endpoint_path_allowed(endpoint, &path) {
                    return Err(af_domain::UpstreamError::ProtocolError);
                }
                let target = build_provider_websocket_target(endpoint, &path)?;
                let mut projected = request.clone();
                projected.method = Method::GET;
                projected.target = target;
                projected.body = None;
                projected.headers = af_adapter::HeaderMap::new();
                apply_provider_headers(&mut projected.headers, headers, true)?;
                let projected = self.sign_request(projected, Some(&endpoint_id)).await?;
                let mut http_target = Url::parse(&projected.target)
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                if http_target.scheme() != "wss" {
                    return Err(af_domain::UpstreamError::ProtocolError);
                }
                http_target
                    .set_scheme("https")
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                let prepared = UpstreamRequest::new(
                    Method::GET,
                    http_target.to_string(),
                    projected.headers,
                    None,
                )
                .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                let prepared = session_context.prepare_endpoint_request(prepared).await?;
                let mut target = Url::parse(prepared.target())
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                let mut expected_origin = Url::parse(&endpoint.url)
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                expected_origin
                    .set_scheme("https")
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                if target.scheme() != "https"
                    || !same_websocket_origin(&target, &expected_origin)
                    || !endpoint_path_allowed(endpoint, target.path())
                {
                    return Err(af_domain::UpstreamError::ProtocolError);
                }
                target
                    .set_scheme("wss")
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                let connection = match tokio::time::timeout_at(
                    deadline,
                    session_context.connect_websocket(
                        target.as_str(),
                        prepared.headers().clone(),
                        MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES,
                    ),
                )
                .await
                {
                    Ok(Ok(connection)) => connection,
                    Ok(Err(error)) => return websocket_error_event(error, "connect"),
                    Err(_) => return Err(provider_session_timeout()),
                };
                *websocket = Some(ActiveProviderWebSocket {
                    connection,
                    endpoint_id: endpoint_id.clone(),
                    path: target.path().to_owned(),
                });
                websocket_action_result(json!({
                    "type": "connected",
                    "endpoint_id": endpoint_id
                }))
            }
            PluginProviderSessionAction::WebSocketSendText { text } => {
                let connection = websocket
                    .as_mut()
                    .map(|active| &mut active.connection)
                    .ok_or(af_domain::UpstreamError::ProtocolError)?;
                let payload = Bytes::from(text);
                if payload.len() > MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES {
                    return Err(af_domain::UpstreamError::ProtocolError);
                }
                let send_result =
                    tokio::time::timeout_at(deadline, connection.send_text(payload.clone())).await;
                match send_result {
                    Err(_) => {
                        *websocket = None;
                        Err(provider_session_timeout())
                    }
                    Ok(Err(error)) => {
                        *websocket = None;
                        websocket_error_event(error, "send")
                    }
                    Ok(Ok(())) => websocket_action_result(json!({
                        "type": "sent",
                        "frame_type": "text",
                        "bytes": payload.len()
                    })),
                }
            }
            PluginProviderSessionAction::WebSocketSendBinary { data_base64 } => {
                let payload = STANDARD
                    .decode(data_base64)
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                if payload.len() > MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES {
                    return Err(af_domain::UpstreamError::ProtocolError);
                }
                let connection = websocket
                    .as_mut()
                    .map(|active| &mut active.connection)
                    .ok_or(af_domain::UpstreamError::ProtocolError)?;
                let send_result =
                    tokio::time::timeout_at(deadline, connection.send_binary(Bytes::from(payload)))
                        .await;
                match send_result {
                    Err(_) => {
                        *websocket = None;
                        Err(provider_session_timeout())
                    }
                    Ok(Err(error)) => {
                        *websocket = None;
                        websocket_error_event(error, "send")
                    }
                    Ok(Ok(())) => websocket_action_result(json!({
                        "type": "sent",
                        "frame_type": "binary"
                    })),
                }
            }
            PluginProviderSessionAction::WebSocketReceive { timeout_ms } => {
                if websocket.is_none() {
                    return Err(af_domain::UpstreamError::ProtocolError);
                }
                let action_timeout = Duration::from_millis(u64::from(timeout_ms));
                let remaining = deadline.saturating_duration_since(Instant::now());
                let session_deadline_first = remaining <= action_timeout;
                let receive_timeout = action_timeout.min(remaining);
                let received = {
                    let connection = websocket
                        .as_mut()
                        .map(|active| &mut active.connection)
                        .ok_or(af_domain::UpstreamError::ProtocolError)?;
                    connection.receive_with_timeout(receive_timeout).await
                };
                match received {
                    Err(WebSocketTransportError::ReceiveTimeout) if !session_deadline_first => {
                        websocket_action_result(json!({"type": "timeout"}))
                    }
                    Err(WebSocketTransportError::ReceiveTimeout) => Err(provider_session_timeout()),
                    Err(error) => {
                        *websocket = None;
                        websocket_error_event(error, "receive")
                    }
                    Ok(Some(WebSocketFrame::Text(bytes))) => {
                        let text = String::from_utf8(bytes.to_vec())
                            .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                        websocket_text_message(text)
                    }
                    Ok(Some(WebSocketFrame::Binary(bytes))) => websocket_action_result(json!({
                        "type": "message",
                        "frame_type": "binary",
                        "data_base64": STANDARD.encode(bytes)
                    })),
                    Ok(Some(WebSocketFrame::Ping)) => {
                        websocket_action_result(json!({"type": "ping"}))
                    }
                    Ok(Some(WebSocketFrame::Pong)) => {
                        websocket_action_result(json!({"type": "pong"}))
                    }
                    Ok(None) => {
                        *websocket = None;
                        websocket_action_result(json!({"type": "closed"}))
                    }
                }
            }
            PluginProviderSessionAction::WebSocketReceiveBatch {
                timeout_ms,
                max_messages,
            } => {
                if websocket.is_none() {
                    return Err(af_domain::UpstreamError::ProtocolError);
                }
                let action_timeout = Duration::from_millis(u64::from(timeout_ms));
                let remaining = deadline.saturating_duration_since(Instant::now());
                let session_deadline_first = remaining <= action_timeout;
                let receive_timeout = action_timeout.min(remaining);
                let batch = {
                    let connection = websocket
                        .as_mut()
                        .map(|active| &mut active.connection)
                        .ok_or(af_domain::UpstreamError::ProtocolError)?;
                    connection
                        .receive_batch_with_timeout(
                            receive_timeout,
                            usize::from(
                                max_messages.min(MAX_PROVIDER_SESSION_WEBSOCKET_BATCH_FRAMES),
                            ),
                            MAX_PROVIDER_SESSION_WEBSOCKET_BATCH_BYTES,
                        )
                        .await
                };
                match batch {
                    Err(error) => {
                        *websocket = None;
                        websocket_error_event(error, "receive")
                    }
                    Ok(batch)
                        if session_deadline_first && batch.timed_out && batch.frames.is_empty() =>
                    {
                        Err(provider_session_timeout())
                    }
                    Ok(batch) => {
                        if batch.closed {
                            *websocket = None;
                        }
                        let frames = batch
                            .frames
                            .into_iter()
                            .map(websocket_batch_frame_event)
                            .collect::<Result<Vec<_>, _>>()?;
                        websocket_action_result(json!({
                            "type": "batch",
                            "frames": frames,
                            "closed": batch.closed,
                            "timed_out": batch.timed_out,
                            "more_pending": batch.more_pending
                        }))
                    }
                }
            }
            PluginProviderSessionAction::WebSocketAuthenticate { request: auth } => {
                let Some(active) = websocket.as_ref() else {
                    return Err(af_domain::UpstreamError::ProtocolError);
                };
                let (Some(lifecycle), Some(channel), Some(credential)) = (
                    self.lifecycle.as_ref(),
                    request.channel_id,
                    request.credential_id,
                ) else {
                    return Err(af_domain::UpstreamError::ProtocolError);
                };
                let mut auth_request = auth
                    .as_object()
                    .cloned()
                    .ok_or(af_domain::UpstreamError::ProtocolError)?;
                auth_request.insert(
                    "endpoint_id".to_owned(),
                    Value::String(active.endpoint_id.clone()),
                );
                auth_request.insert("path".to_owned(), Value::String(active.path.clone()));
                if serde_json::to_vec(&auth_request).map_or(true, |bytes| {
                    bytes.len() > MAX_PROVIDER_SESSION_WEBSOCKET_AUTH_REQUEST_BYTES
                }) {
                    return Err(af_domain::UpstreamError::ProtocolError);
                }
                let result = lifecycle
                    .authenticate_bound_plugin_websocket(
                        channel.get(),
                        credential,
                        Value::Object(auth_request),
                    )
                    .await
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?
                    .ok_or(af_domain::UpstreamError::ProtocolError)?;
                let PluginCredentialResult::WebSocketAuth { frames } = result else {
                    return Err(af_domain::UpstreamError::ProtocolError);
                };
                let mut sent = 0_u8;
                let mut sent_bytes = 0_usize;
                for frame in frames {
                    let connection = websocket
                        .as_mut()
                        .map(|active| &mut active.connection)
                        .ok_or(af_domain::UpstreamError::ProtocolError)?;
                    let result = match frame {
                        PluginCredentialWebSocketFrame::Text { text } => {
                            sent_bytes = sent_bytes.saturating_add(text.len());
                            tokio::time::timeout_at(
                                deadline,
                                connection.send_text(Bytes::from(text)),
                            )
                            .await
                        }
                        PluginCredentialWebSocketFrame::Binary { data_base64 } => {
                            let payload = STANDARD
                                .decode(data_base64)
                                .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                            sent_bytes = sent_bytes.saturating_add(payload.len());
                            tokio::time::timeout_at(
                                deadline,
                                connection.send_binary(Bytes::from(payload)),
                            )
                            .await
                        }
                    };
                    match result {
                        Err(_) => {
                            *websocket = None;
                            return Err(provider_session_timeout());
                        }
                        Ok(Err(error)) => {
                            *websocket = None;
                            return websocket_error_event(error, "send");
                        }
                        Ok(Ok(())) => {}
                    }
                    sent = sent.saturating_add(1);
                }
                websocket_action_result(json!({
                    "type": "authenticated",
                    "frames_sent": sent,
                    "bytes_sent": sent_bytes
                }))
            }
            PluginProviderSessionAction::WebSocketClose => {
                if let Some(mut connection) = websocket.take() {
                    return match tokio::time::timeout_at(deadline, connection.connection.close())
                        .await
                    {
                        Err(_) => Err(provider_session_timeout()),
                        Ok(Err(error)) => websocket_error_event(error, "close"),
                        Ok(Ok(())) => websocket_action_result(json!({"type": "closed"})),
                    };
                }
                websocket_action_result(json!({"type": "closed"}))
            }
            PluginProviderSessionAction::Complete { .. }
            | PluginProviderSessionAction::Abort { .. } => {
                Err(af_domain::UpstreamError::ProtocolError)
            }
        }
    }

    fn build_session_projection(
        &self,
        request: &ProviderRelayRequest,
        method: String,
        path: String,
        headers: std::collections::BTreeMap<String, String>,
        body: Value,
    ) -> Result<ProviderRelayRequest, af_domain::UpstreamError> {
        let mut url =
            Url::parse(&request.target).map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        let (path_only, query) = path
            .split_once('?')
            .map_or((path.as_str(), None), |(path, query)| (path, Some(query)));
        url.set_path(path_only);
        url.set_query(query);
        let method = Method::from_bytes(method.as_bytes())
            .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        let mut projected = request.clone();
        projected.method = method;
        projected.target = url.to_string();
        projected.body = Some(Bytes::from(
            serde_json::to_vec(&body).map_err(|_| af_domain::UpstreamError::ProtocolError)?,
        ));
        projected.headers = request.headers.clone();
        for (name, value) in headers {
            if protected_provider_header(&name) {
                continue;
            }
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
            let value = HeaderValue::from_str(&value)
                .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
            projected.headers.insert(name, value);
        }
        projected.upstream_response_mode = af_adapter::ResponseMode::Full;
        Ok(projected)
    }

    fn build_endpoint_session_projection(
        &self,
        request: &ProviderRelayRequest,
        endpoint_id: &str,
        method: String,
        path: &str,
        headers: std::collections::BTreeMap<String, String>,
        body: Value,
    ) -> Result<ProviderRelayRequest, af_domain::UpstreamError> {
        let session = request
            .session
            .as_ref()
            .ok_or(af_domain::UpstreamError::ProtocolError)?;
        let endpoint = session
            .endpoints
            .get(endpoint_id)
            .ok_or(af_domain::UpstreamError::ProtocolError)?;
        if !endpoint
            .allowed_methods
            .iter()
            .any(|allowed| allowed == &method)
            || !endpoint_path_allowed(endpoint, path)
        {
            return Err(af_domain::UpstreamError::ProtocolError);
        }
        let method = Method::from_bytes(method.as_bytes())
            .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        let target = build_provider_http_target(endpoint, path)?;
        let mut projected = request.clone();
        projected.method = method;
        projected.target = target;
        projected.body = Some(Bytes::from(
            serde_json::to_vec(&body).map_err(|_| af_domain::UpstreamError::ProtocolError)?,
        ));
        // A different endpoint must never inherit the selected channel's secret headers.
        projected.headers = af_adapter::HeaderMap::new();
        apply_provider_headers(&mut projected.headers, headers, true)?;
        if !projected.headers.contains_key("content-type") {
            projected.headers.insert(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/json"),
            );
        }
        projected.upstream_response_mode = af_adapter::ResponseMode::Full;
        Ok(projected)
    }

    async fn prepare_session_upstream(
        &self,
        request: &ProviderRelayRequest,
        session_context: &ProviderRelaySessionContext<'_>,
    ) -> Result<UpstreamRequest, af_domain::UpstreamError> {
        let upstream = UpstreamRequest::new(
            request.method.clone(),
            request.target.clone(),
            request.headers.clone(),
            request.body.clone(),
        )
        .and_then(|request| request.with_response_body_limit(MAX_PROVIDER_SESSION_VALUE_BYTES))
        .map(|request| request.with_response_mode(af_adapter::ResponseMode::Full))
        .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        session_context.prepare_request(upstream).await
    }

    async fn prepare_endpoint_session_upstream(
        &self,
        request: &ProviderRelayRequest,
        session_context: &ProviderRelaySessionContext<'_>,
    ) -> Result<UpstreamRequest, af_domain::UpstreamError> {
        let upstream = UpstreamRequest::new(
            request.method.clone(),
            request.target.clone(),
            request.headers.clone(),
            request.body.clone(),
        )
        .and_then(|request| request.with_response_body_limit(MAX_PROVIDER_SESSION_VALUE_BYTES))
        .map(|request| request.with_response_mode(af_adapter::ResponseMode::Full))
        .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        session_context.prepare_endpoint_request(upstream).await
    }

    fn session_response(
        &self,
        request: &ProviderRelayRequest,
        body: Value,
    ) -> Result<UpstreamResponse, af_domain::UpstreamError> {
        let body =
            serde_json::to_vec(&body).map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        let mut headers = af_adapter::HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        UpstreamResponse::full_with_limit(
            StatusCode::OK,
            headers,
            Bytes::from(body),
            request.response_body_limit,
        )
        .map_err(|_| af_domain::UpstreamError::ProtocolError)
    }

    async fn send_session_request(
        &self,
        request: UpstreamRequest,
        session_context: &ProviderRelaySessionContext<'_>,
        deadline: Instant,
    ) -> Result<Value, af_domain::UpstreamError> {
        let outcome =
            match tokio::time::timeout_at(deadline, session_context.send_http_request(request))
                .await
            {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(_)) => return Err(af_domain::UpstreamError::ProtocolError),
                Err(_) => return Err(provider_session_timeout()),
            };
        let response = outcome.into_response();
        let status = response.status().as_u16();
        let body = match tokio::time::timeout_at(
            deadline,
            response
                .into_body()
                .into_bytes_with_limit(MAX_PROVIDER_SESSION_VALUE_BYTES),
        )
        .await
        {
            Ok(Ok(body)) => body,
            Ok(Err(_)) => return Err(af_domain::UpstreamError::ProtocolError),
            Err(_) => return Err(provider_session_timeout()),
        };
        let body = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| {
            let events = String::from_utf8_lossy(&body)
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
                .collect::<Vec<_>>();
            if events.is_empty() {
                Value::String(String::from_utf8_lossy(&body).into_owned())
            } else {
                serde_json::json!({"events": events})
            }
        });
        let projection = serde_json::json!({"status": status, "body": body});
        if serde_json::to_vec(&projection)
            .map_or(true, |bytes| bytes.len() > MAX_PROVIDER_SESSION_VALUE_BYTES)
        {
            return Err(af_domain::UpstreamError::ProtocolError);
        }
        Ok(projection)
    }
}

struct ProviderSessionCleanupGuard {
    hook: DatabasePluginProviderRelayHook,
    binding: ProviderRelayBinding,
    model: String,
    session_id: String,
    state: Value,
    websocket: Option<ActiveProviderWebSocket>,
    armed: bool,
}

struct ActiveProviderWebSocket {
    connection: WebSocketConnection,
    endpoint_id: String,
    path: String,
}

impl ProviderSessionCleanupGuard {
    async fn close(
        &mut self,
        request: &ProviderRelayRequest,
        session_context: &ProviderRelaySessionContext<'_>,
        reason: &str,
    ) {
        if let Some(mut connection) = self.websocket.take() {
            let _ = tokio::time::timeout(
                PROVIDER_SESSION_CLEANUP_TIMEOUT,
                connection.connection.close(),
            )
            .await;
        }
        let target = ProviderTarget::from(&self.binding);
        let invocation = self
            .hook
            .invocation(&target, &self.model, "provider-session-close");
        let result = tokio::time::timeout(
            Duration::from_millis(120),
            self.hook.runtime.invoke_provider(
                &self.hook.repository,
                &self.binding.plugin_id,
                self.binding.generation,
                &invocation,
                self.binding.capability_version,
                PluginProviderOperation::SessionClose {
                    session_id: self.session_id.clone(),
                    state: self.state.clone(),
                    reason: reason.to_owned(),
                },
            ),
        )
        .await;
        if let Ok(Ok(PluginProviderResult::Request {
            endpoint_id,
            method,
            path,
            headers,
            body,
        })) = result
        {
            let projected = if let Some(endpoint_id) = endpoint_id.as_deref() {
                self.hook.build_endpoint_session_projection(
                    request,
                    endpoint_id,
                    method,
                    &path,
                    headers,
                    body,
                )
            } else {
                self.hook
                    .build_session_projection(request, method, path, headers, body)
            };
            if let Ok(projected) = projected
                && let Ok(projected) = self
                    .hook
                    .sign_request(projected, endpoint_id.as_deref())
                    .await
            {
                let upstream = if let Some(endpoint_id) = endpoint_id.as_deref() {
                    let endpoint = request
                        .session
                        .as_ref()
                        .and_then(|session| session.endpoints.get(endpoint_id));
                    match (
                        endpoint,
                        self.hook
                            .prepare_endpoint_session_upstream(&projected, session_context)
                            .await,
                    ) {
                        (Some(endpoint), Ok(upstream))
                            if validate_prepared_provider_endpoint(endpoint, &upstream).is_ok() =>
                        {
                            Ok(upstream)
                        }
                        _ => Err(af_domain::UpstreamError::ProtocolError),
                    }
                } else {
                    self.hook
                        .prepare_session_upstream(&projected, session_context)
                        .await
                };
                if let Ok(upstream) = upstream {
                    let _ = tokio::time::timeout(
                        Duration::from_millis(120),
                        self.hook.send_session_request(
                            upstream,
                            session_context,
                            Instant::now() + Duration::from_millis(120),
                        ),
                    )
                    .await;
                }
            }
        }
        self.armed = false;
    }
}

impl Drop for ProviderSessionCleanupGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let hook = self.hook.clone();
        let binding = self.binding.clone();
        let model = self.model.clone();
        let session_id = self.session_id.clone();
        let state = self.state.clone();
        let websocket = self.websocket.take();
        handle.spawn(async move {
            if let Some(mut connection) = websocket {
                let _ = tokio::time::timeout(
                    PROVIDER_SESSION_CLEANUP_TIMEOUT,
                    connection.connection.close(),
                )
                .await;
            }
            let target = ProviderTarget::from(&binding);
            let invocation = hook.invocation(&target, &model, "provider-session-cancel");
            let _ = tokio::time::timeout(
                Duration::from_millis(120),
                hook.runtime.invoke_provider(
                    &hook.repository,
                    &binding.plugin_id,
                    binding.generation,
                    &invocation,
                    binding.capability_version,
                    PluginProviderOperation::SessionClose {
                        session_id,
                        state,
                        reason: "cancelled".to_owned(),
                    },
                ),
            )
            .await;
        });
    }
}

fn provider_session_timeout() -> af_domain::UpstreamError {
    af_domain::UpstreamError::network(af_domain::NetworkFailureKind::RequestTimeout)
}

fn build_provider_websocket_target(
    endpoint: &af_relay::ProviderRelayEndpoint,
    path: &str,
) -> Result<String, af_domain::UpstreamError> {
    let mut url = Url::parse(&endpoint.url).map_err(|_| af_domain::UpstreamError::ProtocolError)?;
    if !matches!(url.scheme(), "https" | "wss")
        || url.host_str().is_none()
        || matches!(url.host(), Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
        || url.port() == Some(0)
        || !endpoint.websocket_enabled
        || !endpoint_path_allowed(endpoint, path)
    {
        return Err(af_domain::UpstreamError::ProtocolError);
    }
    url.set_scheme("wss")
        .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
    let (path_only, query) = path
        .split_once('?')
        .map_or((path, None), |(path, query)| (path, Some(query)));
    url.set_path(path_only);
    url.set_query(query);
    Ok(url.to_string())
}

fn build_provider_http_target(
    endpoint: &af_relay::ProviderRelayEndpoint,
    path: &str,
) -> Result<String, af_domain::UpstreamError> {
    let mut url = Url::parse(&endpoint.url).map_err(|_| af_domain::UpstreamError::ProtocolError)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || matches!(url.host(), Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
        || url.port() == Some(0)
        || !endpoint_path_allowed(endpoint, path)
    {
        return Err(af_domain::UpstreamError::ProtocolError);
    }
    let (path_only, query) = path
        .split_once('?')
        .map_or((path, None), |(path, query)| (path, Some(query)));
    url.set_path(path_only);
    url.set_query(query);
    Ok(url.to_string())
}

fn endpoint_path_allowed(endpoint: &af_relay::ProviderRelayEndpoint, request_path: &str) -> bool {
    let (path, query) = request_path
        .split_once('?')
        .map_or((request_path, None), |(path, query)| (path, Some(query)));
    if path.is_empty()
        || path.len() > 2_048
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['#', '\\', '%'])
        || path.chars().any(char::is_control)
        || path.split('/').any(|segment| matches!(segment, "." | ".."))
        || query.is_some_and(|query| {
            query
                .chars()
                .any(|character| character == '#' || character == '\\' || character.is_control())
        })
    {
        return false;
    }
    endpoint.allowed_paths.iter().any(|prefix| {
        path == prefix
            || path.strip_prefix(prefix).is_some_and(|suffix| {
                prefix == "/" || prefix.ends_with('/') || suffix.starts_with('/')
            })
    })
}

fn validate_prepared_provider_endpoint(
    endpoint: &af_relay::ProviderRelayEndpoint,
    request: &UpstreamRequest,
) -> Result<(), af_domain::UpstreamError> {
    let target =
        Url::parse(request.target()).map_err(|_| af_domain::UpstreamError::ProtocolError)?;
    let expected =
        Url::parse(&endpoint.url).map_err(|_| af_domain::UpstreamError::ProtocolError)?;
    if target.scheme() != "https"
        || !same_websocket_origin(&target, &expected)
        || !endpoint
            .allowed_methods
            .iter()
            .any(|allowed| allowed == request.method().as_str())
        || !endpoint_path_allowed(endpoint, target.path())
    {
        return Err(af_domain::UpstreamError::ProtocolError);
    }
    Ok(())
}

fn same_websocket_origin(left: &Url, right: &Url) -> bool {
    left.host_str()
        .zip(right.host_str())
        .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        && left.port_or_known_default() == right.port_or_known_default()
}

fn apply_provider_headers(
    headers: &mut af_adapter::HeaderMap,
    overrides: std::collections::BTreeMap<String, String>,
    allow_endpoint_auth: bool,
) -> Result<(), af_domain::UpstreamError> {
    for (name, value) in overrides {
        if protected_provider_header(&name)
            && !(allow_endpoint_auth
                && matches!(
                    name.to_ascii_lowercase().as_str(),
                    "authorization" | "cookie"
                ))
        {
            continue;
        }
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        let value =
            HeaderValue::from_str(&value).map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        headers.insert(name, value);
    }
    Ok(())
}

fn websocket_action_result(event: Value) -> Result<Value, af_domain::UpstreamError> {
    let response = json!({"kind": "websocket", "event": event});
    if serde_json::to_vec(&response)
        .map_or(true, |bytes| bytes.len() > MAX_PROVIDER_SESSION_VALUE_BYTES)
    {
        Err(af_domain::UpstreamError::ProtocolError)
    } else {
        Ok(response)
    }
}

fn websocket_error_event(
    error: WebSocketTransportError,
    phase: &str,
) -> Result<Value, af_domain::UpstreamError> {
    let code = match error {
        WebSocketTransportError::InvalidRequestTarget => "invalid_target",
        WebSocketTransportError::InvalidHandshakeHeader => "invalid_handshake_headers",
        WebSocketTransportError::TargetAddressBlocked
        | WebSocketTransportError::RemoteDnsDenied => "target_blocked",
        WebSocketTransportError::TargetResolution => "target_resolution_failed",
        WebSocketTransportError::ConnectTimeout => "connect_timeout",
        WebSocketTransportError::Connect => "connect_failed",
        WebSocketTransportError::HandshakeTimeout => "handshake_timeout",
        WebSocketTransportError::Handshake => "handshake_failed",
        WebSocketTransportError::SendTimeout => "send_timeout",
        WebSocketTransportError::Send => "send_failed",
        WebSocketTransportError::ReceiveTimeout => "receive_timeout",
        WebSocketTransportError::Receive => "receive_failed",
        WebSocketTransportError::MessageTooLarge => "message_too_large",
        WebSocketTransportError::Closed => "connection_closed",
        _ => "unknown_error",
    };
    let mut event = json!({
        "type": "error",
        "phase": phase,
        "code": code
    });
    if phase == "connect" {
        let retryable = error.is_connect()
            && !matches!(
                error,
                WebSocketTransportError::TargetAddressBlocked
                    | WebSocketTransportError::RemoteDnsDenied
            );
        event["retryable"] = Value::Bool(retryable);
    } else {
        event["outcome_unknown"] = Value::Bool(true);
    }
    websocket_action_result(event)
}

fn websocket_text_message(text: String) -> Result<Value, af_domain::UpstreamError> {
    let plain = json!({
        "type": "message",
        "frame_type": "text",
        "data": text.clone()
    });
    if let Ok(response) = websocket_action_result(plain) {
        return Ok(response);
    }
    websocket_action_result(json!({
        "type": "message",
        "frame_type": "text",
        "encoding": "base64",
        "data_base64": STANDARD.encode(text.as_bytes())
    }))
}

fn websocket_batch_frame_event(frame: WebSocketFrame) -> Result<Value, af_domain::UpstreamError> {
    let event = match frame {
        WebSocketFrame::Text(bytes) => json!({
            "type": "message",
            "frame_type": "text",
            "encoding": "base64",
            "data_base64": STANDARD.encode(bytes)
        }),
        WebSocketFrame::Binary(bytes) => json!({
            "type": "message",
            "frame_type": "binary",
            "encoding": "base64",
            "data_base64": STANDARD.encode(bytes)
        }),
        WebSocketFrame::Ping => json!({"type": "ping"}),
        WebSocketFrame::Pong => json!({"type": "pong"}),
    };
    if serde_json::to_vec(&event).map_or(true, |bytes| {
        bytes.len() > MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES * 2
    }) {
        Err(af_domain::UpstreamError::ProtocolError)
    } else {
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_websocket_targets_keep_the_admin_origin_and_relative_path() {
        assert_eq!(
            build_provider_websocket_target(
                &af_relay::ProviderRelayEndpoint {
                    url: "wss://realtime.example.invalid/".to_owned(),
                    allowed_paths: vec!["/".to_owned()],
                    allowed_methods: vec![],
                    websocket_enabled: true,
                },
                "/v1/events?cursor=next",
            )
            .unwrap(),
            "wss://realtime.example.invalid/v1/events?cursor=next"
        );
        for (endpoint, path) in [
            ("ws://realtime.example.invalid", "/events"),
            ("wss://127.0.0.1", "/events"),
            ("wss://user:pass@realtime.example.invalid", "/events"),
            ("wss://realtime.example.invalid/private", "/events"),
            ("wss://realtime.example.invalid", "//other.example/events"),
            ("wss://realtime.example.invalid", "/events#fragment"),
        ] {
            let endpoint = af_relay::ProviderRelayEndpoint {
                url: endpoint.to_owned(),
                allowed_paths: vec!["/".to_owned()],
                allowed_methods: vec![],
                websocket_enabled: true,
            };
            assert!(build_provider_websocket_target(&endpoint, path).is_err());
        }
    }

    #[test]
    fn provider_endpoint_paths_match_authorized_prefix_boundaries() {
        let endpoint = af_relay::ProviderRelayEndpoint {
            url: "https://api.example.invalid".to_owned(),
            allowed_paths: vec!["/v1".to_owned(), "/oauth/".to_owned()],
            allowed_methods: vec!["GET".to_owned()],
            websocket_enabled: false,
        };
        assert!(endpoint_path_allowed(&endpoint, "/v1"));
        assert!(endpoint_path_allowed(&endpoint, "/v1/models?active=true"));
        assert!(endpoint_path_allowed(&endpoint, "/oauth/callback"));
        assert!(!endpoint_path_allowed(&endpoint, "/v10/models"));
        assert!(!endpoint_path_allowed(&endpoint, "/oauth"));
        for path in ["/v1/../admin", "/v1/%2e%2e/admin", "//other.example/v1"] {
            assert!(!endpoint_path_allowed(&endpoint, path));
        }
    }

    #[test]
    fn provider_http_targets_use_the_granted_origin_and_relative_path() {
        let endpoint = af_relay::ProviderRelayEndpoint {
            url: "https://api.example.invalid:8443".to_owned(),
            allowed_paths: vec!["/v1".to_owned()],
            allowed_methods: vec!["POST".to_owned()],
            websocket_enabled: false,
        };
        assert_eq!(
            build_provider_http_target(&endpoint, "/v1/chat?mode=fast").unwrap(),
            "https://api.example.invalid:8443/v1/chat?mode=fast"
        );
        assert!(build_provider_http_target(&endpoint, "/admin").is_err());
        assert!(build_provider_http_target(&endpoint, "//other.example/v1").is_err());
    }

    #[test]
    fn provider_websocket_action_projection_respects_the_session_budget() {
        assert!(websocket_action_result(json!({"type": "ping"})).is_ok());
        assert!(websocket_text_message("x".repeat(128 * 1024)).is_ok());
        assert!(websocket_text_message("\0".repeat(128 * 1024)).is_ok());
        assert!(
            websocket_action_result(json!({
                "type": "message",
                "data": "x".repeat(MAX_PROVIDER_SESSION_VALUE_BYTES)
            }))
            .is_err()
        );
    }

    #[test]
    fn provider_websocket_failure_events_distinguish_fallback_from_unknown_outcomes() {
        let connect = websocket_error_event(WebSocketTransportError::Connect, "connect").unwrap();
        assert_eq!(connect["event"]["retryable"], true);
        assert!(connect["event"].get("outcome_unknown").is_none());

        let send = websocket_error_event(WebSocketTransportError::SendTimeout, "send").unwrap();
        assert_eq!(send["event"]["outcome_unknown"], true);
        assert!(send["event"].get("retryable").is_none());

        let blocked =
            websocket_error_event(WebSocketTransportError::TargetAddressBlocked, "connect")
                .unwrap();
        assert_eq!(blocked["event"]["retryable"], false);
    }
}
