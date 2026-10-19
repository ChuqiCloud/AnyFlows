//! 代理池配置、成员与绑定。节点复用专属代理目录，手动绑定在订阅变动后仍然有效。
mod cleanup;
pub use cleanup::{ProxyCleanupMode, ProxyCleanupRequest, ProxyCleanupResult, ProxyCleanupTarget};
use std::{collections::BTreeMap, future::Future, time::Duration};

use af_domain::ProxyId;
use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, QueryTrait,
    Set, TransactionTrait, sea_query::Expr,
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    CredentialProxyCreateRecord, CredentialProxyRecord, DatabasePool, EncryptedCredentialEnvelope,
    credential_proxy::{encrypted_json, envelope_from_json, record_from_model},
    entity::{
        SensitiveString, channels, credentials, proxies, proxy_pool_bindings as bindings,
        proxy_pool_nodes as nodes, proxy_pools as pools,
    },
};

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProxyPoolRepositoryError {
    #[error("代理池不存在")]
    NotFound,
    #[error("代理池冲突、正在操作或仍被引用")]
    Conflict,
    #[error("代理池输入无效")]
    InvalidInput,
    #[error("代理池存储操作失败")]
    Internal,
}
type Result<T> = std::result::Result<T, ProxyPoolRepositoryError>;

#[derive(Clone)]
pub struct ProxyPoolRecord {
    pub proxy: CredentialProxyRecord,
    pub settings_secret: EncryptedCredentialEnvelope,
    pub subscribed: bool,
    pub interval_secs: i32,
    pub revision: i64,
    pub last_refresh_at: Option<i64>,
    pub last_refresh_attempt_at: Option<i64>,
    pub last_check_at: Option<i64>,
    pub status: String,
    pub work_until: i64,
    pub nodes: Vec<ProxyPoolNodeRecord>,
    pub channel_ids: Vec<i64>,
}
#[derive(Clone)]
pub struct ProxyPoolNodeRecord {
    pub proxy: CredentialProxyRecord,
    pub fingerprint: String,
    pub healthy: bool,
    pub latency_ms: Option<i64>,
    pub checked_at: Option<i64>,
    pub remark: String,
}

