use af_domain::{GroupId, IpCidr, TrustedClientIp, UserId};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    sea_query::{Condition, Expr},
};

use super::{
    PersonalAccessTokenAuditIdentity, PersonalAccessTokenLookup, PersonalAccessTokenPageRecord,
    PersonalAccessTokenPolicyRecord, PersonalAccessTokenRecord,
    PersonalAccessTokenRejectionReason as Reason, PersonalAccessTokenRepository as Repository,
    PersonalAccessTokenRepositoryError as Error, validation,
};
use crate::entity::{
    access_token_policies as policies, groups, user_access_tokens as tokens, users,
};

pub(super) async fn list(
    repository: &Repository,
    owner: UserId,
    after: Option<i64>,
    limit: usize,
) -> Result<PersonalAccessTokenPageRecord, Error> {
    let user = users::Entity::find_by_id(owner.get())
        .one(repository.pool.connection())
        .await
        .map_err(|_| Error::Query)?
        .ok_or(Error::OwnerUnavailable)?;
    if user.status != 1 || user.deleted_at.is_some() {
        return Err(Error::OwnerUnavailable);
    }
    let mut query = tokens::Entity::find()
        .filter(tokens::Column::UserId.eq(owner.get()))
        .filter(tokens::Column::RevokedAt.is_null())
        .order_by_desc(tokens::Column::Id)
        .limit((limit + 1) as u64);
    if let Some(after) = after {
        query = query.filter(tokens::Column::Id.lt(after));
    }
    let mut models = query
        .all(repository.pool.connection())
        .await
        .map_err(|_| Error::Query)?;
    let has_more = models.len() > limit;
    models.truncate(limit);
    let next_cursor = has_more
        .then(|| models.last().map(|model| model.id))
        .flatten();
    let tokens = models
        .into_iter()
        .map(record)
        .collect::<Result<Vec<_>, _>>()?;
    let capacity = policy(repository).await?.max_tokens_per_user;
    Ok(PersonalAccessTokenPageRecord {
        tokens,
        next_cursor,
        capacity,
    })
}

pub(super) async fn policy(
    repository: &Repository,
) -> Result<PersonalAccessTokenPolicyRecord, Error> {
    let model = policies::Entity::find_by_id(1_i64)
        .one(repository.pool.connection())
        .await
        .map_err(|_| Error::Query)?
        .ok_or(Error::Invariant)?;
    policy_record(model)
}

pub(super) fn policy_record(
    model: policies::Model,
) -> Result<PersonalAccessTokenPolicyRecord, Error> {
    if model.id != 1 || !(1..=1000).contains(&model.max_tokens_per_user) || model.version <= 0 {
        return Err(Error::Invariant);
    }
    Ok(PersonalAccessTokenPolicyRecord {
        enabled: model.enabled,
        issuance_enabled: model.issuance_enabled,
        max_tokens_per_user: model.max_tokens_per_user as usize,
        version: model.version,
    })
}

pub(super) async fn authenticate(
    repository: &Repository,
    digest: &str,
    client_ip: TrustedClientIp,
) -> Result<PersonalAccessTokenLookup, Error> {
    let Some(token) = tokens::Entity::find()
        .filter(tokens::Column::SecretHash.eq(digest))
        .one(repository.pool.connection())
        .await
        .map_err(|_| Error::Query)?
    else {
        return Ok(PersonalAccessTokenLookup::Rejected(None));
    };
    let user_id = UserId::new(token.user_id).map_err(|_| Error::Invariant)?;
    let reject = |reason| {
        PersonalAccessTokenLookup::Rejected(Some(PersonalAccessTokenAuditIdentity {
            user_id,
            token_id: token.id,
            reason,
        }))
    };
    let Some(user) = users::Entity::find_by_id(token.user_id)
        .one(repository.pool.connection())
        .await
        .map_err(|_| Error::Query)?
    else {
        return Ok(reject(Reason::AccountUnavailable));
    };
    if user.status != 1 || user.deleted_at.is_some() {
        return Ok(reject(Reason::AccountUnavailable));
    }
    if !policy(repository).await?.enabled {
        return Ok(reject(Reason::PolicyDisabled));
    }
    if !token.enabled || token.revoked_at.is_some() {
        return Ok(reject(Reason::Disabled));
    }
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    if token.expires_at.is_some_and(|expiry| expiry <= now) {
        return Ok(reject(Reason::Expired));
    }
    let mut snapshot = record(token.clone())?;
    if let Some(ips) = snapshot.allow_ips.as_ref() {
        let networks = ips
            .iter()
            .map(|ip| ip.parse::<IpCidr>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Error::Invariant)?;
        if !networks.iter().any(|network| network.contains(client_ip)) {
            return Ok(reject(Reason::IpDenied));
        }
    }
    snapshot.scopes = validation::effective_grant(user.role, snapshot.scopes)?;
    if snapshot.scopes.is_empty() {
        return Ok(reject(Reason::Invalid));
    }
    let group_id = GroupId::new(user.default_group_id).map_err(|_| Error::Invariant)?;
    if groups::Entity::find_by_id(group_id.get())
        .filter(groups::Column::DeletedAt.is_null())
        .one(repository.pool.connection())
        .await
        .map_err(|_| Error::Query)?
        .is_none()
    {
        return Ok(reject(Reason::AccountUnavailable));
    }
    // Metadata sampling is intentionally independent of the authoritative authorization read.
    if token
        .last_used_at
        .is_none_or(|last| now.saturating_sub(last) >= 60)
    {
        let _ = tokens::Entity::update_many()
            .filter(tokens::Column::Id.eq(token.id))
            .filter(
                Condition::any()
                    .add(tokens::Column::LastUsedAt.is_null())
                    .add(tokens::Column::LastUsedAt.lte(now - 60)),
            )
            .col_expr(tokens::Column::LastUsedAt, Expr::value(now))
            .col_expr(
                tokens::Column::LastUsedIp,
                Expr::value(client_ip.as_ip().to_string()),
            )
            .exec(repository.pool.connection())
            .await;
    }
    Ok(PersonalAccessTokenLookup::Authenticated {
        user_id,
        role: user.role,
        group_id,
        token: snapshot,
    })
}

pub(super) fn record(model: tokens::Model) -> Result<PersonalAccessTokenRecord, Error> {
    if model.id <= 0
        || model.version <= 0
        || model.created_at < 0
        || model.updated_at < model.created_at
        || !super::valid_digest(&model.secret_hash)
        || !validation::text(&model.name, 128)
        || !model.token_hint.starts_with("af_pat_")
        || model.token_hint.len() != 19
    {
        return Err(Error::Invariant);
    }
    let scopes = validation::json_strings(&model.scopes)?;
    if scopes.is_empty()
        || scopes.len() > super::MAX_PERSONAL_ACCESS_TOKEN_SCOPES
        || scopes.iter().any(|scope| !validation::valid_scope(scope))
    {
        return Err(Error::Invariant);
    }
    let allow_ips = validation::json_ips(model.allow_ips.as_ref())?;
    Ok(PersonalAccessTokenRecord {
        id: model.id,
        user_id: UserId::new(model.user_id).map_err(|_| Error::Invariant)?,
        token_hint: model.token_hint,
        name: model.name,
        scopes,
        enabled: model.enabled,
        expires_at: model.expires_at,
        allow_ips,
        created_at: model.created_at,
        updated_at: model.updated_at,
        last_used_at: model.last_used_at,
        last_used_ip: model.last_used_ip,
        version: model.version,
    })
}
