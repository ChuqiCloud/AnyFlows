use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{MerchantPoolRepository, MerchantServiceWatch, MerchantServiceWatchPage};
use af_domain::MerchantPoolError;
use thiserror::Error;

use crate::SessionPrincipal;

pub type MerchantServiceWatchFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, MerchantServiceWatchError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MerchantServiceWatchError {
    #[error("merchant service watch request is invalid")]
    InvalidInput,
    #[error("merchant service is unavailable")]
    NotFound,
    #[error("merchant service watch limit reached")]
    LimitReached,
    #[error("merchant service watch service failed")]
    Internal,
}

pub trait MerchantServiceWatchService: Send + Sync {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
    ) -> MerchantServiceWatchFuture<'a, MerchantServiceWatchPage>;

    fn watch<'a>(
        &'a self,
        principal: SessionPrincipal,
        pool_id: i64,
        service_id: i64,
    ) -> MerchantServiceWatchFuture<'a, MerchantServiceWatch>;

    fn unwatch<'a>(
        &'a self,
        principal: SessionPrincipal,
        pool_id: i64,
        service_id: i64,
    ) -> MerchantServiceWatchFuture<'a, bool>;
}

pub struct DatabaseMerchantServiceWatchService {
    repository: Arc<MerchantPoolRepository>,
}

impl DatabaseMerchantServiceWatchService {
    #[must_use]
    pub fn new(repository: Arc<MerchantPoolRepository>) -> Self {
        Self { repository }
    }
}

impl MerchantServiceWatchService for DatabaseMerchantServiceWatchService {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
    ) -> MerchantServiceWatchFuture<'a, MerchantServiceWatchPage> {
        Box::pin(async move {
            self.repository
                .watched_services(principal.user_id())
                .await
                .map_err(map_error)
        })
    }

    fn watch<'a>(
        &'a self,
        principal: SessionPrincipal,
        pool_id: i64,
        service_id: i64,
    ) -> MerchantServiceWatchFuture<'a, MerchantServiceWatch> {
        Box::pin(async move {
            self.repository
                .watch_service(principal.user_id(), pool_id, service_id)
                .await
                .map_err(map_error)
        })
    }

    fn unwatch<'a>(
        &'a self,
        principal: SessionPrincipal,
        pool_id: i64,
        service_id: i64,
    ) -> MerchantServiceWatchFuture<'a, bool> {
        Box::pin(async move {
            self.repository
                .unwatch_service(principal.user_id(), pool_id, service_id)
                .await
                .map_err(map_error)
        })
    }
}

impl fmt::Debug for DatabaseMerchantServiceWatchService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseMerchantServiceWatchService(<redacted>)")
    }
}

fn map_error(error: MerchantPoolError) -> MerchantServiceWatchError {
    match error {
        MerchantPoolError::InvalidInput => MerchantServiceWatchError::InvalidInput,
        MerchantPoolError::NotFound | MerchantPoolError::Forbidden => {
            MerchantServiceWatchError::NotFound
        }
        MerchantPoolError::LimitReached => MerchantServiceWatchError::LimitReached,
        MerchantPoolError::Conflict | MerchantPoolError::InUse | MerchantPoolError::Unavailable => {
            MerchantServiceWatchError::Internal
        }
    }
}
