use super::*;
use crate::{QuotaFundingContext, QuotaFundingExtension, QuotaFundingFuture};

struct Funding;

impl QuotaFundingExtension for Funding {
    fn lock<'a>(
        &'a self,
        _: &'a sea_orm::DatabaseTransaction,
        _: &'a QuotaFundingContext,
    ) -> QuotaFundingFuture<'a> {
        Box::pin(async { Ok(()) })
    }
    fn reserve<'a>(
        &'a self,
        tx: &'a sea_orm::DatabaseTransaction,
        ctx: &'a QuotaFundingContext,
        _: GatewayPrincipal,
    ) -> QuotaFundingFuture<'a> {
        Box::pin(async move {
            tx.execute_unprepared(&format!(
                "UPDATE extension_funds SET balance = balance - {0}, frozen = frozen + {0}",
                ctx.reserved.units()
            ))
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
            Ok(())
        })
    }
    fn replay<'a>(
        &'a self,
        _: &'a sea_orm::DatabaseTransaction,
        _: &'a QuotaFundingContext,
        _: GatewayPrincipal,
    ) -> QuotaFundingFuture<'a> {
        Box::pin(async { Ok(()) })
    }
    fn settle<'a>(
        &'a self,
        tx: &'a sea_orm::DatabaseTransaction,
        ctx: &'a QuotaFundingContext,
        actual: Quota,
    ) -> QuotaFundingFuture<'a> {
        Box::pin(async move {
            let result = tx.execute_unprepared(&format!("UPDATE extension_funds SET balance = balance + {0} - {1}, frozen = frozen - {0}, used = used + {1} WHERE balance + {0} >= {1}", ctx.reserved.units(), actual.units()))
                .await.map_err(|_| QuotaRepositoryError::Query)?;
            if result.rows_affected() == 1 {
                Ok(())
            } else {
                Err(QuotaRepositoryError::OrganizationQuotaInsufficient)
            }
        })
    }
    fn pending<'a>(
        &'a self,
        _: &'a sea_orm::DatabaseTransaction,
        _: &'a QuotaFundingContext,
        _: Quota,
    ) -> QuotaFundingFuture<'a> {
        Box::pin(async { Ok(()) })
    }
    fn refund<'a>(
        &'a self,
        tx: &'a sea_orm::DatabaseTransaction,
        ctx: &'a QuotaFundingContext,
    ) -> QuotaFundingFuture<'a> {
        Box::pin(async move {
            tx.execute_unprepared(&format!(
                "UPDATE extension_funds SET balance = balance + {0}, frozen = frozen - {0}",
                ctx.reserved.units()
            ))
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
            Ok(())
        })
    }
}

async fn organization_fixture() -> Result<Fixture, Box<dyn Error>> {
    let mut fixture = fixture(FixtureConfig::default()).await?;
    fixture
        .pool
        .extension_connection()
        .execute_unprepared(
            "CREATE TABLE extension_funds (balance BIGINT, frozen BIGINT, used BIGINT)",
        )
        .await?;
    fixture
        .pool
        .extension_connection()
        .execute_unprepared("INSERT INTO extension_funds VALUES (100, 0, 0)")
        .await?;
    tokens::Entity::update_many()
        .filter(tokens::Column::Id.eq(fixture.principal.token_id().get()))
        .col_expr(tokens::Column::OrganizationId, Expr::value(1_i64))
        .col_expr(tokens::Column::OrganizationMembershipId, Expr::value(1_i64))
        .exec(fixture.pool.extension_connection())
        .await?;
    fixture.principal = GatewayPrincipal::organization(
        fixture.principal.token_id(),
        fixture.principal.user_id(),
        fixture.principal.group_id(),
        af_domain::OrganizationGatewayPrincipal::new(
            af_domain::OrganizationId::new(1)?,
            af_domain::OrganizationMembershipId::new(1)?,
            None,
        ),
    );
    fixture.repository = fixture.repository.with_funding_extension(Arc::new(Funding));
    Ok(fixture)
}

async fn funds(fixture: &Fixture) -> Result<(i64, i64, i64), Box<dyn Error>> {
    let row = fixture
        .pool
        .extension_connection()
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DbBackend::Sqlite,
            "SELECT balance, frozen, used FROM extension_funds",
        ))
        .await?
        .ok_or("missing extension funds")?;
    Ok((
        row.try_get("", "balance")?,
        row.try_get("", "frozen")?,
        row.try_get("", "used")?,
    ))
}

#[tokio::test]
async fn funding_uses_shared_idempotency_and_rolls_back_on_token_shortage()
-> Result<(), Box<dyn Error>> {
    let fixture = organization_fixture().await?;
    let id = reservation_id(0xc1);
    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(61))
            .await,
        Err(QuotaRepositoryError::TokenQuotaInsufficient)
    );
    assert_eq!(funds(&fixture).await?, (100, 0, 0));
    assert_eq!(reservation_snapshot(&fixture.pool, id).await?, None);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(30))
        .await?;
    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(30))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved)
    );
    assert_eq!(funds(&fixture).await?, (70, 30, 0));
    fixture.repository.settle(id, quota(20)).await?;
    assert_eq!(
        fixture.repository.settle(id, quota(20)).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(funds(&fixture).await?, (80, 0, 20));
    let account = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(account.user_quota, FixtureConfig::default().user_quota);
    assert_eq!(
        account.user_frozen_quota,
        FixtureConfig::default().user_frozen_quota
    );
    assert_eq!(account.token_remain_quota, 40);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn funding_supplement_failure_preserves_actual_and_refund_is_terminal()
-> Result<(), Box<dyn Error>> {
    let fixture = organization_fixture().await?;
    let id = reservation_id(0xc2);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(30))
        .await?;
    fixture
        .pool
        .extension_connection()
        .execute_unprepared("UPDATE extension_funds SET balance = 0")
        .await?;
    assert_eq!(
        fixture.repository.settle(id, quota(40)).await,
        Err(QuotaRepositoryError::OrganizationQuotaInsufficient)
    );
    assert_eq!(funds(&fixture).await?, (0, 30, 0));
    assert_eq!(
        fixture.repository.refund(id).await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        fixture.repository.settle(id, quota(39)).await,
        Err(QuotaRepositoryError::Conflict)
    );
    fixture
        .pool
        .extension_connection()
        .execute_unprepared("UPDATE extension_funds SET balance = 20")
        .await?;
    fixture.repository.settle(id, quota(40)).await?;
    assert_eq!(funds(&fixture).await?, (10, 0, 40));
    let refund = reservation_id(0xc3);
    fixture
        .repository
        .precharge(refund, fixture.principal, quota(5))
        .await?;
    fixture.repository.refund(refund).await?;
    assert_eq!(
        fixture.repository.refund(refund).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded)
    );
    assert_eq!(funds(&fixture).await?, (10, 0, 40));
    fixture.close().await?;
    Ok(())
}