#[derive(Clone)]
pub struct ProxyPoolRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}
impl ProxyPoolRepository {
    #[must_use]
    pub fn new(pool: DatabasePool, operation_timeout: Duration) -> Self {
        Self {
            pool,
            operation_timeout,
        }
    }
    async fn run<T>(&self, operation: impl Future<Output = Result<T>>) -> Result<T> {
        timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| ProxyPoolRepositoryError::Internal)?
    }
    pub async fn list(&self) -> Result<Vec<ProxyPoolRecord>> {
        self.run(async {
            let pool_models = pools::Entity::find()
                .order_by_asc(pools::Column::Id)
                .limit(101)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            if pool_models.len() > 100 {
                return Err(ProxyPoolRepositoryError::Internal);
            }
            let mut records = Vec::new();
            for model in pool_models {
                records.push(self.load(model).await?);
            }
            Ok(records)
        })
        .await
    }
    pub async fn get(&self, id: ProxyId) -> Result<ProxyPoolRecord> {
        self.run(async {
            let model = pools::Entity::find_by_id(id.get())
                .one(self.pool.connection())
                .await
                .map_err(internal)?
                .ok_or(ProxyPoolRepositoryError::NotFound)?;
            self.load(model).await
        })
        .await
    }
    pub async fn proxy(&self, id: ProxyId) -> Result<CredentialProxyRecord> {
        self.run(async {
            let model = proxies::Entity::find_by_id(id.get())
                .filter(proxies::Column::Enabled.eq(true))
                .filter(proxies::Column::DeletedAt.is_null())
                .one(self.pool.connection())
                .await
                .map_err(internal)?
                .ok_or(ProxyPoolRepositoryError::NotFound)?;
            record_from_model(model).map_err(internal)
        })
        .await
    }
    async fn load(&self, model: pools::Model) -> Result<ProxyPoolRecord> {
        let proxy = proxies::Entity::find_by_id(model.id)
            .filter(proxies::Column::DeletedAt.is_null())
            .one(self.pool.connection())
            .await
            .map_err(internal)?
            .ok_or(ProxyPoolRepositoryError::NotFound)?;
        let members = nodes::Entity::find()
            .filter(nodes::Column::PoolId.eq(model.id))
            .filter(nodes::Column::Present.eq(true))
            .order_by_asc(nodes::Column::ProxyId)
            .limit(257)
            .all(self.pool.connection())
            .await
            .map_err(internal)?;
        if members.len() > 256 {
            return Err(ProxyPoolRepositoryError::Internal);
        }
        let mut member_proxies = proxies::Entity::find()
            .filter(proxies::Column::Id.is_in(members.iter().map(|m| m.proxy_id)))
            .filter(proxies::Column::DeletedAt.is_null())
            .all(self.pool.connection())
            .await
            .map_err(internal)?
            .into_iter()
            .map(|m| (m.id, m))
            .collect::<BTreeMap<_, _>>();
        let mut node_records = Vec::new();
        for member in members {
            if let Some(proxy) = member_proxies.remove(&member.proxy_id) {
                node_records.push(ProxyPoolNodeRecord {
                    proxy: record_from_model(proxy).map_err(internal)?,
                    fingerprint: member.fingerprint,
                    healthy: member.healthy,
                    latency_ms: member.latency_ms,
                    checked_at: member.checked_at,
                    remark: member.remark,
                });
            }
        }
        let channel_ids = bindings::Entity::find()
            .filter(bindings::Column::PoolId.eq(model.id))
            .all(self.pool.connection())
            .await
            .map_err(internal)?
            .into_iter()
            .map(|m| m.channel_id)
            .collect();
        Ok(ProxyPoolRecord {
            proxy: record_from_model(proxy).map_err(internal)?,
            settings_secret: envelope_from_json(model.settings_secret).map_err(internal)?,
            subscribed: model.subscribed,
            interval_secs: model.interval_secs,
            revision: model.revision,
            last_refresh_at: model.last_refresh_at,
            last_refresh_attempt_at: model.last_refresh_attempt_at,
            last_check_at: model.last_check_at,
            status: model.status,
            work_until: model.work_until,
            nodes: node_records,
            channel_ids,
        })
    }
    /// 创建稳定代理 ID；未测活的空池只能在池解析器中失败关闭。
    pub async fn create<F>(
        &self,
        name: String,
        subscribed: bool,
        interval_secs: i32,
        encrypt: F,
    ) -> Result<ProxyId>
    where
        F: FnOnce(ProxyId) -> Result<EncryptedCredentialEnvelope> + Send,
    {
        self.run(async {
            let tx = self.pool.connection().begin().await.map_err(internal)?;
            let count = pools::Entity::find()
                .limit(100)
                .all(&tx)
                .await
                .map_err(internal)?
                .len();
            if count >= 100 {
                return Err(ProxyPoolRepositoryError::Conflict);
            }
            let now = TimeDateTimeWithTimeZone::now_utc();
            let proxy = proxies::ActiveModel {
                name: Set(name.clone()),
                active_name: Set(Some(name)),
                scheme: Set("http".to_owned()),
                host: Set(SensitiveString::from("unavailable.invalid".to_owned())),
                port: Set(9),
                enabled: Set(true),
                trust_proxy_dns: Set(false),
                version: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&tx)
            .await
            .map_err(|_| ProxyPoolRepositoryError::Conflict)?;
            let id = ProxyId::new(proxy.id).map_err(internal)?;
            pools::ActiveModel {
                id: Set(id.get()),
                settings_secret: Set(encrypted_json(encrypt(id)?).map_err(internal)?),
                subscribed: Set(subscribed),
                interval_secs: Set(interval_secs),
                revision: Set(1),
                status: Set("pending".to_owned()),
                work_until: Set(0),
                ..Default::default()
            }
            .insert(&tx)
            .await
            .map_err(internal)?;
            tx.commit().await.map_err(internal)?;
            Ok(id)
        })
        .await
    }
    pub async fn update(
        &self,
        id: ProxyId,
        revision: i64,
        name: String,
        subscribed: bool,
        interval_secs: i32,
        secret: EncryptedCredentialEnvelope,
    ) -> Result<()> {
        self.run(async {
            let tx = self.pool.connection().begin().await.map_err(internal)?;
            let result = pools::Entity::update_many()
                .filter(pools::Column::Id.eq(id.get()))
                .filter(pools::Column::Revision.eq(revision))
                .col_expr(
                    pools::Column::SettingsSecret,
                    Expr::value(encrypted_json(secret).map_err(internal)?),
                )
                .col_expr(pools::Column::Subscribed, Expr::value(subscribed))
                .col_expr(pools::Column::IntervalSecs, Expr::value(interval_secs))
                .col_expr(pools::Column::Revision, Expr::value(revision + 1))
                .col_expr(pools::Column::Status, Expr::value("pending"))
                .col_expr(
                    pools::Column::LastRefreshAt,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(
                    pools::Column::LastRefreshAttemptAt,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(pools::Column::LastCheckAt, Expr::value(Option::<i64>::None))
                .col_expr(pools::Column::WorkUntil, Expr::value(0_i64))
                .exec(&tx)
                .await
                .map_err(internal)?;
            if result.rows_affected != 1 {
                return Err(ProxyPoolRepositoryError::Conflict);
            }
            proxies::Entity::update_many()
                .filter(proxies::Column::Id.eq(id.get()))
                .col_expr(proxies::Column::Name, Expr::value(name.clone()))
                .col_expr(proxies::Column::ActiveName, Expr::value(name))
                .col_expr(
                    proxies::Column::UpdatedAt,
                    Expr::value(TimeDateTimeWithTimeZone::now_utc()),
                )
                .exec(&tx)
                .await
                .map_err(|_| ProxyPoolRepositoryError::Conflict)?;
            // 检测目标或 DNS 策略变更后，旧的健康结论不能继续使用。
            nodes::Entity::update_many()
                .filter(nodes::Column::PoolId.eq(id.get()))
                .col_expr(nodes::Column::Healthy, Expr::value(false))
                .col_expr(nodes::Column::CheckedAt, Expr::value(Option::<i64>::None))
                .col_expr(nodes::Column::LatencyMs, Expr::value(Option::<i64>::None))
                .exec(&tx)
                .await
                .map_err(internal)?;
            tx.commit().await.map_err(internal)
        })
        .await
    }
    /// 替换订阅当前成员，移出的节点仍保留在代理目录，避免破坏手动绑定。
    pub async fn replace_nodes<F>(
        &self,
        id: ProxyId,
        revision: i64,
        candidates: Vec<(String, String, CredentialProxyCreateRecord)>,
        encrypt: F,
    ) -> Result<()>
    where
        F: Fn(usize, ProxyId) -> Result<Option<EncryptedCredentialEnvelope>> + Send,
    {
        self.run(async {
            if candidates.is_empty() || candidates.len() > 256 {
                return Err(ProxyPoolRepositoryError::InvalidInput);
            }
            let tx = self.pool.connection().begin().await.map_err(internal)?;
            let guarded = pools::Entity::update_many()
                .filter(pools::Column::Id.eq(id.get()))
                .filter(pools::Column::Revision.eq(revision))
                .col_expr(
                    pools::Column::LastRefreshAt,
                    Expr::value(TimeDateTimeWithTimeZone::now_utc().unix_timestamp()),
                )
                .exec(&tx)
                .await
                .map_err(internal)?;
            if guarded.rows_affected != 1 {
                return Err(ProxyPoolRepositoryError::Conflict);
            }
            let existing = nodes::Entity::find()
                .filter(nodes::Column::PoolId.eq(id.get()))
                .filter(
                    nodes::Column::ProxyId.in_subquery(
                        proxies::Entity::find()
                            .select_only()
                            .column(proxies::Column::Id)
                            .filter(proxies::Column::DeletedAt.is_null())
                            .into_query(),
                    ),
                )
                .all(&tx)
                .await
                .map_err(internal)?
                .into_iter()
                .map(|m| (m.fingerprint.clone(), m))
                .collect::<BTreeMap<_, _>>();
            nodes::Entity::update_many()
                .filter(nodes::Column::PoolId.eq(id.get()))
                .col_expr(nodes::Column::Present, Expr::value(false))
                .exec(&tx)
                .await
                .map_err(internal)?;
            for (index, (fingerprint, remark, record)) in candidates.into_iter().enumerate() {
                if let Some(member) = existing.get(&fingerprint) {
                    nodes::Entity::update_many()
                        .filter(nodes::Column::PoolId.eq(id.get()))
                        .filter(nodes::Column::ProxyId.eq(member.proxy_id))
                        .col_expr(nodes::Column::Present, Expr::value(true))
                        .col_expr(nodes::Column::Remark, Expr::value(remark))
                        .exec(&tx)
                        .await
                        .map_err(internal)?;
                    continue;
                }
                let now = TimeDateTimeWithTimeZone::now_utc();
                let proxy = proxies::ActiveModel {
                    name: Set(record.name.clone()),
                    active_name: Set(Some(record.name)),
                    scheme: Set(record.scheme.as_str().to_owned()),
                    host: Set(SensitiveString::from(record.host)),
                    port: Set(i32::from(record.port)),
                    username: Set(None),
                    password_secret: Set(None),
                    trust_proxy_dns: Set(record.trust_proxy_dns),
                    enabled: Set(true),
                    version: Set(1),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(&tx)
                .await
                .map_err(internal)?;
                let node_id = ProxyId::new(proxy.id).map_err(internal)?;
                let password = encrypt(index, node_id)?
                    .map(encrypted_json)
                    .transpose()
                    .map_err(internal)?;
                if record.username.is_some() != password.is_some() {
                    return Err(ProxyPoolRepositoryError::InvalidInput);
                }
                proxies::Entity::update_many()
                    .filter(proxies::Column::Id.eq(proxy.id))
                    .col_expr(
                        proxies::Column::Username,
                        Expr::value(record.username.map(SensitiveString::from)),
                    )
                    .col_expr(proxies::Column::PasswordSecret, Expr::value(password))
                    .exec(&tx)
                    .await
                    .map_err(internal)?;
                nodes::ActiveModel {
                    pool_id: Set(id.get()),
                    proxy_id: Set(node_id.get()),
                    fingerprint: Set(fingerprint),
                    remark: Set(remark),
                    present: Set(true),
                    healthy: Set(false),
                    ..Default::default()
                }
                .insert(&tx)
                .await
                .map_err(internal)?;
            }
            tx.commit().await.map_err(internal)
        })
        .await
    }
    pub async fn claim(&self, id: ProxyId, revision: i64, now: i64, refresh: bool) -> Result<bool> {
        self.run(async {
            let mut update = pools::Entity::update_many()
                .filter(pools::Column::Id.eq(id.get()))
                .filter(pools::Column::Revision.eq(revision))
                .filter(pools::Column::WorkUntil.lte(now))
                .col_expr(pools::Column::WorkUntil, Expr::value(now + 120));
            if refresh {
                update = update.col_expr(pools::Column::LastRefreshAttemptAt, Expr::value(now));
            }
            Ok(update
                .exec(self.pool.connection())
                .await
                .map_err(internal)?
                .rows_affected
                == 1)
        })
        .await
    }
    pub async fn finish(
        &self,
        id: ProxyId,
        revision: i64,
        status: &str,
        measurements: Vec<(ProxyId, i64, Option<i64>)>,
    ) -> Result<()> {
        self.run(async {
            let tx = self.pool.connection().begin().await.map_err(internal)?;
            let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
            let result = pools::Entity::update_many()
                .filter(pools::Column::Id.eq(id.get()))
                .filter(pools::Column::Revision.eq(revision))
                .col_expr(pools::Column::Status, Expr::value(status))
                .col_expr(pools::Column::WorkUntil, Expr::value(0_i64))
                .col_expr(pools::Column::LastCheckAt, Expr::value(now))
                .exec(&tx)
                .await
                .map_err(internal)?;
            if result.rows_affected != 1 {
                return Err(ProxyPoolRepositoryError::Conflict);
            }
            for (node, version, latency) in measurements {
                nodes::Entity::update_many()
                    .filter(nodes::Column::PoolId.eq(id.get()))
                    .filter(nodes::Column::ProxyId.eq(node.get()))
                    .filter(
                        nodes::Column::ProxyId.in_subquery(
                            proxies::Entity::find()
                                .select_only()
                                .column(proxies::Column::Id)
                                .filter(proxies::Column::Version.eq(version))
                                .filter(proxies::Column::DeletedAt.is_null())
                                .into_query(),
                        ),
                    )
                    .col_expr(nodes::Column::Healthy, Expr::value(latency.is_some()))
                    .col_expr(nodes::Column::LatencyMs, Expr::value(latency))
                    .col_expr(nodes::Column::CheckedAt, Expr::value(now))
                    .exec(&tx)
                    .await
                    .map_err(internal)?;
            }
            tx.commit().await.map_err(internal)
        })
        .await
    }
    pub async fn bind(&self, channel_id: i64, pool_id: Option<ProxyId>) -> Result<()> {
        self.run(async {
            let tx = self.pool.connection().begin().await.map_err(internal)?;
            if channel_id < 0
                || (channel_id > 0
                    && channels::Entity::find_by_id(channel_id)
                        .filter(channels::Column::DeletedAt.is_null())
                        .one(&tx)
                        .await
                        .map_err(internal)?
                        .is_none())
            {
                return Err(ProxyPoolRepositoryError::InvalidInput);
            }
            if let Some(id) = pool_id {
                pools::Entity::find_by_id(id.get())
                    .lock_exclusive()
                    .one(&tx)
                    .await
                    .map_err(internal)?
                    .ok_or(ProxyPoolRepositoryError::NotFound)?;
            }
            bindings::Entity::delete_by_id(channel_id)
                .exec(&tx)
                .await
                .map_err(internal)?;
            if let Some(id) = pool_id {
                bindings::ActiveModel {
                    channel_id: Set(channel_id),
                    pool_id: Set(id.get()),
                }
                .insert(&tx)
                .await
                .map_err(internal)?;
            }
            tx.commit().await.map_err(internal)
        })
        .await
    }
    pub async fn bound_pool(&self, channel_id: i64) -> Result<Option<ProxyId>> {
        self.run(async {
            let records = bindings::Entity::find()
                .filter(bindings::Column::ChannelId.is_in([0, channel_id]))
                .order_by_desc(bindings::Column::ChannelId)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            records
                .first()
                .map(|m| ProxyId::new(m.pool_id).map_err(internal))
                .transpose()
        })
        .await
    }
    pub async fn delete(&self, id: ProxyId) -> Result<()> {
        let result = self
            .cleanup(ProxyCleanupRequest {
                target: ProxyCleanupTarget::Pools,
                mode: ProxyCleanupMode::Selected,
                pool_id: None,
                revision: None,
                ids: vec![id],
                after_id: None,
                preview: false,
            })
            .await?;
        if !result.protected_ids.is_empty() {
            return Err(ProxyPoolRepositoryError::Conflict);
        }
        if result.deleted_ids.is_empty() {
            return Err(ProxyPoolRepositoryError::NotFound);
        }
        Ok(())
    }
}
fn internal<T>(_: T) -> ProxyPoolRepositoryError {
    ProxyPoolRepositoryError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CredentialProxyScheme, DatabaseOptions, MigrationOptions};
    use sea_orm_migration::MigratorTrait as _;
    fn envelope() -> EncryptedCredentialEnvelope {
        EncryptedCredentialEnvelope::new("test", [1; 24], vec![2; 32]).unwrap()
    }
    #[tokio::test]
    async fn proxy_pool_refresh_preserves_ids_health_and_binding_precedence() {
        let database = crate::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let repo = ProxyPoolRepository::new(database.clone(), Duration::from_secs(5));
        let id = repo
            .create("订阅测试".to_owned(), true, 60, |_| Ok(envelope()))
            .await
            .unwrap();
        assert!(
            crate::migration::Migrator::down(database.connection(), Some(2))
                .await
                .is_err()
        );
        crate::migration::Migrator::up(database.connection(), None)
            .await
            .unwrap();
        assert!(repo.get(id).await.unwrap().nodes.is_empty());
        let candidate = || {
            vec![(
                "a".repeat(64),
                "hk".to_owned(),
                CredentialProxyCreateRecord::new(
                    "node-test".to_owned(),
                    CredentialProxyScheme::Socks5,
                    "192.0.2.1".to_owned(),
                    1080,
                    None,
                    false,
                    true,
                ),
            )]
        };
        repo.replace_nodes(id, 1, candidate(), |_, _| Ok(None))
            .await
            .unwrap();
        let node = repo.get(id).await.unwrap().nodes[0].proxy.proxy_id();
        assert!(repo.claim(id, 1, 100, true).await.unwrap());
        assert!(!repo.claim(id, 1, 101, true).await.unwrap());
        assert_eq!(
            repo.get(id).await.unwrap().last_refresh_attempt_at,
            Some(100)
        );
        repo.finish(id, 1, "ready", vec![(node, 1, Some(10))])
            .await
            .unwrap();
        assert!(repo.claim(id, 1, 150, false).await.unwrap());
        assert_eq!(
            repo.get(id).await.unwrap().last_refresh_attempt_at,
            Some(100)
        );
        repo.finish(id, 1, "ready", vec![]).await.unwrap();
        repo.replace_nodes(id, 1, candidate(), |_, _| {
            panic!("unchanged node must reuse its ID")
        })
        .await
        .unwrap();
        let pool = repo.get(id).await.unwrap();
        assert_eq!(pool.nodes[0].proxy.proxy_id(), node);
        assert!(pool.nodes[0].healthy);
        repo.finish(id, 1, "refresh_failed", vec![]).await.unwrap();
        assert!(repo.get(id).await.unwrap().nodes[0].healthy);
        repo.bind(0, Some(id)).await.unwrap();
        assert_eq!(repo.bound_pool(123).await.unwrap(), Some(id));
        assert_eq!(
            repo.delete(id).await.unwrap_err(),
            ProxyPoolRepositoryError::Conflict
        );
        repo.update(id, 1, "订阅测试".to_owned(), true, 120, envelope())
            .await
            .unwrap();
        assert_eq!(
            repo.finish(id, 1, "ready", vec![(node, 1, Some(1))])
                .await
                .unwrap_err(),
            ProxyPoolRepositoryError::Conflict
        );
        assert!(!repo.get(id).await.unwrap().nodes[0].healthy);
        // 模拟检测期间节点配置更新，旧版本的成功结果不得恢复健康。
        proxies::Entity::update_many()
            .filter(proxies::Column::Id.eq(node.get()))
            .col_expr(proxies::Column::Version, Expr::value(2_i64))
            .exec(database.connection())
            .await
            .unwrap();
        repo.finish(id, 2, "ready", vec![(node, 1, Some(1))])
            .await
            .unwrap();
        assert!(!repo.get(id).await.unwrap().nodes[0].healthy);
        repo.finish(id, 2, "ready", vec![(node, 2, Some(2))])
            .await
            .unwrap();
        assert!(repo.get(id).await.unwrap().nodes[0].healthy);
        let proxies =
            crate::CredentialProxyRepository::new(database.clone(), Duration::from_secs(5))
                .unwrap();
        assert_eq!(
            proxies.delete(node).await.unwrap(),
            crate::CredentialProxyDeleteOutcome::Deleted
        );
        repo.replace_nodes(id, 2, candidate(), |_, _| Ok(None))
            .await
            .unwrap();
        let restored = repo.get(id).await.unwrap();
        assert_eq!(restored.nodes.len(), 1);
        assert_ne!(restored.nodes[0].proxy.proxy_id(), node);
        assert!(!restored.nodes[0].healthy);
        repo.bind(0, None).await.unwrap();
        repo.delete(id).await.unwrap();
        assert!(matches!(
            repo.get(id).await,
            Err(ProxyPoolRepositoryError::NotFound)
        ));
        crate::migration::Migrator::down(database.connection(), Some(2))
            .await
            .unwrap();
        crate::migration::Migrator::up(database.connection(), None)
            .await
            .unwrap();
    }
}
