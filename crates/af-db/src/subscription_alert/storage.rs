use sea_orm::{DatabaseTransaction, TransactionTrait};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::DatabasePool;

use super::types::SubscriptionBalanceAlertRepositoryError;

pub(super) async fn begin(
    pool: &DatabasePool,
) -> Result<DatabaseTransaction, SubscriptionBalanceAlertRepositoryError> {
    pool.connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)
}

pub(super) async fn commit(
    transaction: DatabaseTransaction,
) -> Result<(), SubscriptionBalanceAlertRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)
}

pub(super) async fn rollback(
    transaction: DatabaseTransaction,
) -> Result<(), SubscriptionBalanceAlertRepositoryError> {
    transaction
        .rollback()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)
}
