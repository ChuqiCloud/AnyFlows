use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use af_account::{
    OrganizationInvitationCipher, OrganizationSsoLoginError, OrganizationSsoLoginStart,
    OrganizationSsoOidcIdTokenVerificationConfig, OrganizationSsoOidcIdTokenVerifier,
    OrganizationSsoOidcJwksClient, OrganizationSsoOidcLoginMaterial,
    OrganizationSsoOidcTokenExchangeClient, OrganizationSsoOidcTokenExchangeConfig,
    OrganizationSsoOidcTokenExchangeInput, SystemSecretCipher, organization_sso_login_state_digest,
    organization_sso_login_ticket_digest,
};
use af_db::{
    EncryptedCredentialEnvelope, OrganizationMembershipRecord, OrganizationRepository,
    OrganizationRepositoryError, OrganizationSsoLoginTransactionCreate,
    OrganizationSsoLoginTransactionRecord, OrganizationSsoLoginTransactionResult,
    OrganizationSsoProviderRepository, OrganizationSsoRepositoryError,
};
use af_domain::{
    OrganizationId, OrganizationSsoProtocol, OrganizationSsoProviderKey,
    OrganizationSsoProviderStatus,
};
use af_httpclient::HttpClientProvider;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

const LOGIN_TRANSACTION_TTL_SECONDS: i64 = 10 * 60;
const LOGIN_TICKET_TTL_SECONDS: i64 = 2 * 60;
const TICKET_RANDOM_BYTES: usize = 32;
const VERIFIED_RESULT_CODE: i16 = 1;
const REJECTED_RESULT_CODE: i16 = 2;

/// 企业 SSO 登录事务服务的异步结果。
pub type OrganizationSsoLoginTransactionFuture<'a, T> = Pin<
    Box<dyn Future<Output = Result<T, OrganizationSsoLoginTransactionServiceError>> + Send + 'a>,
>;

/// 登录事务启动后可交给 HTTP 层的脱敏绑定摘要。
///
/// state、PKCE verifier、nonce 和协议原文不会离开服务层；调用方只能拿到
/// 用于后续边界校验的摘要、Provider 版本快照与短期过期时间。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationSsoLoginTransactionStart {
    protocol: OrganizationSsoProtocol,
    provider_version: i64,
    state_digest: String,
    request_digest: String,
    expires_at: i64,
}

impl OrganizationSsoLoginTransactionStart {
    fn new(
        protocol: OrganizationSsoProtocol,
        provider_version: i64,
        state_digest: String,
        request_digest: String,
        expires_at: i64,
    ) -> Self {
        Self {
            protocol,
            provider_version,
            state_digest,
            request_digest,
            expires_at,
        }
    }

    /// 返回创建事务时锁定的协议。
    #[must_use]
    pub const fn protocol(&self) -> OrganizationSsoProtocol {
        self.protocol
    }

    /// 返回创建事务时锁定的 Provider 版本。
    #[must_use]
    pub const fn provider_version(&self) -> i64 {
        self.provider_version
    }

    /// 返回 state 摘要；不返回 state 明文。
    #[must_use]
    pub fn state_digest(&self) -> &str {
        &self.state_digest
    }

    /// 返回调用方提交的请求摘要。
    #[must_use]
    pub fn request_digest(&self) -> &str {
        &self.request_digest
    }

    /// 返回事务过期时间（UTC Unix 秒）。
    #[must_use]
    pub const fn expires_at(&self) -> i64 {
        self.expires_at
    }
}

/// 已领取登录事务的安全投影；OIDC 短期材料仅在内部持有，Debug 和外部投影均不暴露。
#[derive(Eq, PartialEq)]
pub struct ClaimedOrganizationSsoLoginTransaction {
    transaction_id: i64,
    organization_id: OrganizationId,
    provider_id: i64,
    protocol: OrganizationSsoProtocol,
    provider_version: i64,
    request_digest: String,
    oidc_login_material: Option<OrganizationSsoOidcLoginMaterial>,
}

impl ClaimedOrganizationSsoLoginTransaction {
    fn from_record(
        record: OrganizationSsoLoginTransactionRecord,
        oidc_login_material: Option<OrganizationSsoOidcLoginMaterial>,
    ) -> Self {
        Self {
            transaction_id: record.id(),
            organization_id: record.organization_id(),
            provider_id: record.provider_id(),
            protocol: record.protocol(),
            provider_version: record.provider_version(),
            request_digest: record.request_digest().to_owned(),
            oidc_login_material,
        }
    }

    /// 返回单次登录事务的内部标识，只可与同一企业范围共同使用。
    #[must_use]
    pub const fn transaction_id(&self) -> i64 {
        self.transaction_id
    }

    /// 返回 state 所属企业；固定回调入口只能从已领取事务取得该范围。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回已绑定 Provider 的内部标识，供后续受控回调适配器读取同企业快照。
    #[must_use]
    pub const fn provider_id(&self) -> i64 {
        self.provider_id
    }

