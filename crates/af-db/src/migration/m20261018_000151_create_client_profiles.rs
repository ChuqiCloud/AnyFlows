use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

use super::schema::{table, timestamp};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[derive(DeriveIden)]
enum Profiles {
    #[sea_orm(iden = "client_profiles")]
    Table,
    ProfileKey,
    Name,
    Service,
    Protocols,
    CredentialKinds,
    BuiltIn,
    Enabled,
    LatestRevision,
    Version,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Revisions {
    #[sea_orm(iden = "client_profile_revisions")]
    Table,
    ProfileKey,
    Revision,
    Definition,
    CreatedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Bindings {
    #[sea_orm(iden = "channel_client_profiles")]
    Table,
    ChannelId,
    ProfileKey,
    Revision,
    Version,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut profiles = table(manager, Profiles::Table);
        profiles
            .col(
                ColumnDef::new(Profiles::ProfileKey)
                    .string_len(64)
                    .not_null()
                    .primary_key(),
            )
            .col(ColumnDef::new(Profiles::Name).string_len(128).not_null())
            .col(ColumnDef::new(Profiles::Service).string_len(32).not_null())
            .col(
                ColumnDef::new(Profiles::Protocols)
                    .string_len(512)
                    .not_null(),
            )
            .col(
                ColumnDef::new(Profiles::CredentialKinds)
                    .string_len(256)
                    .not_null(),
            )
            .col(
                ColumnDef::new(Profiles::BuiltIn)
                    .boolean()
                    .not_null()
                    .default(false),
            )
            .col(
                ColumnDef::new(Profiles::Enabled)
                    .boolean()
                    .not_null()
                    .default(true),
            )
            .col(
                ColumnDef::new(Profiles::LatestRevision)
                    .big_integer()
                    .not_null()
                    .default(1_i64),
            )
            .col(
                ColumnDef::new(Profiles::Version)
                    .big_integer()
                    .not_null()
                    .default(1_i64),
            )
            .col(timestamp(manager, Profiles::CreatedAt))
            .col(timestamp(manager, Profiles::UpdatedAt));
        manager.create_table(profiles).await?;
        let mut revisions = table(manager, Revisions::Table);
        revisions
            .col(
                ColumnDef::new(Revisions::ProfileKey)
                    .string_len(64)
                    .not_null(),
            )
            .col(ColumnDef::new(Revisions::Revision).big_integer().not_null())
            .col(
                ColumnDef::new(Revisions::Definition)
                    .json_binary()
                    .not_null(),
            )
            .col(ColumnDef::new(Revisions::CreatedBy).big_integer())
            .col(timestamp(manager, Revisions::CreatedAt))
            .primary_key(
                Index::create()
                    .col(Revisions::ProfileKey)
                    .col(Revisions::Revision),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_client_profile_revision")
                    .from(Revisions::Table, Revisions::ProfileKey)
                    .to(Profiles::Table, Profiles::ProfileKey)
                    .on_delete(ForeignKeyAction::Restrict),
            );
        manager.create_table(revisions).await?;
        let mut bindings = table(manager, Bindings::Table);
        bindings
            .col(
                ColumnDef::new(Bindings::ChannelId)
                    .big_integer()
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(Bindings::ProfileKey)
                    .string_len(64)
                    .not_null(),
            )
            .col(ColumnDef::new(Bindings::Revision).big_integer().not_null())
            .col(
                ColumnDef::new(Bindings::Version)
                    .big_integer()
                    .not_null()
                    .default(1_i64),
            )
            .col(timestamp(manager, Bindings::UpdatedAt))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_channel_client_profile_revision")
                    .from(Bindings::Table, (Bindings::ProfileKey, Bindings::Revision))
                    .to(
                        Revisions::Table,
                        (Revisions::ProfileKey, Revisions::Revision),
                    )
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_channel_client_profile_channel")
                    .from(Bindings::Table, Bindings::ChannelId)
                    .to(Alias::new("channels"), Alias::new("id"))
                    .on_delete(ForeignKeyAction::Cascade),
            );
        manager.create_table(bindings).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_channel_client_profile_reference")
                    .table(Bindings::Table)
                    .col(Bindings::ProfileKey)
                    .col(Bindings::Revision)
                    .col(Bindings::ChannelId)
                    .to_owned(),
            )
            .await?;
        seed_initial_revisions(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for name in [
            "channel_client_profiles",
            "client_profile_revisions",
            "client_profiles",
        ] {
            manager
                .drop_table(Table::drop().table(Alias::new(name)).to_owned())
                .await?;
        }
        Ok(())
    }
}

