use std::{collections::BTreeMap, future::Future, time::Duration};

use af_domain::{
    ChannelId, ChannelType, ClientProfileBinding, ClientProfileDefinition, ClientProfileSnapshot,
    CredentialKind, Protocol, UserId, validate_client_profile_key,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, TransactionTrait, sea_query::Expr,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, DatabaseTimestamp, SchedulerCatalogSubject,
    channel_settings::{client_simulation_body_profile, client_simulation_profile},
    entity::{
        channel_client_profiles as bindings, channels, client_profile_revisions as revisions,
        client_profiles as profiles,
    },
    scheduler_outbox::enqueue_scheduler_catalog_change,
};

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ClientProfileRepositoryError {
    #[error("客户端档案输入无效")]
    InvalidInput,
    #[error("客户端档案不存在")]
    NotFound,
    #[error("客户端档案已变更、受保护或仍被引用")]
    Conflict,
    #[error("客户端档案数据库访问失败")]
    Storage,
    #[error("客户端档案数据库访问超时")]
    Timeout,
    #[error("客户端档案持久化状态无效")]
    Invariant,
}

type Error = ClientProfileRepositoryError;

#[derive(Clone, Debug, Default)]
pub struct ClientProfileQuery {
    pub after: Option<String>,
    pub search: Option<String>,
    pub enabled: Option<bool>,
    pub service: Option<String>,
    pub protocol: Option<Protocol>,
    pub credential_kind: Option<CredentialKind>,
    /// 零表示默认每页 25 条。
    pub limit: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClientProfileRecord {
    pub key: String,
    pub name: String,
    pub service: String,
    pub built_in: bool,
    pub enabled: bool,
    pub latest_revision: i64,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClientProfilePage {
    pub items: Vec<ClientProfileRecord>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClientProfileRevisionPage {
    pub items: Vec<ClientProfileSnapshot>,
    pub next_cursor: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClientProfileReference {
    pub channel_id: i64,
    pub binding: ClientProfileBinding,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClientProfileReferencePage {
    pub items: Vec<ClientProfileReference>,
    pub next_cursor: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientProfileWrite {
    pub expected_version: i64,
    pub definition: ClientProfileDefinition,
}

#[derive(Clone, Debug)]
pub struct ClientProfileRepository {
    pool: DatabasePool,
}

impl ClientProfileRepository {
    pub const fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    async fn bounded<T>(&self, work: impl Future<Output = Result<T, Error>>) -> Result<T, Error> {
        timeout(
            Duration::from_secs(5),
            work.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| Error::Timeout)?
    }

    pub async fn list(&self, query: ClientProfileQuery) -> Result<ClientProfilePage, Error> {
        let limit = page_size(query.limit)?;
        if let Some(key) = &query.after {
            validate_key(key)?;
        }
        if query
            .search
            .as_deref()
            .is_some_and(|q| q.len() > 128 || q.chars().any(char::is_control))
        {
            return Err(Error::InvalidInput);
        }
        if query
            .service
            .as_deref()
            .is_some_and(|s| !matches!(s, "standard" | "claude_code" | "codex" | "open_code"))
        {
            return Err(Error::InvalidInput);
        }
        self.bounded(async {
            let mut select = profiles::Entity::find().order_by_asc(profiles::Column::ProfileKey);
            if let Some(after) = query.after {
                select = select.filter(profiles::Column::ProfileKey.gt(after));
            }
            if let Some(q) = query.search.filter(|q| !q.is_empty()) {
                select = select.filter(
                    Condition::any()
                        .add(profiles::Column::Name.contains(&q))
                        .add(profiles::Column::ProfileKey.contains(q)),
                );
            }
            if let Some(enabled) = query.enabled {
                select = select.filter(profiles::Column::Enabled.eq(enabled));
            }
            if let Some(service) = query.service {
                select = select.filter(profiles::Column::Service.eq(service));
            }
            if let Some(protocol) = query.protocol {
                select = select.filter(
                    profiles::Column::Protocols.contains(format!(",{},", protocol.as_str())),
                );
            }
            if let Some(kind) = query.credential_kind {
                select = select.filter(
                    profiles::Column::CredentialKinds.contains(format!(",{},", kind.as_str())),
                );
            }
            let mut models = select
                .limit((limit + 1) as u64)
                .all(self.pool.connection())
                .await
                .map_err(storage)?;
            let has_more = models.len() > limit;
            models.truncate(limit);
            let items = models.into_iter().map(record).collect::<Vec<_>>();
            let next_cursor = has_more
                .then(|| items.last().map(|r| r.key.clone()))
                .flatten();
            Ok(ClientProfilePage { items, next_cursor })
        })
        .await
    }

    pub async fn get(&self, key: &str) -> Result<ClientProfileRecord, Error> {
        validate_key(key)?;
        self.bounded(async {
            profiles::Entity::find_by_id(key.to_owned())
                .one(self.pool.connection())
                .await
                .map_err(storage)?
                .map(record)
                .ok_or(Error::NotFound)
        })
        .await
    }

    pub async fn revision(&self, key: &str, revision: i64) -> Result<ClientProfileSnapshot, Error> {
        validate_binding(key, revision)?;
        self.bounded(find_revision(self.pool.connection(), key, revision))
            .await
    }

    pub async fn revisions(
        &self,
        key: &str,
        before: Option<i64>,
        limit: usize,
    ) -> Result<ClientProfileRevisionPage, Error> {
        validate_key(key)?;
        let limit = page_size(limit)?;
        if before.is_some_and(|v| v < 1) {
            return Err(Error::InvalidInput);
        }
        self.bounded(async {
            let mut select = revisions::Entity::find()
                .filter(revisions::Column::ProfileKey.eq(key))
                .order_by_desc(revisions::Column::Revision);
            if let Some(before) = before {
                select = select.filter(revisions::Column::Revision.lt(before));
            }
            let mut rows = select
                .limit((limit + 1) as u64)
                .all(self.pool.connection())
                .await
                .map_err(storage)?;
            let more = rows.len() > limit;
            rows.truncate(limit);
            let items = rows
                .into_iter()
                .map(snapshot)
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = more
                .then(|| items.last().map(|r| r.binding.revision))
                .flatten();
            Ok(ClientProfileRevisionPage { items, next_cursor })
        })
        .await
    }

    /// 版本 CAS 与新修订写入同事务；不更新任何已有渠道引用。
    pub async fn save(
        &self,
        key: &str,
        write: ClientProfileWrite,
        operator: UserId,
    ) -> Result<ClientProfileSnapshot, Error> {
        validate_key(key)?;
        write
            .definition
            .validate()
            .map_err(|_| Error::InvalidInput)?;
        if write.expected_version < 0 {
            return Err(Error::InvalidInput);
        }
        self.bounded(async {
            let tx = self.pool.connection().begin().await.map_err(storage)?;
            let current = profiles::Entity::find_by_id(key.to_owned())
                .lock_exclusive()
                .one(&tx)
                .await
                .map_err(storage)?;
            let now = DatabaseTimestamp::now_utc();
            let revision = if let Some(current) = current {
                if current.built_in || current.version != write.expected_version {
                    return Err(Error::Conflict);
                }
                let next = current
                    .latest_revision
                    .checked_add(1)
                    .ok_or(Error::Invariant)?;
                let result = profiles::Entity::update_many()
                    .filter(profiles::Column::ProfileKey.eq(key))
                    .filter(profiles::Column::Version.eq(current.version))
                    .col_expr(
                        profiles::Column::Name,
                        Expr::value(write.definition.name.clone()),
                    )
                    .col_expr(
                        profiles::Column::Service,
                        Expr::value(service_name(&write.definition)?),
                    )
                    .col_expr(
                        profiles::Column::Protocols,
                        Expr::value(protocol_names(&write.definition)),
                    )
                    .col_expr(
                        profiles::Column::CredentialKinds,
                        Expr::value(credential_names(&write.definition)),
                    )
                    .col_expr(profiles::Column::LatestRevision, Expr::value(next))
                    .col_expr(
                        profiles::Column::Version,
                        Expr::value(current.version.checked_add(1).ok_or(Error::Invariant)?),
                    )
                    .col_expr(profiles::Column::UpdatedAt, Expr::value(now))
                    .exec(&tx)
                    .await
                    .map_err(storage)?;
                if result.rows_affected != 1 {
                    return Err(Error::Conflict);
                }
                next
            } else {
                if write.expected_version != 0
                    || matches!(
                        key,
                        "standard" | "claude-code" | "claude-code-date" | "codex" | "opencode"
                    )
                {
                    return Err(Error::Conflict);
                }
                profiles::ActiveModel {
                    profile_key: Set(key.to_owned()),
                    name: Set(write.definition.name.clone()),
                    service: Set(service_name(&write.definition)?),
                    protocols: Set(protocol_names(&write.definition)),
                    credential_kinds: Set(credential_names(&write.definition)),
                    built_in: Set(false),
                    enabled: Set(true),
                    latest_revision: Set(1),
                    version: Set(1),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(&tx)
                .await
                .map_err(|_| Error::Conflict)?;
                1
            };
            revisions::ActiveModel {
                profile_key: Set(key.to_owned()),
                revision: Set(revision),
                definition: Set(
                    serde_json::to_value(&write.definition).map_err(|_| Error::Invariant)?
                ),
                created_by: Set(Some(operator.get())),
                created_at: Set(now),
            }
            .insert(&tx)
            .await
            .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            Ok(ClientProfileSnapshot {
                binding: ClientProfileBinding {
                    key: key.to_owned(),
                    revision,
                },
                definition: write.definition,
            })
        })
        .await
    }

    /// 停用通过现有 outbox 移除所引用渠道；正在发送的请求持有旧快照。
    pub async fn set_enabled(
        &self,
        key: &str,
        expected_version: i64,
        enabled: bool,
    ) -> Result<ClientProfileRecord, Error> {
        validate_key(key)?;
        if expected_version < 1 {
            return Err(Error::InvalidInput);
        }
        self.bounded(async {
            let tx = self.pool.connection().begin().await.map_err(storage)?;
            let current = profiles::Entity::find_by_id(key.to_owned())
                .lock_exclusive()
                .one(&tx)
                .await
                .map_err(storage)?
                .ok_or(Error::NotFound)?;
            if current.version != expected_version {
                return Err(Error::Conflict);
            }
            let now = DatabaseTimestamp::now_utc();
            let result = profiles::Entity::update_many()
                .filter(profiles::Column::ProfileKey.eq(key))
                .filter(profiles::Column::Version.eq(expected_version))
                .col_expr(profiles::Column::Enabled, Expr::value(enabled))
                .col_expr(
                    profiles::Column::Version,
                    Expr::value(expected_version.checked_add(1).ok_or(Error::Invariant)?),
                )
                .col_expr(profiles::Column::UpdatedAt, Expr::value(now))
                .exec(&tx)
                .await
                .map_err(storage)?;
            if result.rows_affected != 1 {
                return Err(Error::Conflict);
            }
            let references = bindings::Entity::find()
                .filter(bindings::Column::ProfileKey.eq(key))
                .all(&tx)
                .await
                .map_err(storage)?;
            for reference in references {
                publish(&tx, reference.channel_id, now).await?;
            }
            let result = profiles::Entity::find_by_id(key.to_owned())
                .one(&tx)
                .await
                .map_err(storage)?
                .ok_or(Error::Invariant)?;
            tx.commit().await.map_err(storage)?;
            Ok(record(result))
        })
        .await
    }

    pub async fn delete_revision(
        &self,
        key: &str,
        revision: i64,
        expected_version: i64,
    ) -> Result<(), Error> {
        validate_binding(key, revision)?;
        self.bounded(async {
            let tx = self.pool.connection().begin().await.map_err(storage)?;
            let profile = profiles::Entity::find_by_id(key.to_owned())
                .lock_exclusive()
                .one(&tx)
                .await
                .map_err(storage)?
                .ok_or(Error::NotFound)?;
            if profile.built_in
                || profile.latest_revision == revision
                || profile.version != expected_version
            {
                return Err(Error::Conflict);
            }
            if bindings::Entity::find()
                .filter(bindings::Column::ProfileKey.eq(key))
                .filter(bindings::Column::Revision.eq(revision))
                .one(&tx)
                .await
                .map_err(storage)?
                .is_some()
            {
                return Err(Error::Conflict);
            }
            let result = revisions::Entity::delete_by_id((key.to_owned(), revision))
                .exec(&tx)
                .await
                .map_err(|_| Error::Conflict)?;
            if result.rows_affected != 1 {
                return Err(Error::NotFound);
            }
            let result = profiles::Entity::update_many()
                .filter(profiles::Column::ProfileKey.eq(key))
                .filter(profiles::Column::Version.eq(expected_version))
                .col_expr(
                    profiles::Column::Version,
                    Expr::value(expected_version.checked_add(1).ok_or(Error::Invariant)?),
                )
                .col_expr(
                    profiles::Column::UpdatedAt,
                    Expr::value(DatabaseTimestamp::now_utc()),
                )
                .exec(&tx)
                .await
                .map_err(storage)?;
            if result.rows_affected != 1 {
                return Err(Error::Conflict);
            }
            tx.commit().await.map_err(storage)
        })
        .await
    }

    pub async fn references(
        &self,
        key: &str,
        after: Option<i64>,
        limit: usize,
    ) -> Result<ClientProfileReferencePage, Error> {
        validate_key(key)?;
        let limit = page_size(limit)?;
        if after.is_some_and(|v| v < 1) {
            return Err(Error::InvalidInput);
        }
        self.bounded(async {
            let mut select = bindings::Entity::find()
                .filter(bindings::Column::ProfileKey.eq(key))
                .order_by_asc(bindings::Column::ChannelId);
            if let Some(after) = after {
                select = select.filter(bindings::Column::ChannelId.gt(after));
            }
            let mut rows = select
                .limit((limit + 1) as u64)
                .all(self.pool.connection())
                .await
                .map_err(storage)?;
            let more = rows.len() > limit;
            rows.truncate(limit);
            let items = rows.into_iter().map(reference).collect::<Vec<_>>();
            let next_cursor = more.then(|| items.last().map(|r| r.channel_id)).flatten();
            Ok(ClientProfileReferencePage { items, next_cursor })
        })
        .await
    }

    pub async fn channel_binding(
        &self,
        channel: ChannelId,
    ) -> Result<Option<ClientProfileReference>, Error> {
        self.bounded(async {
            Ok(bindings::Entity::find_by_id(channel.get())
                .one(self.pool.connection())
                .await
                .map_err(storage)?
                .map(reference))
        })
        .await
    }

    /// 固定修订并校验身份头冲突；不清除或覆盖历史 Claude 设置。
    pub async fn bind(
        &self,
        channel: ChannelId,
        binding: ClientProfileBinding,
        expected_version: i64,
    ) -> Result<ClientProfileReference, Error> {
        binding.validate().map_err(|_| Error::InvalidInput)?;
        if expected_version < 0 {
            return Err(Error::InvalidInput);
        }
        self.bounded(async {
            let tx = self.pool.connection().begin().await.map_err(storage)?;
            let profile = profiles::Entity::find_by_id(binding.key.clone())
                .lock_exclusive()
                .one(&tx)
                .await
                .map_err(storage)?
                .ok_or(Error::NotFound)?;
            if !profile.enabled {
                return Err(Error::Conflict);
            }
            let snapshot = find_revision(&tx, &binding.key, binding.revision).await?;
            let channel_record = channels::Entity::find_by_id(channel.get())
                .lock_exclusive()
                .one(&tx)
                .await
                .map_err(storage)?
                .filter(|c| c.deleted_at.is_none())
                .ok_or(Error::NotFound)?;
            validate_profile_channel(&snapshot, &channel_record)?;
            let current = bindings::Entity::find_by_id(channel.get())
                .one(&tx)
                .await
                .map_err(storage)?;
            let now = DatabaseTimestamp::now_utc();
            let version = if let Some(current) = current {
                if current.version != expected_version {
                    return Err(Error::Conflict);
                }
                let version = expected_version.checked_add(1).ok_or(Error::Invariant)?;
                let result = bindings::Entity::update_many()
                    .filter(bindings::Column::ChannelId.eq(channel.get()))
                    .filter(bindings::Column::Version.eq(expected_version))
                    .col_expr(
                        bindings::Column::ProfileKey,
                        Expr::value(binding.key.clone()),
                    )
                    .col_expr(bindings::Column::Revision, Expr::value(binding.revision))
                    .col_expr(bindings::Column::Version, Expr::value(version))
                    .col_expr(bindings::Column::UpdatedAt, Expr::value(now))
                    .exec(&tx)
                    .await
                    .map_err(storage)?;
                if result.rows_affected != 1 {
                    return Err(Error::Conflict);
                }
                version
            } else {
                if expected_version != 0 {
                    return Err(Error::Conflict);
                }
                bindings::ActiveModel {
                    channel_id: Set(channel.get()),
                    profile_key: Set(binding.key.clone()),
                    revision: Set(binding.revision),
                    version: Set(1),
                    updated_at: Set(now),
                }
                .insert(&tx)
                .await
                .map_err(|_| Error::Conflict)?;
                1
            };
            publish(&tx, channel.get(), now).await?;
            tx.commit().await.map_err(storage)?;
            Ok(ClientProfileReference {
                channel_id: channel.get(),
                binding,
                version,
            })
        })
        .await
    }
}

/// 批量解析渠道绑定；None 表示明确停用，缺失键表示未选择新档案。
pub(crate) async fn runtime_profiles<C: ConnectionTrait>(
    connection: &C,
    channel_ids: &[i64],
) -> Result<BTreeMap<i64, Option<ClientProfileSnapshot>>, Error> {
    let mut resolved = BTreeMap::new();
    for batch in channel_ids.chunks(400) {
        let rows = bindings::Entity::find()
            .filter(bindings::Column::ChannelId.is_in(batch.iter().copied()))
            .all(connection)
            .await
            .map_err(storage)?;
        if rows.is_empty() {
            continue;
        }
        let keys = rows
            .iter()
            .map(|r| r.profile_key.clone())
            .collect::<Vec<_>>();
        let profiles = profiles::Entity::find()
            .filter(profiles::Column::ProfileKey.is_in(keys))
            .all(connection)
            .await
            .map_err(storage)?
            .into_iter()
            .map(|p| (p.profile_key.clone(), p))
            .collect::<BTreeMap<_, _>>();
        let predicate = rows.iter().fold(Condition::any(), |c, r| {
            c.add(
                Condition::all()
                    .add(revisions::Column::ProfileKey.eq(&r.profile_key))
                    .add(revisions::Column::Revision.eq(r.revision)),
            )
        });
        let revisions = revisions::Entity::find()
            .filter(predicate)
            .all(connection)
            .await
            .map_err(storage)?
            .into_iter()
            .map(|r| Ok(((r.profile_key.clone(), r.revision), snapshot(r)?)))
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        for row in rows {
            let enabled = profiles
                .get(&row.profile_key)
                .ok_or(Error::Invariant)?
                .enabled;
            let revision = revisions
                .get(&(row.profile_key, row.revision))
                .ok_or(Error::Invariant)?
                .clone();
            resolved.insert(row.channel_id, enabled.then_some(revision));
        }
    }
    Ok(resolved)
}

pub(crate) fn validate_profile_channel(
    snapshot: &ClientProfileSnapshot,
    channel: &channels::Model,
) -> Result<(), Error> {
    let kind: ChannelType = channel.r#type.parse().map_err(|_| Error::Invariant)?;
    let protocol: Protocol = channel.protocol.parse().map_err(|_| Error::Invariant)?;
    if !snapshot.definition.matches_channel(kind, protocol) {
        return Err(Error::InvalidInput);
    }
    let settings = channel.settings.clone().into_inner();
    let legacy_header = client_simulation_profile(&settings).map_err(|_| Error::Invariant)?;
    let legacy_body = client_simulation_body_profile(&settings).map_err(|_| Error::Invariant)?;
    if legacy_header.is_some() {
        let expected = if legacy_body.is_some() {
            "claude-code-date"
        } else {
            "claude-code"
        };
        if snapshot.binding.key != expected || snapshot.binding.revision != 1 {
            return Err(Error::Conflict);
        }
    } else if legacy_body.is_some() {
        return Err(Error::Conflict);
    }
    let headers = channel
        .header_override
        .pairs()
        .map_err(|_| Error::Invariant)?;
    // 旧版身份中间件本就覆盖这些值；迁移须保留其优先级和可用性。
    if legacy_header.is_none()
        && headers.iter().any(|(k, _)| {
            snapshot
                .definition
                .identity_headers
                .keys()
                .any(|p| p.eq_ignore_ascii_case(k))
        })
    {
        return Err(Error::Conflict);
    }
    Ok(())
}

pub(crate) async fn validate_existing_profile_binding<C: ConnectionTrait>(
    connection: &C,
    channel: &channels::Model,
) -> Result<(), Error> {
    if let Some(binding) = bindings::Entity::find_by_id(channel.id)
        .lock_exclusive()
        .one(connection)
        .await
        .map_err(storage)?
    {
        let profile = find_revision(connection, &binding.profile_key, binding.revision).await?;
        validate_profile_channel(&profile, channel)?;
    }
    Ok(())
}

async fn find_revision<C: ConnectionTrait>(
    connection: &C,
    key: &str,
    revision: i64,
) -> Result<ClientProfileSnapshot, Error> {
    snapshot(
        revisions::Entity::find_by_id((key.to_owned(), revision))
            .one(connection)
            .await
            .map_err(storage)?
            .ok_or(Error::NotFound)?,
    )
}

fn snapshot(model: revisions::Model) -> Result<ClientProfileSnapshot, Error> {
    let snapshot = ClientProfileSnapshot {
        binding: ClientProfileBinding {
            key: model.profile_key,
            revision: model.revision,
        },
        definition: serde_json::from_value(model.definition).map_err(|_| Error::Invariant)?,
    };
    snapshot.validate().map_err(|_| Error::Invariant)?;
    Ok(snapshot)
}
fn record(model: profiles::Model) -> ClientProfileRecord {
    ClientProfileRecord {
        key: model.profile_key,
        name: model.name,
        service: model.service,
        built_in: model.built_in,
        enabled: model.enabled,
        latest_revision: model.latest_revision,
        version: model.version,
    }
}
fn reference(model: bindings::Model) -> ClientProfileReference {
    ClientProfileReference {
        channel_id: model.channel_id,
        binding: ClientProfileBinding {
            key: model.profile_key,
            revision: model.revision,
        },
        version: model.version,
    }
}
fn page_size(limit: usize) -> Result<usize, Error> {
    match limit {
        0 => Ok(25),
        1..=100 => Ok(limit),
        _ => Err(Error::InvalidInput),
    }
}
fn validate_key(key: &str) -> Result<(), Error> {
    validate_client_profile_key(key).map_err(|_| Error::InvalidInput)
}
fn validate_binding(key: &str, revision: i64) -> Result<(), Error> {
    ClientProfileBinding {
        key: key.to_owned(),
        revision,
    }
    .validate()
    .map_err(|_| Error::InvalidInput)
}
fn storage(_: sea_orm::DbErr) -> Error {
    Error::Storage
}
fn service_name(definition: &ClientProfileDefinition) -> Result<String, Error> {
    serde_json::to_value(definition.service)
        .map_err(|_| Error::Invariant)?
        .as_str()
        .map(str::to_owned)
        .ok_or(Error::Invariant)
}
fn protocol_names(definition: &ClientProfileDefinition) -> String {
    format!(
        ",{},",
        definition
            .protocols
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn credential_names(definition: &ClientProfileDefinition) -> String {
    format!(
        ",{},",
        definition
            .credential_kinds
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(",")
    )
}
async fn publish<C: ConnectionTrait>(
    connection: &C,
    channel_id: i64,
    now: DatabaseTimestamp,
) -> Result<(), Error> {
    let channel = ChannelId::new(channel_id).map_err(|_| Error::Invariant)?;
    enqueue_scheduler_catalog_change(connection, SchedulerCatalogSubject::Channel(channel), now)
        .await
        .map_err(storage)
}
