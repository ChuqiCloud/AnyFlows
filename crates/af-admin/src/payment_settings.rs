use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_account::{PlainSystemSecret, SystemSecretCipher, SystemSecretKind};
use af_db::{
    PaymentSecretUpdate, PaymentSettingsRecord, PaymentSettingsRepository,
    PaymentSettingsRepositoryError, PaymentSettingsWriteRecord,
};
use thiserror::Error;
use tokio::sync::Mutex;
use url::Url;

use crate::{SessionPrincipal, SessionRole};

const MAX_GATEWAY_URL_BYTES: usize = 2_048;
const MAX_MERCHANT_ID_BYTES: usize = 128;

/// 管理员可见的在线支付设置，服务端密钥只暴露是否已配置。
#[derive(Clone, Eq, PartialEq)]
pub struct AdminPaymentSettings {
    record: PaymentSettingsRecord,
}

impl AdminPaymentSettings {
    fn from_record(record: PaymentSettingsRecord) -> Self {
        Self { record }
    }

    #[must_use]
    pub const fn stripe_enabled(&self) -> bool {
        self.record.stripe_enabled()
    }

    #[must_use]
    pub fn stripe_publishable_key(&self) -> Option<&str> {
        self.record.stripe_publishable_key()
    }

    #[must_use]
    pub const fn stripe_secret_key_configured(&self) -> bool {
        self.record.stripe_secret_key_configured()
    }

    #[must_use]
    pub const fn stripe_webhook_secret_configured(&self) -> bool {
        self.record.stripe_webhook_secret_configured()
    }

    #[must_use]
    pub const fn stripe_signature_tolerance_seconds(&self) -> u16 {
        self.record.stripe_signature_tolerance_seconds()
    }

    #[must_use]
    pub const fn epay_enabled(&self) -> bool {
        self.record.epay_enabled()
    }

    #[must_use]
    pub fn epay_gateway_url(&self) -> Option<&str> {
        self.record.epay_gateway_url()
    }

    #[must_use]
    pub fn epay_merchant_id(&self) -> Option<&str> {
        self.record.epay_merchant_id()
    }

    #[must_use]
    pub const fn epay_merchant_key_configured(&self) -> bool {
        self.record.epay_merchant_key_configured()
    }

    #[must_use]
    pub const fn epay_alipay_enabled(&self) -> bool {
        self.record.epay_alipay_enabled()
    }

    #[must_use]
    pub const fn epay_wxpay_enabled(&self) -> bool {
        self.record.epay_wxpay_enabled()
    }

    #[must_use]
    pub const fn epay_qr_enabled(&self) -> bool {
        self.record.epay_qr_enabled()
    }

    #[must_use]
    pub const fn epay_refund_enabled(&self) -> bool {
        self.record.epay_refund_enabled()
    }

    #[must_use]
    pub const fn refund_auto_submit_enabled(&self) -> bool {
        self.record.refund_auto_submit_enabled()
    }

    #[must_use]
    pub const fn epay_quota_per_cny(&self) -> i64 {
        self.record.epay_quota_per_cny()
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.record.version()
    }
}

impl fmt::Debug for AdminPaymentSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminPaymentSettings(<已脱敏>)")
    }
}

/// 管理员完整保存支付设置的命令；密钥留空表示保留，清除必须显式声明。
pub struct AdminPaymentSettingsCommand {
    expected_version: i64,
    stripe_enabled: bool,
    stripe_publishable_key: Option<String>,
    stripe_secret_key: Option<PlainSystemSecret>,
    clear_stripe_secret_key: bool,
    stripe_webhook_secret: Option<PlainSystemSecret>,
    clear_stripe_webhook_secret: bool,
    stripe_signature_tolerance_seconds: u16,
    epay_enabled: bool,
    epay_gateway_url: Option<String>,
    epay_merchant_id: Option<String>,
    epay_merchant_key: Option<PlainSystemSecret>,
    clear_epay_merchant_key: bool,
    epay_alipay_enabled: bool,
    epay_wxpay_enabled: bool,
    epay_qr_enabled: bool,
    epay_refund_enabled: bool,
    refund_auto_submit_enabled: bool,
    epay_quota_per_cny: i64,
}

