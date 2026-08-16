use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

use af_account::{
    OrganizationInvitationCipher, OrganizationInvitationCipherError,
    PlainOrganizationInvitationEmail,
};
use af_db::OrganizationMembershipRecord;
use af_db::{
    MAX_ORGANIZATION_INVITATION_BATCH_SIZE, OrganizationInvitationCreate,
    OrganizationInvitationPreviewItem, OrganizationInvitationRecord, OrganizationRepository,
    OrganizationRepositoryError,
};
use af_domain::{
    OrganizationId, OrganizationInvitationKey, OrganizationRole, OrganizationTeamId, UserId,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use getrandom::fill as random_fill;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::SessionPrincipal;

const TOKEN_PREFIX: &str = "oinv_";
const TOKEN_SECRET_BYTES: usize = 32;
/// 默认邀请有效期：七天。
pub const DEFAULT_ORGANIZATION_INVITATION_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
/// 邀请最短有效期：一小时。
pub const MIN_ORGANIZATION_INVITATION_TTL_SECONDS: i64 = 60 * 60;
/// 邀请最长有效期：三十天。
pub const MAX_ORGANIZATION_INVITATION_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;

pub type OrganizationInvitationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, OrganizationInvitationError>> + Send + 'a>>;

/// 管理端单条邀请命令；邮箱只在安全服务边界短暂存在。
pub struct OrganizationInvitationCommand {
    email: String,
    role: OrganizationRole,
    team_id: Option<OrganizationTeamId>,
    expires_at: i64,
}

impl OrganizationInvitationCommand {
    /// 创建邀请命令并校验角色、团队和绝对过期时间。
    pub fn new(
        email: String,
        role: OrganizationRole,
        team_id: Option<OrganizationTeamId>,
        expires_at: i64,
    ) -> Result<Self, OrganizationInvitationError> {
        if role.is_owner() {
            return Err(OrganizationInvitationError::InvalidInput);
        }
        Ok(Self {
            email,
            role,
            team_id,
            expires_at,
        })
    }
}

impl std::fmt::Debug for OrganizationInvitationCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OrganizationInvitationCommand")
            .field("role", &self.role)
            .field("has_team", &self.team_id.is_some())
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// 批量邀请命令；批次键由客户端生成但必须是固定小写十六进制。
pub struct OrganizationInvitationBatchCommand {
    batch_key: OrganizationInvitationKey,
    items: Vec<OrganizationInvitationCommand>,
}

/// 批量预览命令；不需要幂等键，也不会生成任何令牌或密文。
pub struct OrganizationInvitationPreviewCommand {
    items: Vec<OrganizationInvitationCommand>,
}

impl OrganizationInvitationPreviewCommand {
    /// 创建最多 200 项的预览命令。
    pub fn new(
        items: Vec<OrganizationInvitationCommand>,
    ) -> Result<Self, OrganizationInvitationError> {
        if items.is_empty() || items.len() > MAX_ORGANIZATION_INVITATION_BATCH_SIZE {
            return Err(OrganizationInvitationError::BatchTooLarge);
        }
        Ok(Self { items })
    }
}

/// 预览逐行问题；客户端可按稳定码展示并阻止提交问题行。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationInvitationPreviewIssue {
    InvalidEmail,
    InvalidExpiration,
    DuplicateEmail,
    ExistingMember,
    PendingInvitation,
    InvalidTeam,
    SeatUnavailable,
}

impl OrganizationInvitationPreviewIssue {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidEmail => "invalid_email",
            Self::InvalidExpiration => "invalid_expiration",
            Self::DuplicateEmail => "duplicate_email",
            Self::ExistingMember => "existing_member",
            Self::PendingInvitation => "pending_invitation",
            Self::InvalidTeam => "invalid_team",
            Self::SeatUnavailable => "seat_unavailable",
        }
    }
}

/// 预览结果中的单条行状态。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationInvitationPreviewItemResult {
    index: usize,
    issues: Vec<OrganizationInvitationPreviewIssue>,
}

