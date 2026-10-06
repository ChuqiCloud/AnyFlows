use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_account::{DecryptedSystemSecret, PlainSystemSecret, SystemSecretCipher, SystemSecretKind};
use af_db::{
    DatabaseTimestamp, EmailPasswordUpdate, EmailSettingsRecord, EmailSettingsRepository,
    EmailSettingsRepositoryError, EmailSettingsWriteRecord, EmailTlsMode,
};
use af_domain::Quota;
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{SessionPrincipal, SessionRole};

const MAX_HOST_BYTES: usize = 255;
const MAX_EMAIL_BYTES: usize = 320;
const MAX_FROM_NAME_BYTES: usize = 128;
const MIN_TIMEOUT_SECONDS: u16 = 1;
const MAX_TIMEOUT_SECONDS: u16 = 60;
const TEST_EMAIL_SUBJECT: &str = "AnyFlows SMTP 配置测试";
const TEST_EMAIL_BODY: &str =
    "这是一封由 AnyFlows 发送的 SMTP 配置测试邮件。收到此邮件表示当前邮件服务器配置可用。";
const REGISTRATION_EMAIL_SUBJECT: &str = "AnyFlows 注册验证码";
const EMAIL_BINDING_SUBJECT: &str = "AnyFlows 邮箱绑定验证码";
const PASSWORD_RESET_EMAIL_SUBJECT: &str = "AnyFlows 密码重置";

/// 管理 API 使用的 SMTP TLS 模式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminEmailTlsMode {
    StartTls,
    Tls,
}

impl From<AdminEmailTlsMode> for EmailTlsMode {
    fn from(value: AdminEmailTlsMode) -> Self {
        match value {
            AdminEmailTlsMode::StartTls => Self::StartTls,
            AdminEmailTlsMode::Tls => Self::Tls,
        }
    }
}

impl From<EmailTlsMode> for AdminEmailTlsMode {
    fn from(value: EmailTlsMode) -> Self {
        match value {
            EmailTlsMode::StartTls => Self::StartTls,
            EmailTlsMode::Tls => Self::Tls,
        }
    }
}

/// 管理员可读取的 SMTP 设置投影，永不包含密码密文或明文。
#[derive(Clone, Eq, PartialEq)]
pub struct AdminEmailSettings {
    enabled: bool,
    host: String,
    port: u16,
    tls_mode: AdminEmailTlsMode,
    username: Option<String>,
    password_configured: bool,
    from_address: String,
    from_name: Option<String>,
    reply_to: Option<String>,
    timeout_seconds: u16,
    version: i64,
    delivery_ready: bool,
}

