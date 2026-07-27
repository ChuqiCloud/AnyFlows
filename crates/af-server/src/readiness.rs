use af_db::DatabasePool;
use af_http::{ReadinessFuture, ReadinessProbe};

/// 通过 af-db 的有界 ping 检查当前唯一外部运行依赖。
pub(crate) struct DatabaseReadinessProbe {
    database: DatabasePool,
}

impl DatabaseReadinessProbe {
    pub(crate) fn new(database: DatabasePool) -> Self {
        Self { database }
    }
}

impl ReadinessProbe for DatabaseReadinessProbe {
    fn check(&self) -> ReadinessFuture<'_> {
        Box::pin(async move { self.database.ping().await.is_ok() })
    }
}

#[cfg(test)]
mod tests {
    use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};

    use super::*;

    #[tokio::test]
    async fn shared_pool_close_makes_database_probe_not_ready() {
        let database = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let probe = DatabaseReadinessProbe::new(database.clone());

        assert!(probe.check().await);
        database.close().await.unwrap();
        assert!(!probe.check().await);
    }
}