impl OrganizationInvitationPreviewItemResult {
    fn new(index: usize) -> Self {
        Self {
            index,
            issues: Vec::new(),
        }
    }

    fn add_issue(&mut self, issue: OrganizationInvitationPreviewIssue) {
        if !self.issues.contains(&issue) {
            self.issues.push(issue);
        }
    }

    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    #[must_use]
    pub fn issues(&self) -> &[OrganizationInvitationPreviewIssue] {
        &self.issues
    }

    #[must_use]
    pub fn ready(&self) -> bool {
        self.issues.is_empty()
    }
}

/// 批量预览的安全结果，不包含邮箱、令牌或密码学材料。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationInvitationPreviewResult {
    items: Vec<OrganizationInvitationPreviewItemResult>,
    available_seats: i64,
}

impl OrganizationInvitationPreviewResult {
    fn new(items: Vec<OrganizationInvitationPreviewItemResult>, available_seats: i64) -> Self {
        Self {
            items,
            available_seats,
        }
    }

    #[must_use]
    pub fn items(&self) -> &[OrganizationInvitationPreviewItemResult] {
        &self.items
    }

    #[must_use]
    pub const fn available_seats(&self) -> i64 {
        self.available_seats
    }

    #[must_use]
    pub fn ready(&self) -> bool {
        self.items
            .iter()
            .all(OrganizationInvitationPreviewItemResult::ready)
    }
}

impl OrganizationInvitationBatchCommand {
    /// 创建最多 200 项的批量命令。
    pub fn new(
        batch_key: OrganizationInvitationKey,
        items: Vec<OrganizationInvitationCommand>,
    ) -> Result<Self, OrganizationInvitationError> {
        if items.is_empty() || items.len() > MAX_ORGANIZATION_INVITATION_BATCH_SIZE {
            return Err(OrganizationInvitationError::BatchTooLarge);
        }
        Ok(Self { batch_key, items })
    }
}

/// 管理端返回的邀请摘要和一次性投递令牌。
#[derive(Clone, Debug)]
pub struct IssuedOrganizationInvitation {
    record: OrganizationInvitationRecord,
    token: Option<String>,
}

/// 批量提交结果；批次接口不应向客户端导出其中的一次性令牌。
#[derive(Clone, Debug)]
pub struct OrganizationInvitationBatchResult {
    invitations: Vec<IssuedOrganizationInvitation>,
    replayed: bool,
}

impl OrganizationInvitationBatchResult {
    #[must_use]
    pub fn invitations(&self) -> &[IssuedOrganizationInvitation] {
        &self.invitations
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

impl IssuedOrganizationInvitation {
    /// 返回邀请安全投影。
    #[must_use]
    pub fn record(&self) -> &OrganizationInvitationRecord {
        &self.record
    }

    /// 返回仅首次创建/重发响应提供的明文令牌。
    #[must_use]
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }
}

/// 邀请接受成功后返回的企业和成员关系。
#[derive(Clone, Debug)]
pub struct AcceptedOrganizationInvitation {
    pub organization_id: OrganizationId,
    pub membership: OrganizationMembershipRecord,
}

/// 企业邀请服务错误；不携带邮箱、企业标识、令牌或数据库详情。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationInvitationError {
    #[error("企业邀请输入无效")]
    InvalidInput,
    #[error("企业邀请批次超过容量")]
    BatchTooLarge,
    #[error("当前企业角色无权管理邀请")]
    Forbidden,
    #[error("企业邀请或企业不存在")]
    NotFound,
    #[error("企业邀请状态冲突")]
    Conflict,
    #[error("企业席位容量已用尽")]
    SeatLimitReached,
    #[error("企业邀请已过期")]
    Expired,
    #[error("企业邀请邮箱不匹配")]
    EmailMismatch,
    #[error("企业邀请服务内部失败")]
    Internal,
}

