use super::*;
use crate::entity::{merchant_pool_settings as settings, merchant_pool_terms as terms};
use sea_orm::sea_query::OnConflict;

impl MerchantPoolRepository {
    pub async fn settings(&self) -> Result<MerchantPoolSettings, MerchantPoolError> {
        self.bounded(async {
            Ok(MerchantPoolSettings {
                commission_bps: global_commission(self.pool.connection()).await?,
            })
        })
        .await
    }
    pub async fn write_settings(
        &self,
        actor: UserId,
        write: MerchantPoolSettings,
    ) -> Result<MerchantPoolSettings, MerchantPoolError> {
        if write.commission_bps > 10_000 {
            return Err(MerchantPoolError::InvalidInput);
        }
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            settings::Entity::insert(settings::ActiveModel {
                id: Set(1),
                commission_bps: Set(i32::from(write.commission_bps)),
                updated_at: Set(DatabaseTimestamp::now_utc()),
            })
            .on_conflict(
                OnConflict::column(settings::Column::Id)
                    .update_columns([settings::Column::CommissionBps, settings::Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&txn)
            .await
            .map_err(internal)?;
            audit_control(
                &txn,
                actor,
                "commission.configure",
                1,
                None,
                Some(serde_json::json!({"commission_bps":write.commission_bps})),
            )
            .await?;
            txn.commit().await.map_err(internal)?;
            Ok(write)
        })
        .await
    }
    pub async fn terms(
        &self,
        pool_id: i64,
        merchant_id: i64,
    ) -> Result<MerchantPoolTerms, MerchantPoolError> {
        self.bounded(async {
            find_pool(self.pool.connection(), pool_id).await?;
            if profiles::Entity::find_by_id(merchant_id)
                .filter(profiles::Column::DeletedAt.is_null())
                .one(self.pool.connection())
                .await
                .map_err(internal)?
                .is_none()
            {
                return Err(MerchantPoolError::NotFound);
            }
            let row = terms::Entity::find_by_id((pool_id, merchant_id))
                .one(self.pool.connection())
                .await
                .map_err(internal)?;
            Ok(MerchantPoolTerms {
                commission_bps: row
                    .and_then(|row| row.commission_bps)
                    .map(commission_value)
                    .transpose()?,
            })
        })
        .await
    }
    pub async fn write_terms(
        &self,
        actor: UserId,
        pool_id: i64,
        merchant_id: i64,
        write: MerchantPoolTerms,
    ) -> Result<MerchantPoolTerms, MerchantPoolError> {
        if write.commission_bps.is_some_and(|bps| bps > 10_000) {
            return Err(MerchantPoolError::InvalidInput);
        }
        self.terms(pool_id, merchant_id).await?;
        self.bounded(async {
            let txn=self.pool.connection().begin().await.map_err(internal)?;
            super::deletion::lock_live_pool(&txn,pool_id).await?;
            if profiles::Entity::find_by_id(merchant_id).filter(profiles::Column::DeletedAt.is_null())
                .one(&txn).await.map_err(internal)?.is_none() { return Err(MerchantPoolError::NotFound); }
            terms::Entity::insert(terms::ActiveModel{pool_id:Set(pool_id),merchant_id:Set(merchant_id),
                commission_bps:Set(write.commission_bps.map(i32::from)),updated_at:Set(DatabaseTimestamp::now_utc())})
                .on_conflict(OnConflict::columns([terms::Column::PoolId,terms::Column::MerchantId])
                    .update_columns([terms::Column::CommissionBps,terms::Column::UpdatedAt]).to_owned())
                .exec(&txn).await.map_err(internal)?;
            audit_control(&txn,actor,"merchant.commission",pool_id,None,Some(serde_json::json!({"merchant_id":merchant_id,"commission_bps":write.commission_bps}))).await?;
            txn.commit().await.map_err(internal)?;
            Ok(write)
        }).await
    }
    pub(super) async fn effective_pool(
        &self,
        row: pools::Model,
        merchant_id: Option<i64>,
    ) -> Result<MerchantPool, MerchantPoolError> {
        let effective = resolve_commission(self.pool.connection(), &row, merchant_id).await?;
        let mut pool = pool_snapshot(row)?;
        pool.effective_commission_bps = effective;
        Ok(pool)
    }
}

async fn global_commission<C: ConnectionTrait>(connection: &C) -> Result<u16, MerchantPoolError> {
    settings::Entity::find_by_id(1)
        .one(connection)
        .await
        .map_err(internal)?
        .map_or(Ok(500), |row| commission_value(row.commission_bps))
}
pub(super) async fn resolve_commission<C: ConnectionTrait>(
    connection: &C,
    pool: &pools::Model,
    merchant_id: Option<i64>,
) -> Result<u16, MerchantPoolError> {
    if let Some(merchant_id) = merchant_id
        && let Some(bps) = terms::Entity::find_by_id((pool.id, merchant_id))
            .one(connection)
            .await
            .map_err(internal)?
            .and_then(|row| row.commission_bps)
    {
        return commission_value(bps);
    }
    if pool.commission_inherit_global {
        global_commission(connection).await
    } else {
        commission_value(pool.commission_bps)
    }
}