    /// 返回事务创建时固定的协议，回调不得由客户端指定。
    #[must_use]
    pub const fn protocol(&self) -> OrganizationSsoProtocol {
        self.protocol
    }

    /// 返回事务创建时固定的 Provider 版本，后续回调必须复验该快照。
    #[must_use]
    pub const fn provider_version(&self) -> i64 {
        self.provider_version
    }

    /// 返回调用方创建的请求摘要，用于与受控请求上下文进行一致性校验。
    #[must_use]
    pub fn request_digest(&self) -> &str {
        &self.request_digest
    }

    /// 返回仅用于 OIDC token 交换与 ID Token nonce 校验的短期材料，不得复制或记录。
    #[must_use]
    pub fn oidc_login_material(&self) -> Option<&OrganizationSsoOidcLoginMaterial> {
        self.oidc_login_material.as_ref()
    }
}

impl fmt::Debug for ClaimedOrganizationSsoLoginTransaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaimedOrganizationSsoLoginTransaction")
            .field("transaction_id", &self.transaction_id)
            .field("organization_id", &self.organization_id)
            .field("provider_id", &self.provider_id)
            .field("protocol", &self.protocol)
            .field("provider_version", &self.provider_version)
            .field("request_digest", &"<redacted>")
            .field(
                "has_oidc_login_material",
                &self.oidc_login_material.is_some(),
            )
            .finish()
    }
}

/// 成功完成登录事务后仅返回一次的短期票据；Debug 不输出票据明文。
#[derive(Eq, PartialEq)]
pub struct IssuedOrganizationSsoLoginTicket {
    ticket: String,
    expires_at: i64,
}

impl IssuedOrganizationSsoLoginTicket {
    /// 返回只可进行一次交换的明文票据，调用方不得记录或持久化该值。
    #[must_use]
    pub fn ticket(&self) -> &str {
        &self.ticket
    }

    /// 返回票据的 UTC Unix 秒过期时间。
    #[must_use]
    pub const fn expires_at(&self) -> i64 {
        self.expires_at
    }
}

impl fmt::Debug for IssuedOrganizationSsoLoginTicket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedOrganizationSsoLoginTicket")
            .field("ticket", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// 单次票据交换后的受控事务投影，供后续身份绑定层继续处理。
#[derive(Clone, Eq, PartialEq)]
pub struct ExchangedOrganizationSsoLoginTicket {
    transaction_id: i64,
    provider_id: i64,
    protocol: OrganizationSsoProtocol,
    provider_version: i64,
    assertion_digest: String,
    subject_digest: Option<String>,
    invitation_email_fingerprint: Option<[u8; 32]>,
}

impl ExchangedOrganizationSsoLoginTicket {
    fn from_record(
        record: OrganizationSsoLoginTransactionRecord,
    ) -> Result<Self, OrganizationSsoLoginTransactionServiceError> {
        if record.result_code() != Some(VERIFIED_RESULT_CODE) {
            return Err(OrganizationSsoLoginTransactionServiceError::Internal);
        }
        let assertion_digest = record
            .assertion_digest()
            .ok_or(OrganizationSsoLoginTransactionServiceError::Internal)?
            .to_owned();
        // 当前只把 OIDC 验证器产生的 subject 摘要交给身份绑定层；SAML 仍保持关闭。
        let subject_digest =
            (record.protocol() == OrganizationSsoProtocol::Oidc).then(|| assertion_digest.clone());
        let invitation_email_fingerprint = match record.protocol() {
            OrganizationSsoProtocol::Oidc => record.invitation_email_fingerprint().copied(),
            OrganizationSsoProtocol::Saml if record.invitation_email_fingerprint().is_some() => {
                return Err(OrganizationSsoLoginTransactionServiceError::Internal);
            }
            OrganizationSsoProtocol::Saml => None,
        };
        Ok(Self {
            transaction_id: record.id(),
            provider_id: record.provider_id(),
            protocol: record.protocol(),
            provider_version: record.provider_version(),
            assertion_digest,
            subject_digest,
            invitation_email_fingerprint,
        })
    }

    /// 返回事务内部标识，只可与同一企业范围共同使用。
    #[must_use]
    pub const fn transaction_id(&self) -> i64 {
        self.transaction_id
    }

    /// 返回已绑定 Provider 的内部标识，供后续身份绑定适配器读取同企业快照。
    #[must_use]
    pub const fn provider_id(&self) -> i64 {
        self.provider_id
    }

    /// 返回事务创建时锁定的协议。
    #[must_use]
    pub const fn protocol(&self) -> OrganizationSsoProtocol {
        self.protocol
    }

    /// 返回事务创建时锁定的 Provider 版本。
    #[must_use]
    pub const fn provider_version(&self) -> i64 {
        self.provider_version
    }

    /// 返回经协议验证器计算的断言摘要，不返回断言正文。
    #[must_use]
    pub fn assertion_digest(&self) -> &str {
        &self.assertion_digest
    }

    /// 返回 OIDC 验证器产生的 subject 摘要；SAML 尚未开放身份绑定，因此返回空值。
    #[must_use]
    pub fn subject_digest(&self) -> Option<&str> {
        self.subject_digest.as_deref()
    }

    /// 返回已验证 OIDC 邮箱的企业隔离指纹；仅供后续受邀请 JIT 绑定消费。
    #[must_use]
    pub fn invitation_email_fingerprint(&self) -> Option<&[u8; 32]> {
        self.invitation_email_fingerprint.as_ref()
    }
}

impl fmt::Debug for ExchangedOrganizationSsoLoginTicket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExchangedOrganizationSsoLoginTicket")
            .field("transaction_id", &self.transaction_id)
            .field("provider_id", &self.provider_id)
            .field("protocol", &self.protocol)
            .field("provider_version", &self.provider_version)
            .field("assertion_digest", &"<redacted>")
            .field("has_subject_digest", &self.subject_digest.is_some())
            .field(
                "has_invitation_email_fingerprint",
                &self.invitation_email_fingerprint.is_some(),
            )
            .finish()
    }
}