/// 企业邀请服务；授权由 HTTP 层先通过独立企业上下文校验。
pub trait OrganizationInvitationService: Send + Sync {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        command: OrganizationInvitationCommand,
    ) -> OrganizationInvitationFuture<'a, IssuedOrganizationInvitation>;
    fn create_batch<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        command: OrganizationInvitationBatchCommand,
    ) -> OrganizationInvitationFuture<'a, OrganizationInvitationBatchResult>;
    fn preview_batch<'a>(
        &'a self,
        organization_id: OrganizationId,
        command: OrganizationInvitationPreviewCommand,
    ) -> OrganizationInvitationFuture<'a, OrganizationInvitationPreviewResult>;
    fn list<'a>(
        &'a self,
        organization_id: OrganizationId,
    ) -> OrganizationInvitationFuture<'a, Vec<OrganizationInvitationRecord>>;
    fn revoke<'a>(
        &'a self,
        organization_id: OrganizationId,
        invitation_key: OrganizationInvitationKey,
    ) -> OrganizationInvitationFuture<'a, OrganizationInvitationRecord>;
    fn resend<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        invitation_key: OrganizationInvitationKey,
    ) -> OrganizationInvitationFuture<'a, IssuedOrganizationInvitation>;
    fn accept<'a>(
        &'a self,
        principal: SessionPrincipal,
        token: String,
    ) -> OrganizationInvitationFuture<'a, AcceptedOrganizationInvitation>;
}

/// 使用数据库仓储和启动期邀请密码学边界的实现。
pub struct DatabaseOrganizationInvitationService {
    repository: Arc<OrganizationRepository>,
    cipher: OrganizationInvitationCipher,
}

impl DatabaseOrganizationInvitationService {
    /// 绑定企业仓储和已校验密码学配置。
    #[must_use]
    pub fn new(
        repository: Arc<OrganizationRepository>,
        cipher: OrganizationInvitationCipher,
    ) -> Self {
        Self { repository, cipher }
    }
}

