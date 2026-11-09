//! Session authorization for independent merchant sales and maintenance policy.
use crate::{MerchantFuture, SessionPrincipal, SessionRole};
use af_db::MerchantPoolRepository;
use af_domain::{
    MerchantListQuery, MerchantMaintenance, MerchantMaintenanceWrite, MerchantPage,
    MerchantPoolError, MerchantServicePrice, MerchantServicePriceWrite, UserId,
};

pub trait MerchantServicePolicy: Send + Sync {
    fn prices(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantServicePrice>>;
    fn schedule_price(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        write: MerchantServicePriceWrite,
    ) -> MerchantFuture<'_, MerchantServicePrice>;
    fn cancel_price(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        price: i64,
    ) -> MerchantFuture<'_, MerchantServicePrice>;
    fn maintenance(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantMaintenance>>;
    fn write_maintenance(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        notice: Option<i64>,
        write: MerchantMaintenanceWrite,
    ) -> MerchantFuture<'_, MerchantMaintenance>;
    fn cancel_maintenance(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        notice: i64,
    ) -> MerchantFuture<'_, MerchantMaintenance>;
}

pub struct DatabaseMerchantServicePolicy {
    repository: MerchantPoolRepository,
}

impl DatabaseMerchantServicePolicy {
    #[must_use]
    pub fn new(repository: MerchantPoolRepository) -> Self {
        Self { repository }
    }
}

impl MerchantServicePolicy for DatabaseMerchantServicePolicy {
    fn prices(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantServicePrice>> {
        Box::pin(async move {
            self.repository
                .service_prices(scope(principal, administrative)?, service, query)
                .await
        })
    }
    fn schedule_price(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        write: MerchantServicePriceWrite,
    ) -> MerchantFuture<'_, MerchantServicePrice> {
        Box::pin(async move {
            self.repository
                .schedule_service_price(
                    scope(principal, administrative)?,
                    principal.user_id(),
                    service,
                    write,
                )
                .await
        })
    }
    fn cancel_price(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        price: i64,
    ) -> MerchantFuture<'_, MerchantServicePrice> {
        Box::pin(async move {
            self.repository
                .cancel_service_price(
                    scope(principal, administrative)?,
                    principal.user_id(),
                    service,
                    price,
                )
                .await
        })
    }
    fn maintenance(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantMaintenance>> {
        Box::pin(async move {
            self.repository
                .service_maintenance(scope(principal, administrative)?, service, query)
                .await
        })
    }
    fn write_maintenance(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        notice: Option<i64>,
        write: MerchantMaintenanceWrite,
    ) -> MerchantFuture<'_, MerchantMaintenance> {
        Box::pin(async move {
            self.repository
                .write_service_maintenance(
                    scope(principal, administrative)?,
                    principal.user_id(),
                    service,
                    notice,
                    write,
                )
                .await
        })
    }
    fn cancel_maintenance(
        &self,
        principal: SessionPrincipal,
        administrative: bool,
        service: i64,
        notice: i64,
    ) -> MerchantFuture<'_, MerchantMaintenance> {
        Box::pin(async move {
            self.repository
                .cancel_service_maintenance(
                    scope(principal, administrative)?,
                    principal.user_id(),
                    service,
                    notice,
                )
                .await
        })
    }
}

fn scope(
    principal: SessionPrincipal,
    administrative: bool,
) -> Result<Option<UserId>, MerchantPoolError> {
    if administrative {
        if principal.role() != SessionRole::Admin {
            return Err(MerchantPoolError::Forbidden);
        }
        Ok(None)
    } else {
        Ok(Some(principal.user_id()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn administrative_scope_cannot_be_selected_by_a_regular_session() {
        let user = SessionPrincipal::new(UserId::new(1).expect("user"), SessionRole::User);
        assert!(matches!(
            scope(user, true),
            Err(MerchantPoolError::Forbidden)
        ));
        assert_eq!(
            scope(user, false).expect("owner scope"),
            Some(user.user_id())
        );
    }
}