/// 企业 SSO 登录事务的稳定错误分类，不携带 state、票据或断言原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoLoginTransactionServiceError {
    #[error("企业 SSO 登录事务输入无效")]
    InvalidInput,
    #[error("企业 SSO 登录 Provider 不可用")]
    Unavailable,
    #[error("企业 SSO 登录事务不存在或已被拒绝")]
    Rejected,
    #[error("企业 SSO 登录事务已过期")]
    Expired,
    #[error("企业 SSO 登录事务已处理")]
    AlreadyProcessed,
    #[error("企业 SSO 登录事务状态转换无效")]
    InvalidTransition,
    #[error("企业 SSO 登录事务服务内部失败")]
    Internal,
}

/// 企业 SSO 登录事务协调服务；HTTP 路由必须继续在其外层固定回调地址和响应边界。
pub trait OrganizationSsoLoginTransactionService: Send + Sync {
    /// 读取当前 Active Provider 并创建仅含摘要的短期登录事务。
    fn begin<'a>(
        &'a self,
        organization_id: OrganizationId,
        provider_key: String,
        request_digest: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, OrganizationSsoLoginTransactionStart>;

    /// 按企业范围原子领取 state；调用方须在任何外部协议请求前调用此方法。
    fn claim<'a>(
        &'a self,
        organization_id: OrganizationId,
        state: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, ClaimedOrganizationSsoLoginTransaction>;

    /// 在固定回调路径中按全局唯一 state 原子领取事务并恢复企业范围。
    fn claim_callback<'a>(
        &'a self,
        state: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, ClaimedOrganizationSsoLoginTransaction>;

    /// 在协议签名、配置版本和身份语义均已受控验证后，完成事务并签发单次票据。
    fn complete_verified<'a>(
        &'a self,
        organization_id: OrganizationId,
        transaction_id: i64,
        assertion_digest: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, IssuedOrganizationSsoLoginTicket>;

    /// 在协议验证拒绝后封存已领取事务，不生成票据或持久化敏感原文。
    fn complete_rejected<'a>(
        &'a self,
        organization_id: OrganizationId,
        transaction_id: i64,
    ) -> OrganizationSsoLoginTransactionFuture<'a, ()>;

    /// 按企业范围原子交换一次性票据，供后续身份绑定层继续处理。
    fn exchange_ticket<'a>(
        &'a self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, ExchangedOrganizationSsoLoginTicket>;

    /// 交换一次性 ticket 并解析活动企业成员；邀请事实只允许进入受控 JIT 绑定。
    fn exchange_ticket_and_resolve_identity<'a>(
        &'a self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, OrganizationMembershipRecord> {
        let _ = (organization_id, ticket);
        Box::pin(async { Err(OrganizationSsoLoginTransactionServiceError::Unavailable) })
    }

    /// 交换一次性 ticket 并消费受邀请 JIT 事实；仅允许已验证 OIDC 投影进入绑定仓储。
    fn exchange_ticket_and_bind_invited_identity<'a>(
        &'a self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, OrganizationMembershipRecord> {
        let _ = (organization_id, ticket);
        Box::pin(async { Err(OrganizationSsoLoginTransactionServiceError::Unavailable) })
    }

    /// 完成受控 OIDC 回调；只有交换、签名、声明和 nonce 全部通过后才签发一次性票据。
    fn complete_oidc_callback<'a>(
        &'a self,
        claimed: ClaimedOrganizationSsoLoginTransaction,
        code: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, IssuedOrganizationSsoLoginTicket> {
        let _ = (claimed, code);
        Box::pin(async { Err(OrganizationSsoLoginTransactionServiceError::Unavailable) })
    }
}

/// 使用企业范围仓储协调短期登录事务，不持久化协议敏感材料。
#[derive(Clone)]
pub struct DatabaseOrganizationSsoLoginTransactionService {
    repository: OrganizationSsoProviderRepository,
    organization_repository: Option<OrganizationRepository>,
    cipher: SystemSecretCipher,
    invitation_cipher: OrganizationInvitationCipher,
    http_clients: Arc<RwLock<Option<HttpClientProvider>>>,
}

