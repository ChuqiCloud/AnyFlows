use af_domain::{GroupId, UserId};
use sea_orm::{DatabaseTransaction, EntityTrait};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminUserCreateRecord, AdminUserRepositoryError, admin_user_write::create_user_in_transaction,
    entity::authentication_settings,
};

/// Create a passwordless account inside an extension's transaction.
///
/// The caller must authenticate the external identity and atomically persist its
/// unique binding. This function never resolves existing users by name or email.
/// Accounts receive the configured default group, ordinary role and zero balance;
/// public registration settings and signup incentives do not apply. The caller
/// owns the transaction deadline and must roll back on any subsequent failure.
pub async fn provision_external_account_in_transaction(
    transaction: &DatabaseTransaction,
    opaque_username: &str,
) -> Result<UserId, AdminUserRepositoryError> {
    if opaque_username.is_empty()
        || opaque_username.len() > 64
        || !opaque_username
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(AdminUserRepositoryError::InvalidReference);
    }
    let settings = authentication_settings::Entity::find_by_id(1_i16)
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AdminUserRepositoryError::Query)?
        .ok_or(AdminUserRepositoryError::InvalidReference)?;
    let group_id = settings
        .registration_default_group_id
        .and_then(|value| GroupId::new(value).ok())
        .ok_or(AdminUserRepositoryError::InvalidReference)?;
    let account = create_user_in_transaction(
        transaction,
        AdminUserCreateRecord::new(
            opaque_username.to_owned(),
            None,
            None,
            0,
            1,
            group_id,
            0,
            None,
            None,
        ),
    )
    .await?;
    Ok(account.user_id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DatabaseOptions, MigrationOptions, connect_and_migrate};
    use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
    use std::error::Error;

    #[tokio::test]
    async fn extension_controls_commit_and_rollback_without_signup_privileges()
    -> Result<(), Box<dyn Error>> {
        let pool = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        pool.connection().execute_unprepared(
            "INSERT INTO groups (id, name, display_name, flags) VALUES (1, 'external', 'External', '{}')",
        ).await?;
        pool.connection().execute_unprepared(
            "UPDATE authentication_settings SET registration_enabled = 0, registration_default_group_id = 1, registration_initial_quota = 1000 WHERE id = 1",
        ).await?;
        let transaction = pool.connection().begin().await?;
        provision_external_account_in_transaction(&transaction, "external_rollback").await?;
        transaction.rollback().await?;
        let transaction = pool.connection().begin().await?;
        let user_id =
            provision_external_account_in_transaction(&transaction, "external_commit").await?;
        transaction.commit().await?;
        let row = pool.connection().query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT role, status, quota, default_group_id, email, password_hash FROM users WHERE id = ? AND username = 'external_commit'",
            [user_id.get().into()],
        )).await?.ok_or("missing committed account")?;
        assert_eq!(row.try_get::<i16>("", "role")?, 0);
        assert_eq!(row.try_get::<i16>("", "status")?, 1);
        assert_eq!(row.try_get::<i64>("", "quota")?, 0);
        assert_eq!(row.try_get::<i64>("", "default_group_id")?, 1);
        assert_eq!(row.try_get::<Option<String>>("", "email")?, None);
        assert_eq!(row.try_get::<Option<String>>("", "password_hash")?, None);
        let absent = pool
            .connection()
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS total FROM users WHERE username = 'external_rollback'",
            ))
            .await?
            .ok_or("missing count")?;
        assert_eq!(absent.try_get::<i64>("", "total")?, 0);
        let ledger = pool
            .connection()
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS total FROM wallet_ledger_entries",
            ))
            .await?
            .ok_or("missing ledger count")?;
        assert_eq!(ledger.try_get::<i64>("", "total")?, 0);
        let transaction = pool.connection().begin().await?;
        assert_eq!(
            provision_external_account_in_transaction(&transaction, "external_commit").await,
            Err(AdminUserRepositoryError::Conflict)
        );
        transaction.rollback().await?;
        pool.connection()
            .execute_unprepared(
                "UPDATE authentication_settings SET registration_default_group_id = NULL",
            )
            .await?;
        let transaction = pool.connection().begin().await?;
        assert_eq!(
            provision_external_account_in_transaction(&transaction, "external_unconfigured").await,
            Err(AdminUserRepositoryError::InvalidReference)
        );
        assert_eq!(
            provision_external_account_in_transaction(&transaction, "bad username").await,
            Err(AdminUserRepositoryError::InvalidReference)
        );
        transaction.rollback().await?;
        pool.close().await?;
        Ok(())
    }
}
