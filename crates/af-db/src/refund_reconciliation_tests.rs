use std::{error::Error, time::Duration};

use af_domain::{
    GroupId, Quota, RefundApprovalStatus, RefundManualCompletion, RefundManualResult,
    RefundOrderKind, RefundRequestCreate, RefundRequestCreateOutcome, RefundRequestId,
    RefundRequestKey, RefundRequestStatus, TopupOrderId, TopupPaymentEventId,
    TopupPaymentEventType, TopupRequestId,
};
use sea_orm::{ActiveModelTrait, ConnectionTrait, DatabaseBackend, EntityTrait, Set, Statement};
use sea_orm_migration::MigratorTrait;

use crate::entity::{groups, refund_manual_completions};
use crate::migration::Migrator;
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

#[tokio::test]
async fn epay_manual_completion_is_idempotent_and_keeps_provider_facts_separate()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("epay-manual-refund-test".to_owned()),
        display_name: Set("易支付人工退款测试".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let actor = users
        .create(AdminUserCreateRecord::new(
            "epay-manual-refund-actor".to_owned(),
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

    let topup = TopupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let order_id = TopupOrderId::new([0x61; 16])?;
    let topup_request_id = TopupRequestId::new([0x62; 16])?;
    topup
        .create_order(TopupOrderCreate::new(
            order_id,
            topup_request_id,
            actor.user_id(),
            "epay".to_owned(),
            "alipay".to_owned(),
            900,
            "CNY".to_owned(),
            Quota::new(90)?,
            CREATED_AT,
        )?)
        .await?;
    topup
        .submit_order(TopupOrderSubmission::new(
            order_id,
            1,
            "epay-trade-manual".to_owned(),
            SUBMITTED_AT,
            EXPIRES_AT,
        )?)
        .await?;
    topup
        .accept_verified_event(TopupPaymentEventWrite::new(
            TopupPaymentEventId::new([0x63; 16])?,
            order_id,
            "epay".to_owned(),
            "epay-payment-manual".to_owned(),
            Some("epay-trade-manual".to_owned()),
            900,
            "CNY".to_owned(),
            "alipay".to_owned(),
            TopupPaymentEventType::Succeeded,
            [0x64; 32],
            [0x65; 32],
            RECEIVED_AT,
        )?)
        .await?;

    let refund = RefundRepository::new(pool.clone(), Duration::from_secs(5))?;
    let request_id = RefundRequestId::new([0x71; 16])?;
    let request_key = RefundRequestKey::new([0x72; 16])?;
    let RefundRequestCreateOutcome::Created(_) = refund
        .create_request(RefundRequestCreate::new(
            request_id,
            request_key,
            actor.user_id(),
            RefundOrderKind::Topup,
            order_id.persistence_key(),
            "epay".to_owned(),
            "epay-trade-manual".to_owned(),
            "CNY".to_owned(),
            900,
            400,
            CREATED_AT + 100,
        )?)
        .await?
    else {
        panic!("易支付退款请求必须创建事实")
    };
    let request = match refund
        .approve_request(
            request_id,
            actor.user_id(),
            Some("人工退款审批".to_owned()),
            CREATED_AT + 101,
        )
        .await?
    {
        crate::RefundApprovalOutcome::Applied(request) => request,
        crate::RefundApprovalOutcome::Existing(_) => panic!("首次审批必须写入事实"),
    };
    let completion_key = RefundRequestKey::new([0x73; 16])?;
    let completion = RefundManualCompletion::new(
        request_id,
        completion_key,
        request.version(),
        actor.user_id(),
        RefundManualResult::Completed,
        "manual-bank-ref-20260903".to_owned(),
        CREATED_AT + 102,
    )?;
    let applied = refund.complete_manual_refund(completion).await?;
    let request = match applied {
        crate::RefundSubmissionOutcome::Applied(request) => request,
        crate::RefundSubmissionOutcome::Existing(_) => panic!("首次人工完成必须推进版本"),
    };
    assert_eq!(request.status(), RefundRequestStatus::ManuallySucceeded);
    assert_eq!(request.provider_refund_id(), None);

    let (entries, _) = refund
        .list_admin_reconciliations(RefundReconciliationQuery::default())
        .await?
        .into_parts();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].provider_event_id(), None);
    assert!(entries[0].manual_completion_id().is_some());
    assert_eq!(entries[0].amount_delta_minor(), -400);
    let stored = refund_manual_completions::Entity::find_by_id(1)
        .one(pool.connection())
        .await?
        .expect("人工完成事实必须落库");
    assert_eq!(stored.reference_sha256.len(), 64);
    assert_ne!(stored.reference_sha256, "manual-bank-ref-20260903");

    let existing = refund
        .complete_manual_refund(RefundManualCompletion::new(
            request_id,
            completion_key,
            request.version() - 1,
            actor.user_id(),
            RefundManualResult::Completed,
            "manual-bank-ref-20260903".to_owned(),
            CREATED_AT + 102,
        )?)
        .await?;
    assert!(matches!(
        existing,
        crate::RefundSubmissionOutcome::Existing(_)
    ));

    let conflict = refund
        .complete_manual_refund(RefundManualCompletion::new(
            request_id,
            RefundRequestKey::new([0x74; 16])?,
            request.version() - 1,
            actor.user_id(),
            RefundManualResult::Completed,
            "manual-bank-ref-20260903".to_owned(),
            CREATED_AT + 102,
        )?)
        .await;
    assert!(matches!(
        conflict,
        Err(crate::RefundRepositoryError::Conflict)
    ));

    let failed_request_id = RefundRequestId::new([0x75; 16])?;
    let failed_request_key = RefundRequestKey::new([0x76; 16])?;
    let RefundRequestCreateOutcome::Created(_) = refund
        .create_request(RefundRequestCreate::new(
            failed_request_id,
            failed_request_key,
            actor.user_id(),
            RefundOrderKind::Subscription,
            "ab".repeat(16),
            "epay".to_owned(),
            "epay-subscription-manual".to_owned(),
            "CNY".to_owned(),
            500,
            100,
            CREATED_AT + 200,
        )?)
        .await?
    else {
        panic!("失败人工退款请求必须创建事实")
    };
    let failed_request = match refund
        .approve_request(failed_request_id, actor.user_id(), None, CREATED_AT + 201)
        .await?
    {
        crate::RefundApprovalOutcome::Applied(request) => request,
        crate::RefundApprovalOutcome::Existing(_) => panic!("失败请求首次审批必须写入事实"),
    };
    let failed = refund
        .complete_manual_refund(RefundManualCompletion::new(
            failed_request_id,
            RefundRequestKey::new([0x77; 16])?,
            failed_request.version(),
            actor.user_id(),
            RefundManualResult::Failed,
            "manual-failed-ref".to_owned(),
            CREATED_AT + 202,
        )?)
        .await?;
    let failed_request = match failed {
        crate::RefundSubmissionOutcome::Applied(request) => request,
        crate::RefundSubmissionOutcome::Existing(_) => panic!("首次失败登记必须推进版本"),
    };
    assert_eq!(failed_request.status(), RefundRequestStatus::ManuallyFailed);
    assert_eq!(
        refund
            .list_admin_reconciliations(RefundReconciliationQuery::default())
            .await?
            .entries()
            .len(),
        1
    );

    // 从迁移尾部精确推进到人工退款状态迁移，避免后续新增迁移削弱保护回归。
    let migrations = Migrator::migrations();
    let protected_migration_index = migrations
        .iter()
        .position(|migration| migration.name() == "m20260903_000127_extend_refund_status")
        .expect("人工退款状态迁移必须保留在迁移注册表");
    let rollback_steps = u32::try_from(migrations.len() - protected_migration_index)?;
    let rollback = Migrator::down(pool.connection(), Some(rollback_steps)).await;
    assert!(matches!(
        rollback,
        Err(sea_orm_migration::DbErr::Custom(message))
            if message.contains("无法回退人工退款状态迁移")
    ));

    pool.close().await?;
    Ok(())
}
