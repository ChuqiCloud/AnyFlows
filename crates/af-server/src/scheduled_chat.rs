use std::fmt;

use af_account::{CredentialDecryptor, DecryptedCredential};
use af_adapter::{
    AdaptorSettings, Credential, HeaderMap, HeaderName, HeaderValue, OpenAiAdaptorSettings,
    PooledClient, RelayContext, get_adaptor,
};
use af_db::CredentialStateRepository;
use af_domain::{AfError, ChannelType, CredentialKind, GatewayPrincipal, GroupId, Protocol};
use af_protocol::CanonicalRequest;
use af_relay::{RelayCandidate, RelayStateMachine, relay_openai_chat_with_report};
use af_scheduler::{IndexedRoutePlan, IndexedWeightedScheduler, IndexedWeightedSchedulerError};

use crate::{
    ChatRoutePlanner, PlannedChatExecution, PlannedChatExecutionFuture,
    credential_feedback::{CredentialAttemptTarget, persist_credential_feedback},
    credential_order::order_credentials,
};

/// 基于内存运行时快照装配生产 OpenAI Chat 候选的服务。
#[derive(Clone)]
pub(crate) struct ScheduledChatService {
    scheduler: IndexedWeightedScheduler,
    decryptor: CredentialDecryptor,
    client: PooledClient,
    credential_states: CredentialStateRepository,
}

impl ScheduledChatService {
    /// 使用已加载渠道索引、凭据解密器和受控 HTTP Client 创建生产路由器。
    #[must_use]
    pub(crate) const fn new(
        scheduler: IndexedWeightedScheduler,
        decryptor: CredentialDecryptor,
        client: PooledClient,
        credential_states: CredentialStateRepository,
    ) -> Self {
        Self {
            scheduler,
            decryptor,
            client,
            credential_states,
        }
    }

    fn build_plan(
        &self,
        route: IndexedRoutePlan,
        request: CanonicalRequest,
        request_id: &str,
    ) -> Result<ScheduledChatExecution, AfError> {
        let model = request.model.clone();
        let target_group_id = route.target_group_id();
        let mut relay_candidates = Vec::with_capacity(
            route
                .candidates()
                .len()
                .min(RelayStateMachine::MAX_CANDIDATES),
        );
        let mut attempt_targets = Vec::with_capacity(relay_candidates.capacity());
        'channels: for candidate in route.candidates() {
            let target = candidate.runtime_target();
            if target
                .credentials()
                .iter()
                .any(af_db::SchedulerRuntimeCredentialRecord::proxy_required)
            {
                // 专属代理仓储尚未落地，绑定代理的凭据绝不能回退共享 Client 或直连。
                return Err(AfError::Internal);
            }
            if target.channel_type() != ChannelType::OpenAi
                || target.protocol() != Protocol::OpenAiChat
            {
                return Err(AfError::Internal);
            }
            let adaptor = get_adaptor(
                target.channel_type(),
                AdaptorSettings::OpenAi(OpenAiAdaptorSettings::new(vec![model.clone()])),
            )
            .map_err(|_| AfError::Internal)?;
            if adaptor.default_protocol() != target.protocol() {
                return Err(AfError::Internal);
            }

            let mut context = RelayContext::new(self.client.clone());
            if let Some(base_url) = target.base_url() {
                context = context
                    .with_base_url(base_url)
                    .map_err(|_| AfError::Internal)?;
            }
            context = context
                .with_request_id(request_id.to_owned())
                .map_err(|_| AfError::Internal)?;
            let headers = header_overrides(target)?;
            for runtime_credential in
                order_credentials(request_id, target.channel_id(), target.credentials())
            {
                if relay_candidates.len() == RelayStateMachine::MAX_CANDIDATES {
                    break 'channels;
                }
                let decrypted = self
                    .decryptor
                    .decrypt_envelope(
                        target.channel_id(),
                        runtime_credential.credential_id(),
                        runtime_credential.credential_kind(),
                        runtime_credential.envelope(),
                    )
                    .map_err(|_| AfError::Internal)?;
                let credential = adaptor_credential(&decrypted)?;
                relay_candidates.push(
                    RelayCandidate::new(adaptor.clone(), context.clone(), credential)
                        .with_header_overrides(headers.clone())
                        .with_channel_group(target.channel_id()),
                );
                attempt_targets.push(CredentialAttemptTarget::new(
                    target.channel_id(),
                    runtime_credential.credential_id(),
                    runtime_credential.credential_kind(),
                )?);
            }
        }
        let machine = RelayStateMachine::new(relay_candidates).map_err(|_| AfError::Internal)?;
        Ok(ScheduledChatExecution {
            target_group_id,
            machine,
            attempt_targets,
            credential_states: self.credential_states.clone(),
            scheduler: self.scheduler.clone(),
            request,
            request_id: request_id.to_owned(),
        })
    }
}

