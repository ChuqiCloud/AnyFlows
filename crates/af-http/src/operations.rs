use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use axum::{
    Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
};
use http::{
    StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE},
};

/// 进程存活探针路径。
pub const HEALTH_PATH: &str = "/healthz";
/// 服务就绪探针路径。
pub const READINESS_PATH: &str = "/readyz";

const STARTING: u8 = 0;
const READY: u8 = 1;
const DRAINING: u8 = 2;
const HEALTH_BODY: &str = r#"{"status":"ok"}"#;
const READY_BODY: &str = r#"{"status":"ready"}"#;
const NOT_READY_BODY: &str = r#"{"status":"not_ready"}"#;

/// 一次有界就绪检查返回的异步结果。
pub type ReadinessFuture<'a> = Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

/// 由服务装配层实现的运行依赖就绪检查。
///
/// 实现必须自行施加硬截止且不得重试；错误细节只能在依赖边界内消化，不能进入
/// HTTP 响应。M1 增加 Redis、缓存快照等依赖时应在同一实现内并发聚合。
pub trait ReadinessProbe: Send + Sync {
    /// 检查当前运行依赖是否全部可用。
    fn check(&self) -> ReadinessFuture<'_>;
}

/// 控制服务启动、就绪与排空阶段的单调句柄。
///
/// 新句柄默认未就绪；进入排空后即使再次调用 [`Self::mark_ready`] 也不会重新开放。
#[derive(Clone)]
pub struct ReadinessHandle {
    inner: Arc<ReadinessState>,
}

struct ReadinessState {
    phase: AtomicU8,
    probe: Box<dyn ReadinessProbe>,
}

impl ReadinessHandle {
    /// 从真实运行依赖探针创建未就绪句柄。
    #[must_use]
    pub fn new(probe: impl ReadinessProbe + 'static) -> Self {
        Self {
            inner: Arc::new(ReadinessState {
                phase: AtomicU8::new(STARTING),
                probe: Box::new(probe),
            }),
        }
    }

    /// 在所有启动阶段完成且监听器绑定成功后进入就绪状态。
    pub fn mark_ready(&self) {
        let _ =
            self.inner
                .phase
                .compare_exchange(STARTING, READY, Ordering::AcqRel, Ordering::Acquire);
    }

    /// 在触发 HTTP 排空前永久关闭就绪状态。
    pub fn begin_draining(&self) {
        self.inner.phase.store(DRAINING, Ordering::Release);
    }

    /// 返回服务是否已经进入不可逆的排空阶段。
    #[must_use]
    pub fn is_draining(&self) -> bool {
        self.inner.phase.load(Ordering::Acquire) == DRAINING
    }

    async fn check(&self) -> bool {
        if self.inner.phase.load(Ordering::Acquire) != READY {
            return false;
        }
        let dependencies_ready = self.inner.probe.check().await;
        dependencies_ready && self.inner.phase.load(Ordering::Acquire) == READY
    }
}

impl fmt::Debug for ReadinessHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let phase = match self.inner.phase.load(Ordering::Acquire) {
            STARTING => "starting",
            READY => "ready",
            _ => "draining",
        };
        formatter
            .debug_struct("ReadinessHandle")
            .field("phase", &phase)
            .finish_non_exhaustive()
    }
}

pub(crate) fn operations_router(readiness: ReadinessHandle) -> Router {
    Router::new()
        .route(HEALTH_PATH, get(healthz))
        .route(READINESS_PATH, get(readyz))
        .with_state(readiness)
}

async fn healthz() -> Response {
    operational_response(StatusCode::OK, HEALTH_BODY)
}

async fn readyz(State(readiness): State<ReadinessHandle>) -> Response {
    if readiness.check().await {
        operational_response(StatusCode::OK, READY_BODY)
    } else {
        operational_response(StatusCode::SERVICE_UNAVAILABLE, NOT_READY_BODY)
    }
}

fn operational_response(status: StatusCode, body: &'static str) -> Response {
    (
        status,
        [
            (CONTENT_TYPE, "application/json; charset=utf-8"),
            (CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}
