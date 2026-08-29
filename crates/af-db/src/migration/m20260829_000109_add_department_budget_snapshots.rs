use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

use super::iden::{
    billing_organization_reservation_departments as reservation_departments,
    billing_organization_reservations, organization_budget_policies, organization_budget_windows,
    organization_departments,
};
use super::schema::timestamp;

const POLICY_DEPARTMENT_FK: &str = "fk_organization_budget_policy_department";
const WINDOW_DEPARTMENT_FK: &str = "fk_organization_budget_window_department";
const RESERVATION_DEPARTMENT_FK: &str = "fk_billing_org_reservation_department";
const RESERVATION_PARENT_FK: &str = "fk_billing_org_reservation_departments_parent";
const RESERVATION_DEPARTMENT_INDEX: &str = "idx_billing_org_reservation_departments";
const WINDOW_SCOPE_ID_INDEX: &str = "uq_organization_budget_windows_scope_id";
const SQLITE_TRIGGERS: [&str; 4] = [
    "trg_organization_budget_policy_department_insert",
    "trg_organization_budget_policy_department_update",
    "trg_organization_budget_window_department_insert",
    "trg_organization_budget_window_department_update",
];

/// 扩展部门预算策略、窗口并保存请求期间的部门路径快照。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_big_integer_column_if_missing(
            manager,
            organization_budget_policies::Entity,
            organization_budget_policies::Column::DepartmentId,
        )
        .await?;
        add_big_integer_column_if_missing(
            manager,
            organization_budget_windows::Entity,
            organization_budget_windows::Column::DepartmentId,
        )
        .await?;
        add_string_column_if_missing(
            manager,
            billing_organization_reservations::Entity,
            billing_organization_reservations::Column::DepartmentPath,
        )
        .await?;
        upgrade_budget_scope_checks(manager).await?;
        manager
            .create_index(
                Index::create()
                    .name(WINDOW_SCOPE_ID_INDEX)
                    .table(organization_budget_windows::Entity)
                    .col(organization_budget_windows::Column::OrganizationId)
                    .col(organization_budget_windows::Column::Id)
                    .unique()
                    .to_owned(),
            )
            .await?;
        create_reservation_departments(manager).await?;
        manager
            .create_index(
                Index::create()
                    .name(RESERVATION_DEPARTMENT_INDEX)
                    .table(reservation_departments::Entity)
                    .col(reservation_departments::Column::OrganizationId)
                    .col(reservation_departments::Column::DepartmentId)
                    .col(reservation_departments::Column::PathIndex)
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() == DbBackend::Sqlite {
            create_sqlite_triggers(manager).await?;
        } else {
            manager
                .create_foreign_key(
                    ForeignKey::create()
                        .name(POLICY_DEPARTMENT_FK)
                        .from(
                            organization_budget_policies::Entity,
                            (
                                organization_budget_policies::Column::OrganizationId,
                                organization_budget_policies::Column::DepartmentId,
                            ),
                        )
                        .to(
                            organization_departments::Entity,
                            (
                                organization_departments::Column::OrganizationId,
                                organization_departments::Column::Id,
                            ),
                        )
                        .on_update(ForeignKeyAction::Restrict)
                        .on_delete(ForeignKeyAction::Restrict)
                        .to_owned(),
                )
                .await?;
            manager
                .create_foreign_key(
                    ForeignKey::create()
                        .name(WINDOW_DEPARTMENT_FK)
                        .from(
                            organization_budget_windows::Entity,
                            (
                                organization_budget_windows::Column::OrganizationId,
                                organization_budget_windows::Column::DepartmentId,
                            ),
                        )
                        .to(
                            organization_departments::Entity,
                            (
                                organization_departments::Column::OrganizationId,
                                organization_departments::Column::Id,
                            ),
                        )
                        .on_update(ForeignKeyAction::Restrict)
                        .on_delete(ForeignKeyAction::Restrict)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            for trigger in SQLITE_TRIGGERS {
                manager
                    .get_connection()
                    .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                    .await?;
            }
        } else {
            manager
                .drop_foreign_key(
                    ForeignKey::drop()
                        .name(WINDOW_DEPARTMENT_FK)
                        .table(organization_budget_windows::Entity)
                        .to_owned(),
                )
                .await?;
            manager
                .drop_foreign_key(
                    ForeignKey::drop()
                        .name(POLICY_DEPARTMENT_FK)
                        .table(organization_budget_policies::Entity)
                        .to_owned(),
                )
                .await?;
        }
        downgrade_budget_scope_checks(manager).await?;
        manager
            .drop_index(
                Index::drop()
                    .name(RESERVATION_DEPARTMENT_INDEX)
                    .table(reservation_departments::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(reservation_departments::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name(WINDOW_SCOPE_ID_INDEX)
                    .table(organization_budget_windows::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(billing_organization_reservations::Entity)
                    .drop_column(billing_organization_reservations::Column::DepartmentPath)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(organization_budget_windows::Entity)
                    .drop_column(organization_budget_windows::Column::DepartmentId)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(organization_budget_policies::Entity)
                    .drop_column(organization_budget_policies::Column::DepartmentId)
                    .to_owned(),
            )
            .await
    }
}

async fn add_big_integer_column_if_missing<T, C>(
    manager: &SchemaManager<'_>,
    table: T,
    column: C,
) -> Result<(), DbErr>
where
    T: IntoIden + sea_orm::Iden + 'static,
    C: IntoIden + sea_orm::Iden + 'static,
{
    let table_name = table.to_string();
    let column_name = column.to_string();
    if manager.has_column(&table_name, &column_name).await? {
        return Ok(());
    }
    manager
        .alter_table(
            Table::alter()
                .table(table)
                .add_column(ColumnDef::new(column).big_integer())
                .to_owned(),
        )
        .await
}

async fn add_string_column_if_missing<T, C>(
    manager: &SchemaManager<'_>,
    table: T,
    column: C,
) -> Result<(), DbErr>
where
    T: IntoIden + sea_orm::Iden + 'static,
    C: IntoIden + sea_orm::Iden + 'static,
{
    let table_name = table.to_string();
    let column_name = column.to_string();
    if manager.has_column(&table_name, &column_name).await? {
        return Ok(());
    }
    manager
        .alter_table(
            Table::alter()
                .table(table)
                .add_column(ColumnDef::new(column).string_len(255))
                .to_owned(),
        )
        .await
}

/// 将历史预算表的三层作用域检查升级为包含部门作用域的四层检查。
async fn upgrade_budget_scope_checks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => upgrade_sqlite_scope_checks(manager).await,
        DbBackend::Postgres | DbBackend::MySql => {
            for table in [
                "organization_budget_policies",
                "organization_budget_windows",
            ] {
                let constraints = load_scope_constraints(manager, table).await?;
                for constraint in constraints {
                    let escaped = constraint.replace('"', "\"\"");
                    let statement = match manager.get_database_backend() {
                        DbBackend::Postgres => {
                            format!("ALTER TABLE \"{table}\" DROP CONSTRAINT \"{escaped}\"")
                        }
                        DbBackend::MySql => format!(
                            "ALTER TABLE `{table}` DROP CHECK `{}`",
                            constraint.replace('`', "``")
                        ),
                        DbBackend::Sqlite => unreachable!(),
                    };
                    manager
                        .get_connection()
                        .execute_unprepared(&statement)
                        .await?;
                }
            }
            add_scope_constraints(manager, true).await
        }
    }
}

async fn load_scope_constraints(
    manager: &SchemaManager<'_>,
    table: &str,
) -> Result<Vec<String>, DbErr> {
    let backend = manager.get_database_backend();
    let sql = match backend {
        DbBackend::Postgres => format!(
            "SELECT tc.constraint_name AS constraint_name \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.check_constraints cc \
               ON cc.constraint_catalog = tc.constraint_catalog \
              AND cc.constraint_schema = tc.constraint_schema \
              AND cc.constraint_name = tc.constraint_name \
             WHERE tc.constraint_schema = current_schema() \
               AND tc.table_name = '{table}' \
               AND tc.constraint_type = 'CHECK' \
               AND cc.check_clause ILIKE '%scope_kind%'"
        ),
        DbBackend::MySql => format!(
            "SELECT tc.CONSTRAINT_NAME AS constraint_name \
             FROM information_schema.TABLE_CONSTRAINTS tc \
             JOIN information_schema.CHECK_CONSTRAINTS cc \
               ON cc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA \
              AND cc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME \
             WHERE tc.CONSTRAINT_SCHEMA = DATABASE() \
               AND tc.TABLE_NAME = '{table}' \
               AND tc.CONSTRAINT_TYPE = 'CHECK' \
               AND LOWER(cc.CHECK_CLAUSE) LIKE '%scope_kind%'"
        ),
        DbBackend::Sqlite => unreachable!(),
    };
    let rows = manager
        .get_connection()
        .query_all(Statement::from_string(backend, sql))
        .await?;
    rows.into_iter()
        .map(|row| row.try_get("", "constraint_name"))
        .collect()
}

async fn downgrade_budget_scope_checks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => downgrade_sqlite_scope_checks(manager).await,
        DbBackend::Postgres | DbBackend::MySql => {
            for table in [
                "organization_budget_policies",
                "organization_budget_windows",
            ] {
                for constraint in load_scope_constraints(manager, table).await? {
                    let statement = match manager.get_database_backend() {
                        DbBackend::Postgres => format!(
                            "ALTER TABLE \"{table}\" DROP CONSTRAINT \"{}\"",
                            constraint.replace('"', "\"\"")
                        ),
                        DbBackend::MySql => format!(
                            "ALTER TABLE `{table}` DROP CHECK `{}`",
                            constraint.replace('`', "``")
                        ),
                        DbBackend::Sqlite => unreachable!(),
                    };
                    manager
                        .get_connection()
                        .execute_unprepared(&statement)
                        .await?;
                }
            }
            add_scope_constraints(manager, false).await
        }
    }
}