impl AdminEmailSettings {
    /// 为替代应用服务实现构造经过校验的脱敏设置投影。
    #[allow(clippy::too_many_arguments, reason = "字段与 SMTP 设置投影一一对应")]
    pub fn new(
        enabled: bool,
        host: String,
        port: u16,
        tls_mode: AdminEmailTlsMode,
        username: Option<String>,
        password_configured: bool,
        from_address: String,
        from_name: Option<String>,
        reply_to: Option<String>,
        timeout_seconds: u16,
        version: i64,
    ) -> Result<Self, AdminEmailSettingsError> {
        let username = normalize_optional(username);
        let from_name = normalize_optional(from_name);
        let reply_to = normalize_optional(reply_to);
        if port == 0
            || version <= 0
            || !(MIN_TIMEOUT_SECONDS..=MAX_TIMEOUT_SECONDS).contains(&timeout_seconds)
            || (!host.is_empty() && !is_valid_host(&host))
            || (!from_address.is_empty() && !is_valid_email_address(&from_address))
            || !valid_optional_text(username.as_deref(), MAX_EMAIL_BYTES)
            || !valid_optional_text(from_name.as_deref(), MAX_FROM_NAME_BYTES)
            || reply_to
                .as_deref()
                .is_some_and(|value| !is_valid_email_address(value))
            || (enabled && (host.is_empty() || from_address.is_empty()))
            || username.is_some() != password_configured
        {
            return Err(AdminEmailSettingsError::InvalidInput);
        }
        let delivery_ready = enabled && !host.is_empty() && !from_address.is_empty();
        Ok(Self {
            enabled,
            host,
            port,
            tls_mode,
            username,
            password_configured,
            from_address,
            from_name,
            reply_to,
            timeout_seconds,
            version,
            delivery_ready,
        })
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub const fn tls_mode(&self) -> AdminEmailTlsMode {
        self.tls_mode
    }

    #[must_use]
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    #[must_use]
    pub const fn password_configured(&self) -> bool {
        self.password_configured
    }

    #[must_use]
    pub fn from_address(&self) -> &str {
        &self.from_address
    }

    #[must_use]
    pub fn from_name(&self) -> Option<&str> {
        self.from_name.as_deref()
    }

    #[must_use]
    pub fn reply_to(&self) -> Option<&str> {
        self.reply_to.as_deref()
    }

    #[must_use]
    pub const fn timeout_seconds(&self) -> u16 {
        self.timeout_seconds
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    #[must_use]
    pub const fn delivery_ready(&self) -> bool {
        self.delivery_ready
    }

    fn from_record(record: &EmailSettingsRecord) -> Self {
        Self {
            enabled: record.enabled(),
            host: record.host().to_owned(),
            port: record.port(),
            tls_mode: record.tls_mode().into(),
            username: record.username().map(str::to_owned),
            password_configured: record.password_configured(),
            from_address: record.from_address().to_owned(),
            from_name: record.from_name().map(str::to_owned),
            reply_to: record.reply_to().map(str::to_owned),
            timeout_seconds: record.timeout_seconds(),
            version: record.version(),
            delivery_ready: record.delivery_ready(),
        }
    }
}

impl fmt::Debug for AdminEmailSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminEmailSettings")
            .field("enabled", &self.enabled)
            .field("host", &"<已脱敏>")
            .field("port", &self.port)
            .field("tls_mode", &self.tls_mode)
            .field("username", &self.username.as_ref().map(|_| "<已脱敏>"))
            .field("password_configured", &self.password_configured)
            .field("from_address", &"<已脱敏>")
            .field("from_name", &self.from_name.as_ref().map(|_| "<已脱敏>"))
            .field("reply_to", &self.reply_to.as_ref().map(|_| "<已脱敏>"))
            .field("timeout_seconds", &self.timeout_seconds)
            .field("version", &self.version)
            .field("delivery_ready", &self.delivery_ready)
            .finish()
    }
}

/// 管理员完整保存 SMTP 设置的命令，密码缺失表示保留现有密文。
pub struct AdminEmailSettingsCommand {
    enabled: bool,
    host: String,
    port: u16,
    tls_mode: AdminEmailTlsMode,
    username: Option<String>,
    password: Option<PlainSystemSecret>,
    from_address: String,
    from_name: Option<String>,
    reply_to: Option<String>,
    timeout_seconds: u16,
}

impl AdminEmailSettingsCommand {
    /// 校验全部结构化字段；是否具备旧密码由持久化后的有效快照再次闭合。
    #[allow(clippy::too_many_arguments, reason = "字段与 SMTP 设置契约一一对应")]
    pub fn new(
        enabled: bool,
        host: String,
        port: u16,
        tls_mode: AdminEmailTlsMode,
        username: Option<String>,
        password: Option<String>,
        from_address: String,
        from_name: Option<String>,
        reply_to: Option<String>,
        timeout_seconds: u16,
    ) -> Result<Self, AdminEmailSettingsError> {
        let username = normalize_optional(username);
        let from_name = normalize_optional(from_name);
        let reply_to = normalize_optional(reply_to);
        if port == 0
            || !(MIN_TIMEOUT_SECONDS..=MAX_TIMEOUT_SECONDS).contains(&timeout_seconds)
            || (!host.is_empty() && !is_valid_host(&host))
            || (!from_address.is_empty() && !is_valid_email_address(&from_address))
            || !valid_optional_text(username.as_deref(), MAX_EMAIL_BYTES)
            || !valid_optional_text(from_name.as_deref(), MAX_FROM_NAME_BYTES)
            || reply_to
                .as_deref()
                .is_some_and(|value| !is_valid_email_address(value))
            || (enabled && (host.is_empty() || from_address.is_empty()))
            || (username.is_none() && password.is_some())
        {
            return Err(AdminEmailSettingsError::InvalidInput);
        }
        let password = password
            .map(PlainSystemSecret::new)
            .transpose()
            .map_err(|_| AdminEmailSettingsError::InvalidInput)?;
        Ok(Self {
            enabled,
            host,
            port,
            tls_mode,
            username,
            password,
            from_address,
            from_name,
            reply_to,
            timeout_seconds,
        })
    }

