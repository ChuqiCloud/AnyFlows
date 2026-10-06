use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    RefundReceiptOutcome, RefundReceiptWrite, RefundRepository, RefundRepositoryError,
    RefundSubmissionOutcome,
};
use af_domain::{
    MAX_REFUND_PROVIDER_REFUND_ID_BYTES, RefundRequestId, RefundRequestRecord, RefundRequestStatus,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

const MAX_REFUND_PAYMENT_REFERENCE_BYTES: usize = 128;

/// Provider 原路退款调用的最小本地事实；不会携带商户密钥或原始支付 payload。
pub struct RefundProviderRequest {
    request_id: RefundRequestId,
    payment_reference: String,
    amount_minor: i64,
    currency: String,
}

impl RefundProviderRequest {
    /// 从已持久化退款事实和原支付标识构造 Provider 请求。
    pub fn new(
        record: &RefundRequestRecord,
        payment_reference: String,
    ) -> Result<Self, RefundProviderInputError> {
        if payment_reference.is_empty()
            || payment_reference.len() > MAX_REFUND_PAYMENT_REFERENCE_BYTES
            || payment_reference.trim() != payment_reference
            || payment_reference.chars().any(char::is_control)
        {
            return Err(RefundProviderInputError::InvalidPaymentReference);
        }
        Ok(Self {
            request_id: record.request_id(),
            payment_reference,
            amount_minor: record.refund_amount_minor(),
            currency: record.currency().to_owned(),
        })
    }

    #[must_use]
    pub const fn request_id(&self) -> RefundRequestId {
        self.request_id
    }
    #[must_use]
    pub fn payment_reference(&self) -> &str {
        &self.payment_reference
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i64 {
        self.amount_minor
    }
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
}

impl fmt::Debug for RefundProviderRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RefundProviderRequest(<已脱敏>)")
    }
}

/// 已绑定 Provider 退款标识的恢复请求。
pub struct RefundProviderRecoveryRequest {
    request: RefundProviderRequest,
    provider_refund_id: String,
}

impl RefundProviderRecoveryRequest {
    pub fn new(
        request: RefundProviderRequest,
        provider_refund_id: String,
    ) -> Result<Self, RefundProviderInputError> {
        if !valid_provider_refund_id(&provider_refund_id) {
            return Err(RefundProviderInputError::InvalidProviderRefundId);
        }
        Ok(Self {
            request,
            provider_refund_id,
        })
    }

    #[must_use]
    pub const fn request(&self) -> &RefundProviderRequest {
        &self.request
    }
    #[must_use]
    pub fn provider_refund_id(&self) -> &str {
        &self.provider_refund_id
    }
}

impl fmt::Debug for RefundProviderRecoveryRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RefundProviderRecoveryRequest(<已脱敏>)")
    }
}

/// Provider 已接受的退款标识。
pub struct RefundProviderResult {
    provider_refund_id: String,
}

impl RefundProviderResult {
    pub fn new(provider_refund_id: String) -> Result<Self, RefundProviderInputError> {
        if !valid_provider_refund_id(&provider_refund_id) {
            return Err(RefundProviderInputError::InvalidProviderRefundId);
        }
        Ok(Self { provider_refund_id })
    }

    #[must_use]
    pub fn provider_refund_id(&self) -> &str {
        &self.provider_refund_id
    }
}

impl fmt::Debug for RefundProviderResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RefundProviderResult(<已脱敏>)")
    }
}

/// 原路退款 Provider 提交与恢复端口。
pub trait RefundProvider: Send + Sync + 'static {
    fn provider(&self) -> &str;
    fn submit<'a>(&'a self, request: RefundProviderRequest) -> RefundProviderFuture<'a>;
    fn recover<'a>(&'a self, request: RefundProviderRecoveryRequest) -> RefundProviderFuture<'a>;
}

pub type RefundProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RefundProviderResult, RefundProviderError>> + Send + 'a>>;

/// Provider 退款调用的闭合错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundProviderError {
    #[error("退款 Provider 拒绝请求")]
    Rejected,
    #[error("退款 Provider 结果未知")]
    OutcomeUnknown,
    #[error("退款 Provider 暂不可用")]
    Unavailable,
    #[error("退款 Provider 响应无效")]
    InvalidResponse,
}

/// 已验签退款回执的规范字段。
pub struct VerifiedRefundReceipt {
    request_id: RefundRequestId,
    provider_event_id: String,
    provider_refund_id: String,
    status: RefundRequestStatus,
    amount_minor: i64,
    currency: String,
    signature_key_fingerprint: [u8; 32],
}