async fn add_scope_constraints(
    manager: &SchemaManager<'_>,
    include_department: bool,
) -> Result<(), DbErr> {
    let statements = if include_department {
        [
            "ALTER TABLE {policy} ADD CONSTRAINT ck_organization_budget_policies_scope_kind CHECK ({scope} IN (1, 2, 3, 4))",
            "ALTER TABLE {policy} ADD CONSTRAINT ck_organization_budget_policies_subject_shape CHECK (({scope} = 1 AND {team} IS NULL AND {membership} IS NULL AND {department} IS NULL) OR ({scope} = 2 AND {team} IS NOT NULL AND {membership} IS NULL AND {department} IS NULL) OR ({scope} = 3 AND {team} IS NULL AND {membership} IS NOT NULL AND {department} IS NULL) OR ({scope} = 4 AND {team} IS NULL AND {membership} IS NULL AND {department} IS NOT NULL))",
            "ALTER TABLE {window} ADD CONSTRAINT ck_organization_budget_windows_scope_kind CHECK ({scope} IN (1, 2, 3, 4))",
            "ALTER TABLE {window} ADD CONSTRAINT ck_organization_budget_windows_subject_shape CHECK (({scope} = 1 AND {team} IS NULL AND {membership} IS NULL AND {department} IS NULL) OR ({scope} = 2 AND {team} IS NOT NULL AND {membership} IS NULL AND {department} IS NULL) OR ({scope} = 3 AND {team} IS NULL AND {membership} IS NOT NULL AND {department} IS NULL) OR ({scope} = 4 AND {team} IS NULL AND {membership} IS NULL AND {department} IS NOT NULL))",
        ]
    } else {
        [
            "ALTER TABLE {policy} ADD CONSTRAINT ck_organization_budget_policies_scope_kind CHECK ({scope} IN (1, 2, 3))",
            "ALTER TABLE {policy} ADD CONSTRAINT ck_organization_budget_policies_subject_shape CHECK (({scope} = 1 AND {team} IS NULL AND {membership} IS NULL) OR ({scope} = 2 AND {team} IS NOT NULL AND {membership} IS NULL) OR ({scope} = 3 AND {team} IS NULL AND {membership} IS NOT NULL))",
            "ALTER TABLE {window} ADD CONSTRAINT ck_organization_budget_windows_scope_kind CHECK ({scope} IN (1, 2, 3))",
            "ALTER TABLE {window} ADD CONSTRAINT ck_organization_budget_windows_subject_shape CHECK (({scope} = 1 AND {team} IS NULL AND {membership} IS NULL) OR ({scope} = 2 AND {team} IS NOT NULL AND {membership} IS NULL) OR ({scope} = 3 AND {team} IS NULL AND {membership} IS NOT NULL))",
        ]
    };
    for template in statements {
        let (quote, policy, window) = match manager.get_database_backend() {
            DbBackend::Postgres => (
                '"',
                "\"organization_budget_policies\"",
                "\"organization_budget_windows\"",
            ),
            DbBackend::MySql => (
                '`',
                "`organization_budget_policies`",
                "`organization_budget_windows`",
            ),
            DbBackend::Sqlite => unreachable!(),
        };
        let quoted = |name: &str| format!("{quote}{name}{quote}");
        let statement = template
            .replace("{policy}", policy)
            .replace("{window}", window)
            .replace("{scope}", &quoted("scope_kind"))
            .replace("{team}", &quoted("team_id"))
            .replace("{membership}", &quoted("membership_id"))
            .replace("{department}", &quoted("department_id"));
        manager
            .get_connection()
            .execute_unprepared(&statement)
            .await?;
    }
    Ok(())
}