    fn into_record(
        self,
        cipher: &SystemSecretCipher,
    ) -> Result<EmailSettingsWriteRecord, AdminEmailSettingsError> {
        let password_update = if self.username.is_none() {
            EmailPasswordUpdate::Clear
        } else if let Some(password) = self.password.as_ref() {
            EmailPasswordUpdate::Replace(
                cipher
                    .encrypt(SystemSecretKind::SmtpPassword, password)
                    .map_err(|_| AdminEmailSettingsError::Internal)?,
            )
        } else {
            EmailPasswordUpdate::Keep
        };
        Ok(EmailSettingsWriteRecord::new(
            self.enabled,
            self.host,
            self.port,
            self.tls_mode.into(),
            self.username,
            password_update,
            self.from_address,
            self.from_name,
            self.reply_to,
            self.timeout_seconds,
        ))
    }
}

impl fmt::Debug for AdminEmailSettingsCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminEmailSettingsCommand(<已脱敏>)")
    }
}

/// 发送测试邮件的目标地址命令。
pub struct AdminEmailTestCommand {
    recipient: String,
}

impl AdminEmailTestCommand {
    pub fn new(recipient: String) -> Result<Self, AdminEmailSettingsError> {
        if !is_valid_email_address(&recipient) {
            return Err(AdminEmailSettingsError::InvalidInput);
        }
        Ok(Self { recipient })
    }
}

impl fmt::Debug for AdminEmailTestCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminEmailTestCommand(<已脱敏>)")
    }
}

/// 应用服务交给 SMTP 适配器的完整投递请求。
pub struct EmailDeliveryRequest {
    host: String,
    port: u16,
    tls_mode: AdminEmailTlsMode,
    username: Option<String>,
    password: Option<DecryptedSystemSecret>,
    from_address: String,
    from_name: Option<String>,
    reply_to: Option<String>,
    recipient: String,
    subject: String,
    body: Zeroizing<String>,
    timeout_seconds: u16,
}

impl EmailDeliveryRequest {
    fn from_record(
        record: &EmailSettingsRecord,
        cipher: &SystemSecretCipher,
        recipient: String,
        subject: String,
        body: String,
    ) -> Result<Self, AdminEmailSettingsError> {
        if !record.delivery_ready() || !is_valid_email_address(&recipient) {
            return Err(AdminEmailSettingsError::NotConfigured);
        }
        let password = record
            .password_secret()
            .map(|secret| {
                cipher
                    .decrypt(SystemSecretKind::SmtpPassword, secret)
                    .map_err(|_| AdminEmailSettingsError::Internal)
            })
            .transpose()?;
        Ok(Self {
            host: record.host().to_owned(),
            port: record.port(),
            tls_mode: record.tls_mode().into(),
            username: record.username().map(str::to_owned),
            password,
            from_address: record.from_address().to_owned(),
            from_name: record.from_name().map(str::to_owned),
            reply_to: record.reply_to().map(str::to_owned),
            recipient,
            subject,
            body: Zeroizing::new(body),
            timeout_seconds: record.timeout_seconds(),
        })
    }

    pub(crate) fn test(
        record: &EmailSettingsRecord,
        cipher: &SystemSecretCipher,
        recipient: String,
    ) -> Result<Self, AdminEmailSettingsError> {
        Self::from_record(
            record,
            cipher,
            recipient,
            TEST_EMAIL_SUBJECT.to_owned(),
            TEST_EMAIL_BODY.to_owned(),
        )
    }

