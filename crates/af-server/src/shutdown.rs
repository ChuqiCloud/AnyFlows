use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::Notify;

/// 进程内共享的幂等关闭控制器。
#[derive(Clone, Debug, Default)]
pub struct ShutdownController {
    inner: Arc<ShutdownState>,
}

#[derive(Debug, Default)]
struct ShutdownState {
    triggered: AtomicBool,
    notify: Notify,
}

impl ShutdownController {
    /// 创建尚未触发的关闭控制器。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 首次触发时唤醒全部等待者；重复调用只返回 `false`。
    #[must_use]
    pub fn trigger(&self) -> bool {
        if self
            .inner
            .triggered
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        self.inner.notify.notify_waiters();
        true
    }

    /// 返回关闭是否已经触发。
    #[must_use]
    pub fn is_triggered(&self) -> bool {
        self.inner.triggered.load(Ordering::Acquire)
    }

    /// 等待关闭；在触发前后创建的等待者都不会漏掉通知。
    pub async fn cancelled(&self) {
        if self.is_triggered() {
            return;
        }
        let notified = self.inner.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if self.is_triggered() {
            return;
        }
        notified.await;
    }
}

/// 等待跨平台 Ctrl+C，并在 Unix 上同时等待 SIGTERM。
///
/// 信号监听初始化失败时按 fail-closed 处理：记录静态分类后立即进入关闭流程，
/// 不把底层系统错误写入日志。
pub async fn system_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let Ok(mut terminate) = signal(SignalKind::terminate()) else {
            tracing::error!(error_kind = "signal_registration", "注册关闭信号失败");
            return;
        };
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if result.is_err() {
                    tracing::error!(error_kind = "signal_receive", "接收关闭信号失败");
                }
            }
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    if tokio::signal::ctrl_c().await.is_err() {
        tracing::error!(error_kind = "signal_receive", "接收关闭信号失败");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn repeated_trigger_is_idempotent_and_late_waiters_finish() {
        let shutdown = ShutdownController::new();
        let waiter = {
            let shutdown = shutdown.clone();
            tokio::spawn(async move { shutdown.cancelled().await })
        };

        assert!(shutdown.trigger());
        assert!(!shutdown.trigger());
        timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
        timeout(Duration::from_secs(1), shutdown.cancelled())
            .await
            .unwrap();
        assert!(shutdown.is_triggered());
    }
}
