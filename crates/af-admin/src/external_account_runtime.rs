use std::{sync::Arc, time::Duration};

use af_account::{PlainSystemSecret, SystemSecretCipher};
use af_db::{
    EncryptedCredentialEnvelope, ExternalAccountBalanceSnapshotRecord,
    ExternalAccountBalanceSnapshotWrite, ExternalAccountConnectionCreate,
    ExternalAccountConnectionRecord, ExternalAccountConnectionRepository,
    ExternalAccountInitialBalanceSnapshot, ExternalAccountRepositoryError, ExternalBalanceStatus,
};
use af_domain::UserId;
use af_httpclient::HttpClientProvider;
use af_plugin_contract::{CapabilityId, CapabilityRequirement, ContractVersion};
use af_plugin_runtime::{
    ConnectorInvocation, ConnectorOperationError, ConnectorRegistry, ConnectorRegistryError,
    ExternalPlatformCredential,
};
use serde_json::json;
use time::OffsetDateTime;

use crate::{
    ExternalAccount, ExternalAccountBalancePage, ExternalAccountBalanceSnapshot,
    ExternalAccountBalanceStatus, ExternalAccountCredential, ExternalAccountError,
    ExternalAccountFuture, ExternalAccountLinkCommand, SessionPrincipal,
    SupportedExternalConnector, UserExternalAccountService,
};

const EXTERNAL_PROFILE_CAPABILITY: &str = "external.account.profile.read";
const EXTERNAL_BALANCE_CAPABILITY: &str = "external.account.balance.read";
const CONTRACT_VERSION: ContractVersion = ContractVersion::new(1, 0);

/// Production host service for user-owned external account links.
pub struct DatabaseUserExternalAccountService {
    repository: ExternalAccountConnectionRepository,
    cipher: SystemSecretCipher,
    connectors: Arc<ConnectorRegistry>,
    clients: HttpClientProvider,
}

impl DatabaseUserExternalAccountService {
    pub fn new(
        repository: ExternalAccountConnectionRepository,
        cipher: SystemSecretCipher,
        clients: HttpClientProvider,
    ) -> Result<Self, ExternalAccountError> {
        let connectors = ConnectorRegistry::builtin().map_err(map_registry_error)?;
        Ok(Self {
            repository,
            cipher,
            connectors: Arc::new(connectors),
            clients,
        })
    }

    fn connector_context(
        &self,
        connection_id: &str,
        connector_id: &str,
        base_url: &str,
        secret: ExternalPlatformCredential,
    ) -> Result<
        (
            af_plugin_runtime::NegotiatedConnector,
            ConnectorInvocation,
            ExternalPlatformCredential,
        ),
        ExternalAccountError,
    > {
        let plugin_id = af_plugin_contract::PluginId::new(connector_id.to_owned())
            .map_err(|_| ExternalAccountError::Unsupported)?;
        let manifest = self
            .connectors
            .manifest(&plugin_id)
            .map_err(map_registry_error)?;
        let requirements = [
            CapabilityRequirement::new(
                CapabilityId::new(EXTERNAL_PROFILE_CAPABILITY)
                    .expect("static capability identifier is valid"),
                1,
                1,
                true,
            )
            .map_err(|_| ExternalAccountError::Internal)?,
            CapabilityRequirement::new(
                CapabilityId::new(EXTERNAL_BALANCE_CAPABILITY)
                    .expect("static capability identifier is valid"),
                1,
                1,
                true,
            )
            .map_err(|_| ExternalAccountError::Internal)?,
        ];
        let negotiated = self
            .connectors
            .negotiate(&plugin_id, CONTRACT_VERSION, &requirements)
            .map_err(map_registry_error)?;
        let config = json!({ "baseUrl": base_url });
        self.connectors
            .validate_config(&plugin_id, &config)
            .map_err(map_registry_error)?;
        let timeout = Duration::from_millis(u64::from(manifest.limits.request_timeout_ms));
        let client = self
            .clients
            .get(Some(timeout))
            .map_err(|_| ExternalAccountError::Unavailable)?;
        let request_id = uuid::Uuid::new_v4().to_string();
        let deadline_unix_ms =
            now_unix_millis().saturating_add(u64::from(manifest.limits.request_timeout_ms));
        let invocation = ConnectorInvocation::new(
            connection_id.to_owned(),
            af_plugin_contract::InvocationMetadata {
                request_id,
                deadline_unix_ms,
            },
            config,
            base_url.to_owned(),
            manifest.limits.max_response_bytes,
            client,
        )
        .map_err(|_| ExternalAccountError::InvalidInput)?;
        Ok((negotiated, invocation, secret))
    }

