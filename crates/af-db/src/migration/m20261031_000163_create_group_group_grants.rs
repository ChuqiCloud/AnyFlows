use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        extend_selection_mode_constraint(manager).await?;
        let table = Table::create()
            .table(Alias::new("group_group_grants"))
            .if_not_exists()
            .col(
                ColumnDef::new(Alias::new("target_group_id"))
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("source_group_id"))
                    .big_integer()
                    .not_null(),
            )
            .primary_key(
                Index::create()
                    .name("pk_group_group_grants")
                    .col(Alias::new("target_group_id"))
                    .col(Alias::new("source_group_id")),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_group_group_grant_target")
                    .from(
                        Alias::new("group_group_grants"),
                        Alias::new("target_group_id"),
                    )
                    .to(Alias::new("groups"), Alias::new("id"))
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_group_group_grant_source")
                    .from(
                        Alias::new("group_group_grants"),
                        Alias::new("source_group_id"),
                    )
                    .to(Alias::new("groups"), Alias::new("id"))
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .to_owned();
        manager.create_table(table).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_group_group_grants_source_target")
                    .table(Alias::new("group_group_grants"))
                    .col(Alias::new("source_group_id"))
                    .col(Alias::new("target_group_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        restore_selection_mode_constraint(manager).await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("group_group_grants"))
                    .to_owned(),
            )
            .await
    }
}

async fn extend_selection_mode_constraint(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    rewrite_selection_mode_constraint(manager, true).await
}

async fn restore_selection_mode_constraint(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let row = connection
        .query_one(Statement::from_string(
            manager.get_database_backend(),
            "SELECT COUNT(*) AS selected_group_count FROM group_key_policies WHERE selection_mode = 'selected_groups'",
        ))
        .await?;
    if let Some(row) = row {
        let count: i64 = row.try_get("", "selected_group_count")?;
        if count > 0 {
            return Err(DbErr::Custom(
                "存在 selected_groups 策略，无法回退分组选组授权迁移".to_owned(),
            ));
        }
    }
    rewrite_selection_mode_constraint(manager, false).await
}

async fn rewrite_selection_mode_constraint(
    manager: &SchemaManager<'_>,
    allow_selected_groups: bool,
) -> Result<(), DbErr> {
    let selection_values = if allow_selected_groups {
        "'default_only', 'all_users', 'selected_users', 'selected_groups'"
    } else {
        "'default_only', 'all_users', 'selected_users'"
    };
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Postgres => {
            connection
                .execute_unprepared(&format!(
                    r#"DO $$
DECLARE constraint_name text;
BEGIN
    SELECT conname INTO constraint_name
      FROM pg_constraint
     WHERE conrelid = 'group_key_policies'::regclass
       AND contype = 'c'
       AND pg_get_constraintdef(oid) LIKE '%selection_mode%';
    IF constraint_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE group_key_policies DROP CONSTRAINT %I', constraint_name);
    END IF;
END $$;
ALTER TABLE group_key_policies
    ADD CONSTRAINT ck_group_key_policies_selection_mode
    CHECK (selection_mode IN ({}))"#,
                    selection_values
                ))
                .await?;
        }
        DbBackend::MySql => {
            let rows = connection
                .query_all(Statement::from_string(
                    DbBackend::MySql,
                    r#"SELECT tc.CONSTRAINT_NAME AS constraint_name
FROM information_schema.TABLE_CONSTRAINTS tc
JOIN information_schema.CHECK_CONSTRAINTS cc
  ON cc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA
 AND cc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME
WHERE tc.CONSTRAINT_SCHEMA = DATABASE()
  AND tc.TABLE_NAME = 'group_key_policies'
  AND tc.CONSTRAINT_TYPE = 'CHECK'
  AND LOWER(cc.CHECK_CLAUSE) LIKE '%selection_mode%'"#,
                ))
                .await?;
            for row in rows {
                let constraint: String = row.try_get("", "constraint_name")?;
                connection
                    .execute_unprepared(&format!(
                        "ALTER TABLE `group_key_policies` DROP CHECK `{}`",
                        constraint.replace('`', "``")
                    ))
                    .await?;
            }
            connection
                .execute_unprepared(&format!(
                    "ALTER TABLE `group_key_policies` ADD CONSTRAINT `ck_group_key_policies_selection_mode` CHECK (`selection_mode` IN ({}))",
                    selection_values
                ))
                .await?;
        }
        DbBackend::Sqlite => {
            connection
                .execute_unprepared("PRAGMA foreign_keys = OFF")
                .await?;
            let result = async {
                let statements = [
                    format!("CREATE TABLE group_key_policies_new (group_id INTEGER NOT NULL PRIMARY KEY, selection_mode VARCHAR(32) NOT NULL CHECK (selection_mode IN ({})), FOREIGN KEY (group_id) REFERENCES groups(id) ON UPDATE CASCADE ON DELETE CASCADE)", selection_values),
                    "INSERT INTO group_key_policies_new (group_id, selection_mode) SELECT group_id, selection_mode FROM group_key_policies".to_owned(),
                    "DROP TABLE group_key_policies".to_owned(),
                    "ALTER TABLE group_key_policies_new RENAME TO group_key_policies".to_owned(),
                ];
                for statement in statements {
                    connection.execute_unprepared(&statement).await?;
                }
                Ok::<(), DbErr>(())
            }
            .await;
            let restore = connection
                .execute_unprepared("PRAGMA foreign_keys = ON")
                .await
                .map(|_| ());
            match (result, restore) {
                (Err(error), _) => return Err(error),
                (Ok(()), Err(error)) => return Err(error),
                (Ok(()), Ok(())) => {}
            }
        }
    }
    Ok(())
}