impl VerifiedRefundReceipt {
    #[allow(
        clippy::too_many_arguments,
        reason = "回执字段逐项对应 Provider 验签边界"
    )]
    pub fn new(
        request_id: RefundRequestId,
        provider_event_id: String,
        provider_refund_id: String,
        status: RefundRequestStatus,
        amount_minor: i64,
        currency: String,
        signature_key_fingerprint: [u8; 32],
    ) -> Result<Self, RefundReceiptVerificationError> {
        if provider_event_id.is_empty()
            || provider_event_id.len() > 128
            || provider_event_id.trim() != provider_event_id
            || provider_event_id.chars().any(char::is_control)
            || !valid_provider_refund_id(&provider_refund_id)
            || !matches!(
                status,
                RefundRequestStatus::Succeeded | RefundRequestStatus::Failed
            )
            || amount_minor <= 0
            || currency.len() != 3
            || !currency.bytes().all(|byte| byte.is_ascii_uppercase())
        {
            return Err(RefundReceiptVerificationError::InvalidReceipt);
        }
        Ok(Self {
            request_id,
            provider_event_id,
            provider_refund_id,
            status,
            amount_minor,
            currency,
            signature_key_fingerprint,
        })
    }
}

impl fmt::Debug for VerifiedRefundReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VerifiedRefundReceipt(<已脱敏>)")
    }
}

/// Provider 异步回执验签端口。
pub trait RefundReceiptVerifier: Send + Sync + 'static {
    fn provider(&self) -> &str;
    fn verify(
        &self,
        request: &PaymentWebhookRequest<'_>,
    ) -> Result<VerifiedRefundReceipt, RefundReceiptVerificationError>;
}

/// 复用支付 webhook 原始请求边界的退款回执输入。
pub use crate::payment_webhook::PaymentWebhookRequest;

/// 退款回执处理器：验签后只写摘要，并以退款请求版本 CAS 推进状态。
#[derive(Clone)]
pub struct RefundReceiptProcessor {
    verifier: Arc<dyn RefundReceiptVerifier>,
    repository: RefundRepository,
}

impl RefundReceiptProcessor {
    #[must_use]
    pub fn new(verifier: Arc<dyn RefundReceiptVerifier>, repository: RefundRepository) -> Self {
        Self {
            verifier,
            repository,
        }
    }

    pub async fn process(
        &self,
        request: PaymentWebhookRequest<'_>,
    ) -> Result<RefundReceiptOutcome, RefundReceiptProcessorError> {
        let verified = self
            .verifier
            .verify(&request)
            .map_err(|_| RefundReceiptProcessorError::VerificationRejected)?;
        let event_key = hex_lower(Uuid::new_v4().as_bytes());
        let payload_sha256: [u8; 32] = Sha256::digest(request.payload()).into();
        let fingerprint = hex_lower(&verified.signature_key_fingerprint);
        let payload_sha256 = hex_lower(&payload_sha256);
        let write = RefundReceiptWrite {
            event_key,
            request_id: verified.request_id,
            provider: self.verifier.provider().to_owned(),
            provider_event_id: verified.provider_event_id,
            provider_refund_id: verified.provider_refund_id,
            status: verified.status,
            amount_minor: verified.amount_minor,
            currency: verified.currency,
            signature_key_fingerprint: fingerprint,
            payload_sha256,
            received_at: request.received_at(),
            processed_at: request.received_at(),
            created_at: request.received_at(),
        };
        self.repository
            .apply_receipt(write)
            .await
            .map_err(Into::into)
    }
}

/// 退款回执 HTTP 层可观察的闭合结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefundReceiptHandlerOutcome {
    /// 新回执已推进退款请求终态。
    Applied,
    /// 相同回执已幂等处理，未重复推进状态。
    Existing,
    /// 回执引用的退款请求不存在。
    NotFound,
}

/// 退款回执 HTTP 处理器的擦除接口。
pub trait RefundReceiptHandler: Send + Sync + 'static {
    fn handle<'a>(&'a self, request: PaymentWebhookRequest<'a>) -> RefundReceiptHandlerFuture<'a>;
}

pub type RefundReceiptHandlerFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<RefundReceiptHandlerOutcome, RefundReceiptProcessorError>>
            + Send
            + 'a,
    >,
>;

impl RefundReceiptHandler for RefundReceiptProcessor {
    fn handle<'a>(&'a self, request: PaymentWebhookRequest<'a>) -> RefundReceiptHandlerFuture<'a> {
        Box::pin(async move { self.process(request).await.map(map_receipt_outcome) })
    }
}