async fn upgrade_sqlite_scope_checks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let mut rewrites = Vec::new();
    for table in [
        "organization_budget_policies",
        "organization_budget_windows",
    ] {
        let row = connection
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                format!("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '{table}'"),
            ))
            .await?
            .ok_or_else(|| DbErr::Custom(format!("预算表不存在: {table}")))?;
        let sql: String = row.try_get("", "sql")?;
        let rewritten = rewrite_sqlite_budget_checks(&sql, true)
            .ok_or_else(|| DbErr::Custom(format!("无法升级预算表约束: {table}")))?;
        if rewritten != sql {
            rewrites.push((table, rewritten));
        }
    }
    if rewrites.is_empty() {
        return Ok(());
    }

    connection
        .execute_unprepared("PRAGMA writable_schema = ON")
        .await?;
    let result = async {
        for (table, sql) in &rewrites {
            connection
                .execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "UPDATE sqlite_master SET sql = ? WHERE type = 'table' AND name = ?",
                    [sql.clone().into(), (*table).into()],
                ))
                .await?;
        }
        Ok::<(), DbErr>(())
    }
    .await;
    let disable_result = connection
        .execute_unprepared("PRAGMA writable_schema = OFF")
        .await;
    result?;
    disable_result?;
    let version = connection
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA schema_version",
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("SQLite schema 版本读取失败".to_owned()))?
        .try_get::<i32>("", "schema_version")?;
    connection
        .execute_unprepared(&format!("PRAGMA schema_version = {}", version + 1))
        .await?;
    let integrity = connection
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA integrity_check",
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("SQLite integrity_check 未返回结果".to_owned()))?;
    let integrity: String = integrity.try_get("", "integrity_check")?;
    if integrity != "ok" {
        return Err(DbErr::Custom(
            "SQLite 预算表约束升级后完整性检查失败".to_owned(),
        ));
    }
    Ok(())
}

