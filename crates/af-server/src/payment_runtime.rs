use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, RwLock},
    time::Duration,
};

use af_account::{SystemSecretCipher, SystemSecretKind};
use af_admin::{
    AdminRefundError, AdminRefundSubmitter, DatabaseUserTopupService, EPAY_ALIPAY_PAYMENT_METHOD,
    EPAY_WXPAY_PAYMENT_METHOD, PaymentSettingsApplyFuture, PaymentSettingsRuntimeApplier,
    PaymentSettingsRuntimeError, SessionPrincipal, UserTopupConfiguration, UserTopupError,
    UserTopupOrderCreateCommand, UserTopupOrderCreateFuture, UserTopupProviderRoute,
    UserTopupService,
};
use af_billing::{
    DatabasePaymentWebhookRouter, EASYPAY_PAYMENT_PROVIDER, EasyPayPaymentMethod,
    EasyPayPaymentProvider, PaymentOrderProvider, PaymentWebhookHandler, PaymentWebhookProcessor,
    RefundProvider, RefundReceiptHandler, RefundReceiptProcessor, RefundSubmissionOutcome,
    RefundSubmissionProcessor, STRIPE_PAYMENT_PROVIDER, StripePaymentProvider,
};
use af_db::{
    EncryptedCredentialEnvelope, PaymentSettingsRecord, RefundRepository, SiteSettingsRepository,
    SubscriptionRepository, TopupRepository,
};
use af_http::{PaymentWebhookProcessorRegistry, RefundReceiptProcessorRegistry};
use af_httpclient::HttpClientProvider;
use url::Url;

use crate::stripe_payment_intent::StripePaymentIntentProvider;
use crate::stripe_refund::StripeRefundProvider;

const EASYPAY_PRODUCT_NAME: &str = "AnyFlows 余额充值";

/// 支付运行时的不可变快照；一次请求始终持有同一代 Provider 组合。
struct PaymentRuntimeSnapshot {
    user_topup_service: Option<Arc<dyn UserTopupService>>,
    webhook_processors: HashMap<String, Arc<dyn PaymentWebhookHandler>>,
    refund_providers: HashMap<String, Arc<dyn RefundProvider>>,
    refund_receipt_processors: HashMap<String, Arc<dyn RefundReceiptHandler>>,
    epay_manual_refund_enabled: bool,
    refund_auto_submit_enabled: bool,
}

impl PaymentRuntimeSnapshot {
    fn disabled() -> Self {
        Self {
            user_topup_service: None,
            webhook_processors: HashMap::new(),
            refund_providers: HashMap::new(),
            refund_receipt_processors: HashMap::new(),
            epay_manual_refund_enabled: false,
            refund_auto_submit_enabled: false,
        }
    }
}

/// 支付 Provider 的稳定运行时门面；新配置只影响切换后的新请求。
pub(crate) struct PaymentRuntime {
    snapshot: RwLock<Arc<PaymentRuntimeSnapshot>>,
    topup_repository: TopupRepository,
    refund_repository: RefundRepository,
    subscription_repository: SubscriptionRepository,
    site_settings_repository: SiteSettingsRepository,
    upstream_clients: HttpClientProvider,
    request_timeout: Duration,
}

impl PaymentRuntime {
    /// 使用稳定数据库与网络依赖创建初始禁用的支付运行时。
    pub(crate) fn new(
        topup_repository: TopupRepository,
        refund_repository: RefundRepository,
        subscription_repository: SubscriptionRepository,
        site_settings_repository: SiteSettingsRepository,
        upstream_clients: HttpClientProvider,
        request_timeout: Duration,
    ) -> Self {
        Self {
            snapshot: RwLock::new(Arc::new(PaymentRuntimeSnapshot::disabled())),
            topup_repository,
            refund_repository,
            subscription_repository,
            site_settings_repository,
            upstream_clients,
            request_timeout,
        }
    }