/// 初始迁移快照独立于运行时代码，升级内置档案时另加迁移。
async fn seed_initial_revisions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let all_protocols = "openai_chat,openai_responses,anthropic,gemini";
    let all_kinds = "api_key,oauth,setup_token,bedrock,service_account,upstream";
    let seeds = [
        (
            "standard",
            "标准请求",
            "standard",
            "standard",
            all_protocols,
            all_kinds,
            serde_json::json!({}),
        ),
        (
            "claude-code",
            "Claude Code",
            "claude_code",
            "standard",
            "anthropic",
            "oauth",
            serde_json::json!({"user-agent":"claude-cli/2.1.114 (external, sdk-cli)","x-app":"cli"}),
        ),
        (
            "claude-code-date",
            "Claude Code（UTC 日期）",
            "claude_code",
            "claude_utc_date_v1",
            "anthropic",
            "oauth",
            serde_json::json!({"user-agent":"claude-cli/2.1.114 (external, sdk-cli)","x-app":"cli"}),
        ),
        (
            "codex",
            "Codex",
            "codex",
            "codex_v1",
            "openai_responses",
            "oauth",
            serde_json::json!({}),
        ),
        (
            "opencode",
            "OpenCode",
            "open_code",
            "open_code_v1",
            "openai_chat,openai_responses,anthropic",
            "api_key",
            serde_json::json!({"user-agent":"opencode/latest/2.0.29/cli","x-opencode-client":"cli"}),
        ),
    ];
    let backend = manager.get_database_backend();
    for (key, name, service, behavior, protocols, kinds, headers) in seeds {
        let definition = serde_json::json!({"name":name,"description":"","icon":null,"service":service,
            "protocols": protocols.split(',').collect::<Vec<_>>(), "credential_kinds": kinds.split(',').collect::<Vec<_>>(),
            "identity_headers":headers, "session_strategy":"explicit", "behavior":behavior});
        let insert = Query::insert()
            .into_table(Profiles::Table)
            .columns([
                Profiles::ProfileKey,
                Profiles::Name,
                Profiles::Service,
                Profiles::Protocols,
                Profiles::CredentialKinds,
                Profiles::BuiltIn,
            ])
            .values_panic([
                key.into(),
                name.into(),
                service.into(),
                format!(",{protocols},").into(),
                format!(",{kinds},").into(),
                true.into(),
            ])
            .to_owned();
        manager
            .get_connection()
            .execute(backend.build(&insert))
            .await?;
        let insert = Query::insert()
            .into_table(Revisions::Table)
            .columns([
                Revisions::ProfileKey,
                Revisions::Revision,
                Revisions::Definition,
            ])
            .values_panic([key.into(), 1_i64.into(), definition.into()])
            .to_owned();
        manager
            .get_connection()
            .execute(backend.build(&insert))
            .await?;
    }
    // 历史 Claude 字段保留；只有新绑定显式切换时才更改其行为。
    let sql = match backend {
        DbBackend::Postgres => {
            "INSERT INTO channel_client_profiles (channel_id, profile_key, revision) SELECT id, CASE WHEN settings->>'client_simulation_body_profile' = 'anthropic_cli_system_date_v1' THEN 'claude-code-date' ELSE 'claude-code' END, 1 FROM channels WHERE settings->>'client_simulation_profile' = 'anthropic_cli_headers_v1'"
        }
        DbBackend::MySql => {
            "INSERT INTO channel_client_profiles (channel_id, profile_key, revision) SELECT id, CASE WHEN JSON_UNQUOTE(JSON_EXTRACT(settings, '$.client_simulation_body_profile')) = 'anthropic_cli_system_date_v1' THEN 'claude-code-date' ELSE 'claude-code' END, 1 FROM channels WHERE JSON_UNQUOTE(JSON_EXTRACT(settings, '$.client_simulation_profile')) = 'anthropic_cli_headers_v1'"
        }
        DbBackend::Sqlite => {
            "INSERT INTO channel_client_profiles (channel_id, profile_key, revision) SELECT id, CASE WHEN json_extract(settings, '$.client_simulation_body_profile') = 'anthropic_cli_system_date_v1' THEN 'claude-code-date' ELSE 'claude-code' END, 1 FROM channels WHERE json_extract(settings, '$.client_simulation_profile') = 'anthropic_cli_headers_v1'"
        }
    };
    manager
        .get_connection()
        .execute(Statement::from_string(backend, sql.to_owned()))
        .await?;
    Ok(())
}