    fn encrypt_secret(
        &self,
        user_id: UserId,
        connector_id: &str,
        base_url: &str,
        external_subject: &str,
        secret: &ExternalAccountCredential,
    ) -> Result<EncryptedCredentialEnvelope, ExternalAccountError> {
        let issuer = canonical_issuer(base_url)?;
        let plaintext = PlainSystemSecret::new(secret.expose().to_owned())
            .map_err(|_| ExternalAccountError::InvalidInput)?;
        self.cipher
            .encrypt_with_aad(
                &external_account_aad(user_id, connector_id, &issuer, external_subject),
                &plaintext,
            )
            .map_err(|_| ExternalAccountError::Internal)
    }

    fn decrypt_secret(
        &self,
        record: &af_db::ExternalAccountSecretRecord,
    ) -> Result<ExternalPlatformCredential, ExternalAccountError> {
        let plaintext = self
            .cipher
            .decrypt_with_aad(
                &external_account_aad(
                    record.connection.user_id,
                    &record.connection.connector_id,
                    &record.connection.base_url,
                    &record.connection.external_subject,
                ),
                &record.envelope,
            )
            .map_err(|_| ExternalAccountError::Internal)?;
        Ok(ExternalPlatformCredential::from_zeroizing(
            plaintext.into_zeroizing(),
        ))
    }

    async fn latest_snapshot(
        &self,
        user_id: UserId,
        connection_id: i64,
    ) -> Result<Option<ExternalAccountBalanceSnapshot>, ExternalAccountError> {
        Ok(self
            .repository
            .list_snapshots(user_id, connection_id, 1)
            .await
            .map_err(map_repository_error)?
            .into_iter()
            .next()
            .map(snapshot_dto))
    }

    async fn record_balance(
        &self,
        user_id: UserId,
        connection_id: i64,
        result: Result<af_plugin_contract::ExternalBalanceSnapshot, ConnectorOperationError>,
    ) -> Result<ExternalAccountBalanceSnapshot, ExternalAccountError> {
        let (amount, unit, status) = match result {
            Ok(snapshot) => (
                Some(snapshot.amount),
                Some(snapshot.unit),
                ExternalBalanceStatus::Available,
            ),
            Err(ConnectorOperationError::Connector(error))
                if matches!(
                    error.code,
                    af_plugin_contract::ConnectorErrorCode::AuthenticationRequired
                        | af_plugin_contract::ConnectorErrorCode::Forbidden
                        | af_plugin_contract::ConnectorErrorCode::Conflict
                ) =>
            {
                (None, None, ExternalBalanceStatus::AuthenticationRequired)
            }
            Err(_) => (None, None, ExternalBalanceStatus::Unavailable),
        };
        let record = self
            .repository
            .insert_snapshot(ExternalAccountBalanceSnapshotWrite {
                user_id,
                connection_id,
                amount,
                unit,
                status,
                observed_at: i64::try_from(now_unix_millis())
                    .map_err(|_| ExternalAccountError::Internal)?,
            })
            .await
            .map_err(map_repository_error)?;
        Ok(snapshot_dto(record))
    }

