use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    MAX_ORGANIZATION_TOKEN_PAGE_SIZE, OrganizationTokenCreateRecord,
    OrganizationTokenLookupOutcome, OrganizationTokenMutationOutcome, OrganizationTokenRecord,
    OrganizationTokenRepository, OrganizationTokenRepositoryError, OrganizationTokenWriteRecord,
};
use af_domain::{OrganizationPermission, OrganizationTeamId, TokenId, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    IssuedApiKey, OrganizationContext, PresentedApiKey,
    token_write::{
        validate_ip_allowlist, validate_model_limits, validate_name, validate_non_negative,
    },
};

pub const DEFAULT_ORGANIZATION_TOKEN_PAGE_SIZE: usize = 50;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OrganizationTokenStatus {
    Enabled,
    Disabled,
}

impl OrganizationTokenStatus {
    fn from_database(value: i16) -> Result<Self, OrganizationTokenError> {
        match value {
            1 => Ok(Self::Enabled),
            2 => Ok(Self::Disabled),
            _ => Err(OrganizationTokenError::Internal),
        }
    }

    const fn database_value(self) -> i16 {
        match self {
            Self::Enabled => 1,
            Self::Disabled => 2,
        }
    }
}

pub struct OrganizationToken {
    token_id: TokenId,
    owner_user_id: UserId,
    key_prefix: String,
    name: String,
    status: OrganizationTokenStatus,
    team_id: Option<OrganizationTeamId>,
    remain_quota: i64,
    unlimited_quota: bool,
    used_quota: i64,
    expired_at: Option<i64>,
    model_limits: Option<Vec<String>>,
    allow_ips: Option<Vec<String>>,
    rate_limit_5h: Option<i64>,
    rate_limit_1d: Option<i64>,
    rate_limit_7d: Option<i64>,
    usage_5h: i64,
    usage_1d: i64,
    usage_7d: i64,
    max_requests: Option<i64>,
    used_requests: i64,
    created_at: i64,
    updated_at: i64,
}

impl OrganizationToken {
    fn from_record(record: OrganizationTokenRecord) -> Result<Self, OrganizationTokenError> {
        let team_id = record.team_id();
        let token = record.token();
        Ok(Self {
            token_id: token.token_id(),
            owner_user_id: token.user_id(),
            key_prefix: token.key_prefix().to_owned(),
            name: token.name().to_owned(),
            status: OrganizationTokenStatus::from_database(token.status())?,
            team_id,
            remain_quota: token.remain_quota(),
            unlimited_quota: token.unlimited_quota(),
            used_quota: token.used_quota(),
            expired_at: token.expired_at(),
            model_limits: token.model_limits().map(<[String]>::to_vec),
            allow_ips: token.allow_ips().map(<[String]>::to_vec),
            rate_limit_5h: token.rate_limit_5h(),
            rate_limit_1d: token.rate_limit_1d(),
            rate_limit_7d: token.rate_limit_7d(),
            usage_5h: token.usage_5h(),
            usage_1d: token.usage_1d(),
            usage_7d: token.usage_7d(),
            max_requests: token.max_requests(),
            used_requests: token.used_requests(),
            created_at: token.created_at(),
            updated_at: token.updated_at(),
        })
    }

