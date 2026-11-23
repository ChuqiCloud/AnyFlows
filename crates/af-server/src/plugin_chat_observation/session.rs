use super::*;
use af_adapter::{TransportDispatchOutcome, UpstreamRequest};
use af_httpclient::{WebSocketConnection, WebSocketFrame, WebSocketTransportError};
use af_plugin_contract::{
    MAX_PROVIDER_SESSION_VALUE_BYTES, MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES,
    PluginProviderOperation, PluginProviderResult, PluginProviderSessionAction,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;

const MAX_PROVIDER_SESSION_STEPS: u8 = 128;
const PROVIDER_SESSION_CLEANUP_TIMEOUT: Duration = Duration::from_millis(120);

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
        websocket: &mut Option<WebSocketConnection>,
    ) -> Result<Value, af_domain::UpstreamError> {
        match action {
            PluginProviderSessionAction::Request {
                method,
                path,
                headers,
                body,
                stream: false,
            } => {
                let current =
                    self.build_session_projection(request, method, path, headers, body)?;
                let current = self.sign_request(current).await?;
                let upstream = self
                    .prepare_session_upstream(&current, session_context)
                    .await?;
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
                    let _ =
                        tokio::time::timeout(PROVIDER_SESSION_CLEANUP_TIMEOUT, previous.close())
                            .await;
                }
                let session = request
                    .session
                    .as_ref()
                    .ok_or(af_domain::UpstreamError::ProtocolError)?;
                let base = session
                    .websocket_endpoints
                    .get(&endpoint_id)
                    .ok_or(af_domain::UpstreamError::ProtocolError)?;
                let target = build_provider_websocket_target(base, &path)?;
                let mut projected = request.clone();
                projected.method = Method::GET;
                projected.target = target;
                projected.body = None;
                projected.headers = request.headers.clone();
                apply_provider_headers(&mut projected.headers, headers)?;
                let projected = self.sign_request(projected).await?;
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
                let prepared = session_context.prepare_request(prepared).await?;
                let mut target = Url::parse(prepared.target())
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                if target.scheme() != "https" || !same_websocket_origin(&target, &http_target) {
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
                *websocket = Some(connection);
                websocket_action_result(json!({
                    "type": "connected",
                    "endpoint_id": endpoint_id
                }))
            }
            PluginProviderSessionAction::WebSocketSendText { text } => {
                let connection = websocket
                    .as_mut()
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
            PluginProviderSessionAction::WebSocketClose => {
                if let Some(mut connection) = websocket.take() {
                    return match tokio::time::timeout_at(deadline, connection.close()).await {
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
    websocket: Option<WebSocketConnection>,
    armed: bool,
}

impl ProviderSessionCleanupGuard {
    async fn close(
        &mut self,
        request: &ProviderRelayRequest,
        session_context: &ProviderRelaySessionContext<'_>,
        reason: &str,
    ) {
        if let Some(mut connection) = self.websocket.take() {
            let _ =
                tokio::time::timeout(PROVIDER_SESSION_CLEANUP_TIMEOUT, connection.close()).await;
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
            method,
            path,
            headers,
            body,
        })) = result
            && let Ok(projected) = self
                .hook
                .build_session_projection(request, method, path, headers, body)
            && let Ok(projected) = self.hook.sign_request(projected).await
            && let Ok(upstream) = self
                .hook
                .prepare_session_upstream(&projected, session_context)
                .await
        {
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
                let _ = tokio::time::timeout(PROVIDER_SESSION_CLEANUP_TIMEOUT, connection.close())
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
    endpoint: &str,
    path: &str,
) -> Result<String, af_domain::UpstreamError> {
    let mut url = Url::parse(endpoint).map_err(|_| af_domain::UpstreamError::ProtocolError)?;
    if url.scheme() != "wss"
        || url.host_str().is_none()
        || matches!(url.host(), Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
        || url.port() == Some(0)
        || path.is_empty()
        || path.len() > 2_048
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.bytes().any(|byte| matches!(byte, b'#' | b'\\'))
        || path.chars().any(char::is_control)
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

fn same_websocket_origin(left: &Url, right: &Url) -> bool {
    left.host_str()
        .zip(right.host_str())
        .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        && left.port_or_known_default() == right.port_or_known_default()
}

fn apply_provider_headers(
    headers: &mut af_adapter::HeaderMap,
    overrides: std::collections::BTreeMap<String, String>,
) -> Result<(), af_domain::UpstreamError> {
    for (name, value) in overrides {
        if protected_provider_header(&name) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_websocket_targets_keep_the_admin_origin_and_relative_path() {
        assert_eq!(
            build_provider_websocket_target(
                "wss://realtime.example.invalid/",
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
            assert!(build_provider_websocket_target(endpoint, path).is_err());
        }
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
