use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use af_db::{DatabaseOptions, DatabasePool, MigrationOptions, connect_and_migrate};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{Database, DatabaseConnection};
use serde_json::json;

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(1);

/// 为跨 crate 测试提供仓储连接池与独立种子连接。
pub(crate) struct SqliteTestDatabase {
    pool: DatabasePool,
    seed: DatabaseConnection,
    files: SqliteTestDatabaseFiles,
}

impl SqliteTestDatabase {
    /// 创建文件型 SQLite 测试库，确保种子连接与仓储连接池共享同一份数据。
    pub(crate) async fn new(label: &str) -> Self {
        let files = SqliteTestDatabaseFiles::new(label);
        let pool = connect_and_migrate(
            &DatabaseOptions::new(&files.url).expect("SQLite 测试地址必须有效"),
            MigrationOptions::default(),
        )
        .await
        .expect("SQLite 测试迁移必须成功");
        let seed = Database::connect(&files.url)
            .await
            .expect("SQLite 测试种子连接必须成功");
        Self { pool, seed, files }
    }

    /// 返回受测仓储使用的连接池。
    pub(crate) const fn pool(&self) -> &DatabasePool {
        &self.pool
    }

    /// 返回仅供测试准备数据使用的独立连接。
    pub(crate) const fn seed(&self) -> &DatabaseConnection {
        &self.seed
    }

    /// 关闭全部连接后删除临时数据库文件。
    pub(crate) async fn close(self) {
        self.seed.close().await.expect("SQLite 种子连接必须关闭");
        self.pool.close().await.expect("SQLite 测试连接池必须关闭");
        drop(self.files);
    }
}

/// 构造只用于实体反序列化与外键测试的合法密文封套 JSON。
pub(crate) fn test_encrypted_envelope_json(marker: u8) -> String {
    json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": "video-task-test",
        "nonce": URL_SAFE_NO_PAD.encode([marker; 24]),
        "ciphertext": URL_SAFE_NO_PAD.encode([marker; 32]),
    })
    .to_string()
}

struct SqliteTestDatabaseFiles {
    path: PathBuf,
    url: String,
}

impl SqliteTestDatabaseFiles {
    fn new(label: &str) -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-{label}-{}-{serial}.db",
            std::process::id()
        ));
        let mut sqlite_path = path.to_string_lossy().replace('\\', "/");
        if !sqlite_path.starts_with('/') {
            sqlite_path.insert(0, '/');
        }
        Self {
            path,
            url: format!("sqlite://{sqlite_path}?mode=rwc"),
        }
    }
}

impl Drop for SqliteTestDatabaseFiles {
    fn drop(&mut self) {
        for suffix in ["", "-shm", "-wal"] {
            let candidate = PathBuf::from(format!("{}{suffix}", self.path.display()));
            let _ = fs::remove_file(candidate);
        }
    }
}
