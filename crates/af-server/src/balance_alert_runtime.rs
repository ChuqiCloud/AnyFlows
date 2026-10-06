use std::{future::Future, time::Duration};

use af_admin::BalanceAlertTask;
use tokio::time::{MissedTickBehavior, interval};

const BALANCE_ALERT_SCAN_INTERVAL: Duration = Duration::from_secs(60);

/// 在监督器生命周期内周期执行余额预警任务。
#[derive(Clone, Debug)]
pub(crate) struct BalanceAlertRuntime {
    task: BalanceAlertTask,
}

impl BalanceAlertRuntime {
    #[must_use]
    pub(crate) const fn new(task: BalanceAlertTask) -> Self {
        Self { task }
    }

    /// 首轮立即执行；停机信号到达后不再领取新的投递租约。
    pub(crate) async fn run_until<F>(self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        let mut ticker = interval(BALANCE_ALERT_SCAN_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                () = &mut shutdown => break,
                _ = ticker.tick() => self.run_once().await,
            }
        }
    }

    async fn run_once(&self) {
        match self.task.run_once().await {
            Ok(report) if report != Default::default() => {
                tracing::info!(
                    enqueued = report.enqueued(),
                    sent = report.sent(),
                    retry_scheduled = report.retry_scheduled(),
                    terminal_failed = report.terminal_failed(),
                    skipped = report.skipped(),
                    stale_canceled = report.stale_canceled(),
                    exhausted_failed = report.exhausted_failed(),
                    "余额预警任务完成一轮扫描"
                );
            }
            Ok(_) => tracing::debug!("余额预警任务完成空闲扫描"),
            Err(error) => {
                tracing::error!(error_kind = error.error_kind(), "余额预警任务本轮执行失败")
            }
        }
    }
}