impl DatabaseOrganizationSsoLoginTransactionService {
    /// 绑定企业 SSO 仓储及启动期密钥；邮件指纹只使用邀请密码学边界派生。
    #[must_use]
    pub fn new(
        repository: OrganizationSsoProviderRepository,
        cipher: SystemSecretCipher,
        invitation_cipher: OrganizationInvitationCipher,
    ) -> Self {
        Self {
            repository,
            organization_repository: None,
            cipher,
            invitation_cipher,
            http_clients: Arc::new(RwLock::new(None)),
        }
    }

    /// 注入组织仓储，启用受邀请 SSO ticket 的 JIT 消费入口。
    #[must_use]
    pub fn new_with_organization_repository(
        repository: OrganizationSsoProviderRepository,
        organization_repository: OrganizationRepository,
        cipher: SystemSecretCipher,
        invitation_cipher: OrganizationInvitationCipher,
    ) -> Self {
        Self {
            repository,
            organization_repository: Some(organization_repository),
            cipher,
            invitation_cipher,
            http_clients: Arc::new(RwLock::new(None)),
        }
    }

    /// 注入已完成安全基线配置的共享 HTTP Client；只在服务器进入 relay 阶段后调用。
    pub fn set_http_clients(&self, http_clients: HttpClientProvider) {
        if let Ok(mut slot) = self.http_clients.write() {
            *slot = Some(http_clients);
        }
    }

    /// 编排受控 OIDC 回调；任一内部步骤失败都会封存已领取事务。
    async fn complete_oidc_callback_inner(
        &self,
        claimed: ClaimedOrganizationSsoLoginTransaction,
        code: String,
    ) -> Result<IssuedOrganizationSsoLoginTicket, OrganizationSsoLoginTransactionServiceError> {
        let organization_id = claimed.organization_id();
        let transaction_id = claimed.transaction_id();
        match self.complete_oidc_callback_attempt(&claimed, code).await {
            Ok(ticket) => Ok(ticket),
            Err(error) => match self
                .complete_rejected_inner(organization_id, transaction_id)
                .await
            {
                Ok(()) => Err(error),
                Err(_) => Err(OrganizationSsoLoginTransactionServiceError::Internal),
            },
        }
    }

    /// 执行一次 OIDC 回调验证尝试，不在此方法中持久化失败结果。
    async fn complete_oidc_callback_attempt(
        &self,
        claimed: &ClaimedOrganizationSsoLoginTransaction,
        code: String,
    ) -> Result<IssuedOrganizationSsoLoginTicket, OrganizationSsoLoginTransactionServiceError> {
        if claimed.protocol() != OrganizationSsoProtocol::Oidc {
            return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
        }
        // 先复制短期材料，释放事务借用后再执行 Provider 和网络编排。
        let organization_id = claimed.organization_id();
        let transaction_id = claimed.transaction_id();
        let provider_id = claimed.provider_id();
        let provider_version = claimed.provider_version();
        let (pkce_verifier, nonce) = {
            let material = claimed
                .oidc_login_material()
                .ok_or(OrganizationSsoLoginTransactionServiceError::Rejected)?;
            (
                Zeroizing::new(material.pkce_verifier().to_owned()),
                Zeroizing::new(material.nonce().to_owned()),
            )
        };
        let provider = self
            .repository
            .provider_by_id(organization_id, provider_id)
            .await
            .map_err(map_repository_error)?;
        if provider.protocol() != OrganizationSsoProtocol::Oidc
            || provider.version() != provider_version
            || provider.status() != OrganizationSsoProviderStatus::Active
        {
            return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
        }
        let projection = provider
            .metadata_projection()
            .ok_or(OrganizationSsoLoginTransactionServiceError::Rejected)?;
        let config = OrganizationSsoOidcTokenExchangeConfig::from_metadata_projection(
            provider_version,
            projection,
        )
        .map_err(|_| OrganizationSsoLoginTransactionServiceError::Rejected)?;
        let envelope = self
            .repository
            .provider_secret_by_id(organization_id, provider_id)
            .await
            .map_err(map_repository_error)?
            .ok_or(OrganizationSsoLoginTransactionServiceError::Unavailable)?;
        let exchange = OrganizationSsoOidcTokenExchangeClient::from_encrypted_secret(
            &self.cipher,
            organization_id,
            provider.provider_key(),
            config.clone(),
            &envelope,
        )
        .map_err(|_| OrganizationSsoLoginTransactionServiceError::Unavailable)?;
        let http_clients = self
            .http_clients
            .read()
            .ok()
            .and_then(|slot| slot.clone())
            .ok_or(OrganizationSsoLoginTransactionServiceError::Unavailable)?;
        let http_client = http_clients
            .get(None)
            .map_err(|_| OrganizationSsoLoginTransactionServiceError::Unavailable)?;
        let input = OrganizationSsoOidcTokenExchangeInput::new(code, pkce_verifier.to_string())
            .map_err(|_| OrganizationSsoLoginTransactionServiceError::Rejected)?;
        let token_set = exchange
            .exchange(&http_client, input)
            .await
            .map_err(|_| OrganizationSsoLoginTransactionServiceError::Rejected)?;
        let jwks = OrganizationSsoOidcJwksClient::from_token_exchange_config(&config)
            .fetch(&http_client)
            .await
            .map_err(|_| OrganizationSsoLoginTransactionServiceError::Rejected)?;
        let verifier = OrganizationSsoOidcIdTokenVerifier::new(
            OrganizationSsoOidcIdTokenVerificationConfig::new(
                config.issuer().to_string(),
                config.audience().to_owned(),
            )
            .map_err(|_| OrganizationSsoLoginTransactionServiceError::Rejected)?,
        );
        let identity = verifier
            .verify(token_set.id_token(), &jwks, nonce.as_str())
            .map_err(|_| OrganizationSsoLoginTransactionServiceError::Rejected)?;
        let assertion_digest = identity.assertion_digest();
        let invitation_email_fingerprint = self
            .invitation_cipher
            .email_fingerprint(organization_id, identity.invitation_email());
        self.complete_verified_inner(
            organization_id,
            transaction_id,
            assertion_digest,
            Some(invitation_email_fingerprint),
        )
        .await
    }