async fn downgrade_sqlite_scope_checks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let mut rewrites = Vec::new();
    for table in [
        "organization_budget_policies",
        "organization_budget_windows",
    ] {
        let row = connection
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                format!("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '{table}'"),
            ))
            .await?
            .ok_or_else(|| DbErr::Custom(format!("预算表不存在: {table}")))?;
        let sql: String = row.try_get("", "sql")?;
        let rewritten = rewrite_sqlite_budget_checks(&sql, false)
            .ok_or_else(|| DbErr::Custom(format!("无法回退预算表约束: {table}")))?;
        if rewritten != sql {
            rewrites.push((table, rewritten));
        }
    }
    if rewrites.is_empty() {
        return Ok(());
    }
    connection
        .execute_unprepared("PRAGMA writable_schema = ON")
        .await?;
    let result = async {
        for (table, sql) in &rewrites {
            connection
                .execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "UPDATE sqlite_master SET sql = ? WHERE type = 'table' AND name = ?",
                    [sql.clone().into(), (*table).into()],
                ))
                .await?;
        }
        Ok::<(), DbErr>(())
    }
    .await;
    let disable_result = connection
        .execute_unprepared("PRAGMA writable_schema = OFF")
        .await;
    result?;
    disable_result?;
    Ok(())
}

