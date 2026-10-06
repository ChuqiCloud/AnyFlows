use std::time::Duration;

use sea_orm::DbErr;
use thiserror::Error;
use tokio::time::error::Elapsed;

/// 数据库配置错误。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DatabaseOptionsError {
    /// 数据库地址为空。
    #[error("数据库地址不能为空")]
    EmptyUrl,
    /// 数据库地址不符合 URL 语法。
    #[error("数据库地址格式无效")]
    InvalidUrl,
    /// 数据库协议不在受支持范围内。
    #[error("不支持的数据库协议：{scheme}")]
    UnsupportedScheme { scheme: String },
    /// 最大连接数不能为零。
    #[error("数据库最大连接数必须大于零")]
    ZeroMaxConnections,
    /// 最小连接数超过最大连接数。
    #[error("数据库最小连接数 {min} 不能大于最大连接数 {max}")]
    MinConnectionsExceedMax { min: u32, max: u32 },
    /// 必需的超时配置为零。
    #[error("数据库超时配置 {field} 必须大于零")]
    ZeroTimeout { field: &'static str },
}

/// 数据库基础设施错误。
#[derive(Debug, Error)]
pub enum DatabaseError {
    /// 配置校验失败。
    #[error(transparent)]
    Options(#[from] DatabaseOptionsError),
    /// 建立数据库连接池失败。
    #[error("数据库连接失败")]
    Connect(#[source] DbErr),
    /// 创建连接池并建立首条连接超过截止时间。
    #[error("数据库连接超过 {timeout:?}")]
    ConnectTimeout {
        timeout: Duration,
        #[source]
        source: Elapsed,
    },
    /// 数据库健康检查执行失败。
    #[error("数据库健康检查失败")]
    HealthCheck(#[source] DbErr),
    /// 数据库健康检查超过截止时间。
    #[error("数据库健康检查超过 {timeout:?}")]
    HealthCheckTimeout {
        timeout: Duration,
        #[source]
        source: Elapsed,
    },
    /// 数据库迁移执行失败。
    #[error("数据库迁移失败")]
    Migration(#[source] DbErr),
    /// 数据库迁移超过截止时间。
    #[error("数据库迁移超过 {timeout:?}")]
    MigrationTimeout {
        timeout: Duration,
        #[source]
        source: Elapsed,
    },
    /// 扩展迁移误用了公共迁移表。
    #[error("数据库扩展迁移必须使用独立迁移表：{table}")]
    MigrationTableConflict { table: String },
    /// 历史迁移接管声明引用了扩展注册表中不存在的版本。
    #[error("数据库迁移历史接管版本不存在：{version}")]
    MigrationAdoptionVersionUnknown { version: String },
    /// 关闭数据库连接池失败。
    #[error("数据库连接池关闭失败")]
    Close(#[source] DbErr),
    /// 关闭数据库连接池超过截止时间。
    #[error("数据库连接池关闭超过 {timeout:?}")]
    CloseTimeout {
        timeout: Duration,
        #[source]
        source: Elapsed,
    },
}
