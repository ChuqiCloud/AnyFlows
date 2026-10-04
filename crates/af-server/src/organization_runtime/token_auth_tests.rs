use std::{error::Error, sync::Arc, time::Duration};

use af_db::{
    OrganizationCreate, OrganizationEntitlementWrite, OrganizationRepository, TokenAuthLookup,
    TokenAuthLookupOutcome, TokenAuthRepository,
};
use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, TrustedClientIp, UserId,
};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::token_auth::DatabaseOrganizationTokenAuthValidator;
use crate::test_database::SqliteTestDatabase;

const USER_ID: i64 = 7_001;
const GROUP_ID: i64 = 7_002;
const TOKEN_ID: i64 = 7_003;
const VALID_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn test_client_ip() -> TrustedClientIp {
    TrustedClientIp::new("192.0.2.10".parse().expect("测试地址必须有效"))
}

#[tokio::test]
async fn organization_token_auth_uses_runtime_boundary() -> Result<(), Box<dyn Error>> {
    let database = SqliteTestDatabase::new("organization-token-auth").await;
    let backend = database.seed().get_database_backend();
    seed_user_and_token(database.seed(), backend).await;

    let created = OrganizationRepository::new(database.pool().clone(), Duration::from_secs(1))?
        .create(OrganizationCreate::new(
            "Token Auth Organization".to_owned(),
            "token-auth-organization".to_owned(),
            UserId::new(USER_ID)?,
            OrganizationEntitlementWrite::new(
                OrganizationEntitlementSnapshot::new(
                    OrganizationEntitlementCapacity::new(10, 10, 10)?,
                    true,
                    false,
                    false,
                    90,
                    946_684_800,
                    4_102_444_800,
                    Some(4_102_444_800),
                )?,
                "auth-test".to_owned(),
                None,
                None,
            )?,
        ))
        .await?;
    let (organization, owner_membership) = created.into_parts();
    database
        .seed()
        .execute(Statement::from_sql_and_values(
            backend,
            "UPDATE tokens SET organization_id = ?, organization_membership_id = ? WHERE id = ?",
            [
                organization.id().get().into(),
                owner_membership.id().get().into(),
                TOKEN_ID.into(),
            ],
        ))
        .await?;

    let lookup = TokenAuthLookup::new(VALID_HASH)?;
    let repository = TokenAuthRepository::new(database.pool().clone(), Duration::from_secs(1))?;
    assert_eq!(
        repository.lookup(&lookup, test_client_ip()).await?,
        TokenAuthLookupOutcome::Rejected
    );

    let repository = repository.with_organization_validator(Arc::new(
        DatabaseOrganizationTokenAuthValidator::new(database.pool().clone()),
    ));
    let outcome = repository.lookup(&lookup, test_client_ip()).await?;
    let TokenAuthLookupOutcome::Authenticated { principal, .. } = outcome else {
        return Err("active organization token should authenticate".into());
    };
    let organization_principal = principal
        .organization_principal()
        .ok_or("organization token must not become a personal principal")?;
    assert_eq!(organization_principal.organization_id(), organization.id());
    assert_eq!(
        organization_principal.membership_id(),
        owner_membership.id()
    );

    database
        .seed()
        .execute(Statement::from_sql_and_values(
            backend,
            "UPDATE organizations SET status = ? WHERE id = ?",
            [2_i16.into(), organization.id().get().into()],
        ))
        .await?;
    assert_eq!(
        repository.lookup(&lookup, test_client_ip()).await?,
        TokenAuthLookupOutcome::Rejected
    );
    database.close().await;
    Ok(())
}

async fn seed_user_and_token(connection: &sea_orm::DatabaseConnection, backend: DbBackend) {
    connection
        .execute(Statement::from_sql_and_values(
            backend,
            "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, ?)",
            [
                GROUP_ID.into(),
                "token-auth-group".into(),
                "Token Auth Group".into(),
                "{}".into(),
            ],
        ))
        .await
        .expect("测试分组必须写入");
    connection
        .execute(Statement::from_sql_and_values(
            backend,
            "INSERT INTO users (id, username, status, default_group_id, quota, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [
                USER_ID.into(),
                "token-auth-user".into(),
                1_i16.into(),
                GROUP_ID.into(),
                1_000_000_000_i64.into(),
                "token-auth-aff".into(),
                "{}".into(),
            ],
        ))
        .await
        .expect("测试用户必须写入");
    connection
        .execute(Statement::from_sql_and_values(
            backend,
            "INSERT INTO tokens (id, user_id, key_hash, key_prefix, name, status, remain_quota) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [
                TOKEN_ID.into(),
                USER_ID.into(),
                VALID_HASH.into(),
                "sk-af-token-auth".into(),
                "token-auth-token".into(),
                1_i16.into(),
                1_000_000_000_i64.into(),
            ],
        ))
        .await
        .expect("测试令牌必须写入");
}