    async fn build_snapshot(
        &self,
        record: &PaymentSettingsRecord,
        cipher: &SystemSecretCipher,
    ) -> Result<PaymentRuntimeSnapshot, PaymentSettingsRuntimeError> {
        let mut routes = Vec::new();
        let mut webhook_processors: HashMap<String, Arc<dyn PaymentWebhookHandler>> =
            HashMap::new();
        let mut refund_providers: HashMap<String, Arc<dyn RefundProvider>> = HashMap::new();
        let mut refund_receipt_processors: HashMap<String, Arc<dyn RefundReceiptHandler>> =
            HashMap::new();

        if record.stripe_enabled() {
            self.add_stripe(
                record,
                cipher,
                &mut routes,
                &mut webhook_processors,
                &mut refund_providers,
                &mut refund_receipt_processors,
            )?;
        }
        if record.epay_enabled() {
            self.add_epay(record, cipher, &mut routes, &mut webhook_processors)
                .await?;
        }

        let user_topup_service = if routes.is_empty() {
            None
        } else {
            let service = DatabaseUserTopupService::new(self.topup_repository.clone(), routes)
                .map_err(|_| runtime_failed("topup_service"))?;
            Some(Arc::new(service) as Arc<dyn UserTopupService>)
        };
        Ok(PaymentRuntimeSnapshot {
            user_topup_service,
            webhook_processors,
            refund_providers,
            refund_receipt_processors,
            epay_manual_refund_enabled: record.epay_refund_enabled(),
            refund_auto_submit_enabled: record.refund_auto_submit_enabled(),
        })
    }

    fn add_stripe(
        &self,
        record: &PaymentSettingsRecord,
        cipher: &SystemSecretCipher,
        routes: &mut Vec<UserTopupProviderRoute>,
        processors: &mut HashMap<String, Arc<dyn PaymentWebhookHandler>>,
        refund_providers: &mut HashMap<String, Arc<dyn RefundProvider>>,
        refund_receipt_processors: &mut HashMap<String, Arc<dyn RefundReceiptHandler>>,
    ) -> Result<(), PaymentSettingsRuntimeError> {
        let publishable_key = record
            .stripe_publishable_key()
            .ok_or_else(|| runtime_failed("stripe_publishable_key"))?;
        let secret_key = decrypt_required(
            cipher,
            SystemSecretKind::StripeSecretKey,
            record.stripe_secret_key(),
            "stripe_secret_key",
        )?;
        let webhook_secret = decrypt_required(
            cipher,
            SystemSecretKind::StripeWebhookSecret,
            record.stripe_webhook_secret(),
            "stripe_webhook_secret",
        )?;
        let order_provider = Arc::new(
            StripePaymentIntentProvider::new(
                secret_key.expose_secret(),
                self.upstream_clients.clone(),
                self.request_timeout,
            )
            .map_err(|_| runtime_failed("stripe_order_provider"))?,
        );
        routes.push(
            UserTopupProviderRoute::stripe(publishable_key.to_owned(), order_provider)
                .map_err(|_| runtime_failed("stripe_route"))?,
        );

        let refund_provider = Arc::new(
            StripeRefundProvider::new(
                secret_key.expose_secret(),
                self.upstream_clients.clone(),
                self.request_timeout,
            )
            .map_err(|_| runtime_failed("stripe_refund_provider"))?,
        );
        refund_providers.insert(STRIPE_PAYMENT_PROVIDER.to_owned(), refund_provider);

        let webhook_provider = Arc::new(
            StripePaymentProvider::new(
                webhook_secret.expose_secret(),
                u64::from(record.stripe_signature_tolerance_seconds()),
            )
            .map_err(|_| runtime_failed("stripe_webhook_provider"))?,
        );
        processors.insert(
            STRIPE_PAYMENT_PROVIDER.to_owned(),
            Arc::new(PaymentWebhookProcessor::new(
                webhook_provider.clone(),
                Arc::new(DatabasePaymentWebhookRouter::new(
                    self.topup_repository.clone(),
                    self.subscription_repository.clone(),
                )),
            )),
        );
        refund_receipt_processors.insert(
            STRIPE_PAYMENT_PROVIDER.to_owned(),
            Arc::new(RefundReceiptProcessor::new(
                webhook_provider,
                self.refund_repository.clone(),
            )),
        );
        Ok(())
    }