/// 退款提交编排器：先抢占本地状态，再调用 Provider，最后以 CAS 绑定结果。
///
/// Provider 返回结果未知或响应无法校验时保持 `Submitted`，后续只能通过同一请求恢复，
/// 避免网络重试造成重复资金动作。
#[derive(Clone)]
pub struct RefundSubmissionProcessor {
    provider: Arc<dyn RefundProvider>,
    repository: RefundRepository,
}

impl RefundSubmissionProcessor {
    #[must_use]
    pub fn new(provider: Arc<dyn RefundProvider>, repository: RefundRepository) -> Self {
        Self {
            provider,
            repository,
        }
    }

    /// 提交或幂等复用同一退款请求；`now` 是 Unix 秒时间戳。
    pub async fn submit(
        &self,
        request_id: RefundRequestId,
        payment_reference: String,
        now: u64,
    ) -> Result<RefundSubmissionOutcome, RefundSubmissionProcessorError> {
        let record = self.load_request(request_id).await?;
        let request = self.build_request(&record, payment_reference)?;
        let claimed = self
            .repository
            .claim_submission(request_id, record.version(), now)
            .await
            .map_err(RefundSubmissionProcessorError::from)?;
        let submitted = match claimed {
            RefundSubmissionOutcome::Existing(record) => {
                return Ok(RefundSubmissionOutcome::Existing(record));
            }
            RefundSubmissionOutcome::Applied(record) => record,
        };
        let expected_version = submitted.version();
        match self.provider.submit(request).await {
            Ok(result) => self
                .repository
                .bind_provider_refund_id(
                    request_id,
                    expected_version,
                    result.provider_refund_id().to_owned(),
                    now,
                )
                .await
                .map_err(RefundSubmissionProcessorError::from),
            Err(RefundProviderError::Rejected) => {
                self.repository
                    .mark_failed(request_id, expected_version, now)
                    .await
                    .map_err(RefundSubmissionProcessorError::from)?;
                Err(RefundSubmissionProcessorError::Rejected)
            }
            Err(RefundProviderError::OutcomeUnknown) => {
                Err(RefundSubmissionProcessorError::OutcomeUnknown)
            }
            Err(RefundProviderError::Unavailable) => {
                Err(RefundSubmissionProcessorError::Unavailable)
            }
            Err(RefundProviderError::InvalidResponse) => {
                Err(RefundSubmissionProcessorError::InvalidResponse)
            }
        }
    }

    /// 使用已绑定的 Provider 退款标识恢复结果；不会重新发起退款。
    pub async fn recover(
        &self,
        request_id: RefundRequestId,
        payment_reference: String,
    ) -> Result<RefundSubmissionOutcome, RefundSubmissionProcessorError> {
        let record = self.load_request(request_id).await?;
        let provider_refund_id = record
            .provider_refund_id()
            .ok_or(RefundSubmissionProcessorError::Conflict)?
            .to_owned();
        let request = self.build_request(&record, payment_reference)?;
        let recovery = RefundProviderRecoveryRequest::new(request, provider_refund_id.clone())
            .map_err(|_| RefundSubmissionProcessorError::InvalidInput)?;
        let result = self
            .provider
            .recover(recovery)
            .await
            .map_err(RefundSubmissionProcessorError::from)?;
        if result.provider_refund_id() != provider_refund_id {
            return Err(RefundSubmissionProcessorError::InvalidResponse);
        }
        Ok(RefundSubmissionOutcome::Existing(record))
    }

    async fn load_request(
        &self,
        request_id: RefundRequestId,
    ) -> Result<RefundRequestRecord, RefundSubmissionProcessorError> {
        self.repository
            .get_request(request_id)
            .await
            .map_err(RefundSubmissionProcessorError::from)?
            .ok_or(RefundSubmissionProcessorError::NotFound)
    }

    fn build_request(
        &self,
        record: &RefundRequestRecord,
        payment_reference: String,
    ) -> Result<RefundProviderRequest, RefundSubmissionProcessorError> {
        if record.provider() != self.provider.provider() {
            return Err(RefundSubmissionProcessorError::Conflict);
        }
        RefundProviderRequest::new(record, payment_reference)
            .map_err(|_| RefundSubmissionProcessorError::InvalidInput)
    }
}

impl fmt::Debug for RefundSubmissionProcessor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RefundSubmissionProcessor(<已脱敏>)")
    }
}