    #[must_use]
    pub const fn token_id(&self) -> TokenId {
        self.token_id
    }
    #[must_use]
    pub const fn owner_user_id(&self) -> UserId {
        self.owner_user_id
    }
    #[must_use]
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationTokenStatus {
        self.status
    }
    #[must_use]
    pub const fn team_id(&self) -> Option<OrganizationTeamId> {
        self.team_id
    }
    #[must_use]
    pub const fn remain_quota(&self) -> i64 {
        self.remain_quota
    }
    #[must_use]
    pub const fn unlimited_quota(&self) -> bool {
        self.unlimited_quota
    }
    #[must_use]
    pub const fn used_quota(&self) -> i64 {
        self.used_quota
    }
    #[must_use]
    pub const fn expired_at(&self) -> Option<i64> {
        self.expired_at
    }
    #[must_use]
    pub fn model_limits(&self) -> Option<&[String]> {
        self.model_limits.as_deref()
    }
    #[must_use]
    pub fn allow_ips(&self) -> Option<&[String]> {
        self.allow_ips.as_deref()
    }
    #[must_use]
    pub const fn rate_limit_5h(&self) -> Option<i64> {
        self.rate_limit_5h
    }
    #[must_use]
    pub const fn rate_limit_1d(&self) -> Option<i64> {
        self.rate_limit_1d
    }
    #[must_use]
    pub const fn rate_limit_7d(&self) -> Option<i64> {
        self.rate_limit_7d
    }
    #[must_use]
    pub const fn usage_5h(&self) -> i64 {
        self.usage_5h
    }
    #[must_use]
    pub const fn usage_1d(&self) -> i64 {
        self.usage_1d
    }
    #[must_use]
    pub const fn usage_7d(&self) -> i64 {
        self.usage_7d
    }
    #[must_use]
    pub const fn max_requests(&self) -> Option<i64> {
        self.max_requests
    }
    #[must_use]
    pub const fn used_requests(&self) -> i64 {
        self.used_requests
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

impl fmt::Debug for OrganizationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationToken(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationTokenListQuery {
    after: Option<TokenId>,
    limit: usize,
}

impl OrganizationTokenListQuery {
    pub fn new(after: Option<TokenId>, limit: usize) -> Result<Self, OrganizationTokenError> {
        if !(1..=MAX_ORGANIZATION_TOKEN_PAGE_SIZE).contains(&limit) {
            return Err(OrganizationTokenError::InvalidInput);
        }
        Ok(Self { after, limit })
    }

    #[must_use]
    pub const fn after(self) -> Option<TokenId> {
        self.after
    }
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for OrganizationTokenListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ORGANIZATION_TOKEN_PAGE_SIZE,
        }
    }
}

pub struct OrganizationTokenPage {
    tokens: Vec<OrganizationToken>,
    next_cursor: Option<TokenId>,
}

impl OrganizationTokenPage {
    #[must_use]
    pub fn tokens(&self) -> &[OrganizationToken] {
        &self.tokens
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<TokenId> {
        self.next_cursor
    }
}

pub struct OrganizationTokenWriteCommand {
    name: String,
    status: OrganizationTokenStatus,
    remain_quota: i64,
    unlimited_quota: bool,
    expired_at: Option<i64>,
    model_limits: Option<Vec<String>>,
    allow_ips: Option<Vec<String>>,
    rate_limit_5h: Option<i64>,
    rate_limit_1d: Option<i64>,
    rate_limit_7d: Option<i64>,
    max_requests: Option<i64>,
}

impl OrganizationTokenWriteCommand {
    #[allow(clippy::too_many_arguments, reason = "字段与企业 Key 表单一一对应")]
    pub fn new(
        name: String,
        status: OrganizationTokenStatus,
        remain_quota: i64,
        unlimited_quota: bool,
        expired_at: Option<i64>,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
        rate_limit_5h: Option<i64>,
        rate_limit_1d: Option<i64>,
        rate_limit_7d: Option<i64>,
        max_requests: Option<i64>,
    ) -> Result<Self, OrganizationTokenError> {
        validate_name(&name).map_err(|_| OrganizationTokenError::InvalidInput)?;
        validate_non_negative(remain_quota).map_err(|_| OrganizationTokenError::InvalidInput)?;
        if expired_at.is_some_and(|value| value < 0)
            || [rate_limit_5h, rate_limit_1d, rate_limit_7d, max_requests]
                .into_iter()
                .flatten()
                .any(|value| value < 0)
        {
            return Err(OrganizationTokenError::InvalidInput);
        }
        validate_model_limits(model_limits.as_deref())
            .map_err(|_| OrganizationTokenError::InvalidInput)?;
        validate_ip_allowlist(allow_ips.as_deref())
            .map_err(|_| OrganizationTokenError::InvalidInput)?;
        Ok(Self {
            name,
            status,
            remain_quota,
            unlimited_quota,
            expired_at,
            model_limits,
            allow_ips,
            rate_limit_5h,
            rate_limit_1d,
            rate_limit_7d,
            max_requests,
        })
    }

    fn into_record(self) -> OrganizationTokenWriteRecord {
        OrganizationTokenWriteRecord::new(
            self.name,
            self.status.database_value(),
            self.remain_quota,
            self.unlimited_quota,
            self.expired_at,
            self.model_limits,
            self.allow_ips,
            self.rate_limit_5h,
            self.rate_limit_1d,
            self.rate_limit_7d,
            self.max_requests,
        )
    }
}

impl fmt::Debug for OrganizationTokenWriteCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationTokenWriteCommand(<redacted>)")
    }
}