    async fn begin_inner(
        &self,
        organization_id: OrganizationId,
        provider_key: String,
        request_digest: String,
    ) -> Result<OrganizationSsoLoginTransactionStart, OrganizationSsoLoginTransactionServiceError>
    {
        let provider_key = OrganizationSsoProviderKey::from_persistence_key(&provider_key)
            .map_err(|_| OrganizationSsoLoginTransactionServiceError::InvalidInput)?;
        let provider = self
            .repository
            .provider(organization_id, provider_key)
            .await
            .map_err(map_repository_error)?;
        if provider.status() != OrganizationSsoProviderStatus::Active {
            return Err(OrganizationSsoLoginTransactionServiceError::Unavailable);
        }
        let start =
            OrganizationSsoLoginStart::new(request_digest).map_err(map_login_material_error)?;
        let expires_at = expires_at(LOGIN_TRANSACTION_TTL_SECONDS)?;
        let state_digest = start.state_digest().to_owned();
        let request_digest = start.request_digest().to_owned();
        let login_material_envelope = match provider.protocol() {
            OrganizationSsoProtocol::Oidc => {
                let secret = start
                    .into_oidc_material()
                    .into_plain_system_secret()
                    .map_err(map_login_material_error)?;
                let envelope = self
                    .cipher
                    .encrypt_organization_sso_login_material(
                        organization_id,
                        provider.id(),
                        provider.protocol(),
                        provider.version(),
                        &state_digest,
                        &secret,
                    )
                    .map_err(|_| OrganizationSsoLoginTransactionServiceError::Internal)?;
                Some(envelope_json(&envelope))
            }
            OrganizationSsoProtocol::Saml => None,
        };
        let write = OrganizationSsoLoginTransactionCreate::new(
            provider_key,
            provider.protocol(),
            provider.version(),
            state_digest,
            request_digest,
            expires_at,
        );
        let write = if let Some(envelope) = login_material_envelope {
            write.with_login_material_envelope(envelope)
        } else {
            write
        };
        let record = self
            .repository
            .create_login_transaction(organization_id, write)
            .await
            .map_err(map_repository_error)?;
        Ok(OrganizationSsoLoginTransactionStart::new(
            record.protocol(),
            record.provider_version(),
            record.state_digest().to_owned(),
            record.request_digest().to_owned(),
            record.expires_at(),
        ))
    }

    async fn claim_inner(
        &self,
        organization_id: OrganizationId,
        state: String,
    ) -> Result<ClaimedOrganizationSsoLoginTransaction, OrganizationSsoLoginTransactionServiceError>
    {
        // 先原子领取再进入协议层，避免同一 state 被并发回调重复消费。
        let state_digest =
            organization_sso_login_state_digest(&state).map_err(map_login_material_error)?;
        let record = self
            .repository
            .claim_login_transaction(organization_id, state_digest, current_timestamp()?)
            .await
            .map_err(map_repository_error)?;
        self.recover_claimed_login_transaction(record).await
    }

    async fn claim_callback_inner(
        &self,
        state: String,
    ) -> Result<ClaimedOrganizationSsoLoginTransaction, OrganizationSsoLoginTransactionServiceError>
    {
        // 固定回调没有企业路径参数，必须先由全局唯一 state 恢复并锁定企业范围。
        let state_digest =
            organization_sso_login_state_digest(&state).map_err(map_login_material_error)?;
        let record = self
            .repository
            .claim_login_transaction_by_state(state_digest, current_timestamp()?)
            .await
            .map_err(map_repository_error)?;
        self.recover_claimed_login_transaction(record).await
    }