impl OrganizationInvitationService for DatabaseOrganizationInvitationService {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        command: OrganizationInvitationCommand,
    ) -> OrganizationInvitationFuture<'a, IssuedOrganizationInvitation> {
        Box::pin(async move {
            let (write, token) =
                self.material(organization_id, principal.user_id(), command, None)?;
            let record = self
                .repository
                .create_invitation(organization_id, write)
                .await
                .map_err(map_repository_error)?;
            Ok(IssuedOrganizationInvitation {
                record,
                token: Some(token),
            })
        })
    }

    fn create_batch<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        command: OrganizationInvitationBatchCommand,
    ) -> OrganizationInvitationFuture<'a, OrganizationInvitationBatchResult> {
        Box::pin(async move {
            let digest = batch_digest(&command);
            let item_count = command.items.len();
            let mut writes = Vec::with_capacity(item_count);
            let mut tokens = Vec::with_capacity(item_count);
            for item in command.items {
                let (write, token) = self.material(
                    organization_id,
                    principal.user_id(),
                    item,
                    Some((command.batch_key, digest, item_count)),
                )?;
                writes.push(write);
                tokens.push(token);
            }
            let records = self
                .repository
                .create_invitation_batch(organization_id, writes)
                .await
                .map_err(map_repository_error)?;
            let replayed = records.replayed();
            let records = records.records().to_vec();
            let invitations = records
                .into_iter()
                .enumerate()
                .map(|(index, record)| IssuedOrganizationInvitation {
                    record,
                    token: if replayed {
                        None
                    } else {
                        Some(tokens[index].clone())
                    },
                })
                .collect();
            Ok(OrganizationInvitationBatchResult {
                invitations,
                replayed,
            })
        })
    }

    fn preview_batch<'a>(
        &'a self,
        organization_id: OrganizationId,
        command: OrganizationInvitationPreviewCommand,
    ) -> OrganizationInvitationFuture<'a, OrganizationInvitationPreviewResult> {
        Box::pin(async move {
            let mut results = (0..command.items.len())
                .map(OrganizationInvitationPreviewItemResult::new)
                .collect::<Vec<_>>();
            let mut valid_items = Vec::new();
            let mut fingerprints = BTreeSet::new();
            for (index, item) in command.items.iter().enumerate() {
                let normalized = match normalize_email(&item.email) {
                    Ok(value) => value,
                    Err(_) => {
                        results[index].add_issue(OrganizationInvitationPreviewIssue::InvalidEmail);
                        String::new()
                    }
                };
                if !valid_expiration(item.expires_at) {
                    results[index].add_issue(OrganizationInvitationPreviewIssue::InvalidExpiration);
                }
                if normalized.is_empty() {
                    continue;
                }
                let plain = PlainOrganizationInvitationEmail::new(normalized.clone())
                    .map_err(map_cipher_error)?;
                let fingerprint = self.cipher.email_fingerprint(organization_id, &plain);
                if !fingerprints.insert(fingerprint) {
                    results[index].add_issue(OrganizationInvitationPreviewIssue::DuplicateEmail);
                }
                valid_items.push((
                    index,
                    OrganizationInvitationPreviewItem::new(normalized, fingerprint, item.team_id),
                ));
            }
            let snapshot = if valid_items.is_empty() {
                None
            } else {
                Some(
                    self.repository
                        .preview_invitations(
                            organization_id,
                            valid_items.iter().map(|(_, item)| item.clone()).collect(),
                        )
                        .await
                        .map_err(map_repository_error)?,
                )
            };
            if let Some(snapshot) = snapshot {
                for ((index, _), row) in valid_items.iter().zip(snapshot.rows()) {
                    if row.existing_member() {
                        results[*index]
                            .add_issue(OrganizationInvitationPreviewIssue::ExistingMember);
                    }
                    if row.pending_invitation() {
                        results[*index]
                            .add_issue(OrganizationInvitationPreviewIssue::PendingInvitation);
                    }
                    if !row.valid_team() {
                        results[*index].add_issue(OrganizationInvitationPreviewIssue::InvalidTeam);
                    }
                }
                let mut remaining = snapshot.available_seats();
                for result in &mut results {
                    if result.ready() {
                        if remaining > 0 {
                            remaining -= 1;
                        } else {
                            result.add_issue(OrganizationInvitationPreviewIssue::SeatUnavailable);
                        }
                    }
                }
                return Ok(OrganizationInvitationPreviewResult::new(
                    results,
                    snapshot.available_seats(),
                ));
            }
            Ok(OrganizationInvitationPreviewResult::new(results, 0))
        })
    }

    fn list<'a>(
        &'a self,
        organization_id: OrganizationId,
    ) -> OrganizationInvitationFuture<'a, Vec<OrganizationInvitationRecord>> {
        Box::pin(async move {
            self.repository
                .list_invitations(organization_id)
                .await
                .map_err(map_repository_error)
        })
    }

    fn revoke<'a>(
        &'a self,
        organization_id: OrganizationId,
        invitation_key: OrganizationInvitationKey,
    ) -> OrganizationInvitationFuture<'a, OrganizationInvitationRecord> {
        Box::pin(async move {
            self.repository
                .revoke_invitation(organization_id, invitation_key)
                .await
                .map_err(map_repository_error)
        })
    }

    fn resend<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        invitation_key: OrganizationInvitationKey,
    ) -> OrganizationInvitationFuture<'a, IssuedOrganizationInvitation> {
        Box::pin(async move {
            let context = self
                .repository
                .invitation_acceptance_context(invitation_key)
                .await
                .map_err(map_repository_error)?;
            let (old_record, envelope) = context.into_parts();
            if old_record.organization_id() != organization_id {
                return Err(OrganizationInvitationError::NotFound);
            }
            let email = self
                .cipher
                .decrypt_email(organization_id, invitation_key, &envelope)
                .map_err(map_cipher_error)?;
            let command = OrganizationInvitationCommand::new(
                email.expose_email().to_owned(),
                old_record.role(),
                old_record.team_id(),
                now().saturating_add(DEFAULT_ORGANIZATION_INVITATION_TTL_SECONDS),
            )?;
            let (write, token) =
                self.material(organization_id, principal.user_id(), command, None)?;
            let record = self
                .repository
                .resend_invitation(organization_id, invitation_key, write)
                .await
                .map_err(map_repository_error)?;
            Ok(IssuedOrganizationInvitation {
                record,
                token: Some(token),
            })
        })
    }

    fn accept<'a>(
        &'a self,
        principal: SessionPrincipal,
        token: String,
    ) -> OrganizationInvitationFuture<'a, AcceptedOrganizationInvitation> {
        Box::pin(async move {
            let (invitation_key, token_secret) = parse_token(&token)?;
            let context = self
                .repository
                .invitation_acceptance_context(invitation_key)
                .await
                .map_err(map_repository_error)?;
            let (record, envelope) = context.into_parts();
            let organization_id = record.organization_id();
            let email = self
                .cipher
                .decrypt_email(organization_id, invitation_key, &envelope)
                .map_err(map_cipher_error)?;
            let plain = PlainOrganizationInvitationEmail::new(email.expose_email().to_owned())
                .map_err(map_cipher_error)?;
            let fingerprint = self.cipher.email_fingerprint(organization_id, &plain);
            let digest = self
                .cipher
                .token_digest(organization_id, invitation_key, &token_secret);
            let membership = self
                .repository
                .accept_invitation(
                    organization_id,
                    invitation_key,
                    digest,
                    fingerprint,
                    principal.user_id(),
                    plain.expose_email().to_owned(),
                )
                .await
                .map_err(map_repository_error)?;
            Ok(AcceptedOrganizationInvitation {
                organization_id,
                membership,
            })
        })
    }
}