    async fn refresh_record(
        &self,
        user_id: UserId,
        connection: ExternalAccountConnectionRecord,
    ) -> Result<ExternalAccount, ExternalAccountError> {
        let secret_record = self
            .repository
            .secret(user_id, connection.id)
            .await
            .map_err(map_repository_error)?;
        let credential = self.decrypt_secret(&secret_record)?;
        let (negotiated, invocation, credential) = self.connector_context(
            &connection.id.to_string(),
            &connection.connector_id,
            &connection.base_url,
            credential,
        )?;
        let profile = self
            .connectors
            .read_external_account_profile(
                &negotiated,
                &invocation,
                &connection.external_subject,
                &credential,
            )
            .await;
        let display_name = profile
            .as_ref()
            .ok()
            .map(|profile| profile.display_name.clone());
        let balance = if profile.is_ok() {
            self.connectors
                .read_external_balance(
                    &negotiated,
                    &invocation,
                    &connection.external_subject,
                    &credential,
                )
                .await
        } else {
            Err(profile.expect_err("profile failure was checked"))
        };
        let connection = if let Some(display_name) = display_name {
            self.repository
                .update_profile(user_id, connection.id, display_name, now_unix_seconds())
                .await
                .map_err(map_repository_error)?
        } else {
            connection
        };
        let latest_balance = Some(self.record_balance(user_id, connection.id, balance).await?);
        Ok(connection_dto(connection, latest_balance))
    }
}

impl UserExternalAccountService for DatabaseUserExternalAccountService {
    fn connectors(&self) -> Vec<SupportedExternalConnector> {
        self.connectors
            .manifests()
            .into_iter()
            .map(|manifest| {
                let id = manifest.plugin_id.as_str().to_owned();
                SupportedExternalConnector {
                    display_name: if manifest.display_name.is_empty() {
                        id.clone()
                    } else {
                        manifest.display_name
                    },
                    id,
                    authentication_modes: manifest.authentication_modes,
                }
            })
            .collect()
    }

    fn list(&self, principal: SessionPrincipal) -> ExternalAccountFuture<'_, Vec<ExternalAccount>> {
        Box::pin(async move {
            let records = self
                .repository
                .list(principal.user_id())
                .await
                .map_err(map_repository_error)?;
            let mut accounts = Vec::with_capacity(records.len());
            for record in records {
                let latest = self.latest_snapshot(principal.user_id(), record.id).await?;
                accounts.push(connection_dto(record, latest));
            }
            Ok(accounts)
        })
    }

    fn link(
        &self,
        principal: SessionPrincipal,
        command: ExternalAccountLinkCommand,
    ) -> ExternalAccountFuture<'_, ExternalAccount> {
        Box::pin(async move {
            let (connector_id, base_url, account_credential) = command.into_parts();
            let credential =
                ExternalPlatformCredential::new(account_credential.expose().to_owned())
                    .map_err(|_| ExternalAccountError::InvalidInput)?;
            let (negotiated, invocation, credential) =
                self.connector_context("pending-link", &connector_id, &base_url, credential)?;
            let profile = self
                .connectors
                .authenticate_external_account(&negotiated, &invocation, &credential)
                .await
                .map_err(map_connector_error)?;
            let balance = self
                .connectors
                .read_external_balance(&negotiated, &invocation, &profile.subject, &credential)
                .await;
            let encrypted = self.encrypt_secret(
                principal.user_id(),
                &connector_id,
                &base_url,
                &profile.subject,
                &account_credential,
            )?;
            let observed_at =
                i64::try_from(now_unix_millis()).map_err(|_| ExternalAccountError::Internal)?;
            let initial_snapshot = initial_snapshot(balance, observed_at);
            let connection = self
                .repository
                .create_with_snapshot(
                    ExternalAccountConnectionCreate {
                        user_id: principal.user_id(),
                        connector_id,
                        external_subject: profile.subject,
                        display_name: profile.display_name,
                        base_url,
                        secret: encrypted,
                        now: now_unix_seconds(),
                    },
                    initial_snapshot.clone(),
                )
                .await
                .map_err(map_repository_error)?;
            Ok(connection_dto(
                connection,
                Some(snapshot_dto_from_initial(initial_snapshot)),
            ))
        })
    }

    fn refresh(
        &self,
        principal: SessionPrincipal,
        connection_id: i64,
    ) -> ExternalAccountFuture<'_, ExternalAccount> {
        Box::pin(async move {
            let connection = self
                .repository
                .get(principal.user_id(), connection_id)
                .await
                .map_err(map_repository_error)?;
            self.refresh_record(principal.user_id(), connection).await
        })
    }

    fn unlink(
        &self,
        principal: SessionPrincipal,
        connection_id: i64,
    ) -> ExternalAccountFuture<'_, ()> {
        Box::pin(async move {
            self.repository
                .delete(principal.user_id(), connection_id)
                .await
                .map_err(map_repository_error)
        })
    }

    fn balance_history(
        &self,
        principal: SessionPrincipal,
        connection_id: i64,
        limit: u32,
    ) -> ExternalAccountFuture<'_, ExternalAccountBalancePage> {
        Box::pin(async move {
            self.repository
                .get(principal.user_id(), connection_id)
                .await
                .map_err(map_repository_error)?;
            let items = self
                .repository
                .list_snapshots(principal.user_id(), connection_id, limit)
                .await
                .map_err(map_repository_error)?
                .into_iter()
                .map(snapshot_dto)
                .collect();
            Ok(ExternalAccountBalancePage {
                connection_id,
                items,
            })
        })
    }
}

