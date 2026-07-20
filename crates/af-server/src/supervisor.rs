use std::{future::Future, panic::AssertUnwindSafe, time::Duration};

use futures_util::FutureExt as _;
use thiserror::Error;
use tokio::{task::JoinSet, time::sleep};

use crate::ShutdownController;

const DEFAULT_RESTART_DELAY: Duration = Duration::from_millis(100);
const FORCE_STOP_TIMEOUT: Duration = Duration::from_secs(1);

/// 后台任务在关闭阶段的收尾结果。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorShutdown {
    /// 全部任务主动响应关闭并退出。
    Drained,
    /// 截止时间到达后取消了剩余任务。
    TimedOut,
}

/// 后台任务强制停止错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SupervisorError {
    /// 取消全部任务后仍未能在固定时间内确认退出。
    #[error("后台任务未能在强制关闭截止时间内退出")]
    ForceStopTimeout,
}

/// 后台任务监督器；任务异常结束或 panic 时会在退避后重启。
pub struct BackgroundTaskSupervisor {
    tasks: JoinSet<()>,
    shutdown: ShutdownController,
    restart_delay: Duration,
}

impl BackgroundTaskSupervisor {
    /// 使用共享关闭控制器创建空监督器。
    ///
    /// 调用方必须先安装不会输出原始 panic payload 的进程级 hook；产品入口通过
    /// [`crate::install_process_safety_hooks`] 完成该约束。
    #[must_use]
    pub fn new(shutdown: ShutdownController) -> Self {
        Self {
            tasks: JoinSet::new(),
            shutdown,
            restart_delay: DEFAULT_RESTART_DELAY,
        }
    }

    /// 覆盖任务异常退出后的重启退避，主要供嵌入式运行与测试控制。
    #[must_use]
    #[cfg(test)]
    fn with_restart_delay(mut self, restart_delay: Duration) -> Self {
        self.restart_delay = restart_delay;
        self
    }

    /// 注册一个可重建的长期任务。
    ///
    /// 工厂每次收到同一个关闭域的新句柄。任务应自行监听关闭并完成业务收尾；若未能
    /// 在上层截止时间内退出，监督器会取消其 Future，且不会留下 detached task。
    pub fn spawn<F, Fut>(&mut self, task_kind: &'static str, factory: F)
    where
        F: Fn(ShutdownController) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let shutdown = self.shutdown.clone();
        let restart_delay = self.restart_delay;
        self.tasks.spawn(async move {
            loop {
                if shutdown.is_triggered() {
                    return;
                }
                let result = AssertUnwindSafe(async { factory(shutdown.clone()).await })
                    .catch_unwind()
                    .await;
                if shutdown.is_triggered() {
                    return;
                }
                if result.is_err() {
                    tracing::error!(task_kind, error_kind = "task_panic", "后台任务异常退出");
                } else {
                    tracing::warn!(task_kind, error_kind = "task_stopped", "后台任务提前结束");
                }

                tokio::select! {
                    () = shutdown.cancelled() => return,
                    () = sleep(restart_delay) => {}
                }
            }
        });
    }

    /// 触发关闭并在统一截止时间内等待全部任务退出。
    pub async fn shutdown(
        mut self,
        drain_timeout: Duration,
    ) -> Result<SupervisorShutdown, SupervisorError> {
        let _ = self.shutdown.trigger();
        if self.tasks.is_empty() {
            return Ok(SupervisorShutdown::Drained);
        }
        let drained = tokio::time::timeout(drain_timeout, async {
            while self.tasks.join_next().await.is_some() {}
        })
        .await
        .is_ok();
        if drained {
            return Ok(SupervisorShutdown::Drained);
        }

        self.tasks.abort_all();
        tokio::time::timeout(FORCE_STOP_TIMEOUT, async {
            while self.tasks.join_next().await.is_some() {}
        })
        .await
        .map_err(|_| SupervisorError::ForceStopTimeout)?;
        Ok(SupervisorShutdown::TimedOut)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use tokio::sync::Notify;

    use super::*;

    #[tokio::test]
    async fn panic_is_isolated_restarted_and_then_stopped() {
        let shutdown = ShutdownController::new();
        let starts = Arc::new(AtomicUsize::new(0));
        let restarted = Arc::new(Notify::new());
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown.clone())
            .with_restart_delay(Duration::from_millis(1));
        supervisor.spawn("panic-test", {
            let starts = Arc::clone(&starts);
            let restarted = Arc::clone(&restarted);
            move |task_shutdown| {
                let starts = Arc::clone(&starts);
                let restarted = Arc::clone(&restarted);
                async move {
                    let attempt = starts.fetch_add(1, Ordering::AcqRel);
                    if attempt == 0 {
                        panic!("受控测试 panic");
                    }
                    restarted.notify_one();
                    task_shutdown.cancelled().await;
                }
            }
        });

        tokio::time::timeout(Duration::from_secs(1), restarted.notified())
            .await
            .unwrap();
        assert!(starts.load(Ordering::Acquire) >= 2);
        assert_eq!(
            supervisor.shutdown(Duration::from_secs(1)).await.unwrap(),
            SupervisorShutdown::Drained
        );
    }

    #[tokio::test]
    async fn timeout_cancels_a_task_that_ignores_shutdown() {
        let shutdown = ShutdownController::new();
        let dropped = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown);
        supervisor.spawn("stuck-test", {
            let dropped = Arc::clone(&dropped);
            let started = Arc::clone(&started);
            move |_| {
                let guard = DropCounter(Arc::clone(&dropped));
                let started = Arc::clone(&started);
                async move {
                    let _guard = guard;
                    started.notify_one();
                    std::future::pending::<()>().await;
                }
            }
        });

        tokio::time::timeout(Duration::from_secs(1), started.notified())
            .await
            .unwrap();
        assert_eq!(
            supervisor
                .shutdown(Duration::from_millis(10))
                .await
                .unwrap(),
            SupervisorShutdown::TimedOut
        );
        assert_eq!(dropped.load(Ordering::Acquire), 1);
    }

    struct DropCounter(Arc<AtomicUsize>);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }
}
