use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use af_db::{
    DatabaseOptions, MigrationOptions, TopupCreditOutcome, TopupExtension, TopupExtensionFuture,
    TopupOrderCreate, TopupOrderCreateOutcome, TopupOrderRecord, TopupRepository,
    TopupRepositoryError, connect_and_migrate,
};
use af_domain::{OrganizationId, Quota, TopupOrderId, TopupRequestId, UserId, WalletEventId};
use sea_orm::{ConnectionTrait, DatabaseTransaction, entity::prelude::TimeDateTimeWithTimeZone};

struct Hooks {
    allowed: AtomicBool,
    fail_audit: AtomicBool,
    validations: AtomicUsize,
}

impl TopupExtension for Hooks {
    fn validate_target<'a>(
        &'a self,
        _: &'a DatabaseTransaction,
        _: OrganizationId,
    ) -> TopupExtensionFuture<'a, bool> {
        Box::pin(async { panic!("payer-aware validation must be used") })
    }
    fn validate_order<'a>(
        &'a self,
        _: &'a DatabaseTransaction,
        organization: OrganizationId,
        payer: UserId,
    ) -> TopupExtensionFuture<'a, bool> {
        Box::pin(async move {
            assert_eq!(organization.get(), 7);
            assert_eq!(payer.get(), 1);
            self.validations.fetch_add(1, Ordering::SeqCst);
            Ok(self.allowed.load(Ordering::SeqCst))
        })
    }
    fn order_created<'a>(
        &'a self,
        transaction: &'a DatabaseTransaction,
        _: &'a TopupOrderRecord,
    ) -> TopupExtensionFuture<'a, ()> {
        Box::pin(async move {
            transaction
                .execute_unprepared("INSERT INTO extension_audit (id) VALUES (1)")
                .await
                .map_err(|_| TopupRepositoryError::Query)?;
            if self.fail_audit.load(Ordering::SeqCst) {
                return Err(TopupRepositoryError::Query);
            }
            Ok(())
        })
    }
    fn credit<'a>(
        &'a self,
        _: &'a DatabaseTransaction,
        _: OrganizationId,
        _: WalletEventId,
        _: UserId,
        _: Quota,
        _: TimeDateTimeWithTimeZone,
    ) -> TopupExtensionFuture<'a, TopupCreditOutcome> {
        Box::pin(async { Ok(TopupCreditOutcome::Credited) })
    }
}

fn order() -> Result<TopupOrderCreate, Box<dyn Error>> {
    Ok(TopupOrderCreate::new_for_organization(
        TopupOrderId::new([1; 16])?,
        TopupRequestId::new([2; 16])?,
        UserId::new(1)?,
        OrganizationId::new(7)?,
        "stripe".to_owned(),
        "card".to_owned(),
        100,
        "USD".to_owned(),
        Quota::new(1000)?,
        1_900_000_000,
    )?)
}

#[tokio::test]
async fn extension_audit_rolls_back_with_order_and_retries_recheck_payer()
-> Result<(), Box<dyn Error>> {
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    for sql in [
        "INSERT INTO groups (id, name, display_name, flags) VALUES (1, 'topup', 'Topup', '{}')",
        "INSERT INTO users (id, username, default_group_id, aff_code, settings) VALUES (1, 'payer', 1, 'payer', '{}')",
        "CREATE TABLE extension_audit (id INTEGER PRIMARY KEY)",
    ] {
        pool.extension_connection().execute_unprepared(sql).await?;
    }
    let hooks = Arc::new(Hooks {
        allowed: AtomicBool::new(true),
        fail_audit: AtomicBool::new(true),
        validations: AtomicUsize::new(0),
    });
    let repository =
        TopupRepository::new(pool.clone(), Duration::from_secs(5))?.with_extension(hooks.clone());
    assert!(matches!(
        repository.create_order(order()?).await,
        Err(TopupRepositoryError::Query)
    ));
    assert!(
        repository
            .get_order(TopupOrderId::new([1; 16])?)
            .await?
            .is_none()
    );
    hooks.fail_audit.store(false, Ordering::SeqCst);
    assert!(matches!(
        repository.create_order(order()?).await?,
        TopupOrderCreateOutcome::Created(_)
    ));
    assert!(matches!(
        repository.create_order(order()?).await?,
        TopupOrderCreateOutcome::Existing(_)
    ));
    hooks.allowed.store(false, Ordering::SeqCst);
    assert!(matches!(
        repository.create_order(order()?).await?,
        TopupOrderCreateOutcome::NotFound
    ));
    assert_eq!(hooks.validations.load(Ordering::SeqCst), 4);
    pool.close().await?;
    Ok(())
}