fn external_account_aad(
    user_id: UserId,
    connector_id: &str,
    issuer: &str,
    external_subject: &str,
) -> Vec<u8> {
    let user_id = user_id.get().to_string();
    let components = [
        "anyflows:external-account-credential:v1".as_bytes(),
        user_id.as_bytes(),
        connector_id.as_bytes(),
        issuer.as_bytes(),
        external_subject.as_bytes(),
    ];
    let mut aad = Vec::with_capacity(components.iter().map(|part| part.len() + 1).sum());
    for (index, part) in components.iter().enumerate() {
        if index > 0 {
            aad.push(0);
        }
        aad.extend_from_slice(part);
    }
    aad
}

fn canonical_issuer(value: &str) -> Result<String, ExternalAccountError> {
    let endpoint = url::Url::parse(value).map_err(|_| ExternalAccountError::InvalidInput)?;
    if endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(ExternalAccountError::InvalidInput);
    }
    Ok(endpoint.to_string().trim_end_matches('/').to_owned())
}

fn connection_dto(
    record: ExternalAccountConnectionRecord,
    latest_balance: Option<ExternalAccountBalanceSnapshot>,
) -> ExternalAccount {
    ExternalAccount {
        id: record.id,
        connector_id: record.connector_id,
        external_subject: record.external_subject,
        display_name: record.display_name,
        base_url: record.base_url,
        created_at: record.created_at,
        updated_at: record.updated_at,
        latest_balance,
    }
}

fn snapshot_dto(record: ExternalAccountBalanceSnapshotRecord) -> ExternalAccountBalanceSnapshot {
    ExternalAccountBalanceSnapshot {
        amount: record.amount,
        unit: record.unit,
        status: match record.status {
            ExternalBalanceStatus::Available => ExternalAccountBalanceStatus::Available,
            ExternalBalanceStatus::Unavailable => ExternalAccountBalanceStatus::Unavailable,
            ExternalBalanceStatus::AuthenticationRequired => {
                ExternalAccountBalanceStatus::AuthenticationRequired
            }
        },
        observed_at: record.observed_at,
    }
}