pub struct IssuedOrganizationToken {
    token: OrganizationToken,
    api_key: IssuedApiKey,
}

impl IssuedOrganizationToken {
    #[must_use]
    pub const fn token(&self) -> &OrganizationToken {
        &self.token
    }
    #[must_use]
    pub const fn api_key(&self) -> &PresentedApiKey {
        self.api_key.key()
    }
}

impl fmt::Debug for IssuedOrganizationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IssuedOrganizationToken(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationTokenError {
    #[error("企业 API Key 参数无效")]
    InvalidInput,
    #[error("企业 API Key 权限不足")]
    Forbidden,
    #[error("企业 API Key 不存在")]
    NotFound,
    #[error("企业 API Key 数量已达上限")]
    LimitReached,
    #[error("企业 API Key 操作结果未知")]
    OutcomeUnknown,
    #[error("企业 API Key 内部失败")]
    Internal,
}

pub type OrganizationTokenListFuture<'a> = Pin<
    Box<dyn Future<Output = Result<OrganizationTokenPage, OrganizationTokenError>> + Send + 'a>,
>;
pub type OrganizationTokenGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<OrganizationToken, OrganizationTokenError>> + Send + 'a>>;
pub type OrganizationTokenCreateFuture<'a> = Pin<
    Box<dyn Future<Output = Result<IssuedOrganizationToken, OrganizationTokenError>> + Send + 'a>,
>;
pub type OrganizationTokenUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<OrganizationToken, OrganizationTokenError>> + Send + 'a>>;

pub trait OrganizationTokenService: Send + Sync {
    fn list_self(
        &self,
        context: OrganizationContext,
        query: OrganizationTokenListQuery,
    ) -> OrganizationTokenListFuture<'_>;
    fn list_all(
        &self,
        context: OrganizationContext,
        query: OrganizationTokenListQuery,
    ) -> OrganizationTokenListFuture<'_>;
    fn get_self(
        &self,
        context: OrganizationContext,
        token_id: TokenId,
    ) -> OrganizationTokenGetFuture<'_>;
    fn create(
        &self,
        context: OrganizationContext,
        command: OrganizationTokenWriteCommand,
    ) -> OrganizationTokenCreateFuture<'_>;
    fn update_self(
        &self,
        context: OrganizationContext,
        token_id: TokenId,
        command: OrganizationTokenWriteCommand,
    ) -> OrganizationTokenUpdateFuture<'_>;
    fn disable_self(
        &self,
        context: OrganizationContext,
        token_id: TokenId,
    ) -> OrganizationTokenUpdateFuture<'_>;
    fn disable_any(
        &self,
        context: OrganizationContext,
        token_id: TokenId,
    ) -> OrganizationTokenUpdateFuture<'_>;
}

pub struct DatabaseOrganizationTokenService {
    repository: Arc<OrganizationTokenRepository>,
}

impl DatabaseOrganizationTokenService {
    #[must_use]
    pub fn new(repository: Arc<OrganizationTokenRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationTokenService for DatabaseOrganizationTokenService {
    fn list_self(
        &self,
        context: OrganizationContext,
        query: OrganizationTokenListQuery,
    ) -> OrganizationTokenListFuture<'_> {
        Box::pin(async move {
            require(&context, OrganizationPermission::KeysManageSelf)?;
            list(
                &self.repository,
                context.organization_id(),
                Some(context.user_id()),
                query,
            )
            .await
        })
    }

    fn list_all(
        &self,
        context: OrganizationContext,
        query: OrganizationTokenListQuery,
    ) -> OrganizationTokenListFuture<'_> {
        Box::pin(async move {
            require(&context, OrganizationPermission::KeysReadAll)?;
            list(&self.repository, context.organization_id(), None, query).await
        })
    }

    fn get_self(
        &self,
        context: OrganizationContext,
        token_id: TokenId,
    ) -> OrganizationTokenGetFuture<'_> {
        Box::pin(async move {
            require(&context, OrganizationPermission::KeysManageSelf)?;
            match self
                .repository
                .get(context.organization_id(), Some(context.user_id()), token_id)
                .await
                .map_err(map_error)?
            {
                OrganizationTokenLookupOutcome::Found(record) => {
                    OrganizationToken::from_record(*record)
                }
                OrganizationTokenLookupOutcome::NotFound => Err(OrganizationTokenError::NotFound),
            }
        })
    }

    fn create(
        &self,
        context: OrganizationContext,
        command: OrganizationTokenWriteCommand,
    ) -> OrganizationTokenCreateFuture<'_> {
        Box::pin(async move {
            require(&context, OrganizationPermission::KeysManageSelf)?;
            let issued = IssuedApiKey::generate().map_err(|_| OrganizationTokenError::Internal)?;
            let record = OrganizationTokenCreateRecord::new(
                context.organization_id(),
                context.membership_id(),
                context.user_id(),
                issued.digest().as_str().to_owned(),
                issued.display_prefix().as_str().to_owned(),
                command.into_record(),
            );
            let token = self
                .repository
                .create(record)
                .await
                .map_err(map_error)
                .and_then(OrganizationToken::from_record)?;
            Ok(IssuedOrganizationToken {
                token,
                api_key: issued,
            })
        })
    }

    fn update_self(
        &self,
        context: OrganizationContext,
        token_id: TokenId,
        command: OrganizationTokenWriteCommand,
    ) -> OrganizationTokenUpdateFuture<'_> {
        Box::pin(async move {
            require(&context, OrganizationPermission::KeysManageSelf)?;
            mutation(
                self.repository
                    .update_self(
                        context.organization_id(),
                        context.user_id(),
                        token_id,
                        command.into_record(),
                    )
                    .await,
            )
            .await
        })
    }

    fn disable_self(
        &self,
        context: OrganizationContext,
        token_id: TokenId,
    ) -> OrganizationTokenUpdateFuture<'_> {
        Box::pin(async move {
            require(&context, OrganizationPermission::KeysManageSelf)?;
            mutation(
                self.repository
                    .disable(context.organization_id(), token_id, Some(context.user_id()))
                    .await,
            )
            .await
        })
    }

    fn disable_any(
        &self,
        context: OrganizationContext,
        token_id: TokenId,
    ) -> OrganizationTokenUpdateFuture<'_> {
        Box::pin(async move {
            require(&context, OrganizationPermission::KeysReadAll)?;
            mutation(
                self.repository
                    .disable(context.organization_id(), token_id, None)
                    .await,
            )
            .await
        })
    }
}

