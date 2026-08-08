use std::{fmt, sync::Arc};

use af_account::SystemSecretCipher;
use af_db::{
    BalanceAlertClaimOutcome, BalanceAlertCompletionOutcome, BalanceAlertDeliveryFailureKind,
    BalanceAlertRepository, BalanceAlertRepositoryError, BalanceAlertSettingsRepository,
    BalanceAlertSettingsRepositoryError, DatabaseTimestamp, EmailSettingsRepository,
    EmailSettingsRepositoryError, MAX_BALANCE_ALERT_BATCH_SIZE, SiteSettingsRepository,
    SiteSettingsRepositoryError, SubscriptionBalanceAlertClaimOutcome,
    SubscriptionBalanceAlertRepository, SubscriptionBalanceAlertRepositoryError,
};
use thiserror::Error;

use crate::{EmailDelivery, EmailDeliveryError, EmailDeliveryRequest};

const MAX_BALANCE_ALERT_DELIVERIES_PER_RUN: usize = 16;

/// 单轮余额预警扫描与投递统计，不包含用户身份或邮箱。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BalanceAlertTaskReport {
    enqueued: usize,
    sent: usize,
    retry_scheduled: usize,
    terminal_failed: usize,
    skipped: usize,
    stale_canceled: u64,
    exhausted_failed: u64,
}

impl BalanceAlertTaskReport {
    #[must_use]
    pub const fn enqueued(self) -> usize {
        self.enqueued
    }

    #[must_use]
    pub const fn sent(self) -> usize {
        self.sent
    }

    #[must_use]
    pub const fn retry_scheduled(self) -> usize {
        self.retry_scheduled
    }

    #[must_use]
    pub const fn terminal_failed(self) -> usize {
        self.terminal_failed
    }

    #[must_use]
    pub const fn skipped(self) -> usize {
        self.skipped
    }

    #[must_use]
    pub const fn stale_canceled(self) -> u64 {
        self.stale_canceled
    }

    #[must_use]
    pub const fn exhausted_failed(self) -> u64 {
        self.exhausted_failed
    }
}

/// 余额预警任务的基础设施错误；单封邮件失败由持久化队列吸收。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BalanceAlertTaskError {
    #[error("余额预警全局设置读取失败")]
    Settings,
    #[error("余额预警事件队列操作失败")]
    Queue,
    #[error("余额预警邮件设置读取失败")]
    EmailSettings,
    #[error("余额预警站点设置读取失败")]
    SiteSettings,
}

impl BalanceAlertTaskError {
    /// 返回可用于日志和指标聚合的稳定错误分类。
    #[must_use]
    pub const fn error_kind(self) -> &'static str {
        match self {
            Self::Settings => "balance_alert_settings",
            Self::Queue => "balance_alert_queue",
            Self::EmailSettings => "balance_alert_email_settings",
            Self::SiteSettings => "balance_alert_site_settings",
        }
    }
}

/// 执行一次余额预警发现、领取和 SMTP 投递的应用任务。
#[derive(Clone)]
pub struct BalanceAlertTask {
    settings: BalanceAlertSettingsRepository,
    queue: BalanceAlertRepository,
    subscription_queue: SubscriptionBalanceAlertRepository,
    email_settings: EmailSettingsRepository,
    site_settings: SiteSettingsRepository,
    cipher: SystemSecretCipher,
    delivery: Arc<dyn EmailDelivery>,
}

impl BalanceAlertTask {
    #[must_use]
    pub fn new(
        settings: BalanceAlertSettingsRepository,
        queue: BalanceAlertRepository,
        subscription_queue: SubscriptionBalanceAlertRepository,
        email_settings: EmailSettingsRepository,
        site_settings: SiteSettingsRepository,
        cipher: SystemSecretCipher,
        delivery: Arc<dyn EmailDelivery>,
    ) -> Self {
        Self {
            settings,
            queue,
            subscription_queue,
            email_settings,
            site_settings,
            cipher,
            delivery,
        }
    }

