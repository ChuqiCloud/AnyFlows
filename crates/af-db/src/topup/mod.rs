mod extension;
mod repository;
mod types;

pub use extension::{TopupCreditOutcome, TopupExtension, TopupExtensionFuture};
pub use repository::TopupRepository;
pub use types::{
    MAX_PAYMENT_METHOD_BYTES, MAX_PAYMENT_PROVIDER_BYTES, MAX_PROVIDER_EVENT_ID_BYTES,
    MAX_PROVIDER_ORDER_ID_BYTES, MAX_PROVIDER_TRADE_NO_BYTES, TopupInputError, TopupOrderCreate,
    TopupOrderCreateOutcome, TopupOrderRecord, TopupOrderSubmission, TopupOrderSubmitOutcome,
    TopupPaymentEventOutcome, TopupPaymentEventRejection, TopupPaymentEventWrite,
    TopupRepositoryConfigError, TopupRepositoryError,
};

// 充值集成测试包含企业钱包扩展实体；企业仓库负责运行完整测试套件。