    async fn recover_claimed_login_transaction(
        &self,
        record: OrganizationSsoLoginTransactionRecord,
    ) -> Result<ClaimedOrganizationSsoLoginTransaction, OrganizationSsoLoginTransactionServiceError>
    {
        let oidc_login_material = if record.protocol() == OrganizationSsoProtocol::Oidc {
            match self.decrypt_oidc_login_material(&record) {
                Ok(material) => Some(material),
                Err(()) => {
                    self.reject_claimed_login_transaction(&record).await?;
                    return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
                }
            }
        } else {
            None
        };
        Ok(ClaimedOrganizationSsoLoginTransaction::from_record(
            record,
            oidc_login_material,
        ))
    }

    fn decrypt_oidc_login_material(
        &self,
        record: &OrganizationSsoLoginTransactionRecord,
    ) -> Result<OrganizationSsoOidcLoginMaterial, ()> {
        let envelope = record.login_material_envelope().ok_or(())?;
        let secret = self
            .cipher
            .decrypt_organization_sso_login_material(
                record.organization_id(),
                record.provider_id(),
                record.protocol(),
                record.provider_version(),
                record.state_digest(),
                envelope,
            )
            .map_err(|_| ())?;
        OrganizationSsoOidcLoginMaterial::from_decrypted_system_secret(secret).map_err(|_| ())
    }

    async fn reject_claimed_login_transaction(
        &self,
        record: &OrganizationSsoLoginTransactionRecord,
    ) -> Result<(), OrganizationSsoLoginTransactionServiceError> {
        match self
            .repository
            .reject_login_transaction(record.organization_id(), record.id(), REJECTED_RESULT_CODE)
            .await
        {
            Ok(_) | Err(OrganizationSsoRepositoryError::AlreadyProcessed) => Ok(()),
            Err(error) => Err(map_repository_error(error)),
        }
    }

    async fn complete_verified_inner(
        &self,
        organization_id: OrganizationId,
        transaction_id: i64,
        assertion_digest: String,
        invitation_email_fingerprint: Option<[u8; 32]>,
    ) -> Result<IssuedOrganizationSsoLoginTicket, OrganizationSsoLoginTransactionServiceError> {
        // 明文票据仅在成功完成后返回，数据库始终只接收域分离摘要。
        let ticket = generate_ticket()?;
        let ticket_digest =
            organization_sso_login_ticket_digest(&ticket).map_err(map_login_material_error)?;
        let expires_at = expires_at(LOGIN_TICKET_TTL_SECONDS)?;
        let result = OrganizationSsoLoginTransactionResult::new(
            VERIFIED_RESULT_CODE,
            Some(ticket_digest),
            Some(expires_at),
            Some(assertion_digest),
            Some(expires_at),
        );
        let result = match invitation_email_fingerprint {
            Some(fingerprint) => result.with_invitation_email_fingerprint(fingerprint),
            None => result,
        };
        self.repository
            .complete_login_transaction(
                organization_id,
                transaction_id,
                result,
                current_timestamp()?,
            )
            .await
            .map_err(map_repository_error)?;
        Ok(IssuedOrganizationSsoLoginTicket {
            ticket: ticket.to_string(),
            expires_at,
        })
    }

    async fn complete_rejected_inner(
        &self,
        organization_id: OrganizationId,
        transaction_id: i64,
    ) -> Result<(), OrganizationSsoLoginTransactionServiceError> {
        self.repository
            .reject_login_transaction(organization_id, transaction_id, REJECTED_RESULT_CODE)
            .await
            .map_err(map_repository_error)?;
        Ok(())
    }