impl ChatRoutePlanner for ScheduledChatService {
    fn plan(
        &self,
        principal: &GatewayPrincipal,
        request: CanonicalRequest,
        request_id: &str,
    ) -> Result<Box<dyn PlannedChatExecution>, AfError> {
        let route = self
            .scheduler
            .route_plan(principal.group_id(), &request.model)
            .map_err(map_scheduler_error)?
            .ok_or_else(|| AfError::from(af_domain::UpstreamError::ModelUnsupported))?;
        self.build_plan(route, request, request_id)
            .map(|plan| Box::new(plan) as Box<dyn PlannedChatExecution>)
    }
}

impl fmt::Debug for ScheduledChatService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScheduledChatService")
            .field("scheduler", &self.scheduler)
            .field("decryptor", &"<受控>")
            .field("client", &"<受控>")
            .field("credential_states", &self.credential_states)
            .finish()
    }
}

struct ScheduledChatExecution {
    target_group_id: GroupId,
    machine: RelayStateMachine,
    attempt_targets: Vec<CredentialAttemptTarget>,
    credential_states: CredentialStateRepository,
    scheduler: IndexedWeightedScheduler,
    request: CanonicalRequest,
    request_id: String,
}

impl PlannedChatExecution for ScheduledChatExecution {
    fn target_group_id(&self) -> GroupId {
        self.target_group_id
    }

    fn execute(self: Box<Self>) -> PlannedChatExecutionFuture {
        let Self {
            target_group_id: _,
            machine,
            attempt_targets,
            credential_states,
            scheduler,
            request,
            request_id,
        } = *self;
        Box::pin(async move {
            let outcome = relay_openai_chat_with_report(&machine, request, &request_id).await;
            let (result, report) = outcome.into_parts();
            persist_credential_feedback(&credential_states, &scheduler, &attempt_targets, &report)
                .await;
            result
        })
    }
}

fn adaptor_credential(credential: &DecryptedCredential) -> Result<Credential, AfError> {
    match credential.kind() {
        CredentialKind::ApiKey => Credential::api_key(credential.expose_secret()),
        CredentialKind::Oauth => Credential::oauth(credential.expose_secret()),
        CredentialKind::SetupToken
        | CredentialKind::Bedrock
        | CredentialKind::ServiceAccount
        | CredentialKind::Upstream => return Err(AfError::Internal),
    }
    .map_err(|_| AfError::Internal)
}

