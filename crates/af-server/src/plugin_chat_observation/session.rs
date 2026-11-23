use super::*;
use af_adapter::{TransportDispatchOutcome, UpstreamRequest};
use af_plugin_contract::{
    MAX_PROVIDER_SESSION_VALUE_BYTES, PluginProviderOperation, PluginProviderResult,
    PluginProviderSessionAction,
};
use serde_json::Value;

const MAX_PROVIDER_SESSION_STEPS: u8 = 16;
const PROVIDER_SESSION_TIMEOUT: Duration = Duration::from_secs(30);

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
            armed: true,
        };
        let deadline = Instant::now() + PROVIDER_SESSION_TIMEOUT;
        let initial_action =
            serde_json::from_value::<PluginProviderSessionAction>(session.action.clone())
                .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
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
        let (mut response, mut state) = match initial_action {
            PluginProviderSessionAction::Request {
                method,
                path,
                headers,
                body,
                stream,
            } if !stream => {
                let current =
                    match self.build_session_projection(request, method, path, headers, body) {
                        Ok(current) => current,
                        Err(error) => return (Err(error), session.state.clone()),
                    };
                let current = match self.sign_request(current).await {
                    Ok(current) => current,
                    Err(error) => return (Err(error), session.state.clone()),
                };
                let upstream = match self
                    .prepare_session_upstream(&current, session_context)
                    .await
                {
                    Ok(upstream) => upstream,
                    Err(error) => return (Err(error), session.state.clone()),
                };
                let response = match self
                    .send_session_request(upstream, session_context, deadline)
                    .await
                {
                    Ok(response) => response,
                    Err(error) => return (Err(error), session.state.clone()),
                };
                (response, session.state.clone())
            }
            PluginProviderSessionAction::Complete { response } => {
                let output = match self.session_response(request, response) {
                    Ok(output) => output,
                    Err(error) => return (Err(error), session.state.clone()),
                };
                return (
                    Ok(TransportDispatchOutcome::direct(output)),
                    session.state.clone(),
                );
            }
            _ => {
                return (
                    Err(af_domain::UpstreamError::ProtocolError),
                    session.state.clone(),
                );
            }
        };

        for step in 0..MAX_PROVIDER_SESSION_STEPS {
            if Instant::now() >= deadline {
                return (Err(provider_session_timeout()), state);
            }
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
                action,
            } = result
            else {
                return (Err(af_domain::UpstreamError::ProtocolError), state);
            };
            if session_id != session.session_id {
                return (Err(af_domain::UpstreamError::ProtocolError), state);
            }
            state = next_state;
            cleanup.state = state.clone();
            match action {
                PluginProviderSessionAction::Complete { response } => {
                    let output = match self.session_response(request, response) {
                        Ok(output) => output,
                        Err(error) => return (Err(error), state),
                    };
                    return (Ok(TransportDispatchOutcome::direct(output)), state);
                }
                PluginProviderSessionAction::Abort { .. } => {
                    return (Err(af_domain::UpstreamError::ProtocolError), state);
                }
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
                }
                PluginProviderSessionAction::Request {
                    method,
                    path,
                    headers,
                    body,
                    stream,
                } if !stream => {
                    let current =
                        match self.build_session_projection(request, method, path, headers, body) {
                            Ok(current) => current,
                            Err(error) => return (Err(error), state),
                        };
                    let current = match self.sign_request(current).await {
                        Ok(current) => current,
                        Err(error) => return (Err(error), state),
                    };
                    let upstream = match self
                        .prepare_session_upstream(&current, session_context)
                        .await
                    {
                        Ok(upstream) => upstream,
                        Err(error) => return (Err(error), state),
                    };
                    response = match self
                        .send_session_request(upstream, session_context, deadline)
                        .await
                    {
                        Ok(response) => response,
                        Err(error) => return (Err(error), state),
                    };
                }
                _ => return (Err(af_domain::UpstreamError::ProtocolError), state),
            }
            if step + 1 == MAX_PROVIDER_SESSION_STEPS {
                return (Err(af_domain::UpstreamError::ProtocolError), state);
            }
        }
        (Err(af_domain::UpstreamError::ProtocolError), state)
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
        Ok(serde_json::json!({"status": status, "body": body}))
    }
}

struct ProviderSessionCleanupGuard {
    hook: DatabasePluginProviderRelayHook,
    binding: ProviderRelayBinding,
    model: String,
    session_id: String,
    state: Value,
    armed: bool,
}

impl ProviderSessionCleanupGuard {
    async fn close(
        &mut self,
        request: &ProviderRelayRequest,
        session_context: &ProviderRelaySessionContext<'_>,
        reason: &str,
    ) {
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
        handle.spawn(async move {
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