impl AdminPaymentSettingsCommand {
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理员支付设置契约一一对应"
    )]
    pub fn new(
        expected_version: i64,
        stripe_enabled: bool,
        stripe_publishable_key: Option<String>,
        stripe_secret_key: Option<String>,
        clear_stripe_secret_key: bool,
        stripe_webhook_secret: Option<String>,
        clear_stripe_webhook_secret: bool,
        stripe_signature_tolerance_seconds: u16,
        epay_enabled: bool,
        epay_gateway_url: Option<String>,
        epay_merchant_id: Option<String>,
        epay_merchant_key: Option<String>,
        clear_epay_merchant_key: bool,
        epay_alipay_enabled: bool,
        epay_wxpay_enabled: bool,
        epay_qr_enabled: bool,
        epay_refund_enabled: bool,
        refund_auto_submit_enabled: bool,
        epay_quota_per_cny: i64,
    ) -> Result<Self, AdminPaymentSettingsError> {
        let stripe_publishable_key = normalize_optional(stripe_publishable_key);
        let stripe_secret_key = normalize_secret(stripe_secret_key)?;
        let stripe_webhook_secret = normalize_secret(stripe_webhook_secret)?;
        let epay_gateway_url = normalize_gateway_url(epay_gateway_url)?;
        let epay_merchant_id = normalize_optional(epay_merchant_id);
        let epay_merchant_key = normalize_secret(epay_merchant_key)?;
        if expected_version < 1
            || (stripe_secret_key.is_some() && clear_stripe_secret_key)
            || (stripe_webhook_secret.is_some() && clear_stripe_webhook_secret)
            || (epay_merchant_key.is_some() && clear_epay_merchant_key)
            || epay_merchant_id.as_deref().is_some_and(|value| {
                value.len() > MAX_MERCHANT_ID_BYTES
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            })
        {
            return Err(AdminPaymentSettingsError::InvalidInput);
        }
        Ok(Self {
            expected_version,
            stripe_enabled,
            stripe_publishable_key,
            stripe_secret_key,
            clear_stripe_secret_key,
            stripe_webhook_secret,
            clear_stripe_webhook_secret,
            stripe_signature_tolerance_seconds,
            epay_enabled,
            epay_gateway_url,
            epay_merchant_id,
            epay_merchant_key,
            clear_epay_merchant_key,
            epay_alipay_enabled,
            epay_wxpay_enabled,
            epay_qr_enabled,
            epay_refund_enabled,
            refund_auto_submit_enabled,
            epay_quota_per_cny,
        })
    }

    fn into_record(
        self,
        cipher: &SystemSecretCipher,
    ) -> Result<PaymentSettingsWriteRecord, AdminPaymentSettingsError> {
        Ok(PaymentSettingsWriteRecord::new(
            self.expected_version,
            self.stripe_enabled,
            self.stripe_publishable_key,
            secret_update(
                self.stripe_secret_key.as_ref(),
                self.clear_stripe_secret_key,
                SystemSecretKind::StripeSecretKey,
                cipher,
            )?,
            secret_update(
                self.stripe_webhook_secret.as_ref(),
                self.clear_stripe_webhook_secret,
                SystemSecretKind::StripeWebhookSecret,
                cipher,
            )?,
            self.stripe_signature_tolerance_seconds,
            self.epay_enabled,
            self.epay_gateway_url,
            self.epay_merchant_id,
            secret_update(
                self.epay_merchant_key.as_ref(),
                self.clear_epay_merchant_key,
                SystemSecretKind::EpayMerchantKey,
                cipher,
            )?,
            self.epay_alipay_enabled,
            self.epay_wxpay_enabled,
            self.epay_qr_enabled,
            self.epay_refund_enabled,
            self.refund_auto_submit_enabled,
            self.epay_quota_per_cny,
        ))
    }
}

impl fmt::Debug for AdminPaymentSettingsCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminPaymentSettingsCommand(<已脱敏>)")
    }
}

/// 支付运行时应用端口；实现负责验证站点回调地址并原子切换 Provider 快照。
pub trait PaymentSettingsRuntimeApplier: Send + Sync {
    fn apply<'a>(
        &'a self,
        record: &'a PaymentSettingsRecord,
        cipher: &'a SystemSecretCipher,
    ) -> PaymentSettingsApplyFuture<'a>;
}

pub type PaymentSettingsApplyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), PaymentSettingsRuntimeError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PaymentSettingsRuntimeError {
    #[error("支付运行时配置无效")]
    InvalidConfiguration,
    #[error("支付运行时设置应用失败")]
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminPaymentSettingsError {
    #[error("支付设置输入无效")]
    InvalidInput,
    #[error("当前会话无权管理支付设置")]
    Forbidden,
    #[error("支付设置服务内部失败")]
    Internal,
}

pub type AdminPaymentSettingsReadFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminPaymentSettings, AdminPaymentSettingsError>> + Send + 'a>,
>;
pub type AdminPaymentSettingsUpdateFuture<'a> = AdminPaymentSettingsReadFuture<'a>;

/// 管理员在线支付设置用例端口。
pub trait AdminPaymentSettingsService: Send + Sync {
    fn settings<'a>(&'a self, principal: SessionPrincipal) -> AdminPaymentSettingsReadFuture<'a>;
    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminPaymentSettingsCommand,
    ) -> AdminPaymentSettingsUpdateFuture<'a>;
}

/// 使用数据库、系统密钥和热更新运行时实现支付设置管理。
pub struct DatabaseAdminPaymentSettingsService {
    repository: PaymentSettingsRepository,
    cipher: SystemSecretCipher,
    runtime: Arc<dyn PaymentSettingsRuntimeApplier>,
    update_lock: Arc<Mutex<()>>,
}