fn header_overrides(target: &af_db::SchedulerRuntimeTargetRecord) -> Result<HeaderMap, AfError> {
    let mut headers = HeaderMap::with_capacity(target.headers().len());
    for header in target.headers() {
        let name =
            HeaderName::from_bytes(header.name().as_bytes()).map_err(|_| AfError::Internal)?;
        let mut value = HeaderValue::from_str(header.value()).map_err(|_| AfError::Internal)?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(headers)
}

fn map_scheduler_error(error: IndexedWeightedSchedulerError) -> AfError {
    match error {
        IndexedWeightedSchedulerError::InvalidModel => AfError::InvalidRequest,
        IndexedWeightedSchedulerError::Cache(_)
        | IndexedWeightedSchedulerError::Selection(_)
        | IndexedWeightedSchedulerError::RetryPlan(_) => AfError::Internal,
        _ => AfError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use af_account::credential_plaintext_aad;
    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use af_db::{
        CredentialStateRepository, DatabaseOptions, EncryptedCredentialEnvelope, MigrationOptions,
        SchedulerRuntimeCredentialRecord, SchedulerRuntimeTargetRecord, connect_and_migrate,
    };
    use af_domain::{ChannelId, Operation, UpstreamError};
    use af_httpclient::{HttpClientConfig, HttpClientPool};
    use af_scheduler::{
        ChannelIndexSource, ChannelIndexSourceFuture, ChannelIndexSourceRecord,
        InMemoryChannelIndex,
    };
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use chacha20poly1305::{
        XChaCha20Poly1305, XNonce,
        aead::{Aead as _, KeyInit as _, Payload},
    };
    use serde_json::json;

    use super::*;

    const CHANNEL_ID: i64 = 41;
    const CREDENTIAL_ID: i64 = 51;
    const KEY_ID: &str = "scheduled-chat-test";

    #[tokio::test]
    async fn plan_pins_the_runtime_target_group_before_execution() {
        let service = service(vec![record(false, valid_envelope())]).await;
        let principal = principal(7);

        let plan = service.plan(&principal, request(), "request-plan").unwrap();

        assert_eq!(plan.target_group_id(), GroupId::new(7).unwrap());
    }

    #[tokio::test]
    async fn plan_expands_multiple_credentials_inside_one_channel_candidate() {
        let credentials = vec![
            SchedulerRuntimeCredentialRecord::with_scheduling(
                CREDENTIAL_ID,
                CredentialKind::ApiKey,
                valid_envelope_for(CREDENTIAL_ID, 0x24),
                false,
                10,
                10,
            )
            .unwrap(),
            SchedulerRuntimeCredentialRecord::with_scheduling(
                CREDENTIAL_ID + 1,
                CredentialKind::ApiKey,
                valid_envelope_for(CREDENTIAL_ID + 1, 0x25),
                false,
                0,
                10,
            )
            .unwrap(),
        ];
        let service = service(vec![record_pool(credentials)]).await;
        let route = service
            .scheduler
            .route_plan(GroupId::new(7).unwrap(), "gpt-scheduled")
            .unwrap()
            .unwrap();

        let execution = service
            .build_plan(route, request(), "request-key-pool")
            .unwrap();

        assert_eq!(execution.attempt_targets.len(), 2);
    }

    #[tokio::test]
    async fn missing_candidates_proxy_binding_and_decryption_fail_closed() {
        let principal = principal(7);
        let empty = service(Vec::new()).await;
        assert_eq!(
            empty.plan(&principal, request(), "request-empty").err(),
            Some(AfError::Upstream(UpstreamError::ModelUnsupported))
        );

        let proxy_required = service(vec![record(true, valid_envelope())]).await;
        assert_eq!(
            proxy_required
                .plan(&principal, request(), "request-proxy")
                .err(),
            Some(AfError::Internal)
        );

        let broken = EncryptedCredentialEnvelope::new(KEY_ID, [0x11; 24], vec![0x22; 32]).unwrap();
        let decrypt_failure = service(vec![record(false, broken)]).await;
        assert_eq!(
            decrypt_failure
                .plan(&principal, request(), "request-decrypt")
                .err(),
            Some(AfError::Internal)
        );
    }

    async fn service(records: Vec<ChannelIndexSourceRecord>) -> ScheduledChatService {
        let index = InMemoryChannelIndex::load(Arc::new(StaticSource { records }))
            .await
            .unwrap();
        ScheduledChatService::new(
            IndexedWeightedScheduler::new(index),
            decryptor(),
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
            CredentialStateRepository::new(
                connect_and_migrate(
                    &DatabaseOptions::new("sqlite::memory:").unwrap(),
                    MigrationOptions::default(),
                )
                .await
                .unwrap(),
            ),
        )
    }

    fn record(
        proxy_required: bool,
        envelope: EncryptedCredentialEnvelope,
    ) -> ChannelIndexSourceRecord {
        let credential = SchedulerRuntimeCredentialRecord::new(
            CREDENTIAL_ID,
            CredentialKind::ApiKey,
            envelope,
            proxy_required,
        )
        .unwrap();
        record_pool(vec![credential])
    }

    fn record_pool(credentials: Vec<SchedulerRuntimeCredentialRecord>) -> ChannelIndexSourceRecord {
        let target = SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(CHANNEL_ID).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://scheduled-chat.example".to_owned()),
            credentials,
            vec![("x-scheduled-test".to_owned(), "test-value".to_owned())],
        )
        .unwrap();
        ChannelIndexSourceRecord::with_runtime_target(
            GroupId::new(7).unwrap(),
            "gpt-scheduled",
            10,
            0,
            Arc::new(target),
        )
        .unwrap()
    }

    fn request() -> CanonicalRequest {
        CanonicalRequest::new(
            Operation::Chat,
            "gpt-scheduled".to_owned(),
            Vec::new(),
            false,
        )
    }

    fn principal(group_id: i64) -> GatewayPrincipal {
        GatewayPrincipal::new(
            af_domain::TokenId::new(1).unwrap(),
            af_domain::UserId::new(2).unwrap(),
            GroupId::new(group_id).unwrap(),
        )
    }

    fn decryptor() -> CredentialDecryptor {
        let settings = serde_json::from_value::<CredentialEncryptionSettings>(json!({
            "key_id": KEY_ID,
            "key": URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES]),
        }))
        .unwrap();
        CredentialDecryptor::new(&settings).unwrap()
    }

    fn valid_envelope() -> EncryptedCredentialEnvelope {
        valid_envelope_for(CREDENTIAL_ID, 0x24)
    }

    fn valid_envelope_for(credential_id: i64, marker: u8) -> EncryptedCredentialEnvelope {
        let key = [0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let nonce = [marker; 24];
        let cipher = XChaCha20Poly1305::new_from_slice(&key).unwrap();
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: br#"{"kind":"api_key","api_key":"scheduled-secret"}"#,
                    aad: &credential_plaintext_aad(
                        ChannelId::new(CHANNEL_ID).unwrap(),
                        credential_id,
                        CredentialKind::ApiKey,
                    ),
                },
            )
            .unwrap();
        EncryptedCredentialEnvelope::new(KEY_ID, nonce, ciphertext).unwrap()
    }

    #[derive(Clone)]
    struct StaticSource {
        records: Vec<ChannelIndexSourceRecord>,
    }

    impl ChannelIndexSource for StaticSource {
        fn load<'a>(&'a self) -> ChannelIndexSourceFuture<'a> {
            let records = self.records.clone();
            Box::pin(async move { Ok(records) })
        }
    }
}