    async fn exchange_ticket_inner(
        &self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> Result<ExchangedOrganizationSsoLoginTicket, OrganizationSsoLoginTransactionServiceError>
    {
        let ticket_digest =
            organization_sso_login_ticket_digest(&ticket).map_err(map_login_material_error)?;
        let record = self
            .repository
            .exchange_ticket(organization_id, ticket_digest, current_timestamp()?)
            .await
            .map_err(map_repository_error)?;
        ExchangedOrganizationSsoLoginTicket::from_record(record)
    }

    async fn exchange_ticket_and_resolve_identity_inner(
        &self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> Result<OrganizationMembershipRecord, OrganizationSsoLoginTransactionServiceError> {
        let exchanged = self.exchange_ticket_inner(organization_id, ticket).await?;
        if exchanged.protocol() != OrganizationSsoProtocol::Oidc {
            return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
        }
        let subject_digest = exchanged
            .subject_digest()
            .ok_or(OrganizationSsoLoginTransactionServiceError::Rejected)?
            .to_owned();
        let provider = self
            .repository
            .provider_by_id(organization_id, exchanged.provider_id())
            .await
            .map_err(map_repository_error)?;
        if provider.protocol() != OrganizationSsoProtocol::Oidc
            || provider.version() != exchanged.provider_version()
            || provider.status() != OrganizationSsoProviderStatus::Active
        {
            return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
        }
        let organization_repository = self
            .organization_repository
            .as_ref()
            .ok_or(OrganizationSsoLoginTransactionServiceError::Unavailable)?;

        // 邀请邮箱指纹是唯一允许创建 JIT 成员的事实，不能退化为邮箱匹配。
        let membership =
            if let Some(fingerprint) = exchanged.invitation_email_fingerprint().copied() {
                organization_repository
                    .bind_invited_sso_identity(
                        organization_id,
                        provider.provider_key(),
                        subject_digest.clone(),
                        fingerprint,
                    )
                    .await
                    .map_err(map_invited_binding_error)?
            } else {
                let identity = self
                    .repository
                    .identity(organization_id, provider.provider_key(), subject_digest)
                    .await
                    .map_err(map_repository_error)?;
                if identity.status() != af_domain::OrganizationSsoIdentityStatus::Active {
                    return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
                }
                organization_repository
                    .membership(organization_id, identity.membership_id())
                    .await
                    .map_err(map_invited_binding_error)?
            };
        if membership.organization_id() != organization_id
            || membership.status() != af_domain::OrganizationMembershipStatus::Active
        {
            return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
        }
        let access = organization_repository
            .authorized_access(organization_id, membership.user_id())
            .await
            .map_err(map_invited_binding_error)?;
        if !access.entitlement().sso_enabled() {
            return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
        }
        let now = current_timestamp()?;
        let has_valid_domain = self
            .repository
            .list_domains(organization_id)
            .await
            .map_err(map_repository_error)?
            .into_iter()
            .any(|domain| {
                domain.provider_id() == provider.id()
                    && domain.status() == af_domain::OrganizationSsoDomainStatus::Verified
                    && domain
                        .recheck_at()
                        .is_some_and(|recheck_at| recheck_at > now)
            });
        if !has_valid_domain {
            return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
        }
        Ok(membership)
    }

    async fn exchange_ticket_and_bind_invited_identity_inner(
        &self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> Result<OrganizationMembershipRecord, OrganizationSsoLoginTransactionServiceError> {
        let exchanged = self.exchange_ticket_inner(organization_id, ticket).await?;
        if exchanged.protocol() != OrganizationSsoProtocol::Oidc {
            return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
        }
        let subject_digest = exchanged
            .subject_digest()
            .ok_or(OrganizationSsoLoginTransactionServiceError::Rejected)?
            .to_owned();
        let invitation_email_fingerprint = exchanged
            .invitation_email_fingerprint()
            .copied()
            .ok_or(OrganizationSsoLoginTransactionServiceError::Rejected)?;
        let organization_repository = self
            .organization_repository
            .as_ref()
            .ok_or(OrganizationSsoLoginTransactionServiceError::Unavailable)?;
        let provider = self
            .repository
            .provider_by_id(organization_id, exchanged.provider_id())
            .await
            .map_err(map_repository_error)?;
        if provider.protocol() != OrganizationSsoProtocol::Oidc
            || provider.version() != exchanged.provider_version()
            || provider.status() != OrganizationSsoProviderStatus::Active
        {
            return Err(OrganizationSsoLoginTransactionServiceError::Rejected);
        }
        organization_repository
            .bind_invited_sso_identity(
                organization_id,
                provider.provider_key(),
                subject_digest,
                invitation_email_fingerprint,
            )
            .await
            .map_err(map_invited_binding_error)
    }
}

impl OrganizationSsoLoginTransactionService for DatabaseOrganizationSsoLoginTransactionService {
    fn begin<'a>(
        &'a self,
        organization_id: OrganizationId,
        provider_key: String,
        request_digest: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, OrganizationSsoLoginTransactionStart> {
        Box::pin(self.begin_inner(organization_id, provider_key, request_digest))
    }

    fn claim<'a>(
        &'a self,
        organization_id: OrganizationId,
        state: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, ClaimedOrganizationSsoLoginTransaction> {
        Box::pin(self.claim_inner(organization_id, state))
    }

    fn claim_callback<'a>(
        &'a self,
        state: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, ClaimedOrganizationSsoLoginTransaction> {
        Box::pin(self.claim_callback_inner(state))
    }

    fn complete_verified<'a>(
        &'a self,
        organization_id: OrganizationId,
        transaction_id: i64,
        assertion_digest: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, IssuedOrganizationSsoLoginTicket> {
        Box::pin(self.complete_verified_inner(
            organization_id,
            transaction_id,
            assertion_digest,
            None,
        ))
    }

    fn complete_rejected<'a>(
        &'a self,
        organization_id: OrganizationId,
        transaction_id: i64,
    ) -> OrganizationSsoLoginTransactionFuture<'a, ()> {
        Box::pin(self.complete_rejected_inner(organization_id, transaction_id))
    }

    fn exchange_ticket<'a>(
        &'a self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, ExchangedOrganizationSsoLoginTicket> {
        Box::pin(self.exchange_ticket_inner(organization_id, ticket))
    }

