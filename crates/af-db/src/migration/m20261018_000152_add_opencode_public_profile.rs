use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 新档案独立发布，保留 standard/opencode 初始修订与所有现有绑定。
        let definition = serde_json::json!({
            "name": "OpenCode（公开访问）", "description": "", "icon": null,
            "service": "open_code", "behavior": "open_code_v1",
            "protocols": ["openai_chat", "openai_responses", "anthropic"],
            "credential_kinds": ["public"], "session_strategy": "explicit",
            "identity_headers": {"user-agent": "opencode/latest/2.0.29/cli", "x-opencode-client": "cli"}
        });
        let backend = manager.get_database_backend();
        for insert in [
            Query::insert()
                .into_table(Alias::new("client_profiles"))
                .columns(
                    [
                        "profile_key",
                        "name",
                        "service",
                        "protocols",
                        "credential_kinds",
                        "built_in",
                    ]
                    .map(Alias::new),
                )
                .values_panic([
                    "opencode-public".into(),
                    "OpenCode（公开访问）".into(),
                    "open_code".into(),
                    ",openai_chat,openai_responses,anthropic,".into(),
                    ",public,".into(),
                    true.into(),
                ])
                .to_owned(),
            Query::insert()
                .into_table(Alias::new("client_profile_revisions"))
                .columns(["profile_key", "revision", "definition"].map(Alias::new))
                .values_panic(["opencode-public".into(), 1_i64.into(), definition.into()])
                .to_owned(),
        ] {
            manager
                .get_connection()
                .execute(backend.build(&insert))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for (table, column, value) in [
            ("credentials", "kind", "public"),
            ("channel_client_profiles", "profile_key", "opencode-public"),
        ] {
            let used = Query::select()
                .column(Alias::new(column))
                .from(Alias::new(table))
                .and_where(Expr::col(Alias::new(column)).eq(value))
                .limit(1)
                .to_owned();
            if manager
                .get_connection()
                .query_one(backend.build(&used))
                .await?
                .is_some()
            {
                return Err(DbErr::Custom(
                    "公开访问已被使用，不能回退档案迁移".to_owned(),
                ));
            }
        }
        for table in ["client_profile_revisions", "client_profiles"] {
            let delete = Query::delete()
                .from_table(Alias::new(table))
                .and_where(Expr::col(Alias::new("profile_key")).eq("opencode-public"))
                .to_owned();
            manager
                .get_connection()
                .execute(backend.build(&delete))
                .await?;
        }
        Ok(())
    }
}
