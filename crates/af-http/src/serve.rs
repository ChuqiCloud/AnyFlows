use std::{future::Future, net::SocketAddr, time::Duration};

use axum_server::Handle;
use thiserror::Error;
use tokio::time::{sleep, timeout};

use crate::HttpRouter;

/// 强制终止 HTTP 连接任务后，给服务循环留下的固定收尾时间。
const FORCE_STOP_TIMEOUT: Duration = Duration::from_secs(1);

/// HTTP 服务停止方式，供上层决定后续资源关闭策略。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServeOutcome {
    /// 服务在收到关闭信号前自行停止。
    Stopped,
    /// 所有在途连接在截止时间内完成。
    Drained,
    /// 截止时间到达后强制终止剩余连接。
    DrainTimedOut,
}

/// HTTP 监听与优雅关闭错误。
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum ServeError {
    /// 关闭截止时间不能为零。
    #[error("HTTP 优雅关闭截止时间必须大于零")]
    ZeroDrainTimeout,
    /// 强制关闭信号发出后，服务循环仍未在固定时间内退出。
    #[error("HTTP 服务未能在强制关闭截止时间内退出")]
    ForceStopTimeout,
    /// 监听或服务循环失败。
    #[error("HTTP 服务循环失败")]
    Io(#[source] std::io::Error),
}

/// 已绑定且设置为非阻塞模式的 HTTP 监听器。
pub struct HttpListener {
    inner: std::net::TcpListener,
    local_addr: SocketAddr,
}

impl HttpListener {
    /// 绑定监听地址；端口为零时可通过 [`Self::local_addr`] 读取系统分配的端口。
    pub fn bind(bind: SocketAddr) -> Result<Self, ServeError> {
        let listener = std::net::TcpListener::bind(bind).map_err(ServeError::Io)?;
        listener.set_nonblocking(true).map_err(ServeError::Io)?;
        let local_addr = listener.local_addr().map_err(ServeError::Io)?;
        Ok(Self {
            inner: listener,
            local_addr,
        })
    }

    /// 返回监听器实际绑定的地址。
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// 克隆已绑定监听句柄，供受监督服务循环在异常退出后原址重建。
    pub fn try_clone(&self) -> Result<Self, ServeError> {
        let inner = self.inner.try_clone().map_err(ServeError::Io)?;
        Ok(Self {
            inner,
            local_addr: self.local_addr,
        })
    }
}

/// 绑定并运行 HTTP 服务；收到信号后停止接收新连接并在硬截止时间内排空。
///
/// `axum::serve` 的默认 graceful future 在连接长期不结束时会无限等待。这里使用
/// `axum-server` 的 Handle 同时提供优雅通知和强制关闭，使超时路径不会把连接任务
/// 留在后台后就去关闭数据库或其他共享资源。
pub async fn serve_with_graceful_shutdown<F>(
    listener: HttpListener,
    router: HttpRouter,
    shutdown: F,
    drain_timeout: Duration,
) -> Result<ServeOutcome, ServeError>
where
    F: Future<Output = ()> + Send + 'static,
{
    if drain_timeout.is_zero() {
        return Err(ServeError::ZeroDrainTimeout);
    }

    let handle = Handle::new();
    let server = axum_server::from_tcp(listener.inner)
        .map_err(ServeError::Io)?
        .http1_only()
        .handle(handle.clone())
        .serve(
            router
                .into_router()
                .into_make_service_with_connect_info::<SocketAddr>(),
        );
    let signal_handle = handle.clone();
    let graceful_signal = async move {
        shutdown.await;
        // 截止时间只由外层计时器掌握，避免内部计时器先完成后把强制退出误报为正常排空。
        signal_handle.graceful_shutdown(None);
    };

    tokio::pin!(server);
    tokio::pin!(graceful_signal);
    tokio::select! {
        biased;
        result = &mut server => {
            // 即使 accept loop 意外结束，既有连接仍是独立 task；先强制通知并确认
            // 连接计数归零，再把原始服务结果交给上层。
            handle.shutdown();
            wait_for_connections(&handle).await?;
            result.map(|()| ServeOutcome::Stopped).map_err(ServeError::Io)
        },
        _ = &mut graceful_signal => {
            match timeout(drain_timeout, &mut server).await {
                Ok(result) => result
                    .map(|()| ServeOutcome::Drained)
                    .map_err(ServeError::Io),
                Err(_) => {
                    // 排空截止时间已到，通知服务循环和所有连接任务走强制退出分支。
                    handle.shutdown();
                    let force_stop = async {
                        let result = (&mut server).await;
                        wait_for_connection_count(&handle).await;
                        result
                    };
                    match timeout(FORCE_STOP_TIMEOUT, force_stop).await {
                        Ok(result) => result
                            .map(|()| ServeOutcome::DrainTimedOut)
                            .map_err(ServeError::Io),
                        Err(_) => Err(ServeError::ForceStopTimeout),
                    }
                }
            }
        }
    }
}

async fn wait_for_connections(handle: &Handle<SocketAddr>) -> Result<(), ServeError> {
    timeout(FORCE_STOP_TIMEOUT, wait_for_connection_count(handle))
        .await
        .map_err(|_| ServeError::ForceStopTimeout)
}

async fn wait_for_connection_count(handle: &Handle<SocketAddr>) {
    // axum-server 的强制分支会先结束 accept loop；必须再确认每个独立连接 task
    // 都已丢弃 handler，才能允许上层关闭共享资源。
    while handle.connection_count() != 0 {
        sleep(Duration::from_millis(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use axum::{Router, extract::ConnectInfo, routing::get};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        sync::{Notify, oneshot},
        time::sleep,
    };

    use super::*;

    #[tokio::test]
    async fn rejects_zero_drain_timeout_before_binding() {
        let error = serve_with_graceful_shutdown(
            HttpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap(),
            HttpRouter::new(Router::new()),
            std::future::pending::<()>(),
            Duration::ZERO,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ServeError::ZeroDrainTimeout));
    }

    #[tokio::test]
    async fn real_tcp_requests_receive_their_peer_address() {
        let router = Router::new().route(
            "/peer",
            get(|ConnectInfo(peer): ConnectInfo<SocketAddr>| async move {
                if peer.ip().is_loopback() {
                    "peer-ok"
                } else {
                    "peer-invalid"
                }
            }),
        );
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let listener = HttpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr();
        let server = tokio::spawn(serve_with_graceful_shutdown(
            listener,
            HttpRouter::new(router),
            async move {
                let _ = shutdown_rx.await;
            },
            Duration::from_secs(1),
        ));

        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"GET /peer HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("peer-ok"));

        shutdown_tx.send(()).unwrap();
        assert_eq!(server.await.unwrap().unwrap(), ServeOutcome::Drained);
    }

    #[tokio::test]
    async fn normal_shutdown_drains_an_in_flight_handler() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let entered_handler = Arc::clone(&entered);
        let release_handler = Arc::clone(&release);
        let router = Router::new().route(
            "/hold",
            get(move || {
                let entered = Arc::clone(&entered_handler);
                let release = Arc::clone(&release_handler);
                async move {
                    entered.notify_one();
                    release.notified().await;
                    "ok"
                }
            }),
        );
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let listener = HttpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr();
        let server = tokio::spawn(serve_with_graceful_shutdown(
            listener,
            HttpRouter::new(router),
            async move {
                let _ = shutdown_rx.await;
            },
            Duration::from_millis(100),
        ));

        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"GET /hold HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        timeout(Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        shutdown_tx.send(()).unwrap();
        sleep(Duration::from_millis(5)).await;
        release.notify_one();
        let outcome = server.await.unwrap().unwrap();
        assert_eq!(outcome, ServeOutcome::Drained);

        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("ok"));
    }

    #[tokio::test]
    async fn hard_timeout_forces_an_in_flight_handler_to_stop() {
        let entered = Arc::new(Notify::new());
        let entered_handler = Arc::clone(&entered);
        let handler_dropped = Arc::new(AtomicBool::new(false));
        let dropped_handler = Arc::clone(&handler_dropped);
        let router = Router::new().route(
            "/hold",
            get(move || {
                let entered = Arc::clone(&entered_handler);
                let dropped = DropSignal(Arc::clone(&dropped_handler));
                async move {
                    let _dropped = dropped;
                    entered.notify_one();
                    std::future::pending::<&'static str>().await
                }
            }),
        );
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let listener = HttpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr();
        let server = tokio::spawn(serve_with_graceful_shutdown(
            listener,
            HttpRouter::new(router),
            async move {
                let _ = shutdown_rx.await;
            },
            Duration::from_millis(20),
        ));

        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"GET /hold HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        timeout(Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        sleep(Duration::from_millis(30)).await;
        assert!(!server.is_finished(), "排空计时不得早于关闭信号开始");
        shutdown_tx.send(()).unwrap();
        assert_eq!(server.await.unwrap().unwrap(), ServeOutcome::DrainTimedOut);
        assert!(handler_dropped.load(Ordering::Acquire));

        let mut byte = [0_u8; 1];
        let read = timeout(Duration::from_secs(1), client.read(&mut byte))
            .await
            .unwrap();
        assert!(matches!(read, Ok(0) | Err(_)));
    }

    struct DropSignal(Arc<AtomicBool>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
}
