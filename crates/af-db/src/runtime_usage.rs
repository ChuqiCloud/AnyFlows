//! Bounded prompt-cache aggregates over the public usage ledger.

use sea_orm::{
    ConnectionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Func, Query, SimpleExpr},
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    admin_dashboard::{AdminDashboardRepositoryError, get_count, get_sum, query_one},
    entity::usage_logs,
};

/// Confirmed upstream totals. Absent cache reporting cannot be inferred from zero.
#[derive(Clone, Debug, Default)]
pub struct RuntimeUsageTotals {
    pub upstream_requests: i64,
    pub estimated_requests: i64,
    pub positive_cache_requests: i64,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

impl DatabasePool {
    /// Read the latest 24 hours with one aggregate query and a fixed two-second deadline.
    pub async fn runtime_usage_totals(
        &self,
        end: i64,
    ) -> Result<RuntimeUsageTotals, AdminDashboardRepositoryError> {
        let start = TimeDateTimeWithTimeZone::from_unix_timestamp(end.saturating_sub(86_400))
            .map_err(|_| AdminDashboardRepositoryError::Invariant)?;
        let end = TimeDateTimeWithTimeZone::from_unix_timestamp(end)
            .map_err(|_| AdminDashboardRepositoryError::Invariant)?;
        let col = |c| Expr::col((usage_logs::Entity, c));
        let read = col(usage_logs::Column::CacheRead);
        let write =
            col(usage_logs::Column::CacheCreation5m).add(col(usage_logs::Column::CacheCreation1h));
        let input = Expr::case(
            col(usage_logs::Column::UsageSemantics).eq(2_i16),
            col(usage_logs::Column::InputTokens)
                .add(read.clone())
                .add(write.clone()),
        )
        .finally(col(usage_logs::Column::InputTokens));
        let mut query = Query::select();
        query
            .expr_as(
                conditional_count(col(usage_logs::Column::UsageSource).eq(1_i16)),
                Alias::new("upstream"),
            )
            .expr_as(
                conditional_count(col(usage_logs::Column::UsageSource).eq(2_i16)),
                Alias::new("estimated"),
            )
            .expr_as(
                conditional_count(
                    col(usage_logs::Column::UsageSource)
                        .eq(1_i16)
                        .and(read.clone().gt(0_i64).or(write.clone().gt(0_i64))),
                ),
                Alias::new("positive"),
            )
            .expr_as(
                Func::sum(
                    Expr::case(col(usage_logs::Column::UsageSource).eq(1_i16), input)
                        .finally(0_i64),
                ),
                Alias::new("input"),
            )
            .expr_as(
                Func::sum(
                    Expr::case(col(usage_logs::Column::UsageSource).eq(1_i16), read).finally(0_i64),
                ),
                Alias::new("read"),
            )
            .expr_as(
                Func::sum(
                    Expr::case(col(usage_logs::Column::UsageSource).eq(1_i16), write)
                        .finally(0_i64),
                ),
                Alias::new("write"),
            )
            .from(usage_logs::Entity)
            .and_where(col(usage_logs::Column::EventType).eq(1_i16))
            .and_where(col(usage_logs::Column::CreatedAt).gte(start))
            .and_where(col(usage_logs::Column::CreatedAt).lt(end));
        let operation = async {
            let row = query_one(self.connection(), query.to_owned()).await?;
            let backend = self.connection().get_database_backend();
            Ok(RuntimeUsageTotals {
                upstream_requests: get_count(&row, "upstream", backend)?,
                estimated_requests: get_count(&row, "estimated", backend)?,
                positive_cache_requests: get_count(&row, "positive", backend)?,
                input_tokens: get_sum(&row, "input", backend)?,
                cache_read_tokens: get_sum(&row, "read", backend)?,
                cache_write_tokens: get_sum(&row, "write", backend)?,
            })
        }
        .with_subscriber(NoSubscriber::default());
        tokio::time::timeout(std::time::Duration::from_secs(2), operation)
            .await
            .map_err(|_| AdminDashboardRepositoryError::Timeout)?
    }
}

fn conditional_count(condition: SimpleExpr) -> SimpleExpr {
    Func::count(Expr::case(condition, 1_i64).finally(Expr::value(Option::<i64>::None))).into()
}
