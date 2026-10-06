//! 定价、额度核算与计费会话编排。

mod batch;
mod billing_session;
mod database_sink;
mod easypay;
mod expression_pricing;
mod extensions;
mod group_pricing_cache;
mod lifecycle;
mod model_price_cache;
mod multimodal;
mod payment_order;
mod payment_webhook;
mod pricing;
mod pricing_snapshot;
pub mod quota_math;
mod refund;
mod refund_signal_consumer;
mod refund_signal_queue;
mod stripe;
mod task_billing;
mod usage_record_consumer;
mod usage_record_database_sink;
mod usage_record_queue;
mod video_pricing;

/// 已通过 webhook 处理器校验的持久化事件写入值。
pub use af_db::{RefundSubmissionOutcome, TopupPaymentEventWrite};
pub use batch::{
    BillingBatch, BillingBatchApplyOutcome, BillingBatchError, BillingBatchEvent,
    BillingBatchFlushOutcome, BillingBatchRecordOutcome, BillingBatchSink, BillingBatchSinkError,
    BillingBatchSinkFuture, BillingWriterId, ChannelBillingDelta, FileBillingBatcher,
    TokenBillingDelta, UserBillingDelta,
};
pub use billing_session::{
    BillingSession, BillingSessionError, BillingSessionState, RefundSignalOutcome,
    RefundSignalPort, SettlementRequest,
};
pub use database_sink::DatabaseBillingBatchSink;
pub use easypay::{
    EASYPAY_PAYMENT_PROVIDER, EasyPayPaymentMethod, EasyPayPaymentProvider,
    EasyPayPaymentProviderConfigError, EasyPaySigningError, easy_pay_signature,
};
pub use expression_pricing::{
    BillingExpression, BillingExpressionDefinition, BillingExpressionError,
    BillingExpressionResult, BillingExpressionUsage, BillingExpressionUsageSemantics,
    BillingExpressionVariables, ExpressionVariable, ExpressionVariableUsage, ExpressionVersion,
    MAX_BILLING_EXPRESSION_BYTES,
};
pub use extensions::{
    ContractPriceSource, ContractPriceSourceError, ContractPriceSourceFuture,
    ServiceAccountAuditError, ServiceAccountAuditFuture, ServiceAccountAuditSink,
};
pub use group_pricing_cache::{
    GroupModelRatioSourceRecord, GroupPeakPricing, GroupPricingCache, GroupPricingCacheError,
    GroupPricingLookupError, GroupPricingSnapshot, GroupPricingSource, GroupPricingSourceCatalog,
    GroupPricingSourceError, GroupPricingSourceFuture, GroupPricingSourceRecord,
};
pub use lifecycle::{
    BillingCompletion, BillingLifecycleError, BillingLifecycleState, BillingPrechargeError,
    BillingPrechargeFuture, BillingPrechargePort, BillingRequestLifecycle, BillingRequestPlan,
    BillingSettlementError, BillingSettlementFuture, BillingSettlementPort, BillingUsageContext,
    BillingUsageDimensions, BillingUsageObservation, BillingUsageObservationError,
    BillingUsageRecord, BillingUsageTiming, MAX_USAGE_MODEL_BYTES, MAX_USAGE_REQUEST_ID_BYTES,
    UsageRecordOutcome, UsageRecordPort,
};
pub use model_price_cache::{
    ModelPrice, ModelPriceCache, ModelPriceCacheError, ModelPriceMode, ModelPriceSnapshot,
    ModelPriceSource, ModelPriceSourceError, ModelPriceSourceFuture, ModelPriceSourceRecord,
};
pub use multimodal::{
    BillingDurationSeconds, BillingFactor, BillingImageCount, BillingPixelCount, BillingResolution,
    MAX_BILLING_DURATION_SECONDS, MAX_BILLING_IMAGE_COUNT, MAX_BILLING_RESOLUTION_EDGE,
    MAX_BILLING_RESOLUTION_PIXELS, MAX_MULTIMODAL_DIMENSIONS, MultimodalBillingDimensions,
    MultimodalBillingError, MultimodalDimensionError,
};
pub use payment_order::{
    MAX_PAYMENT_CLIENT_SECRET_BYTES, MAX_PAYMENT_REDIRECT_URL_BYTES, PaymentCheckoutAction,
    PaymentClientSecret, PaymentOrderFuture, PaymentOrderId, PaymentOrderInputError,
    PaymentOrderProvider, PaymentOrderProviderError, PaymentOrderRecoveryRequest,
    PaymentOrderRequest, PaymentOrderSession, PaymentRedirectUrl,
};
pub use payment_webhook::{
    DatabasePaymentWebhookRouter, MAX_PAYMENT_WEBHOOK_HEADER_NAME_BYTES,
    MAX_PAYMENT_WEBHOOK_HEADER_VALUE_BYTES, MAX_PAYMENT_WEBHOOK_HEADERS,
    MAX_PAYMENT_WEBHOOK_PAYLOAD_BYTES, PaymentProvider, PaymentWebhookEventRouter,
    PaymentWebhookHandler, PaymentWebhookHandlerError, PaymentWebhookHandlerFuture,
    PaymentWebhookHandlerOutcome, PaymentWebhookHeader, PaymentWebhookInputError,
    PaymentWebhookProcessor, PaymentWebhookRequest, PaymentWebhookRouteFuture,
    PaymentWebhookVerificationError, PaymentWebhookVerifier, TopupPaymentEventFuture,
    TopupPaymentEventPort, TopupPaymentEventPortError, TopupWebhookOutcome, TopupWebhookProcessor,
    TopupWebhookProcessorError, VerifiedTopupPaymentEvent,
};
pub use pricing::{
    BillingMode, CostBreakdown, EffectiveTokenPrices, PriceData, PricingContext, PricingError,
    PricingRatio, PricingRatios, PricingResolver, RatioPricingResolver, TokenPrices,
};
pub use pricing_snapshot::{
    CachedRequestPricingSnapshotSource, RequestPricingSnapshot, RequestPricingSnapshotError,
    RequestPricingSnapshotFuture, RequestPricingSnapshotSource,
};
pub use quota_math::quota_from_cny_minor;
pub use refund::{
    RefundProvider, RefundProviderError, RefundProviderFuture, RefundProviderInputError,
    RefundProviderRecoveryRequest, RefundProviderRequest, RefundProviderResult,
    RefundReceiptHandler, RefundReceiptHandlerFuture, RefundReceiptHandlerOutcome,
    RefundReceiptProcessor, RefundReceiptProcessorError, RefundReceiptVerificationError,
    RefundReceiptVerifier, RefundSubmissionProcessor, RefundSubmissionProcessorError,
    VerifiedRefundReceipt,
};
pub use refund_signal_consumer::{
    RefundSignalConsumer, RefundSignalConsumerError, RefundSignalSink, RefundSignalSinkError,
    RefundSignalSinkFuture,
};
pub use refund_signal_queue::{
    RefundSignalDelivery, RefundSignalQueue, RefundSignalQueueError, RefundSignalReceiver,
};
pub use stripe::{
    DEFAULT_STRIPE_SIGNATURE_TOLERANCE_SECS, MAX_STRIPE_SIGNATURE_TOLERANCE_SECS,
    STRIPE_PAYMENT_PROVIDER, STRIPE_SIGNATURE_HEADER, StripePaymentProvider,
    StripePaymentProviderConfigError,
};
pub use task_billing::{
    TaskBillingCompletion, TaskBillingError, TaskBillingLifecycle, TaskBillingPlan,
    TaskBillingReleaseConsumer, TaskBillingReleaseConsumerError, TaskBillingReleaseDelivery,
    TaskBillingReleaseQueue, TaskBillingReleaseQueueError, TaskBillingReleaseReceiver,
    TaskBillingReleaseSignalOutcome, TaskBillingReleaseSignalPort, TaskBillingReleaseSink,
    TaskBillingReleaseSinkError, TaskBillingReleaseSinkFuture, TaskBillingReserveError,
    TaskBillingReserveFuture, TaskBillingReservePort, TaskBillingSettlementError,
    TaskBillingSettlementFuture, TaskBillingSettlementPort, TaskBillingSettlementRequest,
    TaskBillingState,
};
pub use usage_record_consumer::{
    UsageRecordConsumer, UsageRecordConsumerError, UsageRecordSink, UsageRecordSinkError,
    UsageRecordSinkFuture,
};
pub use usage_record_database_sink::{
    DatabaseUsageRecordSink, UsageRecordProjection, UsageRecordProjectionError,
};
pub use usage_record_queue::{
    UsageRecordDelivery, UsageRecordQueue, UsageRecordQueueError, UsageRecordReceiver,
};
pub use video_pricing::{
    DEFAULT_XAI_VIDEO_DURATION_SECONDS, XAI_VIDEO_PRICE_CARD_VERSION, XaiVideoPricingError,
    XaiVideoPricingSnapshot,
};

#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod billing_invariants_tests;
#[cfg(test)]
mod billing_session_tests;
#[cfg(test)]
mod expression_pricing_tests;
#[cfg(test)]
mod group_pricing_cache_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod model_price_cache_tests;
#[cfg(test)]
mod multimodal_tests;
#[cfg(test)]
mod payment_webhook_tests;
#[cfg(test)]
mod pricing_snapshot_tests;
#[cfg(test)]
mod pricing_tests;
#[cfg(test)]
mod quota_math_tests;
#[cfg(test)]
mod task_billing_tests;
#[cfg(test)]
mod video_pricing_tests;
