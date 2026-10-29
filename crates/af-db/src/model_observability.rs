use sea_orm::sea_query::{Expr, IntoColumnRef, Query, SimpleExpr};

use crate::{AdminModelLifecycleRecord, entity::models};

/// Keep arbitrary client model strings and unavailable catalog entries out of
/// call observability without deleting stored facts or requiring a route.
pub(crate) fn current_callable_model_predicate(source: impl IntoColumnRef) -> SimpleExpr {
    Expr::exists(
        Query::select()
            .expr(Expr::val(1_i16))
            .from(models::Entity)
            .and_where(Expr::col((models::Entity, models::Column::Model)).equals(source))
            .and_where(Expr::col((models::Entity, models::Column::DeletedAt)).is_null())
            .and_where(
                Expr::col((models::Entity, models::Column::Lifecycle)).is_in([
                    AdminModelLifecycleRecord::Active.database_value(),
                    AdminModelLifecycleRecord::Deprecated.database_value(),
                ]),
            )
            .to_owned(),
    )
}