fn rewrite_sqlite_budget_checks(sql: &str, include_department: bool) -> Option<String> {
    let mut output = String::with_capacity(sql.len() + 160);
    let mut cursor = 0;
    let mut changed = false;
    while let Some(start) = find_check_keyword(sql, cursor) {
        let open = sql[start + 5..]
            .find('(')
            .map(|offset| start + 5 + offset)?;
        let close = matching_parenthesis(sql, open)?;
        let expression = &sql[open + 1..close];
        if !expression.to_ascii_lowercase().contains("scope_kind") {
            output.push_str(&sql[cursor..close + 1]);
            cursor = close + 1;
            continue;
        }
        output.push_str(&sql[cursor..start]);
        let lower = expression.to_ascii_lowercase();
        let replacement = if lower.contains(" in ") || lower.contains("in(") {
            if include_department {
                "CHECK (\"scope_kind\" IN (1, 2, 3, 4))"
            } else {
                "CHECK (\"scope_kind\" IN (1, 2, 3))"
            }
        } else {
            if include_department {
                "CHECK ((\"scope_kind\" = 1 AND \"team_id\" IS NULL AND \"membership_id\" IS NULL AND \"department_id\" IS NULL) OR (\"scope_kind\" = 2 AND \"team_id\" IS NOT NULL AND \"membership_id\" IS NULL AND \"department_id\" IS NULL) OR (\"scope_kind\" = 3 AND \"team_id\" IS NULL AND \"membership_id\" IS NOT NULL AND \"department_id\" IS NULL) OR (\"scope_kind\" = 4 AND \"team_id\" IS NULL AND \"membership_id\" IS NULL AND \"department_id\" IS NOT NULL))"
            } else {
                "CHECK ((\"scope_kind\" = 1 AND \"team_id\" IS NULL AND \"membership_id\" IS NULL) OR (\"scope_kind\" = 2 AND \"team_id\" IS NOT NULL AND \"membership_id\" IS NULL) OR (\"scope_kind\" = 3 AND \"team_id\" IS NULL AND \"membership_id\" IS NOT NULL))"
            }
        };
        output.push_str(replacement);
        cursor = close + 1;
        changed = true;
    }
    if !changed {
        return None;
    }
    output.push_str(&sql[cursor..]);
    Some(output)
}

fn find_check_keyword(sql: &str, from: usize) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut quote = None;
    let mut index = from;
    while index + 5 <= bytes.len() {
        match quote {
            Some(delimiter) => {
                if bytes[index] == delimiter {
                    if bytes.get(index + 1) == Some(&delimiter) {
                        index += 2;
                        continue;
                    }
                    quote = None;
                }
                index += 1;
            }
            None if matches!(bytes[index], b'\'' | b'"' | b'`') => {
                quote = Some(bytes[index]);
                index += 1;
            }
            None => {
                let candidate = &sql[index..index + 5];
                if candidate.eq_ignore_ascii_case("check")
                    && (index == 0 || !bytes[index - 1].is_ascii_alphanumeric())
                    && (index + 5 == bytes.len() || !bytes[index + 5].is_ascii_alphanumeric())
                {
                    return Some(index);
                }
                index += 1;
            }
        }
    }
    None
}

fn matching_parenthesis(sql: &str, open: usize) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut depth = 0_i32;
    let mut quote = None;
    let mut index = open;
    while index < bytes.len() {
        match quote {
            Some(delimiter) => {
                if bytes[index] == delimiter {
                    if bytes.get(index + 1) == Some(&delimiter) {
                        index += 2;
                        continue;
                    }
                    quote = None;
                }
            }
            None if matches!(bytes[index], b'\'' | b'"' | b'`') => quote = Some(bytes[index]),
            None if bytes[index] == b'(' => depth += 1,
            None if bytes[index] == b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            None => {}
        }
        index += 1;
    }
    None
}