    /// 执行一轮任务；配置关闭时只清理过期窗口和耗尽租约。
    pub async fn run_once(&self) -> Result<BalanceAlertTaskReport, BalanceAlertTaskError> {
        let settings = self.settings.settings().await.map_err(map_settings_error)?;
        let now = DatabaseTimestamp::now_utc();
        let window_started_at_epoch = settings
            .window_started_at_epoch(now)
            .map_err(map_settings_error)?;
        let stale_canceled = self
            .queue
            .cancel_stale(window_started_at_epoch, now)
            .await
            .map_err(map_queue_error)?;
        let exhausted_failed = self
            .queue
            .fail_exhausted(now)
            .await
            .map_err(map_queue_error)?;
        let subscription_stale_canceled = self
            .subscription_queue
            .cancel_expired(now)
            .await
            .map_err(map_subscription_queue_error)?;
        let subscription_exhausted_failed = self
            .subscription_queue
            .fail_exhausted(now)
            .await
            .map_err(map_subscription_queue_error)?;
        let mut report = BalanceAlertTaskReport {
            stale_canceled: stale_canceled
                .checked_add(subscription_stale_canceled)
                .ok_or(BalanceAlertTaskError::Queue)?,
            exhausted_failed: exhausted_failed
                .checked_add(subscription_exhausted_failed)
                .ok_or(BalanceAlertTaskError::Queue)?,
            ..BalanceAlertTaskReport::default()
        };
        if !settings.enabled() && !settings.subscription_alert_enabled() {
            return Ok(report);
        }

        if settings.enabled() {
            report.enqueued = self
                .queue
                .enqueue_due(settings, now, MAX_BALANCE_ALERT_BATCH_SIZE)
                .await
                .map_err(map_queue_error)?
                .eligible();
        }
        if settings.subscription_alert_enabled() {
            report.enqueued = report
                .enqueued
                .checked_add(
                    self.subscription_queue
                        .enqueue_due(
                            settings.subscription_remaining_percent(),
                            now,
                            MAX_BALANCE_ALERT_BATCH_SIZE,
                        )
                        .await
                        .map_err(map_subscription_queue_error)?
                        .eligible(),
                )
                .ok_or(BalanceAlertTaskError::Queue)?;
        }
        let email_settings = self
            .email_settings
            .settings()
            .await
            .map_err(map_email_settings_error)?;
        let site_settings = self
            .site_settings
            .settings()
            .await
            .map_err(map_site_settings_error)?;

        if settings.enabled() {
            self.deliver_wallet_alerts(
                settings,
                window_started_at_epoch,
                &email_settings,
                site_settings.site_name(),
                &mut report,
            )
            .await?;
        }
        if settings.subscription_alert_enabled() {
            self.deliver_subscription_alerts(
                &email_settings,
                site_settings.site_name(),
                &mut report,
            )
            .await?;
        }
        Ok(report)
    }

