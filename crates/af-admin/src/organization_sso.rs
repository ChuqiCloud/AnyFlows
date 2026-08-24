use std::{fmt, future::Future, pin::Pin};

use af_account::{PlainSystemSecret, SystemSecretCipher};
use af_db::{
    EncryptedCredentialEnvelope, OrganizationSsoProviderCreate, OrganizationSsoProviderRecord,
    OrganizationSsoProviderRepository, OrganizationSsoProviderSecretChange,
    OrganizationSsoProviderUpdate, OrganizationSsoRepositoryError,
};
use af_domain::{
    OrganizationEntitlementStatus, OrganizationId, OrganizationPermission, OrganizationRole,
    OrganizationSsoProtocol, OrganizationSsoProviderKey, OrganizationSsoProviderStatus,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use thiserror::Error;

use crate::OrganizationContext;

/// 管理端可见的企业 SSO Provider 脱敏投影。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationSsoProvider {
    id: i64,
    organization_id: OrganizationId,
    provider_key: String,
    protocol: OrganizationSsoProtocol,
    status: OrganizationSsoProviderStatus,
    display_name: String,
    metadata_projection: Option<Value>,
    secret_configured: bool,
    version: i64,
    created_at: i64,
    updated_at: i64,
}

impl OrganizationSsoProvider {
    fn from_record(record: OrganizationSsoProviderRecord) -> Self {
        Self {
            id: record.id(),
            organization_id: record.organization_id(),
            provider_key: record.provider_key().persistence_key(),
            protocol: record.protocol(),
            status: record.status(),
            display_name: record.display_name().to_owned(),
            metadata_projection: record.metadata_projection().cloned().map(Value::from),
            secret_configured: record.secret_configured(),
            version: record.version(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub fn provider_key(&self) -> &str {
        &self.provider_key
    }
    #[must_use]
    pub const fn protocol(&self) -> OrganizationSsoProtocol {
        self.protocol
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationSsoProviderStatus {
        self.status
    }
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    #[must_use]
    pub fn metadata_projection(&self) -> Option<&Value> {
        self.metadata_projection.as_ref()
    }
    #[must_use]
    pub const fn secret_configured(&self) -> bool {
        self.secret_configured
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

/// Provider 配置写入命令；明文密钥只在本次调用内存中存在。
pub struct OrganizationSsoProviderCommand {
    pub expected_version: i64,
    pub protocol: OrganizationSsoProtocol,
    pub status: OrganizationSsoProviderStatus,
    pub display_name: String,
    pub metadata_projection: Option<Value>,
    pub client_secret: Option<PlainSystemSecret>,
    pub clear_client_secret: bool,
}

impl OrganizationSsoProviderCommand {
    pub fn new(
        expected_version: i64,
        protocol: OrganizationSsoProtocol,
        status: OrganizationSsoProviderStatus,
        display_name: String,
        metadata_projection: Option<Value>,
        client_secret: Option<String>,
        clear_client_secret: bool,
    ) -> Result<Self, OrganizationSsoProviderServiceError> {
        if expected_version < 0
            || (client_secret.is_some() && clear_client_secret)
            || display_name.is_empty()
            || display_name.len() > 128
            || display_name.trim() != display_name
            || display_name.chars().any(char::is_control)
        {
            return Err(OrganizationSsoProviderServiceError::InvalidInput);
        }
        if metadata_projection.as_ref().is_some_and(|value| {
            !value.is_object() || serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() > 32_768)
        }) {
            return Err(OrganizationSsoProviderServiceError::InvalidInput);
        }
        let client_secret = client_secret
            .filter(|value| !value.is_empty())
            .map(PlainSystemSecret::new)
            .transpose()
            .map_err(|_| OrganizationSsoProviderServiceError::InvalidInput)?;
        Ok(Self {
            expected_version,
            protocol,
            status,
            display_name,
            metadata_projection,
            client_secret,
            clear_client_secret,
        })
    }
}

impl fmt::Debug for OrganizationSsoProviderCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoProviderCommand")
            .field("expected_version", &self.expected_version)
            .field("protocol", &self.protocol)
            .field("status", &self.status)
            .field("display_name", &self.display_name)
            .field(
                "has_metadata_projection",
                &self.metadata_projection.is_some(),
            )
            .field("secret_configured", &self.client_secret.is_some())
            .field("clear_client_secret", &self.clear_client_secret)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OrganizationSsoProviderServiceError {
    #[error("企业 SSO Provider 输入无效")]
    InvalidInput,
    #[error("企业 SSO Provider 权限不足")]
    Forbidden,
    #[error("企业 SSO 商业授权不可用")]
    EntitlementRestricted,
    #[error("企业 SSO Provider 不存在")]
    NotFound,
    #[error("企业 SSO Provider 版本冲突")]
    Conflict,
    #[error("企业 SSO Provider 状态不可用")]
    InvalidTransition,
    #[error("企业 SSO Provider 服务内部失败")]
    Internal,
}

pub type OrganizationSsoProviderListFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<Vec<OrganizationSsoProvider>, OrganizationSsoProviderServiceError>,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationSsoProviderFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationSsoProvider, OrganizationSsoProviderServiceError>>
            + Send
            + 'a,
    >,
>;

/// 企业 Provider 管理服务；所有读写都要求企业上下文和有效 SSO 授权。
pub trait OrganizationSsoProviderService: Send + Sync {
    fn list(&self, context: OrganizationContext) -> OrganizationSsoProviderListFuture<'_>;
    fn get(
        &self,
        context: OrganizationContext,
        provider_key: String,
    ) -> OrganizationSsoProviderFuture<'_>;
    fn create(
        &self,
        context: OrganizationContext,
        command: OrganizationSsoProviderCommand,
    ) -> OrganizationSsoProviderFuture<'_>;
    fn update(
        &self,
        context: OrganizationContext,
        provider_key: String,
        command: OrganizationSsoProviderCommand,
    ) -> OrganizationSsoProviderFuture<'_>;
}

#[derive(Clone)]
pub struct DatabaseOrganizationSsoProviderService {
    repository: OrganizationSsoProviderRepository,
    cipher: SystemSecretCipher,
}

impl DatabaseOrganizationSsoProviderService {
    #[must_use]
    pub const fn new(
        repository: OrganizationSsoProviderRepository,
        cipher: SystemSecretCipher,
    ) -> Self {
        Self { repository, cipher }
    }
}

impl OrganizationSsoProviderService for DatabaseOrganizationSsoProviderService {
    fn list(&self, context: OrganizationContext) -> OrganizationSsoProviderListFuture<'_> {
        Box::pin(async move {
            require_read(&context)?;
            self.repository
                .list_providers(context.organization_id())
                .await
                .map(|records| {
                    records
                        .into_iter()
                        .map(OrganizationSsoProvider::from_record)
                        .collect()
                })
                .map_err(map_repository_error)
        })
    }

    fn get(
        &self,
        context: OrganizationContext,
        provider_key: String,
    ) -> OrganizationSsoProviderFuture<'_> {
        Box::pin(async move {
            require_read(&context)?;
            let provider_key = parse_provider_key(&provider_key)?;
            self.repository
                .provider(context.organization_id(), provider_key)
                .await
                .map(OrganizationSsoProvider::from_record)
                .map_err(map_repository_error)
        })
    }

    fn create(
        &self,
        context: OrganizationContext,
        command: OrganizationSsoProviderCommand,
    ) -> OrganizationSsoProviderFuture<'_> {
        Box::pin(async move {
            require_manage(&context)?;
            if command.expected_version != 0 {
                return Err(OrganizationSsoProviderServiceError::InvalidInput);
            }
            let provider_key = generate_provider_key()?;
            let secret = encrypt_secret(
                &self.cipher,
                context.organization_id(),
                provider_key,
                command.protocol,
                1,
                command.client_secret,
            )?;
            let record = self
                .repository
                .create_provider(
                    context.organization_id(),
                    OrganizationSsoProviderCreate::new(
                        provider_key,
                        command.protocol,
                        command.status,
                        command.display_name,
                        command.metadata_projection,
                        secret,
                    ),
                )
                .await
                .map_err(map_repository_error)?;
            Ok(OrganizationSsoProvider::from_record(record))
        })
    }

    fn update(
        &self,
        context: OrganizationContext,
        provider_key: String,
        command: OrganizationSsoProviderCommand,
    ) -> OrganizationSsoProviderFuture<'_> {
        Box::pin(async move {
            require_manage(&context)?;
            if command.expected_version <= 0 {
                return Err(OrganizationSsoProviderServiceError::InvalidInput);
            }
            let provider_key = parse_provider_key(&provider_key)?;
            let next_version = command
                .expected_version
                .checked_add(1)
                .ok_or(OrganizationSsoProviderServiceError::InvalidInput)?;
            // Provider 版本绑定在密钥 AAD；即使请求未提交新密钥，也必须按新协议和版本重封装。
            let current = self
                .repository
                .provider(context.organization_id(), provider_key)
                .await
                .map_err(map_repository_error)?;
            let secret_change = if command.clear_client_secret {
                OrganizationSsoProviderSecretChange::Clear
            } else if command.client_secret.is_some() {
                OrganizationSsoProviderSecretChange::Replace(
                    encrypt_secret(
                        &self.cipher,
                        context.organization_id(),
                        provider_key,
                        command.protocol,
                        next_version,
                        command.client_secret,
                    )?
                    .ok_or(OrganizationSsoProviderServiceError::Internal)?,
                )
            } else {
                let Some(envelope) = self
                    .repository
                    .provider_secret(context.organization_id(), provider_key)
                    .await
                    .map_err(map_repository_error)?
                else {
                    OrganizationSsoProviderSecretChange::Keep
                };
                let decrypted = self
                    .cipher
                    .decrypt_organization_sso_provider_secret(
                        context.organization_id(),
                        provider_key,
                        current.protocol(),
                        current.version(),
                        &envelope,
                    )
                    .map_err(|_| OrganizationSsoProviderServiceError::Internal)?;
                let plain = PlainSystemSecret::new(decrypted.expose_secret().to_owned())
                    .map_err(|_| OrganizationSsoProviderServiceError::Internal)?;
                OrganizationSsoProviderSecretChange::Replace(
                    encrypt_secret(
                        &self.cipher,
                        context.organization_id(),
                        provider_key,
                        command.protocol,
                        next_version,
                        Some(plain),
                    )?
                    .ok_or(OrganizationSsoProviderServiceError::Internal)?,
                )
            };
            let record = self
                .repository
                .update_provider(
                    context.organization_id(),
                    provider_key,
                    OrganizationSsoProviderUpdate::new(
                        command.protocol,
                        command.status,
                        command.display_name,
                        command.metadata_projection,
                        secret_change,
                        command.expected_version,
                    ),
                )
                .await
                .map_err(map_repository_error)?;
            Ok(OrganizationSsoProvider::from_record(record))
        })
    }
}

impl fmt::Debug for DatabaseOrganizationSsoProviderService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationSsoProviderService(<repository,cipher>)")
    }
}