    async fn add_epay(
        &self,
        record: &PaymentSettingsRecord,
        cipher: &SystemSecretCipher,
        routes: &mut Vec<UserTopupProviderRoute>,
        processors: &mut HashMap<String, Arc<dyn PaymentWebhookHandler>>,
    ) -> Result<(), PaymentSettingsRuntimeError> {
        let gateway_url = record
            .epay_gateway_url()
            .ok_or_else(|| runtime_failed("epay_gateway_url"))?;
        let merchant_id = record
            .epay_merchant_id()
            .ok_or_else(|| runtime_failed("epay_merchant_id"))?;
        let merchant_key = decrypt_required(
            cipher,
            SystemSecretKind::EpayMerchantKey,
            record.epay_merchant_key(),
            "epay_merchant_key",
        )?;
        let site = self
            .site_settings_repository
            .settings()
            .await
            .map_err(|_| runtime_failed("site_settings"))?;
        let public_base_url = site
            .public_base_url()
            .ok_or_else(|| invalid_runtime_configuration("epay_public_base_url"))?;
        let (notify_url, return_url) = epay_callback_urls(public_base_url)?;
        let mut methods = Vec::with_capacity(2);
        if record.epay_alipay_enabled() {
            methods.push(EasyPayPaymentMethod::Alipay);
        }
        if record.epay_wxpay_enabled() {
            methods.push(EasyPayPaymentMethod::WechatPay);
        }
        let provider = Arc::new(
            EasyPayPaymentProvider::new(
                gateway_url.to_owned(),
                merchant_id.to_owned(),
                merchant_key.expose_secret(),
                notify_url,
                return_url,
                methods,
                EASYPAY_PRODUCT_NAME.to_owned(),
            )
            .map_err(|_| runtime_failed("epay_provider"))?,
        );
        // 易支付不提供原路退款接口，退款由管理员审批后登记线下完成事实。
        if record.epay_alipay_enabled() {
            routes.push(
                UserTopupProviderRoute::epay_with_qr(
                    EPAY_ALIPAY_PAYMENT_METHOD,
                    record.epay_qr_enabled(),
                    record.epay_quota_per_cny(),
                    provider.clone(),
                )
                .map_err(|_| runtime_failed("epay_alipay_route"))?,
            );
        }
        if record.epay_wxpay_enabled() {
            routes.push(
                UserTopupProviderRoute::epay_with_qr(
                    EPAY_WXPAY_PAYMENT_METHOD,
                    record.epay_qr_enabled(),
                    record.epay_quota_per_cny(),
                    provider.clone(),
                )
                .map_err(|_| runtime_failed("epay_wxpay_route"))?,
            );
        }
        processors.insert(
            EASYPAY_PAYMENT_PROVIDER.to_owned(),
            Arc::new(PaymentWebhookProcessor::new(
                provider,
                Arc::new(DatabasePaymentWebhookRouter::new(
                    self.topup_repository.clone(),
                    self.subscription_repository.clone(),
                )),
            )),
        );
        Ok(())
    }

    fn current_snapshot(&self) -> Arc<PaymentRuntimeSnapshot> {
        self.snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// 返回当前快照中的退款 Provider，供退款提交/恢复入口复用同一配置版本。
    #[allow(dead_code)]
    pub(crate) fn refund_provider(&self, provider: &str) -> Option<Arc<dyn RefundProvider>> {
        self.current_snapshot()
            .refund_providers
            .get(provider)
            .cloned()
    }
}

impl AdminRefundSubmitter for PaymentRuntime {
    fn manual_refund_enabled<'a>(
        &'a self,
        provider: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<bool, AdminRefundError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Ok(provider == EASYPAY_PAYMENT_PROVIDER
                && self.current_snapshot().epay_manual_refund_enabled)
        })
    }

    fn auto_submit_enabled<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<bool, AdminRefundError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(self.current_snapshot().refund_auto_submit_enabled) })
    }

    fn submit<'a>(
        &'a self,
        record: &'a af_domain::RefundRequestRecord,
        now: u64,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<af_domain::RefundRequestRecord, AdminRefundError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            if record.approval_status() != af_domain::RefundApprovalStatus::Approved
                || !matches!(
                    record.status(),
                    af_domain::RefundRequestStatus::Requested
                        | af_domain::RefundRequestStatus::Failed
                )
            {
                return Err(AdminRefundError::Conflict);
            }
            let payment_reference = record
                .payment_reference()
                .ok_or(AdminRefundError::Unavailable)?;
            let provider = self
                .refund_provider(record.provider())
                .ok_or(AdminRefundError::Unavailable)?;
            let processor =
                RefundSubmissionProcessor::new(provider, self.refund_repository.clone());
            let outcome = processor
                .submit(record.request_id(), payment_reference.to_owned(), now)
                .await
                .map_err(map_admin_refund_submission_error)?;
            Ok(match outcome {
                RefundSubmissionOutcome::Applied(record)
                | RefundSubmissionOutcome::Existing(record) => record,
            })
        })
    }
}

fn map_admin_refund_submission_error(
    error: af_billing::RefundSubmissionProcessorError,
) -> AdminRefundError {
    match error {
        af_billing::RefundSubmissionProcessorError::NotFound => AdminRefundError::NotFound,
        af_billing::RefundSubmissionProcessorError::Conflict => AdminRefundError::Conflict,
        af_billing::RefundSubmissionProcessorError::OutcomeUnknown => {
            AdminRefundError::OutcomeUnknown
        }
        af_billing::RefundSubmissionProcessorError::Unavailable => AdminRefundError::Unavailable,
        af_billing::RefundSubmissionProcessorError::Rejected
        | af_billing::RefundSubmissionProcessorError::InvalidResponse => {
            AdminRefundError::AutoSubmitFailed
        }
        af_billing::RefundSubmissionProcessorError::InvalidInput
        | af_billing::RefundSubmissionProcessorError::Invariant => AdminRefundError::Internal,
    }
}