async fn create_reservation_departments(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use reservation_departments::Column;
    let mut table = Table::create();
    table
        .table(reservation_departments::Entity)
        .if_not_exists()
        .col(
            ColumnDef::new(Column::IdempotencyKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::PathIndex)
                .small_integer()
                .not_null()
                .check(Expr::col(Column::PathIndex).between(0_i16, 7_i16)),
        )
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::DepartmentId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::PolicyId).big_integer().not_null())
        .col(ColumnDef::new(Column::WindowId).big_integer().not_null())
        .col(
            ColumnDef::new(Column::PolicyVersion)
                .big_integer()
                .not_null()
                .check(Expr::col(Column::PolicyVersion).gt(0_i64)),
        )
        .col(
            ColumnDef::new(Column::WindowVersion)
                .big_integer()
                .not_null()
                .check(Expr::col(Column::WindowVersion).gt(0_i64)),
        )
        .col(
            ColumnDef::new(Column::ReservedQuota)
                .big_integer()
                .not_null()
                .check(Expr::col(Column::ReservedQuota).gt(0_i64)),
        )
        .col(timestamp(manager, Column::CreatedAt))
        .col(timestamp(manager, Column::UpdatedAt))
        .primary_key(
            Index::create()
                .col(Column::IdempotencyKey)
                .col(Column::PathIndex),
        )
        .foreign_key(
            ForeignKey::create()
                .name(RESERVATION_PARENT_FK)
                .from(
                    reservation_departments::Entity,
                    reservation_departments::Column::IdempotencyKey,
                )
                .to(
                    billing_organization_reservations::Entity,
                    billing_organization_reservations::Column::IdempotencyKey,
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name(RESERVATION_DEPARTMENT_FK)
                .from(
                    reservation_departments::Entity,
                    (
                        reservation_departments::Column::OrganizationId,
                        reservation_departments::Column::DepartmentId,
                    ),
                )
                .to(
                    organization_departments::Entity,
                    (
                        organization_departments::Column::OrganizationId,
                        organization_departments::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_billing_org_reservation_department_policy")
                .from(
                    reservation_departments::Entity,
                    (
                        reservation_departments::Column::OrganizationId,
                        reservation_departments::Column::PolicyId,
                    ),
                )
                .to(
                    organization_budget_policies::Entity,
                    (
                        organization_budget_policies::Column::OrganizationId,
                        organization_budget_policies::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_billing_org_reservation_department_window")
                .from(
                    reservation_departments::Entity,
                    (
                        reservation_departments::Column::OrganizationId,
                        reservation_departments::Column::WindowId,
                    ),
                )
                .to(
                    organization_budget_windows::Entity,
                    (
                        organization_budget_windows::Column::OrganizationId,
                        organization_budget_windows::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(table.to_owned()).await?;
    Ok(())
}

async fn create_sqlite_triggers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statements = [
        "CREATE TRIGGER trg_organization_budget_policy_department_insert BEFORE INSERT ON organization_budget_policies WHEN NEW.department_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_departments WHERE organization_id = NEW.organization_id AND id = NEW.department_id AND status = 1) BEGIN SELECT RAISE(ABORT, 'invalid organization budget policy department'); END",
        "CREATE TRIGGER trg_organization_budget_policy_department_update BEFORE UPDATE OF organization_id, department_id ON organization_budget_policies WHEN NEW.department_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_departments WHERE organization_id = NEW.organization_id AND id = NEW.department_id AND status = 1) BEGIN SELECT RAISE(ABORT, 'invalid organization budget policy department'); END",
        "CREATE TRIGGER trg_organization_budget_window_department_insert BEFORE INSERT ON organization_budget_windows WHEN NEW.department_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_departments WHERE organization_id = NEW.organization_id AND id = NEW.department_id AND status = 1) BEGIN SELECT RAISE(ABORT, 'invalid organization budget window department'); END",
        "CREATE TRIGGER trg_organization_budget_window_department_update BEFORE UPDATE OF organization_id, department_id ON organization_budget_windows WHEN NEW.department_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_departments WHERE organization_id = NEW.organization_id AND id = NEW.department_id AND status = 1) BEGIN SELECT RAISE(ABORT, 'invalid organization budget window department'); END",
    ];
    for statement in statements {
        manager
            .get_connection()
            .execute_unprepared(statement)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::rewrite_sqlite_budget_checks;

    #[test]
    fn rewrites_legacy_scope_and_subject_checks_without_touching_other_checks() {
        let sql = r#"CREATE TABLE organization_budget_policies (
            scope_kind SMALLINT NOT NULL CHECK ("scope_kind" IN (1, 2, 3)),
            team_id BIGINT,
            membership_id BIGINT,
            department_id BIGINT,
            quota_limit BIGINT CHECK (quota_limit IS NULL OR quota_limit > 0),
            CHECK (("scope_kind" = 1 AND "team_id" IS NULL AND "membership_id" IS NULL)
                OR ("scope_kind" = 2 AND "team_id" IS NOT NULL AND "membership_id" IS NULL)
                OR ("scope_kind" = 3 AND "team_id" IS NULL AND "membership_id" IS NOT NULL))
        )"#;

        let upgraded = rewrite_sqlite_budget_checks(sql, true).expect("旧检查应可升级");
        assert!(upgraded.contains("\"scope_kind\" IN (1, 2, 3, 4)"));
        assert!(upgraded.contains("\"scope_kind\" = 4"));
        assert!(upgraded.contains("quota_limit IS NULL OR quota_limit > 0"));
        assert!(!upgraded.contains("\"scope_kind\" IN (1, 2, 3))"));

        let downgraded = rewrite_sqlite_budget_checks(&upgraded, false).expect("新检查应可回退");
        assert!(downgraded.contains("\"scope_kind\" IN (1, 2, 3))"));
        assert!(!downgraded.contains("\"scope_kind\" = 4"));
        assert!(downgraded.contains("quota_limit IS NULL OR quota_limit > 0"));
    }
}