fn require_read(context: &OrganizationContext) -> Result<(), OrganizationSsoProviderServiceError> {
    if !matches!(
        context.role(),
        OrganizationRole::Owner | OrganizationRole::Admin
    ) || !context.allows(OrganizationPermission::SsoRead)
    {
        return Err(OrganizationSsoProviderServiceError::Forbidden);
    }
    if !context.entitlement().sso_enabled()
        || context.entitlement().status() != OrganizationEntitlementStatus::Active
    {
        return Err(OrganizationSsoProviderServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn require_manage(
    context: &OrganizationContext,
) -> Result<(), OrganizationSsoProviderServiceError> {
    if !matches!(
        context.role(),
        OrganizationRole::Owner | OrganizationRole::Admin
    ) || !context.allows(OrganizationPermission::SsoManage)
    {
        return Err(OrganizationSsoProviderServiceError::Forbidden);
    }
    if !context.entitlement().sso_enabled()
        || context.entitlement().status() != OrganizationEntitlementStatus::Active
    {
        return Err(OrganizationSsoProviderServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn parse_provider_key(
    value: &str,
) -> Result<OrganizationSsoProviderKey, OrganizationSsoProviderServiceError> {
    OrganizationSsoProviderKey::from_persistence_key(value)
        .map_err(|_| OrganizationSsoProviderServiceError::InvalidInput)
}

fn generate_provider_key() -> Result<OrganizationSsoProviderKey, OrganizationSsoProviderServiceError>
{
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| OrganizationSsoProviderServiceError::Internal)?;
    OrganizationSsoProviderKey::new(bytes)
        .map_err(|_| OrganizationSsoProviderServiceError::Internal)
}

fn encrypt_secret(
    cipher: &SystemSecretCipher,
    organization_id: OrganizationId,
    provider_key: OrganizationSsoProviderKey,
    protocol: OrganizationSsoProtocol,
    version: i64,
    secret: Option<PlainSystemSecret>,
) -> Result<Option<Value>, OrganizationSsoProviderServiceError> {
    let Some(secret) = secret else {
        return Ok(None);
    };
    let envelope = cipher
        .encrypt_organization_sso_provider_secret(
            organization_id,
            provider_key,
            protocol,
            version,
            &secret,
        )
        .map_err(|_| OrganizationSsoProviderServiceError::Internal)?;
    Ok(Some(envelope_json(&envelope)))
}

fn envelope_json(envelope: &EncryptedCredentialEnvelope) -> Value {
    json!({
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    })
}

fn map_repository_error(
    error: OrganizationSsoRepositoryError,
) -> OrganizationSsoProviderServiceError {
    match error {
        OrganizationSsoRepositoryError::InvalidInput => {
            OrganizationSsoProviderServiceError::InvalidInput
        }
        OrganizationSsoRepositoryError::NotFound => OrganizationSsoProviderServiceError::NotFound,
        OrganizationSsoRepositoryError::Conflict => OrganizationSsoProviderServiceError::Conflict,
        OrganizationSsoRepositoryError::InvalidTransition => {
            OrganizationSsoProviderServiceError::InvalidTransition
        }
        OrganizationSsoRepositoryError::AlreadyProcessed
        | OrganizationSsoRepositoryError::Expired
        | OrganizationSsoRepositoryError::Unavailable => {
            OrganizationSsoProviderServiceError::InvalidTransition
        }
        OrganizationSsoRepositoryError::Query
        | OrganizationSsoRepositoryError::Timeout
        | OrganizationSsoRepositoryError::Invariant => {
            OrganizationSsoProviderServiceError::Internal
        }
    }
}