impl DatabaseAdminPaymentSettingsService {
    #[must_use]
    pub fn new(
        repository: PaymentSettingsRepository,
        cipher: SystemSecretCipher,
        runtime: Arc<dyn PaymentSettingsRuntimeApplier>,
    ) -> Self {
        Self {
            repository,
            cipher,
            runtime,
            update_lock: Arc::new(Mutex::new(())),
        }
    }

    /// 服务启动时应用数据库中的当前快照。
    pub async fn apply_current(&self) -> Result<(), AdminPaymentSettingsError> {
        let record = self
            .repository
            .settings()
            .await
            .map_err(map_repository_error)?;
        self.runtime
            .apply(&record, &self.cipher)
            .await
            .map_err(|_| AdminPaymentSettingsError::Internal)
    }
}

impl AdminPaymentSettingsService for DatabaseAdminPaymentSettingsService {
    fn settings<'a>(&'a self, principal: SessionPrincipal) -> AdminPaymentSettingsReadFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .settings()
                .await
                .map(AdminPaymentSettings::from_record)
                .map_err(map_repository_error)
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminPaymentSettingsCommand,
    ) -> AdminPaymentSettingsUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            // 数据库保存、运行时切换和失败补偿必须串行，避免交叉覆盖快照。
            let _guard = self.update_lock.lock().await;
            let previous = self
                .repository
                .settings()
                .await
                .map_err(map_repository_error)?;
            let saved = self
                .repository
                .update(command.into_record(&self.cipher)?)
                .await
                .map_err(map_repository_error)?;
            if let Err(runtime_error) = self.runtime.apply(&saved, &self.cipher).await {
                let restore = self
                    .repository
                    .restore_if_version(saved.version(), &previous)
                    .await;
                let reconciled = match restore {
                    Ok(()) => self.runtime.apply(&previous, &self.cipher).await.is_ok(),
                    Err(_) => {
                        // 其他实例可能已经提交更新，此时只能追随数据库最新快照，不能回退到更旧版本。
                        match self.repository.settings().await {
                            Ok(current) => self.runtime.apply(&current, &self.cipher).await.is_ok(),
                            Err(_) => false,
                        }
                    }
                };
                if !reconciled {
                    tracing::error!("支付设置运行时切换失败且无法与数据库快照重新同步");
                    return Err(AdminPaymentSettingsError::Internal);
                }
                return Err(match runtime_error {
                    PaymentSettingsRuntimeError::InvalidConfiguration => {
                        AdminPaymentSettingsError::InvalidInput
                    }
                    PaymentSettingsRuntimeError::Failed => AdminPaymentSettingsError::Internal,
                });
            }
            Ok(AdminPaymentSettings::from_record(saved))
        })
    }
}

fn secret_update(
    secret: Option<&PlainSystemSecret>,
    clear: bool,
    kind: SystemSecretKind,
    cipher: &SystemSecretCipher,
) -> Result<PaymentSecretUpdate, AdminPaymentSettingsError> {
    if clear {
        return Ok(PaymentSecretUpdate::Clear);
    }
    secret.map_or(Ok(PaymentSecretUpdate::Keep), |secret| {
        cipher
            .encrypt(kind, secret)
            .map(PaymentSecretUpdate::Replace)
            .map_err(|_| AdminPaymentSettingsError::Internal)
    })
}

fn normalize_secret(
    value: Option<String>,
) -> Result<Option<PlainSystemSecret>, AdminPaymentSettingsError> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    PlainSystemSecret::new(value)
        .map(Some)
        .map_err(|_| AdminPaymentSettingsError::InvalidInput)
}

fn normalize_gateway_url(
    value: Option<String>,
) -> Result<Option<String>, AdminPaymentSettingsError> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    if value.len() > MAX_GATEWAY_URL_BYTES {
        return Err(AdminPaymentSettingsError::InvalidInput);
    }
    let mut url = Url::parse(&value).map_err(|_| AdminPaymentSettingsError::InvalidInput)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AdminPaymentSettingsError::InvalidInput);
    }
    let path = url.path().trim_end_matches('/');
    let path = ["/submit.php", "/mapi.php", "/api.php"]
        .into_iter()
        .find_map(|suffix| path.strip_suffix(suffix))
        .unwrap_or(path)
        .trim_end_matches('/')
        .to_owned();
    url.set_path(&path);
    Ok(Some(url.to_string().trim_end_matches('/').to_owned()))
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminPaymentSettingsError> {
    if principal.role() != SessionRole::Admin {
        return Err(AdminPaymentSettingsError::Forbidden);
    }
    Ok(())
}

fn map_repository_error(error: PaymentSettingsRepositoryError) -> AdminPaymentSettingsError {
    match error {
        PaymentSettingsRepositoryError::InvalidSettings => AdminPaymentSettingsError::InvalidInput,
        PaymentSettingsRepositoryError::Query
        | PaymentSettingsRepositoryError::Timeout
        | PaymentSettingsRepositoryError::Invariant
        | PaymentSettingsRepositoryError::ConcurrentUpdate => AdminPaymentSettingsError::Internal,
    }
}
