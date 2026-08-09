use std::{fmt, time::Duration};

use af_domain::TokenId;
use sea_orm::{
    ConnectionTrait, DbErr, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, Query, SelectStatement, UpdateStatement},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, entity::tokens};

/// 令牌累计请求数的原子准入结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenRequestAdmissionOutcome {
    /// 本次请求已原子计入令牌累计请求数。
    Admitted,
    /// 令牌已达到配置的累计请求数上限。
    LimitReached,
    /// 令牌在鉴权与准入之间变为不可用。
    Rejected,
}

/// 令牌请求数仓储的构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TokenRequestAdmissionRepositoryConfigError {
    /// 零超时无法形成有效的数据库操作截止时间。
    #[error("令牌请求数准入数据库超时必须大于零")]
    ZeroOperationTimeout,
}

/// 令牌请求数仓储错误；不携带令牌标识或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TokenRequestAdmissionRepositoryError {
    /// 查询或更新失败。
    #[error("令牌请求数准入数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("令牌请求数准入数据库操作超时")]
    Timeout,
    /// 持久化行违反状态、计数或上限不变量。
    #[error("令牌请求数准入持久化状态损坏")]
    Invariant,
}

/// 以数据库条件更新实现跨实例安全的令牌累计请求数准入。
#[derive(Clone)]
pub struct TokenRequestAdmissionRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl TokenRequestAdmissionRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, TokenRequestAdmissionRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(TokenRequestAdmissionRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 原子占用一次请求额度；数据库更新是唯一的计数权威。
    pub async fn admit(
        &self,
        token_id: TokenId,
    ) -> Result<TokenRequestAdmissionOutcome, TokenRequestAdmissionRepositoryError> {
        match timeout(self.operation_timeout, self.admit_inner(token_id)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                TokenRequestAdmissionRepositoryError::Timeout,
            )),
        }
    }

    async fn admit_inner(
        &self,
        token_id: TokenId,
    ) -> Result<TokenRequestAdmissionOutcome, TokenRequestAdmissionRepositoryError> {
        let connection = self.pool.connection();
        let backend = connection.get_database_backend();
        let update = backend.build(&admission_update(token_id));
        let result = connection
            .execute(update)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|error| record_query_error("token_request_admission_update", error))?;

        match result.rows_affected() {
            1 => Ok(TokenRequestAdmissionOutcome::Admitted),
            0 => self.classify_rejection(token_id).await,
            _ => Err(record_internal_error(
                TokenRequestAdmissionRepositoryError::Invariant,
            )),
        }
    }

    async fn classify_rejection(
        &self,
        token_id: TokenId,
    ) -> Result<TokenRequestAdmissionOutcome, TokenRequestAdmissionRepositoryError> {
        let connection = self.pool.connection();
        let backend = connection.get_database_backend();
        let statement = backend.build(&rejection_query(token_id));
        let result = connection
            .query_one(statement)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|error| record_query_error("token_request_admission_classify", error))?;
        let Some(result) = result else {
            return Ok(TokenRequestAdmissionOutcome::Rejected);
        };
        let state = TokenRequestState::try_from_query_result(&result)
            .map_err(|_| record_internal_error(TokenRequestAdmissionRepositoryError::Invariant))?;
        state.classify()
    }
}

impl fmt::Debug for TokenRequestAdmissionRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenRequestAdmissionRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

/// 通过单条条件更新完成检查与递增，避免“先读后增”并发超卖。
fn admission_update(token_id: TokenId) -> UpdateStatement {
    let used_requests = Expr::col(tokens::Column::UsedRequests);
    let finite_available = Expr::col(tokens::Column::MaxRequests)
        .is_null()
        .or(Expr::col(tokens::Column::MaxRequests).gte(0_i64).and(
            used_requests
                .clone()
                .lt(Expr::col(tokens::Column::MaxRequests)),
        ));

    Query::update()
        .table(tokens::Entity)
        .value(
            tokens::Column::UsedRequests,
            used_requests.clone().add(1_i64),
        )
        .and_where(Expr::col(tokens::Column::Id).eq(token_id.get()))
        .and_where(Expr::col(tokens::Column::Status).eq(1_i16))
        .and_where(Expr::col(tokens::Column::DeletedAt).is_null())
        .and_where(used_requests.clone().gte(0_i64))
        .and_where(used_requests.lt(i64::MAX))
        .and_where(finite_available)
        .to_owned()
}

