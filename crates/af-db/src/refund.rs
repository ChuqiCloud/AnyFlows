use std::time::Duration;

use af_domain::{
    RefundOrderKind, RefundRequestCreate, RefundRequestCreateOutcome, RefundRequestId,
    RefundRequestKey, RefundRequestRecord, RefundRequestStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{DatabasePool, entity::refund_requests};

/// 退款请求事实仓储。
///
/// 首切片只建立和读取事实，不调用 Provider、不修改钱包账本；后续 Provider 切片
/// 以 `status + version` 为 CAS 边界继续推进状态。
#[derive(Clone)]
pub struct RefundRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl RefundRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, RefundRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(RefundRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 幂等创建退款请求；同一用户的幂等键和订单各自只能绑定同一事实。
    pub async fn create_request(
        &self,
        write: RefundRequestCreate,
    ) -> Result<RefundRequestCreateOutcome, RefundRepositoryError> {
        let operation = create_request_inner(self.pool.connection(), &write);
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::OutcomeUnknown),
        }
    }

    /// 按稳定退款请求标识读取事实。
    pub async fn get_request(
        &self,
        request_id: RefundRequestId,
    ) -> Result<Option<RefundRequestRecord>, RefundRepositoryError> {
        let operation = load_by_request_key(self.pool.connection(), request_id);
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::Timeout),
        }
    }

    /// 按当前用户和幂等键读取事实，供结果未知恢复使用。
    pub async fn get_by_idempotency_key(
        &self,
        user_id: UserId,
        key: RefundRequestKey,
    ) -> Result<Option<RefundRequestRecord>, RefundRepositoryError> {
        let operation = load_by_idempotency_key(self.pool.connection(), user_id, key);
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::Timeout),
        }
    }
}

async fn create_request_inner(
    connection: &DatabaseConnection,
    write: &RefundRequestCreate,
) -> Result<RefundRequestCreateOutcome, RefundRepositoryError> {
    if let Some(existing) =
        find_by_idempotency_key(connection, write.user_id(), write.idempotency_key())
            .await
            .map_err(map_query)?
    {
        return existing_outcome(existing, write);
    }
    if let Some(existing) = find_by_order(connection, write.order_kind(), write.order_key())
        .await
        .map_err(map_query)?
    {
        return existing_outcome(existing, write);
    }

    let created_at = to_database_time(write.created_at())?;
    let model = refund_requests::ActiveModel {
        id: sea_orm::NotSet,
        request_key: Set(write.request_id().persistence_key()),
        idempotency_key: Set(write.idempotency_key().persistence_key()),
        user_id: Set(write.user_id().get()),
        order_kind: Set(write.order_kind().code()),
        order_key: Set(write.order_key().to_owned()),
        provider: Set(write.provider().to_owned()),
        currency: Set(write.currency().to_owned()),
        original_amount_minor: Set(write.original_amount_minor()),
        refund_amount_minor: Set(write.refund_amount_minor()),
        status: Set(RefundRequestStatus::Requested.code()),
        version: Set(1),
        created_at: Set(created_at),
        updated_at: Set(created_at),
    };

    match model.insert(connection).await {
        Ok(model) => Ok(RefundRequestCreateOutcome::Created(to_record(model)?)),
        Err(_) => {
            // 并发请求可能在首次检查后先完成；重新按两个唯一事实读取，避免把可恢复重试
            // 错误地报告为内部错误，同时不吞掉不同事实的冲突。
            if let Some(existing) =
                find_by_idempotency_key(connection, write.user_id(), write.idempotency_key())
                    .await
                    .map_err(map_query)?
            {
                return existing_outcome(existing, write);
            }
            if let Some(existing) = find_by_order(connection, write.order_kind(), write.order_key())
                .await
                .map_err(map_query)?
            {
                return existing_outcome(existing, write);
            }
            Err(RefundRepositoryError::Query)
        }
    }
}

fn existing_outcome(
    model: refund_requests::Model,
    write: &RefundRequestCreate,
) -> Result<RefundRequestCreateOutcome, RefundRepositoryError> {
    let record = to_record(model)?;
    if record.matches_create(write) {
        Ok(RefundRequestCreateOutcome::Existing(record))
    } else {
        Err(RefundRepositoryError::Conflict)
    }
}