    pub(crate) fn registration_verification(
        record: &EmailSettingsRecord,
        cipher: &SystemSecretCipher,
        recipient: String,
        code: &str,
        ttl_seconds: u64,
    ) -> Result<Self, AdminEmailSettingsError> {
        if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(AdminEmailSettingsError::Internal);
        }
        let ttl_minutes = ttl_seconds.div_ceil(60);
        let body = format!(
            "您的 AnyFlows 注册验证码是：{code}\n\n验证码将在 {ttl_minutes} 分钟后失效。若非本人操作，请忽略此邮件。"
        );
        Self::from_record(
            record,
            cipher,
            recipient,
            REGISTRATION_EMAIL_SUBJECT.to_owned(),
            body,
        )
    }

    pub(crate) fn email_binding_verification(
        record: &EmailSettingsRecord,
        cipher: &SystemSecretCipher,
        recipient: String,
        code: &str,
        ttl_seconds: u64,
    ) -> Result<Self, AdminEmailSettingsError> {
        if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(AdminEmailSettingsError::Internal);
        }
        let ttl_minutes = ttl_seconds.div_ceil(60);
        let body = format!(
            "您的 AnyFlows 邮箱绑定验证码是：{code}\n\n验证码将在 {ttl_minutes} 分钟后失效。若非本人操作，请忽略此邮件。"
        );
        Self::from_record(
            record,
            cipher,
            recipient,
            EMAIL_BINDING_SUBJECT.to_owned(),
            body,
        )
    }

    pub(crate) fn password_reset(
        record: &EmailSettingsRecord,
        cipher: &SystemSecretCipher,
        recipient: String,
        site_name: &str,
        reset_url: &str,
        ttl_seconds: u64,
    ) -> Result<Self, AdminEmailSettingsError> {
        if site_name.is_empty()
            || site_name.chars().any(char::is_control)
            || reset_url.is_empty()
            || reset_url.len() > 4_096
            || reset_url.chars().any(char::is_control)
        {
            return Err(AdminEmailSettingsError::Internal);
        }
        let ttl_minutes = ttl_seconds.div_ceil(60);
        let body = format!(
            "您好，您正在重置 {site_name} 的登录密码。\n\n请在 {ttl_minutes} 分钟内打开以下链接完成重置：\n{reset_url}\n\n若非本人操作，请忽略此邮件。"
        );
        Self::from_record(
            record,
            cipher,
            recipient,
            PASSWORD_RESET_EMAIL_SUBJECT.to_owned(),
            body,
        )
    }

    pub(crate) fn balance_alert(
        record: &EmailSettingsRecord,
        cipher: &SystemSecretCipher,
        recipient: String,
        site_name: &str,
        username: &str,
        current_quota: Quota,
        threshold: Quota,
    ) -> Result<Self, AdminEmailSettingsError> {
        if site_name.is_empty()
            || site_name.len() > 80
            || username.is_empty()
            || username.len() > 64
            || site_name.chars().any(char::is_control)
            || username.chars().any(char::is_control)
            || threshold.is_zero()
            || current_quota >= threshold
        {
            return Err(AdminEmailSettingsError::Internal);
        }
        let subject = format!("{site_name} 余额预警");
        let body = format!(
            "您好，{username}：\n\n您的当前可用额度为 {}，已低于提醒阈值 {}。请及时检查账户余额，以免 API 请求受到影响。\n\n如不希望继续接收此类邮件，可在个人资料的通知偏好中关闭用量提醒。",
            current_quota.units(),
            threshold.units()
        );
        Self::from_record(record, cipher, recipient, subject, body)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "显式传入已校验的订阅与投递快照，避免隐藏跨层依赖"
    )]
    pub(crate) fn subscription_balance_alert(
        record: &EmailSettingsRecord,
        cipher: &SystemSecretCipher,
        recipient: String,
        site_name: &str,
        username: &str,
        plan_name: &str,
        quota_amount: Quota,
        quota_used: Quota,
        window_ends_at: DatabaseTimestamp,
        threshold_percent: i16,
    ) -> Result<Self, AdminEmailSettingsError> {
        if site_name.is_empty()
            || site_name.len() > 80
            || username.is_empty()
            || username.len() > 64
            || plan_name.is_empty()
            || plan_name.len() > 80
            || site_name.chars().any(char::is_control)
            || username.chars().any(char::is_control)
            || plan_name.chars().any(char::is_control)
            || quota_amount.is_zero()
            || quota_used > quota_amount
            || !(1..=99).contains(&threshold_percent)
        {
            return Err(AdminEmailSettingsError::Internal);
        }
        let remaining = quota_amount
            .units()
            .checked_sub(quota_used.units())
            .ok_or(AdminEmailSettingsError::Internal)?;
        let threshold = subscription_remaining_threshold(quota_amount.units(), threshold_percent)?;
        if remaining > threshold {
            return Err(AdminEmailSettingsError::Internal);
        }
        let window_end = format!(
            "{} {:02}:{:02} UTC",
            window_ends_at.date(),
            window_ends_at.hour(),
            window_ends_at.minute()
        );
        let subject = format!("{site_name} 订阅额度预警");
        let body = format!(
            "您好，{username}：\n\n您的订阅计划“{plan_name}”当前剩余额度为 {remaining} / {}，已达到剩余 {threshold_percent}% 的提醒边界。本窗口将在 {window_end} 结束，请及时检查后续用量安排。\n\n同一订阅窗口最多发送一次此类提醒。如不希望继续接收，可在个人资料的通知偏好中关闭用量提醒。",
            quota_amount.units()
        );
        Self::from_record(record, cipher, recipient, subject, body)
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub const fn tls_mode(&self) -> AdminEmailTlsMode {
        self.tls_mode
    }

    #[must_use]
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    #[must_use]
    pub fn password(&self) -> Option<&str> {
        self.password
            .as_ref()
            .map(DecryptedSystemSecret::expose_secret)
    }

    #[must_use]
    pub fn from_address(&self) -> &str {
        &self.from_address
    }

    #[must_use]
    pub fn from_name(&self) -> Option<&str> {
        self.from_name.as_deref()
    }

    #[must_use]
    pub fn reply_to(&self) -> Option<&str> {
        self.reply_to.as_deref()
    }

    #[must_use]
    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    #[must_use]
    pub const fn timeout_seconds(&self) -> u16 {
        self.timeout_seconds
    }
}