impl DatabaseOrganizationInvitationService {
    fn material(
        &self,
        organization_id: OrganizationId,
        inviter: UserId,
        command: OrganizationInvitationCommand,
        batch: Option<(OrganizationInvitationKey, [u8; 32], usize)>,
    ) -> Result<(OrganizationInvitationCreate, String), OrganizationInvitationError> {
        if !valid_expiration(command.expires_at) {
            return Err(OrganizationInvitationError::InvalidInput);
        }
        let email = normalize_email(&command.email)?;
        let plain = PlainOrganizationInvitationEmail::new(email).map_err(map_cipher_error)?;
        let invitation_key = random_key()?;
        let mut token_secret = Zeroizing::new([0_u8; TOKEN_SECRET_BYTES]);
        random_fill(token_secret.as_mut()).map_err(|_| OrganizationInvitationError::Internal)?;
        let token = format!(
            "{TOKEN_PREFIX}{}.{}",
            invitation_key.persistence_key(),
            URL_SAFE_NO_PAD.encode(token_secret.as_slice())
        );
        let email_fingerprint = self.cipher.email_fingerprint(organization_id, &plain);
        let token_digest = self
            .cipher
            .token_digest(organization_id, invitation_key, &token_secret);
        let email_secret = self
            .cipher
            .encrypt_email(organization_id, invitation_key, &plain)
            .map_err(map_cipher_error)?;
        let (batch_key, batch_digest, batch_size) = batch
            .map(|(key, digest, size)| (Some(key), Some(digest), Some(size)))
            .unwrap_or((None, None, None));
        let write = OrganizationInvitationCreate::new(
            invitation_key,
            plain.expose_email().to_owned(),
            email_fingerprint,
            email_secret,
            token_digest,
            command.role,
            command.team_id,
            inviter,
            batch_key,
            batch_digest,
            batch_size,
            command.expires_at,
        )
        .map_err(map_repository_error)?;
        Ok((write, token))
    }
}

fn valid_expiration(expires_at: i64) -> bool {
    let now = now();
    expires_at > now
        && expires_at.saturating_sub(now) >= MIN_ORGANIZATION_INVITATION_TTL_SECONDS
        && expires_at.saturating_sub(now) <= MAX_ORGANIZATION_INVITATION_TTL_SECONDS
}

fn normalize_email(value: &str) -> Result<String, OrganizationInvitationError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 320
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
    {
        return Err(OrganizationInvitationError::InvalidInput);
    }
    let value = value.to_ascii_lowercase();
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || local.starts_with('.')
        || local.ends_with('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return Err(OrganizationInvitationError::InvalidInput);
    }
    Ok(value)
}