async fn load_by_request_key(
    connection: &DatabaseConnection,
    request_id: RefundRequestId,
) -> Result<Option<RefundRequestRecord>, RefundRepositoryError> {
    let model = refund_requests::Entity::find()
        .filter(refund_requests::Column::RequestKey.eq(request_id.persistence_key()))
        .one(connection)
        .await
        .map_err(map_query)?;
    model.map(to_record).transpose()
}

async fn load_by_idempotency_key(
    connection: &DatabaseConnection,
    user_id: UserId,
    key: RefundRequestKey,
) -> Result<Option<RefundRequestRecord>, RefundRepositoryError> {
    let model = find_by_idempotency_key(connection, user_id, key)
        .await
        .map_err(map_query)?;
    model.map(to_record).transpose()
}

async fn find_by_idempotency_key(
    connection: &DatabaseConnection,
    user_id: UserId,
    key: RefundRequestKey,
) -> Result<Option<refund_requests::Model>, sea_orm::DbErr> {
    refund_requests::Entity::find()
        .filter(
            refund_requests::Column::UserId
                .eq(user_id.get())
                .and(refund_requests::Column::IdempotencyKey.eq(key.persistence_key())),
        )
        .one(connection)
        .await
}

async fn find_by_order(
    connection: &DatabaseConnection,
    order_kind: RefundOrderKind,
    order_key: &str,
) -> Result<Option<refund_requests::Model>, sea_orm::DbErr> {
    refund_requests::Entity::find()
        .filter(
            refund_requests::Column::OrderKind
                .eq(order_kind.code())
                .and(refund_requests::Column::OrderKey.eq(order_key)),
        )
        .one(connection)
        .await
}

fn to_record(model: refund_requests::Model) -> Result<RefundRequestRecord, RefundRepositoryError> {
    let request_id = RefundRequestId::from_persistence_key(&model.request_key)
        .map_err(|_| RefundRepositoryError::Invariant)?;
    let idempotency_key = RefundRequestKey::from_persistence_key(&model.idempotency_key)
        .map_err(|_| RefundRepositoryError::Invariant)?;
    let order_kind = RefundOrderKind::try_from(model.order_kind)
        .map_err(|_| RefundRepositoryError::Invariant)?;
    let status = RefundRequestStatus::try_from(model.status)
        .map_err(|_| RefundRepositoryError::Invariant)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    RefundRequestRecord::from_persistence(
        model.id,
        request_id,
        idempotency_key,
        UserId::new(model.user_id).map_err(|_| RefundRepositoryError::Invariant)?,
        order_kind,
        model.order_key,
        model.provider,
        model.currency,
        model.original_amount_minor,
        model.refund_amount_minor,
        status,
        model.version,
        created_at,
        updated_at,
    )
    .map_err(|_| RefundRepositoryError::Invariant)
}

fn to_database_time(value: u64) -> Result<TimeDateTimeWithTimeZone, RefundRepositoryError> {
    let value = i64::try_from(value).map_err(|_| RefundRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| RefundRepositoryError::Invariant)
}

fn unix_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, RefundRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| RefundRepositoryError::Invariant)
}

fn map_query(error: sea_orm::DbErr) -> RefundRepositoryError {
    let _ = error;
    RefundRepositoryError::Query
}

/// 仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundRepositoryConfigError {
    /// 零超时无法形成有效数据库操作截止时间。
    #[error("退款仓储操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 退款请求仓储错误；不携带订单、Provider 或支付材料。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundRepositoryError {
    /// 相同幂等键或订单已经绑定不同事实。
    #[error("退款请求事实冲突")]
    Conflict,
    /// 数据库查询或写入失败。
    #[error("退款数据库操作失败")]
    Query,
    /// 写入结果未知，只能复用原退款请求恢复。
    #[error("退款操作结果未知")]
    OutcomeUnknown,
    /// 只读操作超过硬截止时间。
    #[error("退款数据库操作超时")]
    Timeout,
    /// 持久化快照违反退款不变量。
    #[error("退款持久化状态损坏")]
    Invariant,
}