/// 按向下取整语义计算订阅窗口的剩余额度阈值，并避免整数乘法溢出。
fn subscription_remaining_threshold(
    quota_amount: i64,
    threshold_percent: i16,
) -> Result<i64, AdminEmailSettingsError> {
    let percent = i64::from(threshold_percent);
    let whole = quota_amount
        .div_euclid(100)
        .checked_mul(percent)
        .ok_or(AdminEmailSettingsError::Internal)?;
    let remainder = quota_amount
        .rem_euclid(100)
        .checked_mul(percent)
        .ok_or(AdminEmailSettingsError::Internal)?
        .div_euclid(100);
    whole
        .checked_add(remainder)
        .ok_or(AdminEmailSettingsError::Internal)
}

impl fmt::Debug for EmailDeliveryRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmailDeliveryRequest")
            .field("host", &"<已脱敏>")
            .field("port", &self.port)
            .field("tls_mode", &self.tls_mode)
            .field("authenticated", &self.username.is_some())
            .field("from_address", &"<已脱敏>")
            .field("recipient", &"<已脱敏>")
            .field("timeout_seconds", &self.timeout_seconds)
            .finish_non_exhaustive()
    }
}

/// SMTP 适配器的固定失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EmailDeliveryError {
    #[error("邮件投递配置无效")]
    InvalidConfiguration,
    #[error("邮件投递超时")]
    Timeout,
    #[error("邮件投递失败")]
    Failed,
}

pub type EmailDeliveryFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), EmailDeliveryError>> + Send + 'a>>;

/// 邮件投递端口；实现不得把请求内容或底层服务器响应写入日志。
pub trait EmailDelivery: Send + Sync {
    fn send<'a>(&'a self, request: EmailDeliveryRequest) -> EmailDeliveryFuture<'a>;
}

/// 管理员邮件设置应用服务错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminEmailSettingsError {
    #[error("SMTP 设置输入无效")]
    InvalidInput,
    #[error("当前会话无权管理 SMTP 设置")]
    Forbidden,
    #[error("SMTP 设置尚未完整配置")]
    NotConfigured,
    #[error("测试邮件投递失败")]
    DeliveryFailed,
    #[error("SMTP 设置服务内部错误")]
    Internal,
}

pub type AdminEmailSettingsReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminEmailSettings, AdminEmailSettingsError>> + Send + 'a>>;
pub type AdminEmailSettingsUpdateFuture<'a> = AdminEmailSettingsReadFuture<'a>;
pub type AdminEmailTestFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminEmailSettingsError>> + Send + 'a>>;