/// 仅读取失败分类所需字段；读取结果不参与计数决定。
fn rejection_query(token_id: TokenId) -> SelectStatement {
    Query::select()
        .columns([
            (tokens::Entity, tokens::Column::Status),
            (tokens::Entity, tokens::Column::DeletedAt),
            (tokens::Entity, tokens::Column::MaxRequests),
            (tokens::Entity, tokens::Column::UsedRequests),
        ])
        .from(tokens::Entity)
        .and_where(Expr::col((tokens::Entity, tokens::Column::Id)).eq(token_id.get()))
        .limit(2)
        .to_owned()
}

struct TokenRequestState {
    status: i16,
    deleted_at: Option<TimeDateTimeWithTimeZone>,
    max_requests: Option<i64>,
    used_requests: i64,
}

impl TokenRequestState {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            status: result.try_get("", "status")?,
            deleted_at: result.try_get("", "deleted_at")?,
            max_requests: result.try_get("", "max_requests")?,
            used_requests: result.try_get("", "used_requests")?,
        })
    }

    fn classify(
        self,
    ) -> Result<TokenRequestAdmissionOutcome, TokenRequestAdmissionRepositoryError> {
        if !matches!(self.status, 1 | 2)
            || self.used_requests < 0
            || self.max_requests.is_some_and(|value| value < 0)
        {
            return Err(record_internal_error(
                TokenRequestAdmissionRepositoryError::Invariant,
            ));
        }
        if self.status != 1 || self.deleted_at.is_some() {
            return Ok(TokenRequestAdmissionOutcome::Rejected);
        }
        if self
            .max_requests
            .is_some_and(|limit| self.used_requests >= limit)
        {
            return Ok(TokenRequestAdmissionOutcome::LimitReached);
        }
        // 条件更新未命中但行仍看似可用，只能按未知状态失败关闭。
        Err(record_internal_error(
            TokenRequestAdmissionRepositoryError::Invariant,
        ))
    }
}

fn record_internal_error(
    error: TokenRequestAdmissionRepositoryError,
) -> TokenRequestAdmissionRepositoryError {
    tracing::error!(
        target: "af_db::token_request_admission",
        error_kind = match error {
            TokenRequestAdmissionRepositoryError::Query => "token_request_admission_query",
            TokenRequestAdmissionRepositoryError::Timeout => "token_request_admission_timeout",
            TokenRequestAdmissionRepositoryError::Invariant => {
                "token_request_admission_invariant"
            }
        },
        "令牌请求数准入仓储操作失败"
    );
    error
}

fn record_query_error(
    operation: &'static str,
    _error: DbErr,
) -> TokenRequestAdmissionRepositoryError {
    tracing::error!(
        target: "af_db::token_request_admission",
        error_kind = "token_request_admission_query",
        operation,
        "令牌请求数准入数据库操作失败"
    );
    TokenRequestAdmissionRepositoryError::Query
}

#[cfg(test)]
mod sql_tests {
    use sea_orm::{
        DbBackend,
        sea_query::{MysqlQueryBuilder, PostgresQueryBuilder, SqliteQueryBuilder},
    };

    use super::*;

    #[test]
    fn atomic_update_guards_are_present_in_all_dialects() {
        let token_id = TokenId::new(7).unwrap();
        for (builder, label) in [
            (DbBackend::Postgres, "postgres"),
            (DbBackend::MySql, "mysql"),
            (DbBackend::Sqlite, "sqlite"),
        ] {
            let statement = match builder {
                DbBackend::Postgres => admission_update(token_id).to_string(PostgresQueryBuilder),
                DbBackend::MySql => admission_update(token_id).to_string(MysqlQueryBuilder),
                DbBackend::Sqlite => admission_update(token_id).to_string(SqliteQueryBuilder),
            };
            assert!(statement.contains("used_requests"), "{label}: {statement}");
            assert!(statement.contains("max_requests"), "{label}: {statement}");
            assert!(statement.contains("status"), "{label}: {statement}");
            assert!(statement.contains("deleted_at"), "{label}: {statement}");
            assert!(
                statement.contains("9223372036854775807"),
                "{label}: {statement}"
            );
            assert!(statement.contains("+ 1"), "{label}: {statement}");
        }
    }
}