fn random_key() -> Result<OrganizationInvitationKey, OrganizationInvitationError> {
    for _ in 0..4 {
        let mut bytes = [0_u8; 16];
        random_fill(&mut bytes).map_err(|_| OrganizationInvitationError::Internal)?;
        if let Ok(key) = OrganizationInvitationKey::new(bytes) {
            return Ok(key);
        }
    }
    Err(OrganizationInvitationError::Internal)
}

fn parse_token(
    token: &str,
) -> Result<(OrganizationInvitationKey, [u8; TOKEN_SECRET_BYTES]), OrganizationInvitationError> {
    let Some(value) = token.strip_prefix(TOKEN_PREFIX) else {
        return Err(OrganizationInvitationError::InvalidInput);
    };
    let (key, secret) = value
        .split_once('.')
        .ok_or(OrganizationInvitationError::InvalidInput)?;
    let key = OrganizationInvitationKey::from_persistence_key(key)
        .map_err(|_| OrganizationInvitationError::InvalidInput)?;
    let decoded = URL_SAFE_NO_PAD
        .decode(secret)
        .map_err(|_| OrganizationInvitationError::InvalidInput)?;
    let token_secret = decoded
        .try_into()
        .map_err(|_| OrganizationInvitationError::InvalidInput)?;
    Ok((key, token_secret))
}

fn batch_digest(command: &OrganizationInvitationBatchCommand) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"anyflows:organization-invitation-batch:v1:");
    for item in &command.items {
        digest.update(item.email.as_bytes());
        digest.update([0]);
        digest.update(item.role.code().to_be_bytes());
        digest.update(
            item.team_id
                .map(OrganizationTeamId::get)
                .unwrap_or_default()
                .to_be_bytes(),
        );
        digest.update(item.expires_at.to_be_bytes());
    }
    digest.finalize().into()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| i64::try_from(value.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn map_cipher_error(error: OrganizationInvitationCipherError) -> OrganizationInvitationError {
    match error {
        OrganizationInvitationCipherError::InvalidEmail => {
            OrganizationInvitationError::InvalidInput
        }
        OrganizationInvitationCipherError::MissingKey
        | OrganizationInvitationCipherError::InvalidKey
        | OrganizationInvitationCipherError::Entropy
        | OrganizationInvitationCipherError::Encrypt
        | OrganizationInvitationCipherError::InvalidEnvelope
        | OrganizationInvitationCipherError::KeyMismatch
        | OrganizationInvitationCipherError::Decrypt => OrganizationInvitationError::Internal,
    }
}

fn map_repository_error(error: OrganizationRepositoryError) -> OrganizationInvitationError {
    match error {
        OrganizationRepositoryError::NotFound => OrganizationInvitationError::NotFound,
        OrganizationRepositoryError::Conflict
        | OrganizationRepositoryError::InvitationBatchConflict
        | OrganizationRepositoryError::InvitationNotPending => {
            OrganizationInvitationError::Conflict
        }
        OrganizationRepositoryError::SeatLimitReached => {
            OrganizationInvitationError::SeatLimitReached
        }
        OrganizationRepositoryError::InvitationExpired => OrganizationInvitationError::Expired,
        OrganizationRepositoryError::InvitationEmailMismatch => {
            OrganizationInvitationError::EmailMismatch
        }
        OrganizationRepositoryError::InvalidInput => OrganizationInvitationError::InvalidInput,
        OrganizationRepositoryError::Query
        | OrganizationRepositoryError::Timeout
        | OrganizationRepositoryError::Invariant
        | OrganizationRepositoryError::UserUnavailable
        | OrganizationRepositoryError::InvalidTransition
        | OrganizationRepositoryError::LastOwner
        | OrganizationRepositoryError::TeamHasActiveMembers
        | OrganizationRepositoryError::EntitlementRestricted
        | OrganizationRepositoryError::TeamLimitReached
        | OrganizationRepositoryError::EntropyUnavailable => OrganizationInvitationError::Internal,
    }
}

impl std::fmt::Debug for DatabaseOrganizationInvitationService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DatabaseOrganizationInvitationService(<repository,cipher>)")
    }
}