/// 管理员 SMTP 设置与测试邮件用例端口。
pub trait AdminEmailSettingsService: Send + Sync {
    fn settings<'a>(&'a self, principal: SessionPrincipal) -> AdminEmailSettingsReadFuture<'a>;
    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminEmailSettingsCommand,
    ) -> AdminEmailSettingsUpdateFuture<'a>;
    fn send_test<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminEmailTestCommand,
    ) -> AdminEmailTestFuture<'a>;
}

/// 使用数据库、系统密钥和 SMTP 适配器实现管理员邮件设置用例。
pub struct DatabaseAdminEmailSettingsService {
    repository: EmailSettingsRepository,
    cipher: SystemSecretCipher,
    delivery: Arc<dyn EmailDelivery>,
}

impl DatabaseAdminEmailSettingsService {
    #[must_use]
    pub fn new(
        repository: EmailSettingsRepository,
        cipher: SystemSecretCipher,
        delivery: Arc<dyn EmailDelivery>,
    ) -> Self {
        Self {
            repository,
            cipher,
            delivery,
        }
    }
}

impl AdminEmailSettingsService for DatabaseAdminEmailSettingsService {
    fn settings<'a>(&'a self, principal: SessionPrincipal) -> AdminEmailSettingsReadFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .settings()
                .await
                .map(|record| AdminEmailSettings::from_record(&record))
                .map_err(map_repository_error)
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminEmailSettingsCommand,
    ) -> AdminEmailSettingsUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let record = command.into_record(&self.cipher)?;
            self.repository
                .update(record)
                .await
                .map(|record| AdminEmailSettings::from_record(&record))
                .map_err(map_repository_error)
        })
    }

    fn send_test<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminEmailTestCommand,
    ) -> AdminEmailTestFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let record = self
                .repository
                .settings()
                .await
                .map_err(map_repository_error)?;
            let request = EmailDeliveryRequest::test(&record, &self.cipher, command.recipient)?;
            self.delivery
                .send(request)
                .await
                .map_err(|_| AdminEmailSettingsError::DeliveryFailed)
        })
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminEmailSettingsError> {
    if principal.role() != SessionRole::Admin {
        return Err(AdminEmailSettingsError::Forbidden);
    }
    Ok(())
}

fn map_repository_error(error: EmailSettingsRepositoryError) -> AdminEmailSettingsError {
    match error {
        EmailSettingsRepositoryError::InvalidSettings => AdminEmailSettingsError::InvalidInput,
        EmailSettingsRepositoryError::Query
        | EmailSettingsRepositoryError::Timeout
        | EmailSettingsRepositoryError::Invariant => AdminEmailSettingsError::Internal,
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn is_valid_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_HOST_BYTES
        && value.trim() == value
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        && !value.contains("://")
        && !value.contains(['/', '\\', '@'])
}

fn is_valid_email_address(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_EMAIL_BYTES
        || value.trim() != value
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return false;
    }
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    !local.is_empty()
        && !domain.is_empty()
        && parts.next().is_none()
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

fn valid_optional_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= maximum_bytes
            && value.trim() == value
            && !value.chars().any(char::is_control)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_structured_settings_without_leaking_fields() {
        let command = AdminEmailSettingsCommand::new(
            true,
            "smtp.example.com".to_owned(),
            587,
            AdminEmailTlsMode::StartTls,
            Some("mailer@example.com".to_owned()),
            Some("secret password".to_owned()),
            "from@example.com".to_owned(),
            Some("AnyFlows".to_owned()),
            Some("reply@example.com".to_owned()),
            10,
        )
        .unwrap();
        let rendered = format!("{command:?}");
        for private in ["smtp.example.com", "mailer@example.com", "secret password"] {
            assert!(!rendered.contains(private));
        }
        assert!(
            AdminEmailSettingsCommand::new(
                true,
                "http://smtp.example.com".to_owned(),
                587,
                AdminEmailTlsMode::StartTls,
                None,
                None,
                "from@example.com".to_owned(),
                None,
                None,
                10,
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_password_without_authentication_user() {
        assert!(
            AdminEmailSettingsCommand::new(
                false,
                "smtp.example.com".to_owned(),
                465,
                AdminEmailTlsMode::Tls,
                None,
                Some("secret".to_owned()),
                "from@example.com".to_owned(),
                None,
                None,
                10,
            )
            .is_err()
        );
        assert!(AdminEmailTestCommand::new("invalid".to_owned()).is_err());
    }
}
