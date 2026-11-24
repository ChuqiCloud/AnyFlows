//! Durable plugin data plane. Only the host owns identities, vault and quota.
mod tasks;

use super::*;
use af_account::{PlainSystemSecret, SystemSecretCipher};
use af_db::{EncryptedCredentialEnvelope, PluginHostRecord, PluginHostRepository, QuotaRepository};
use af_plugin_contract::{PLUGIN_CREDENTIAL_LIFECYCLE_CAPABILITY, PluginCredentialResult};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
pub use tasks::PluginNativeTaskProtocol;

#[derive(Clone)]
pub(super) struct DataPlane {
    records: PluginHostRepository,
    cipher: SystemSecretCipher,
    quota: Arc<QuotaRepository>,
    native_tasks: Arc<tokio::sync::Mutex<Option<tasks::NativeTaskCache>>>,
}

fn persistence_error(error: af_db::PluginHostError) -> AdminPluginLifecycleError {
    match error {
        af_db::PluginHostError::Conflict => AdminPluginLifecycleError::Conflict,
        af_db::PluginHostError::Invalid => AdminPluginLifecycleError::InvalidInput,
        af_db::PluginHostError::Unavailable => AdminPluginLifecycleError::Internal,
    }
}

impl DatabaseAdminPluginLifecycleService {
    #[must_use]
    pub fn with_data_plane(
        mut self,
        records: PluginHostRepository,
        cipher: SystemSecretCipher,
        quota: Arc<QuotaRepository>,
    ) -> Self {
        self.data_plane = Some(DataPlane {
            records,
            cipher,
            quota,
            native_tasks: Arc::default(),
        });
        self
    }

