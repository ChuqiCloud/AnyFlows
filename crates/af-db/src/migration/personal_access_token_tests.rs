use std::error::Error;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set,
    sea_query::{Alias, Query},
};
use sea_orm_migration::SchemaManager;

use crate::entity::{
    access_token_audit_logs as audit, access_token_policies as policies,
    access_token_rate_limits as limits, user_access_token_proofs as proofs,
    user_access_tokens as tokens,
};

/// Runs against the isolated SQLite/PostgreSQL/MySQL migration job's upgraded database.
pub(super) async fn check_storage(pool: &crate::DatabasePool) -> Result<(), Box<dyn Error>> {
    let connection = pool.connection();
    let schema = SchemaManager::new(connection);
    for (table, columns) in [
        (
            "user_access_tokens",
            vec![
                "secret_hash",
                "scopes",
                "expires_at",
                "revoked_at",
                "version",
            ],
        ),
        (
            "user_access_token_proofs",
            vec![
                "proof_hash",
                "session_binding",
                "payload_digest",
                "consumed_at",
            ],
        ),
        (
            "access_token_policies",
            vec!["enabled", "issuance_enabled", "version"],
        ),
        (
            "access_token_audit_logs",
            vec!["access_token_id", "resource_id", "client_ip"],
        ),
        (
            "access_token_rate_limits",
            vec!["subject_key", "used_requests", "expires_at"],
        ),
        (
            "platform_audit_logs",
            vec!["actor_credential_kind", "actor_credential_id"],
        ),
        (
            "merchant_payouts",
            vec![
                "initiated_by_user_id",
                "initiated_by_access_token_id",
                "initiated_request_id",
                "initiated_scope",
                "submit_authorized_at",
            ],
        ),
    ] {
        assert!(schema.has_table(table).await?);
        for column in columns {
            assert!(schema.has_column(table, column).await?);
        }
    }
    let policy = policies::Entity::find_by_id(1_i64)
        .one(connection)
        .await?
        .unwrap();
    assert!(policy.enabled);
    assert!(policy.issuance_enabled);
    assert_eq!(policy.max_tokens_per_user, 20);
    assert_eq!(policy.version, 1);
    let backend = connection.get_database_backend();
    let user = 9_900_180_i64;
    let insert_user = Query::insert()
        .into_table(Alias::new("users"))
        .columns(["id", "username", "default_group_id", "aff_code", "settings"].map(Alias::new))
        .values_panic([
            user.into(),
            "pat-migration".into(),
            1_i64.into(),
            "pat-migration-aff".into(),
            serde_json::json!({}).into(),
        ])
        .to_owned();
    connection.execute(backend.build(&insert_user)).await?;
    let token = tokens::ActiveModel {
        user_id: Set(user),
        secret_hash: Set("e".repeat(64)),
        token_hint: Set("af_pat_123456789012".to_owned()),
        name: Set("migration-roundtrip".to_owned()),
        scopes: Set(serde_json::json!(["account:read"])),
        enabled: Set(true),
        expires_at: Set(None),
        revoked_at: Set(None),
        allow_ips: Set(None),
        created_at: Set(10),
        updated_at: Set(10),
        last_used_at: Set(None),
        last_used_ip: Set(None),
        version: Set(1),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    assert!(token.id > 0);
    assert_eq!(token.scopes, serde_json::json!(["account:read"]));
    assert!(token.expires_at.is_none());
    let proof = proofs::ActiveModel {
        proof_hash: Set("f".repeat(64)),
        user_id: Set(user),
        session_version: Set(1),
        session_binding: Set("a".repeat(64)),
        purpose: Set("create".to_owned()),
        target_id: Set(None),
        payload_digest: Set("b".repeat(64)),
        issued_at: Set(10),
        expires_at: Set(310),
        consumed_at: Set(None),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    assert!(proof.id > 0);
    let rate = limits::ActiveModel {
        subject_key: Set("c".repeat(64)),
        window_started_at: Set(0),
        used_requests: Set(1),
        expires_at: Set(120),
    }
    .insert(connection)
    .await?;
    assert_eq!(rate.used_requests, 1);
    let audit = audit::ActiveModel {
        user_id: Set(Some(user)),
        access_token_id: Set(Some(token.id)),
        resource_id: Set(Some("1".to_owned())),
        operation: Set("request".to_owned()),
        request_id: Set("pat-migration-request".to_owned()),
        method: Set("GET".to_owned()),
        route: Set("/api/account/profile".to_owned()),
        required_scope: Set(Some("account:read".to_owned())),
        status_code: Set(200),
        allowed: Set(true),
        client_ip: Set(Some("127.0.0.1".to_owned())),
        created_at: Set(10),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    assert!(audit.id > 0);
    let delete_user = Query::delete()
        .from_table(Alias::new("users"))
        .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(user))
        .to_owned();
    connection.execute(backend.build(&delete_user)).await?;
    // Credential/proof rows follow their owner, while immutable audit provenance remains.
    assert!(
        tokens::Entity::find_by_id(token.id)
            .one(connection)
            .await?
            .is_none()
    );
    assert!(
        proofs::Entity::find_by_id(proof.id)
            .one(connection)
            .await?
            .is_none()
    );
    assert!(
        audit::Entity::find_by_id(audit.id)
            .one(connection)
            .await?
            .is_some()
    );
    audit::Entity::delete_by_id(audit.id)
        .exec(connection)
        .await?;
    limits::Entity::delete_many()
        .filter(limits::Column::SubjectKey.eq("c".repeat(64)))
        .exec(connection)
        .await?;
    Ok(())
}