fn initial_snapshot(
    result: Result<af_plugin_contract::ExternalBalanceSnapshot, ConnectorOperationError>,
    observed_at: i64,
) -> ExternalAccountInitialBalanceSnapshot {
    let (amount, unit, status) = match result {
        Ok(snapshot) => (
            Some(snapshot.amount),
            Some(snapshot.unit),
            ExternalBalanceStatus::Available,
        ),
        Err(ConnectorOperationError::Connector(error))
            if matches!(
                error.code,
                af_plugin_contract::ConnectorErrorCode::AuthenticationRequired
                    | af_plugin_contract::ConnectorErrorCode::Forbidden
                    | af_plugin_contract::ConnectorErrorCode::Conflict
            ) =>
        {
            (None, None, ExternalBalanceStatus::AuthenticationRequired)
        }
        Err(_) => (None, None, ExternalBalanceStatus::Unavailable),
    };
    ExternalAccountInitialBalanceSnapshot {
        amount,
        unit,
        status,
        observed_at,
    }
}

fn snapshot_dto_from_initial(
    snapshot: ExternalAccountInitialBalanceSnapshot,
) -> ExternalAccountBalanceSnapshot {
    ExternalAccountBalanceSnapshot {
        amount: snapshot.amount,
        unit: snapshot.unit,
        status: match snapshot.status {
            ExternalBalanceStatus::Available => ExternalAccountBalanceStatus::Available,
            ExternalBalanceStatus::Unavailable => ExternalAccountBalanceStatus::Unavailable,
            ExternalBalanceStatus::AuthenticationRequired => {
                ExternalAccountBalanceStatus::AuthenticationRequired
            }
        },
        observed_at: snapshot.observed_at,
    }
}

fn map_connector_error(error: ConnectorOperationError) -> ExternalAccountError {
    match error {
        ConnectorOperationError::CapabilityNotNegotiated
        | ConnectorOperationError::NotRegistered => ExternalAccountError::Unsupported,
        ConnectorOperationError::InvalidInvocation => ExternalAccountError::Internal,
        ConnectorOperationError::InvalidResponse => ExternalAccountError::Unavailable,
        ConnectorOperationError::Connector(error) => match error.code {
            af_plugin_contract::ConnectorErrorCode::AuthenticationRequired
            | af_plugin_contract::ConnectorErrorCode::Forbidden => {
                ExternalAccountError::AuthenticationRequired
            }
            af_plugin_contract::ConnectorErrorCode::Unsupported => {
                ExternalAccountError::Unsupported
            }
            af_plugin_contract::ConnectorErrorCode::Conflict => ExternalAccountError::Conflict,
            af_plugin_contract::ConnectorErrorCode::RateLimited => {
                ExternalAccountError::RateLimited
            }
            _ => ExternalAccountError::Unavailable,
        },
    }
}

fn map_registry_error(error: ConnectorRegistryError) -> ExternalAccountError {
    match error {
        ConnectorRegistryError::NotFound | ConnectorRegistryError::MissingCapability { .. } => {
            ExternalAccountError::Unsupported
        }
        ConnectorRegistryError::Configuration(_) => ExternalAccountError::InvalidInput,
        _ => ExternalAccountError::Internal,
    }
}

fn map_repository_error(error: ExternalAccountRepositoryError) -> ExternalAccountError {
    match error {
        ExternalAccountRepositoryError::InvalidInput => ExternalAccountError::InvalidInput,
        ExternalAccountRepositoryError::NotFound => ExternalAccountError::NotFound,
        ExternalAccountRepositoryError::Conflict => ExternalAccountError::Conflict,
        ExternalAccountRepositoryError::LimitReached => ExternalAccountError::LimitReached,
        ExternalAccountRepositoryError::Unavailable => ExternalAccountError::Unavailable,
        ExternalAccountRepositoryError::Invariant => ExternalAccountError::Internal,
    }
}

fn now_unix_seconds() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn now_unix_millis() -> u64 {
    u64::try_from(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000).unwrap_or(1)
}