async fn list(
    repository: &OrganizationTokenRepository,
    organization_id: af_domain::OrganizationId,
    owner_user_id: Option<UserId>,
    query: OrganizationTokenListQuery,
) -> Result<OrganizationTokenPage, OrganizationTokenError> {
    let page = repository
        .list(organization_id, owner_user_id, query.after(), query.limit())
        .await
        .map_err(map_error)?;
    let (records, next_cursor) = page.into_parts();
    let tokens = records
        .into_iter()
        .map(OrganizationToken::from_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OrganizationTokenPage {
        tokens,
        next_cursor,
    })
}

async fn mutation(
    result: Result<OrganizationTokenMutationOutcome, OrganizationTokenRepositoryError>,
) -> Result<OrganizationToken, OrganizationTokenError> {
    match result.map_err(map_error)? {
        OrganizationTokenMutationOutcome::Mutated(record) => {
            OrganizationToken::from_record(*record)
        }
        OrganizationTokenMutationOutcome::NotFound => Err(OrganizationTokenError::NotFound),
    }
}

fn require(
    context: &OrganizationContext,
    permission: OrganizationPermission,
) -> Result<(), OrganizationTokenError> {
    context
        .allows(permission)
        .then_some(())
        .ok_or(OrganizationTokenError::Forbidden)
}

const fn map_error(error: OrganizationTokenRepositoryError) -> OrganizationTokenError {
    match error {
        OrganizationTokenRepositoryError::InvalidInput => OrganizationTokenError::InvalidInput,
        OrganizationTokenRepositoryError::NotFound => OrganizationTokenError::NotFound,
        OrganizationTokenRepositoryError::Restricted => OrganizationTokenError::Forbidden,
        OrganizationTokenRepositoryError::LimitReached => OrganizationTokenError::LimitReached,
        OrganizationTokenRepositoryError::OutcomeUnknown => OrganizationTokenError::OutcomeUnknown,
        OrganizationTokenRepositoryError::Query
        | OrganizationTokenRepositoryError::Timeout
        | OrganizationTokenRepositoryError::Invariant => OrganizationTokenError::Internal,
    }
}

impl fmt::Debug for DatabaseOrganizationTokenService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationTokenService(<redacted>)")
    }
}
