use std::{error::Error, time::Duration};

use af_domain::{
    GroupId, Quota, RefundApprovalStatus, RefundOrderKind, RefundRequestCreate,
    RefundRequestCreateOutcome, RefundRequestId, RefundRequestKey, RefundRequestStatus,
    TopupOrderId, TopupPaymentEventId, TopupPaymentEventType, TopupRequestId,
};
use sea_orm::{ActiveModelTrait, ConnectionTrait, DatabaseBackend, Set, Statement};

use crate::entity::groups;
use crate::{
    AdminUserCreateRecord, AdminUserRepository, DatabaseOptions, MigrationOptions,
    RefundReceiptWrite, RefundReconciliationQuery, RefundRepository, TopupOrderCreate,
    TopupOrderCreateOutcome, TopupOrderSubmission, TopupOrderSubmitOutcome, TopupPaymentEventWrite,
    TopupRepository,
};

const CREATED_AT: u64 = 1_900_000_000;
const SUBMITTED_AT: u64 = CREATED_AT + 10;
const EXPIRES_AT: u64 = CREATED_AT + 3_600;
const RECEIVED_AT: u64 = CREATED_AT + 60;

#[tokio::test]
async fn successful_refund_creates_scoped_append_only_reconciliation() -> Result<(), Box<dyn Error>>
{
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("refund-reconciliation-test".to_owned()),
        display_name: Set("退款对账测试".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let payer = users
        .create(AdminUserCreateRecord::new(
            "refund-payer".to_owned(),
            None,
            None,
            0,
            1,
            GroupId::new(group.id)?,
            0,
            None,
            None,
        ))
        .await?;
    let other = users
        .create(AdminUserCreateRecord::new(
            "refund-other".to_owned(),
            None,
            None,
            0,
            1,
            GroupId::new(group.id)?,
            0,
            None,
            None,
        ))
        .await?;
    let repository = TopupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let order_id = TopupOrderId::new([0x11; 16])?;
    let request_id = TopupRequestId::new([0x12; 16])?;
    let created = repository
        .create_order(TopupOrderCreate::new(
            order_id,
            request_id,
            payer.user_id(),
            "stripe".to_owned(),
            "card".to_owned(),
            1_000,
            "USD".to_owned(),
            Quota::new(100)?,
            CREATED_AT,
        )?)
        .await?;
    assert!(matches!(created, TopupOrderCreateOutcome::Created(_)));
    assert!(matches!(
        repository
            .submit_order(TopupOrderSubmission::new(
                order_id,
                1,
                "provider-order-refund".to_owned(),
                SUBMITTED_AT,
                EXPIRES_AT,
            )?)
            .await?,
        TopupOrderSubmitOutcome::Applied(_)
    ));
    assert!(matches!(
        repository
            .accept_verified_event(TopupPaymentEventWrite::new(
                TopupPaymentEventId::new([0x13; 16])?,
                order_id,
                "stripe".to_owned(),
                "evt-refund-order-paid".to_owned(),
                Some("trade-refund-order".to_owned()),
                1_000,
                "USD".to_owned(),
                "card".to_owned(),
                TopupPaymentEventType::Succeeded,
                [0xa1; 32],
                [0xa2; 32],
                RECEIVED_AT,
            )?)
            .await?,
        crate::TopupPaymentEventOutcome::Applied(_)
    ));

    let refund = RefundRepository::new(pool.clone(), Duration::from_secs(5))?;
    let refund_id = RefundRequestId::new([0x21; 16])?;
    let refund_key = RefundRequestKey::new([0x22; 16])?;
    let RefundRequestCreateOutcome::Created(request) = refund
        .create_request(RefundRequestCreate::new(
            refund_id,
            refund_key,
            payer.user_id(),
            RefundOrderKind::Topup,
            order_id.persistence_key(),
            "stripe".to_owned(),
            "trade-refund-order".to_owned(),
            "USD".to_owned(),
            1_000,
            400,
            CREATED_AT + 100,
        )?)
        .await?
    else {
        panic!("首次退款请求必须创建事实")
    };
    assert_eq!(request.approval_status(), RefundApprovalStatus::Pending);
    let approved = refund
        .approve_request(
            refund_id,
            other.user_id(),
            Some("订单核验通过".to_owned()),
            CREATED_AT + 101,
        )
        .await?;
    let request = match approved {
        crate::RefundApprovalOutcome::Applied(request) => request,
        crate::RefundApprovalOutcome::Existing(_) => panic!("首次审批必须写入事实"),
    };
    assert_eq!(request.approval_status(), RefundApprovalStatus::Approved);
    let submitted = refund
        .claim_submission(refund_id, request.version(), CREATED_AT + 102)
        .await?;
    let request = match submitted {
        crate::RefundSubmissionOutcome::Applied(request) => request,
        crate::RefundSubmissionOutcome::Existing(_) => panic!("首次提交占位必须推进版本"),
    };
    let bound = refund
        .bind_provider_refund_id(
            refund_id,
            request.version(),
            "re_refund_123".to_owned(),
            CREATED_AT + 103,
        )
        .await?;
    let request = match bound {
        crate::RefundSubmissionOutcome::Applied(request) => request,
        crate::RefundSubmissionOutcome::Existing(_) => {
            panic!("首次绑定 Provider 退款标识必须推进版本")
        }
    };
    assert_eq!(request.status(), RefundRequestStatus::Submitted);

    let receipt = RefundReceiptWrite {
        event_key: "31".repeat(16),
        request_id: refund_id,
        provider: "stripe".to_owned(),
        provider_event_id: "evt-refund-succeeded".to_owned(),
        provider_refund_id: "re_refund_123".to_owned(),
        status: RefundRequestStatus::Succeeded,
        amount_minor: 400,
        currency: "USD".to_owned(),
        signature_key_fingerprint: "41".repeat(32),
        payload_sha256: "51".repeat(32),
        received_at: CREATED_AT + 104,
        processed_at: CREATED_AT + 104,
        created_at: CREATED_AT + 104,
    };
    let applied = refund.apply_receipt(receipt.clone()).await?;
    let crate::RefundReceiptOutcome::Applied(request) = applied else {
        panic!("首次成功回执必须推进退款状态")
    };
    assert_eq!(request.status(), RefundRequestStatus::Succeeded);

    let (entries, next_cursor) = refund
        .list_user_reconciliations(payer.user_id(), RefundReconciliationQuery::new(None, 10))
        .await?
        .into_parts();
    assert_eq!(entries.len(), 1);
    assert_eq!(next_cursor, None);
    assert_eq!(entries[0].amount_delta_minor(), -400);
    assert_eq!(entries[0].organization_id(), None);
    assert_eq!(entries[0].approval_actor_id(), other.user_id());
    assert!(
        refund
            .list_user_reconciliations(other.user_id(), RefundReconciliationQuery::default())
            .await?
            .entries()
            .is_empty()
    );
    assert!(
        refund
            .list_organization_reconciliations(
                af_domain::OrganizationId::new(999)?,
                RefundReconciliationQuery::default(),
            )
            .await?
            .entries()
            .is_empty()
    );
    assert_eq!(
        refund
            .list_admin_reconciliations(RefundReconciliationQuery::default())
            .await?
            .entries()
            .len(),
        1
    );

    let existing = refund.apply_receipt(receipt).await?;
    assert!(matches!(existing, crate::RefundReceiptOutcome::Existing(_)));
    assert_eq!(
        refund
            .list_admin_reconciliations(RefundReconciliationQuery::default())
            .await?
            .entries()
            .len(),
        1
    );

    let update = pool
        .connection()
        .execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            "UPDATE refund_reconciliation_entries SET currency = 'CNY' WHERE id = 1",
        ))
        .await;
    assert!(update.is_err());
    let delete = pool
        .connection()
        .execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            "DELETE FROM refund_reconciliation_entries WHERE id = 1",
        ))
        .await;
    assert!(delete.is_err());

    pool.close().await?;
    Ok(())
}