impl fmt::Debug for RefundReceiptProcessor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RefundReceiptProcessor(<已脱敏>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundProviderInputError {
    #[error("退款 Provider 原支付标识无效")]
    InvalidPaymentReference,
    #[error("退款 Provider 退款标识无效")]
    InvalidProviderRefundId,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundReceiptVerificationError {
    #[error("退款回执验签失败")]
    InvalidSignature,
    #[error("退款回执内容无效")]
    InvalidReceipt,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundReceiptProcessorError {
    #[error("退款回执验签拒绝")]
    VerificationRejected,
    #[error("退款回执状态冲突")]
    Conflict,
    #[error("退款回执处理结果未知")]
    OutcomeUnknown,
    #[error("退款回执暂不可用")]
    Unavailable,
    #[error("退款回执内部状态损坏")]
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundSubmissionProcessorError {
    #[error("退款请求不存在")]
    NotFound,
    #[error("退款请求状态冲突")]
    Conflict,
    #[error("退款请求输入无效")]
    InvalidInput,
    #[error("退款 Provider 明确拒绝请求")]
    Rejected,
    #[error("退款 Provider 结果未知")]
    OutcomeUnknown,
    #[error("退款 Provider 暂不可用")]
    Unavailable,
    #[error("退款 Provider 响应无效")]
    InvalidResponse,
    #[error("退款内部状态损坏")]
    Invariant,
}

impl From<RefundProviderError> for RefundSubmissionProcessorError {
    fn from(error: RefundProviderError) -> Self {
        match error {
            RefundProviderError::Rejected => Self::Rejected,
            RefundProviderError::OutcomeUnknown => Self::OutcomeUnknown,
            RefundProviderError::Unavailable => Self::Unavailable,
            RefundProviderError::InvalidResponse => Self::InvalidResponse,
        }
    }
}

impl From<RefundRepositoryError> for RefundSubmissionProcessorError {
    fn from(error: RefundRepositoryError) -> Self {
        match error {
            RefundRepositoryError::Conflict => Self::Conflict,
            RefundRepositoryError::NotFound => Self::NotFound,
            RefundRepositoryError::OutcomeUnknown => Self::OutcomeUnknown,
            RefundRepositoryError::Query | RefundRepositoryError::Timeout => Self::Unavailable,
            RefundRepositoryError::Invariant => Self::Invariant,
        }
    }
}

impl From<RefundRepositoryError> for RefundReceiptProcessorError {
    fn from(error: RefundRepositoryError) -> Self {
        match error {
            RefundRepositoryError::Conflict | RefundRepositoryError::NotFound => Self::Conflict,
            RefundRepositoryError::OutcomeUnknown => Self::OutcomeUnknown,
            RefundRepositoryError::Query | RefundRepositoryError::Timeout => Self::Unavailable,
            RefundRepositoryError::Invariant => Self::Invariant,
        }
    }
}

fn valid_provider_refund_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REFUND_PROVIDER_REFUND_ID_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn map_receipt_outcome(outcome: RefundReceiptOutcome) -> RefundReceiptHandlerOutcome {
    match outcome {
        RefundReceiptOutcome::Applied(_) => RefundReceiptHandlerOutcome::Applied,
        RefundReceiptOutcome::Existing(_) => RefundReceiptHandlerOutcome::Existing,
        RefundReceiptOutcome::NotFound => RefundReceiptHandlerOutcome::NotFound,
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_domain::{RefundApprovalStatus, RefundOrderKind, RefundRequestKey, UserId};

    fn record() -> RefundRequestRecord {
        RefundRequestRecord::from_persistence(
            1,
            RefundRequestId::new([1; 16]).unwrap(),
            RefundRequestKey::new([2; 16]).unwrap(),
            UserId::new(7).unwrap(),
            RefundOrderKind::Topup,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            "stripe".to_owned(),
            Some("pi_original".to_owned()),
            "USD".to_owned(),
            100,
            40,
            None,
            RefundRequestStatus::Requested,
            RefundApprovalStatus::Pending,
            None,
            None,
            1,
            1_900_000_000,
            1_900_000_000,
        )
        .unwrap()
    }

    #[test]
    fn provider_boundaries_redact_and_reject_invalid_identifiers() {
        let request = RefundProviderRequest::new(&record(), "pi_123".to_owned()).unwrap();
        assert_eq!(request.amount_minor(), 40);
        assert!(RefundProviderResult::new("re_123".to_owned()).is_ok());
        assert_eq!(
            RefundProviderResult::new("bad\nrefund".to_owned()).unwrap_err(),
            RefundProviderInputError::InvalidProviderRefundId
        );
        assert!(!format!("{request:?}").contains("pi_123"));
    }

    #[test]
    fn receipt_requires_terminal_refund_state_and_closed_amount() {
        let error = VerifiedRefundReceipt::new(
            RefundRequestId::new([1; 16]).unwrap(),
            "evt_1".to_owned(),
            "re_1".to_owned(),
            RefundRequestStatus::Submitted,
            40,
            "USD".to_owned(),
            [3; 32],
        )
        .unwrap_err();
        assert_eq!(error, RefundReceiptVerificationError::InvalidReceipt);
    }
}