impl PaymentSettingsRuntimeApplier for PaymentRuntime {
    fn apply<'a>(
        &'a self,
        record: &'a PaymentSettingsRecord,
        cipher: &'a SystemSecretCipher,
    ) -> PaymentSettingsApplyFuture<'a> {
        Box::pin(async move {
            let next = Arc::new(self.build_snapshot(record, cipher).await?);
            // 快照完整构造成功后才切换，避免暴露半装配 Provider。
            *self
                .snapshot
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = next;
            Ok(())
        })
    }
}

impl UserTopupService for PaymentRuntime {
    fn configuration(&self) -> Result<UserTopupConfiguration, UserTopupError> {
        let snapshot = self.current_snapshot();
        snapshot
            .user_topup_service
            .as_ref()
            .ok_or(UserTopupError::Unavailable)?
            .configuration()
    }

    fn provider_for(
        &self,
        provider: &str,
        payment_method: &str,
    ) -> Result<Arc<dyn PaymentOrderProvider>, UserTopupError> {
        let snapshot = self.current_snapshot();
        snapshot
            .user_topup_service
            .as_ref()
            .ok_or(UserTopupError::Unavailable)?
            .provider_for(provider, payment_method)
    }

    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserTopupOrderCreateCommand,
    ) -> UserTopupOrderCreateFuture<'a> {
        let snapshot = self.current_snapshot();
        Box::pin(async move {
            let service = snapshot
                .user_topup_service
                .as_ref()
                .ok_or(UserTopupError::Unavailable)?;
            service.create(principal, command).await
        })
    }
}

impl PaymentWebhookProcessorRegistry for PaymentRuntime {
    fn processor(&self, provider: &str) -> Option<Arc<dyn PaymentWebhookHandler>> {
        self.current_snapshot()
            .webhook_processors
            .get(provider)
            .cloned()
    }
}

impl RefundReceiptProcessorRegistry for PaymentRuntime {
    fn processor(&self, provider: &str) -> Option<Arc<dyn RefundReceiptHandler>> {
        self.current_snapshot()
            .refund_receipt_processors
            .get(provider)
            .cloned()
    }
}

impl fmt::Debug for PaymentRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PaymentRuntime(<已脱敏>)")
    }
}

fn decrypt_required<'a>(
    cipher: &'a SystemSecretCipher,
    kind: SystemSecretKind,
    envelope: Option<&'a EncryptedCredentialEnvelope>,
    error_kind: &'static str,
) -> Result<af_account::DecryptedSystemSecret, PaymentSettingsRuntimeError> {
    cipher
        .decrypt(kind, envelope.ok_or_else(|| runtime_failed(error_kind))?)
        .map_err(|_| runtime_failed(error_kind))
}

fn epay_callback_urls(
    public_base_url: &str,
) -> Result<(String, String), PaymentSettingsRuntimeError> {
    let mut base = Url::parse(public_base_url).map_err(|_| runtime_failed("public_base_url"))?;
    base.set_query(None);
    base.set_fragment(None);
    let prefix = base.path().trim_end_matches('/').to_owned();
    base.set_path(&format!("{prefix}/api/payment/webhook/epay"));
    let notify_url = base.to_string();
    base.set_path(&format!("{prefix}/"));
    base.set_fragment(Some("/console/wallet?epay_return=1"));
    Ok((notify_url, base.to_string()))
}

fn runtime_failed(error_kind: &'static str) -> PaymentSettingsRuntimeError {
    tracing::error!(
        target: "af_server::payment_runtime",
        error_kind,
        "支付运行时快照装配失败"
    );
    PaymentSettingsRuntimeError::Failed
}

fn invalid_runtime_configuration(error_kind: &'static str) -> PaymentSettingsRuntimeError {
    tracing::warn!(
        target: "af_server::payment_runtime",
        error_kind,
        "支付运行时配置无效"
    );
    PaymentSettingsRuntimeError::InvalidConfiguration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epay_callbacks_only_derive_from_public_base_url() {
        let (notify, returning) = epay_callback_urls("https://example.com/gateway/").unwrap();
        assert_eq!(
            notify,
            "https://example.com/gateway/api/payment/webhook/epay"
        );
        assert_eq!(
            returning,
            "https://example.com/gateway/#/console/wallet?epay_return=1"
        );
    }
}