    async fn deliver_wallet_alerts(
        &self,
        settings: af_db::BalanceAlertSettingsRecord,
        window_started_at_epoch: i64,
        email_settings: &af_db::EmailSettingsRecord,
        site_name: &str,
        report: &mut BalanceAlertTaskReport,
    ) -> Result<(), BalanceAlertTaskError> {
        for _ in 0..MAX_BALANCE_ALERT_DELIVERIES_PER_RUN {
            let claim_now = DatabaseTimestamp::now_utc();
            if settings
                .window_started_at_epoch(claim_now)
                .map_err(map_settings_error)?
                != window_started_at_epoch
            {
                break;
            }
            let lease = match self
                .queue
                .claim_next(settings, window_started_at_epoch, claim_now)
                .await
                .map_err(map_queue_error)?
            {
                BalanceAlertClaimOutcome::Claimed(lease) => lease,
                BalanceAlertClaimOutcome::Skipped => {
                    report.skipped = report
                        .skipped
                        .checked_add(1)
                        .ok_or(BalanceAlertTaskError::Queue)?;
                    continue;
                }
                BalanceAlertClaimOutcome::Empty => break,
            };
            let delivery_result = EmailDeliveryRequest::balance_alert(
                email_settings,
                &self.cipher,
                lease.recipient().to_owned(),
                site_name,
                lease.username(),
                lease.current_quota(),
                lease.threshold(),
            )
            .map_err(|_| EmailDeliveryError::InvalidConfiguration);
            let delivery_result = match delivery_result {
                Ok(request) => self.delivery.send(request).await,
                Err(error) => Err(error),
            };
            match delivery_result {
                Ok(()) => {
                    let completed_at = DatabaseTimestamp::now_utc();
                    if self
                        .queue
                        .mark_sent(&lease, completed_at)
                        .await
                        .map_err(map_queue_error)?
                        == BalanceAlertCompletionOutcome::Completed
                    {
                        report.sent = report
                            .sent
                            .checked_add(1)
                            .ok_or(BalanceAlertTaskError::Queue)?;
                    }
                }
                Err(error) => {
                    let completed_at = DatabaseTimestamp::now_utc();
                    let failure = delivery_failure_kind(error);
                    if self
                        .queue
                        .record_failure(&lease, failure, completed_at)
                        .await
                        .map_err(map_queue_error)?
                        == BalanceAlertCompletionOutcome::Completed
                    {
                        if matches!(failure, BalanceAlertDeliveryFailureKind::Configuration)
                            || lease.attempt_count() >= af_db::MAX_BALANCE_ALERT_ATTEMPTS
                        {
                            report.terminal_failed = report
                                .terminal_failed
                                .checked_add(1)
                                .ok_or(BalanceAlertTaskError::Queue)?;
                        } else {
                            report.retry_scheduled = report
                                .retry_scheduled
                                .checked_add(1)
                                .ok_or(BalanceAlertTaskError::Queue)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn deliver_subscription_alerts(
        &self,
        email_settings: &af_db::EmailSettingsRecord,
        site_name: &str,
        report: &mut BalanceAlertTaskReport,
    ) -> Result<(), BalanceAlertTaskError> {
        for _ in 0..MAX_BALANCE_ALERT_DELIVERIES_PER_RUN {
            let claim_now = DatabaseTimestamp::now_utc();
            let lease = match self
                .subscription_queue
                .claim_next(claim_now)
                .await
                .map_err(map_subscription_queue_error)?
            {
                SubscriptionBalanceAlertClaimOutcome::Claimed(lease) => lease,
                SubscriptionBalanceAlertClaimOutcome::Skipped => {
                    report.skipped = report
                        .skipped
                        .checked_add(1)
                        .ok_or(BalanceAlertTaskError::Queue)?;
                    continue;
                }
                SubscriptionBalanceAlertClaimOutcome::Empty => break,
            };
            let delivery_result = EmailDeliveryRequest::subscription_balance_alert(
                email_settings,
                &self.cipher,
                lease.recipient().to_owned(),
                site_name,
                lease.username(),
                lease.plan_name(),
                lease.quota_amount(),
                lease.quota_used(),
                lease.window_ends_at(),
                lease.threshold_percent(),
            )
            .map_err(|_| EmailDeliveryError::InvalidConfiguration);
            let delivery_result = match delivery_result {
                Ok(request) => self.delivery.send(request).await,
                Err(error) => Err(error),
            };
            match delivery_result {
                Ok(()) => {
                    let completed_at = DatabaseTimestamp::now_utc();
                    if self
                        .subscription_queue
                        .mark_sent(&lease, completed_at)
                        .await
                        .map_err(map_subscription_queue_error)?
                        == BalanceAlertCompletionOutcome::Completed
                    {
                        report.sent = report
                            .sent
                            .checked_add(1)
                            .ok_or(BalanceAlertTaskError::Queue)?;
                    }
                }
                Err(error) => {
                    let completed_at = DatabaseTimestamp::now_utc();
                    let failure = delivery_failure_kind(error);
                    if self
                        .subscription_queue
                        .record_failure(&lease, failure, completed_at)
                        .await
                        .map_err(map_subscription_queue_error)?
                        == BalanceAlertCompletionOutcome::Completed
                    {
                        if matches!(failure, BalanceAlertDeliveryFailureKind::Configuration)
                            || lease.attempt_count() >= af_db::MAX_BALANCE_ALERT_ATTEMPTS
                        {
                            report.terminal_failed = report
                                .terminal_failed
                                .checked_add(1)
                                .ok_or(BalanceAlertTaskError::Queue)?;
                        } else {
                            report.retry_scheduled = report
                                .retry_scheduled
                                .checked_add(1)
                                .ok_or(BalanceAlertTaskError::Queue)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl fmt::Debug for BalanceAlertTask {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BalanceAlertTask(<已脱敏>)")
    }
}

fn delivery_failure_kind(error: EmailDeliveryError) -> BalanceAlertDeliveryFailureKind {
    match error {
        EmailDeliveryError::InvalidConfiguration => BalanceAlertDeliveryFailureKind::Configuration,
        EmailDeliveryError::Timeout => BalanceAlertDeliveryFailureKind::Timeout,
        EmailDeliveryError::Failed => BalanceAlertDeliveryFailureKind::Transport,
    }
}

fn map_settings_error(_: BalanceAlertSettingsRepositoryError) -> BalanceAlertTaskError {
    BalanceAlertTaskError::Settings
}

fn map_queue_error(_: BalanceAlertRepositoryError) -> BalanceAlertTaskError {
    BalanceAlertTaskError::Queue
}

fn map_subscription_queue_error(
    _: SubscriptionBalanceAlertRepositoryError,
) -> BalanceAlertTaskError {
    BalanceAlertTaskError::Queue
}

fn map_email_settings_error(_: EmailSettingsRepositoryError) -> BalanceAlertTaskError {
    BalanceAlertTaskError::EmailSettings
}

fn map_site_settings_error(_: SiteSettingsRepositoryError) -> BalanceAlertTaskError {
    BalanceAlertTaskError::SiteSettings
}