    fn exchange_ticket_and_resolve_identity<'a>(
        &'a self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, OrganizationMembershipRecord> {
        Box::pin(self.exchange_ticket_and_resolve_identity_inner(organization_id, ticket))
    }

    fn exchange_ticket_and_bind_invited_identity<'a>(
        &'a self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, OrganizationMembershipRecord> {
        Box::pin(self.exchange_ticket_and_bind_invited_identity_inner(organization_id, ticket))
    }

    fn complete_oidc_callback<'a>(
        &'a self,
        claimed: ClaimedOrganizationSsoLoginTransaction,
        code: String,
    ) -> OrganizationSsoLoginTransactionFuture<'a, IssuedOrganizationSsoLoginTicket> {
        Box::pin(self.complete_oidc_callback_inner(claimed, code))
    }
}

impl fmt::Debug for DatabaseOrganizationSsoLoginTransactionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationSsoLoginTransactionService(<repository>)")
    }
}

fn envelope_json(envelope: &EncryptedCredentialEnvelope) -> Value {
    json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    })
}

fn current_timestamp() -> Result<i64, OrganizationSsoLoginTransactionServiceError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| OrganizationSsoLoginTransactionServiceError::Internal)?
        .as_secs();
    i64::try_from(seconds).map_err(|_| OrganizationSsoLoginTransactionServiceError::Internal)
}

fn expires_at(ttl_seconds: i64) -> Result<i64, OrganizationSsoLoginTransactionServiceError> {
    current_timestamp()?
        .checked_add(ttl_seconds)
        .ok_or(OrganizationSsoLoginTransactionServiceError::Internal)
}

fn generate_ticket() -> Result<Zeroizing<String>, OrganizationSsoLoginTransactionServiceError> {
    let mut bytes = [0_u8; TICKET_RANDOM_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|_| OrganizationSsoLoginTransactionServiceError::Internal)?;
    let ticket = URL_SAFE_NO_PAD.encode(bytes);
    bytes.zeroize();
    Ok(Zeroizing::new(ticket))
}

const fn map_login_material_error(
    error: OrganizationSsoLoginError,
) -> OrganizationSsoLoginTransactionServiceError {
    match error {
        OrganizationSsoLoginError::InvalidDigest
        | OrganizationSsoLoginError::InvalidState
        | OrganizationSsoLoginError::InvalidTicket
        | OrganizationSsoLoginError::InvalidLoginMaterial
        | OrganizationSsoLoginError::MissingCallbackField
        | OrganizationSsoLoginError::UnexpectedCallbackField
        | OrganizationSsoLoginError::CallbackTooLarge => {
            OrganizationSsoLoginTransactionServiceError::InvalidInput
        }
        OrganizationSsoLoginError::Entropy => OrganizationSsoLoginTransactionServiceError::Internal,
    }
}

const fn map_repository_error(
    error: OrganizationSsoRepositoryError,
) -> OrganizationSsoLoginTransactionServiceError {
    match error {
        OrganizationSsoRepositoryError::InvalidInput => {
            OrganizationSsoLoginTransactionServiceError::InvalidInput
        }
        OrganizationSsoRepositoryError::NotFound => {
            OrganizationSsoLoginTransactionServiceError::Rejected
        }
        OrganizationSsoRepositoryError::Conflict | OrganizationSsoRepositoryError::Unavailable => {
            OrganizationSsoLoginTransactionServiceError::Unavailable
        }
        OrganizationSsoRepositoryError::AlreadyProcessed => {
            OrganizationSsoLoginTransactionServiceError::AlreadyProcessed
        }
        OrganizationSsoRepositoryError::Expired => {
            OrganizationSsoLoginTransactionServiceError::Expired
        }
        OrganizationSsoRepositoryError::InvalidTransition => {
            OrganizationSsoLoginTransactionServiceError::InvalidTransition
        }
        OrganizationSsoRepositoryError::Query
        | OrganizationSsoRepositoryError::Timeout
        | OrganizationSsoRepositoryError::Invariant => {
            OrganizationSsoLoginTransactionServiceError::Internal
        }
    }
}

const fn map_invited_binding_error(
    error: OrganizationRepositoryError,
) -> OrganizationSsoLoginTransactionServiceError {
    match error {
        OrganizationRepositoryError::InvalidInput
        | OrganizationRepositoryError::NotFound
        | OrganizationRepositoryError::UserUnavailable
        | OrganizationRepositoryError::Conflict
        | OrganizationRepositoryError::InvalidTransition
        | OrganizationRepositoryError::LastOwner
        | OrganizationRepositoryError::TeamHasActiveMembers
        | OrganizationRepositoryError::EntitlementRestricted
        | OrganizationRepositoryError::SeatLimitReached
        | OrganizationRepositoryError::InvitationExpired
        | OrganizationRepositoryError::InvitationNotPending
        | OrganizationRepositoryError::InvitationEmailMismatch
        | OrganizationRepositoryError::InvitationBatchConflict
        | OrganizationRepositoryError::TeamLimitReached => {
            OrganizationSsoLoginTransactionServiceError::Rejected
        }
        OrganizationRepositoryError::EntropyUnavailable
        | OrganizationRepositoryError::Query
        | OrganizationRepositoryError::Timeout
        | OrganizationRepositoryError::Invariant => {
            OrganizationSsoLoginTransactionServiceError::Internal
        }
    }
}