    fn data_plane(&self) -> Result<&DataPlane, AdminPluginLifecycleError> {
        self.data_plane
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)
    }

    /// A binding is explicit and exclusive for one platform credential.
    pub async fn bind_plugin_credential(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        channel_id: i64,
        credential_id: i64,
        version: u16,
        request_id: &str,
    ) -> Result<Value, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let plane = self.data_plane()?;
        if !plane
            .records
            .credential_belongs_to(channel_id, credential_id)
            .await
            .map_err(persistence_error)?
        {
            return Err(AdminPluginLifecycleError::NotFound);
        }
        let (deployment, _) = self
            .prepare_plugin_operation(
                plugin_id,
                PLUGIN_CREDENTIAL_LIFECYCLE_CAPABILITY,
                version,
                request_id,
                1024 * 1024,
            )
            .await?;
        let id = format!("{channel_id}:{credential_id}");
        let record = plane.records.insert(PluginHostRecord {
            kind: "credential".into(), id, plugin_id: plugin_id.into(), owner_id: principal.user_id().get(),
            revision: 1, state: "bound".into(), due_at: None, updated_at: current_timestamp()?,
            payload: json!({"channelId":channel_id,"credentialId":credential_id,"generation":deployment.generation,"version":version}),
        }).await.map_err(persistence_error)?;
        if record.plugin_id != plugin_id
            || record.payload["generation"] != deployment.generation
            || record.state == "revoked"
        {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        Ok(
            json!({"channelId":channel_id,"credentialId":credential_id,"pluginId":plugin_id,"generation":deployment.generation,"state":record.state}),
        )
    }

    fn seal(&self, id: &str, value: &Value) -> Result<Value, AdminPluginLifecycleError> {
        let secret = PlainSystemSecret::new(
            serde_json::to_string(value).map_err(|_| AdminPluginLifecycleError::InvalidInput)?,
        )
        .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let aad = format!("anyflows:plugin-credential:v1:{id}");
        let envelope = self
            .data_plane()?
            .cipher
            .encrypt_with_aad(aad.as_bytes(), &secret)
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        Ok(
            json!({"keyId":envelope.key_id(),"nonce":URL_SAFE_NO_PAD.encode(envelope.nonce()),"ciphertext":URL_SAFE_NO_PAD.encode(envelope.ciphertext())}),
        )
    }

    fn unseal(&self, id: &str, value: &Value) -> Result<Value, AdminPluginLifecycleError> {
        let invalid = || AdminPluginLifecycleError::Internal;
        let nonce = URL_SAFE_NO_PAD
            .decode(value["nonce"].as_str().ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
        let envelope = EncryptedCredentialEnvelope::new(
            value["keyId"].as_str().ok_or_else(invalid)?.to_owned(),
            nonce.try_into().map_err(|_| invalid())?,
            URL_SAFE_NO_PAD
                .decode(value["ciphertext"].as_str().ok_or_else(invalid)?)
                .map_err(|_| invalid())?,
        )
        .map_err(|_| invalid())?;
        let aad = format!("anyflows:plugin-credential:v1:{id}");
        let secret = self
            .data_plane()?
            .cipher
            .decrypt_with_aad(aad.as_bytes(), &envelope)
            .map_err(|_| invalid())?;
        serde_json::from_str(secret.expose_secret()).map_err(|_| invalid())
    }

    /// Login callbacks claim a one-use state before exchanging an authorization code.
    pub async fn credential_operation(
        &self,
        plugin_id: &str,
        channel_id: i64,
        credential_id: i64,
        operation: PluginCredentialOperation,
        callback_state: Option<&str>,
        request_id: &str,
    ) -> Result<PluginCredentialResult, AdminPluginLifecycleError> {
        if matches!(operation, PluginCredentialOperation::Bound { .. }) {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        operation
            .validate()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let plane = self.data_plane()?;
        let id = format!("{channel_id}:{credential_id}");
        let mut record = plane
            .records
            .get("credential", &id)
            .await
            .map_err(persistence_error)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if record.plugin_id != plugin_id
            || record.state == "revoked"
            || !plane
                .records
                .credential_belongs_to(channel_id, credential_id)
                .await
                .map_err(persistence_error)?
        {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let now = current_timestamp()?;
        if record.due_at.is_some_and(|due| due > now) && record.state == "busy" {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        if let PluginCredentialOperation::Login {
            authorization_code: Some(_),
            redirect_uri,
        } = &operation
        {
            let state = callback_state.ok_or(AdminPluginLifecycleError::Forbidden)?;
            let digest = format!("{:x}", Sha256::digest(state.as_bytes()));
            if record.payload["loginStateHash"].as_str() != Some(&digest)
                || record.payload["loginExpires"]
                    .as_i64()
                    .is_none_or(|expires| expires < now)
                || record.payload["redirectUri"].as_str() != redirect_uri.as_deref()
            {
                return Err(AdminPluginLifecycleError::Forbidden);
            }
            record
                .payload
                .as_object_mut()
                .ok_or(AdminPluginLifecycleError::Internal)?
                .remove("loginStateHash");
        }
        let generation = record.payload["generation"]
            .as_i64()
            .ok_or(AdminPluginLifecycleError::Internal)?;
        let version = record.payload["version"]
            .as_u64()
            .and_then(|value| u16::try_from(value).ok())
            .ok_or(AdminPluginLifecycleError::Internal)?;
        let (deployment, mut invocation) = self
            .prepare_plugin_operation(
                plugin_id,
                PLUGIN_CREDENTIAL_LIFECYCLE_CAPABILITY,
                version,
                request_id,
                1024 * 1024,
            )
            .await?;
        if deployment.generation != generation {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        invocation = af_plugin_runtime::CapabilityExecutionContext::new(
            format!("credential:{id}"),
            invocation.metadata().clone(),
            crate::plugin_lifecycle::deployment_public_configuration(&deployment),
            1024 * 1024,
        )
        .map_err(|_| AdminPluginLifecycleError::Internal)?;
        let material = record
            .payload
            .get("vault")
            .map(|vault| self.unseal(&id, vault))
            .transpose()?;
        record.state = "busy".into();
        record.due_at = Some(now + 60);
        record.updated_at = now;
        record = plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        let revoke = matches!(operation, PluginCredentialOperation::Revoke);
        let login_begin = matches!(
            operation,
            PluginCredentialOperation::Login {
                authorization_code: None,
                ..
            }
        );
        let redirect_uri = match &operation {
            PluginCredentialOperation::Login { redirect_uri, .. } => redirect_uri.clone(),
            _ => None,
        };
        let result = self
            .sidecar_runtime
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?
            .invoke_credential(
                &self.repository,
                plugin_id,
                generation,
                &invocation,
                version,
                PluginCredentialOperation::Bound {
                    channel_id,
                    credential_id,
                    material,
                    operation: Box::new(operation),
                },
            )
            .await;
        // No external write is replayed automatically after an ambiguous result.
        let mut result = match result {
            Ok(result) => result,
            Err(error) => {
                record.state = "unknown".into();
                record.due_at = None;
                plane
                    .records
                    .replace(&record)
                    .await
                    .map_err(persistence_error)?;
                return Err(error);
            }
        };
        if let PluginCredentialResult::Session {
            material,
            result: inner,
        } = result
        {
            record.payload["vault"] = self.seal(&id, &material)?;
            result = *inner;
        }
        record.state = if revoke { "revoked" } else { "active" }.into();
        record.due_at = record.payload["refreshAt"].as_i64();
        if let PluginCredentialResult::Refreshed {
            expires_at_unix_ms: Some(expires),
        } = &result
        {
            let expires = i64::try_from(expires / 1000)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            record.due_at = Some((expires - 60).max(now + 30));
            record.payload["refreshAt"] = json!(record.due_at);
        }
        if login_begin
            && let PluginCredentialResult::Login {
                authorization_url: Some(url),
                state,
                complete: false,
            } = &mut result
        {
            let nonce = uuid::Uuid::new_v4().simple().to_string();
            let mut parsed =
                url::Url::parse(url).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            if parsed.scheme() != "https"
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err(AdminPluginLifecycleError::InvalidInput);
            }
            let pairs: Vec<(String, String)> = parsed
                .query_pairs()
                .filter(|(key, _)| key != "state")
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect();
            parsed.set_query(None);
            parsed
                .query_pairs_mut()
                .extend_pairs(pairs)
                .append_pair("state", &nonce);
            *url = parsed.to_string();
            *state = Some(nonce.clone());
            record.payload["loginStateHash"] =
                json!(format!("{:x}", Sha256::digest(nonce.as_bytes())));
            record.payload["loginExpires"] = json!(now + 600);
            record.payload["redirectUri"] = json!(redirect_uri);
            record.state = "login_pending".into();
        }
        if revoke {
            record
                .payload
                .as_object_mut()
                .ok_or(AdminPluginLifecycleError::Internal)?
                .remove("vault");
        }
        record.updated_at = now;
        plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        Ok(result)
    }

    pub async fn sign_bound_plugin_request(
        &self,
        channel: i64,
        credential: i64,
        endpoint_id: Option<&str>,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Option<PluginCredentialResult>, AdminPluginLifecycleError> {
        let Some(record) = self
            .data_plane()?
            .records
            .get("credential", &format!("{channel}:{credential}"))
            .await
            .map_err(persistence_error)?
        else {
            return Ok(None);
        };
        if record.state != "active" {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        self.credential_operation(
            &record.plugin_id,
            channel,
            credential,
            PluginCredentialOperation::Sign {
                method: method.into(),
                path: path.into(),
                endpoint_id: endpoint_id.map(str::to_owned),
                body,
            },
            None,
            "plugin-relay-sign",
        )
        .await
        .map(Some)
    }

    /// Ask the bound credential adapter for private WebSocket auth frames.
    /// The result is consumed by Relay and must never be returned to a Provider plugin.
    pub async fn authenticate_bound_plugin_websocket(
        &self,
        channel: i64,
        credential: i64,
        request: Value,
    ) -> Result<Option<PluginCredentialResult>, AdminPluginLifecycleError> {
        let Some(record) = self
            .data_plane()?
            .records
            .get("credential", &format!("{channel}:{credential}"))
            .await
            .map_err(persistence_error)?
        else {
            return Ok(None);
        };
        if record.state != "active" {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        self.credential_operation(
            &record.plugin_id,
            channel,
            credential,
            PluginCredentialOperation::WebSocketAuth { request },
            None,
            "plugin-relay-websocket-auth",
        )
        .await
        .map(Some)
    }

    pub async fn refresh_plugin_credentials(&self) {
        let Ok(plane) = self.data_plane() else {
            return;
        };
        let Ok(now) = current_timestamp() else {
            return;
        };
        let Ok(records) = plane
            .records
            .list("credential", None, None, Some(now), 32)
            .await
        else {
            return;
        };
        for record in records {
            if record.state != "active" {
                continue;
            }
            if let (Some(channel), Some(credential)) = (
                record.payload["channelId"].as_i64(),
                record.payload["credentialId"].as_i64(),
            ) {
                let _ = self
                    .credential_operation(
                        &record.plugin_id,
                        channel,
                        credential,
                        PluginCredentialOperation::Refresh,
                        None,
                        "plugin-credential-refresh",
                    )
                    .await;
            }
        }
    }
}
