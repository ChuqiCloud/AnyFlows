use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_approval_decisions, organization_approval_notifications,
    organization_approval_requests, organizations, users,
};

use super::{auto_id, nullable_timestamp, table, timestamp};

const NOTIFICATION_KEY_INDEX: &str = "uq_org_approval_notifications_key";
const NOTIFICATION_DECISION_INDEX: &str = "uq_org_approval_notifications_decision";
const NOTIFICATION_RECIPIENT_INDEX: &str = "idx_org_approval_notifications_recipient";
const APPROVAL_NOTIFICATION_TEMPLATE_VERSION: &str = "approval-v1";

/// 创建企业审批决定站内通知事实表。
pub(in crate::migration) async fn create_organization_approval_notifications(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    use organization_approval_notifications::Column;

    let statement = table(manager, organization_approval_notifications::Entity)
        .if_not_exists()
        .col(auto_id(Column::Id))
        .col(
            ColumnDef::new(Column::NotificationKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::RequestId).big_integer().not_null())
        .col(ColumnDef::new(Column::DecisionId).big_integer().not_null())
        .col(
            ColumnDef::new(Column::RecipientUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::DecisionKind)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::TemplateVersion)
                .char_len(16)
                .not_null(),
        )
        .col(nullable_timestamp(manager, Column::OccurredAt).not_null())
        .col(timestamp(manager, Column::CreatedAt))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            Column::NotificationKey,
            32,
        ))
        .check(Expr::col(Column::DecisionKind).between(1_i16, 3_i16))
        .check(Expr::col(Column::TemplateVersion).eq(APPROVAL_NOTIFICATION_TEMPLATE_VERSION))
        .check(Expr::cust("length(trim(template_version)) > 0"))
        .check(timestamp_lte_check(
            manager.get_database_backend(),
            Column::OccurredAt,
            Column::CreatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_notifications_organization")
                .from(
                    organization_approval_notifications::Entity,
                    Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_notifications_request")
                .from(
                    organization_approval_notifications::Entity,
                    (Column::OrganizationId, Column::RequestId),
                )
                .to(
                    organization_approval_requests::Entity,
                    (
                        organization_approval_requests::Column::OrganizationId,
                        organization_approval_requests::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_notifications_decision")
                .from(
                    organization_approval_notifications::Entity,
                    (Column::OrganizationId, Column::DecisionId),
                )
                .to(
                    organization_approval_decisions::Entity,
                    (
                        organization_approval_decisions::Column::OrganizationId,
                        organization_approval_decisions::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_notifications_recipient")
                .from(
                    organization_approval_notifications::Entity,
                    Column::RecipientUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .to_owned();
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(NOTIFICATION_KEY_INDEX)
            .table(organization_approval_notifications::Entity)
            .col(Column::OrganizationId)
            .col(Column::NotificationKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(NOTIFICATION_DECISION_INDEX)
            .table(organization_approval_notifications::Entity)
            .col(Column::OrganizationId)
            .col(Column::DecisionId)
            .unique()
            .to_owned(),
        Index::create()
            .name(NOTIFICATION_RECIPIENT_INDEX)
            .table(organization_approval_notifications::Entity)
            .col(Column::OrganizationId)
            .col(Column::RecipientUserId)
            .col(Column::OccurredAt)
            .col(Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    backfill_organization_approval_notifications(manager).await
}

/// 为升级前已经存在的审批决定补齐通知事实，避免历史决定在通知查询中消失。
async fn backfill_organization_approval_notifications(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let quote = if backend == DbBackend::MySql {
        '`'
    } else {
        '"'
    };
    let identifier = |value: &str| format!("{quote}{value}{quote}");
    let notifications = identifier("organization_approval_notifications");
    let decisions = identifier("organization_approval_decisions");
    let requests = identifier("organization_approval_requests");
    let notification_key = identifier("notification_key");
    let organization_id = identifier("organization_id");
    let request_id = identifier("request_id");
    let decision_id = identifier("decision_id");
    let recipient_user_id = identifier("recipient_user_id");
    let decision_kind = identifier("decision_kind");
    let template_version = identifier("template_version");
    let occurred_at = identifier("occurred_at");
    let created_at = identifier("created_at");
    let decision_key = identifier("decision_key");
    let id = identifier("id");
    let requested_by_user_id = identifier("requested_by_user_id");
    let decided_at = identifier("decided_at");

    // 仅复制已存在且有对应申请的决定；通知键绑定决定键，重复执行不会新增事实。
    let statement = format!(
        "INSERT INTO {notifications} ({notification_key}, {organization_id}, {request_id}, \
         {decision_id}, {recipient_user_id}, {decision_kind}, {template_version}, {occurred_at}, \
         {created_at}) SELECT d.{decision_key}, d.{organization_id}, d.{request_id}, d.{id}, \
         r.{requested_by_user_id}, d.{decision_kind}, '{APPROVAL_NOTIFICATION_TEMPLATE_VERSION}', \
         d.{decided_at}, d.{created_at} FROM {decisions} AS d INNER JOIN {requests} AS r ON \
         r.{organization_id} = d.{organization_id} AND r.{id} = d.{request_id} WHERE NOT EXISTS \
         (SELECT 1 FROM {notifications} AS n WHERE n.{organization_id} = d.{organization_id} AND \
         n.{notification_key} = d.{decision_key})"
    );
    manager
        .get_connection()
        .execute(Statement::from_string(backend, statement))
        .await?;
    Ok(())
}

/// 删除企业审批决定站内通知事实表。
pub(in crate::migration) async fn drop_organization_approval_notifications(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_approval_notifications::Entity)
                .if_exists()
                .to_owned(),
        )
        .await
}

fn fixed_lower_hex_check<C>(backend: DbBackend, column: C, length: usize) -> SimpleExpr
where
    C: Iden,
{
    let name = Iden::to_string(&column);
    match backend {
        DbBackend::Postgres => Expr::cust(format!("\"{name}\" ~ '^[0-9a-f]{{{length}}}$'")),
        DbBackend::MySql => Expr::cust(format!(
            "CHAR_LENGTH(`{name}`) = {length} AND `{name}` REGEXP '^[0-9a-f]{{{length}}}$'"
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            "length(\"{name}\") = {length} AND \"{name}\" NOT GLOB '*[^0-9a-f]*'"
        )),
    }
}

fn timestamp_lte_check<L, R>(backend: DbBackend, left: L, right: R) -> SimpleExpr
where
    L: Iden + 'static,
    R: Iden + 'static,
{
    if backend != DbBackend::Sqlite {
        return Expr::col(left).lte(Expr::col(right));
    }
    let left: SimpleExpr = Func::cust(Alias::new("julianday"))
        .arg(Expr::col(left))
        .into();
    let right: SimpleExpr = Func::cust(Alias::new("julianday"))
        .arg(Expr::col(right))
        .into();
    left.clone()
        .is_not_null()
        .and(right.clone().is_not_null())
        .and(left.lte(right))
}
