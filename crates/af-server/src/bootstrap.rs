use std::{future::Future, sync::Arc, time::Duration};

use af_config::{AppConfig, ConfigError};
use af_db::{
    DatabaseError, DatabaseOptions, DatabaseOptionsError, DatabasePool, MigrationOptions,
    connect_and_migrate,
};
use af_http::{
    HttpListener, HttpRouter, ServeError, ServeOutcome, build_router, serve_with_graceful_shutdown,
};
use af_httpclient::{
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_CACHED_CLIENTS, DEFAULT_READ_TIMEOUT,
    DEFAULT_REQUEST_TIMEOUT, HttpClientConfig, HttpClientError, HttpClientPool, HttpTimeouts,
    PooledClient, ProxyConfig, RemoteDnsPolicy,
};
use af_relay::{RelayBuildError, RelayService};
use af_telemetry::{MetricsHandle, TelemetryError, TracingHandle, init_metrics, init_tracing};
use thiserror::Error;

use crate::{
    BackgroundTaskSupervisor, ShutdownController, SupervisorError, SupervisorShutdown,
    system_shutdown_signal,
};

/// 已加载并校验静态配置。
pub struct Configured;

/// 已安装进程级日志与指标记录器。
pub struct TelemetryReady {
    _tracing: TracingHandle,
    _metrics: MetricsHandle,
}

/// 已连接数据库并执行全部待处理迁移。
///
/// 动态 Option、缓存连接与预热的真实实现应在本阶段之后、监督器启动之前插入；
/// 当前 M0 尚无相应 Source/Repository，因此不会用空操作冒充这些阶段。
pub struct DatabaseReady {
    _telemetry: TelemetryReady,
    database: DatabasePool,
}

/// M0 单上游 relay 已完成安全配置与受控 HTTP Client 装配。
pub struct RelayReady {
    infrastructure: DatabaseReady,
    relay: Arc<RelayService>,
}

/// 后台任务监督域已经建立，当前没有注册尚未实现的业务循环。
pub struct Supervised {
    infrastructure: DatabaseReady,
    relay: Arc<RelayService>,
    supervisor: BackgroundTaskSupervisor,
}

/// HTTP Router 已构建，可以绑定端口并开始服务。
pub struct RouterReady {
    runtime: Supervised,
    router: HttpRouter,
}

/// HTTP 监听器已绑定，可以读取实际地址并开始服务。
pub struct ListenerReady {
    runtime: Supervised,
    router: HttpRouter,
    listener: HttpListener,
}

/// 分阶段服务构建器；每个转换消费前一阶段，禁止跳过初始化顺序。
///
/// ```compile_fail
/// use af_server::{Bootstrap, Configured};
///
/// fn cannot_skip_database(bootstrap: Bootstrap<Configured>) {
///     let _ = bootstrap.build_router();
/// }
/// ```
pub struct Bootstrap<S> {
    config: AppConfig,
    shutdown_timeout: Duration,
    shutdown: ShutdownController,
    stage: S,
}

/// 完整服务关闭结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShutdownReport {
    /// HTTP 在途连接的收尾方式。
    pub http: ServeOutcome,
    /// 后台任务的收尾方式。
    pub background: SupervisorShutdown,
}

/// 顶层启动、监听与资源关闭错误。
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum BootstrapError {
    /// 静态配置加载或校验失败。
    #[error("加载服务配置失败")]
    Config(#[source] ConfigError),
    /// tracing 或 metrics 初始化失败。
    #[error("初始化可观测性失败")]
    Telemetry(#[source] TelemetryError),
    /// 数据库启动参数无效。
    #[error("数据库启动参数无效")]
    DatabaseOptions(#[source] DatabaseOptionsError),
    /// 数据库连接、迁移或关闭失败。
    #[error("数据库生命周期操作失败")]
    Database(#[source] DatabaseError),
    /// HTTP 监听、服务或强制关闭失败。
    #[error("HTTP 服务生命周期操作失败")]
    Http(#[source] ServeError),
    /// 受控上游 HTTP Client 配置或构造失败。
    #[error("初始化上游 HTTP Client 失败")]
    HttpClient(#[source] HttpClientError),
    /// M0 单上游 relay 配置或构造失败。
    #[error("初始化 M0 单上游转发服务失败")]
    Relay(#[source] RelayBuildError),
    /// 后台任务强制关闭失败。
    #[error("后台任务生命周期操作失败")]
    Supervisor(#[source] SupervisorError),
}

impl BootstrapError {
    /// 返回当前错误是否表示仍有任务可能持有运行期资源。
    ///
    /// 该状态无法在同一进程内安全恢复，顶层入口必须立即结束进程，禁止继续析构 DB
    /// 等共享资源后维持服务运行。
    #[must_use]
    pub const fn requires_immediate_exit(&self) -> bool {
        matches!(
            self,
            Self::Http(ServeError::ForceStopTimeout)
                | Self::Supervisor(SupervisorError::ForceStopTimeout)
        )
    }
}

impl Bootstrap<Configured> {
    /// 从已加载配置创建第一阶段；仍会重新校验，避免嵌入式调用绕过约束。
    pub fn new(config: AppConfig) -> Result<Self, BootstrapError> {
        config.validate().map_err(BootstrapError::Config)?;
        let shutdown_timeout = Duration::from_secs(config.server().shutdown_timeout_secs());
        Ok(Self {
            config,
            shutdown_timeout,
            shutdown: ShutdownController::new(),
            stage: Configured,
        })
    }

    /// 安装 stdout JSON tracing 与进程级 Prometheus recorder。
    pub fn init_telemetry(self) -> Result<Bootstrap<TelemetryReady>, BootstrapError> {
        let tracing = init_tracing(self.config.telemetry()).map_err(BootstrapError::Telemetry)?;
        let metrics = init_metrics().map_err(BootstrapError::Telemetry)?;
        Ok(self.transition(TelemetryReady {
            _tracing: tracing,
            _metrics: metrics,
        }))
    }
}

impl Bootstrap<TelemetryReady> {
    /// 创建数据库连接池并在开始监听前执行 pending migrations。
    pub async fn connect_database(self) -> Result<Bootstrap<DatabaseReady>, BootstrapError> {
        let database_options = database_options(&self.config)?;
        let migration_options = migration_options(&self.config)?;
        let database = connect_and_migrate(&database_options, migration_options)
            .await
            .map_err(BootstrapError::Database)?;
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage,
        } = self;
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage: DatabaseReady {
                _telemetry: stage,
                database,
            },
        })
    }
}

impl Bootstrap<DatabaseReady> {
    /// 使用启动期静态单上游配置建立 M0 非流式 relay。
    pub fn init_relay(self) -> Result<Bootstrap<RelayReady>, BootstrapError> {
        let relay = Arc::new(build_relay(&self.config)?);
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage,
        } = self;
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage: RelayReady {
                infrastructure: stage,
                relay,
            },
        })
    }
}

impl Bootstrap<RelayReady> {
    /// 建立后台任务监督域；后续任务必须通过该域注册并共享关闭信号。
    #[must_use]
    pub fn start_supervisor(self) -> Bootstrap<Supervised> {
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage,
        } = self;
        let supervisor = BackgroundTaskSupervisor::new(shutdown.clone());
        let RelayReady {
            infrastructure,
            relay,
        } = stage;
        Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage: Supervised {
                infrastructure,
                relay,
                supervisor,
            },
        }
    }
}

impl Bootstrap<Supervised> {
    /// 构建完整 HTTP Router；此后只允许进入监听阶段。
    #[must_use]
    pub fn build_router(self) -> Bootstrap<RouterReady> {
        let router = build_router(self.config.server(), Arc::clone(&self.stage.relay));
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage,
        } = self;
        Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage: RouterReady {
                runtime: stage,
                router,
            },
        }
    }
}

impl Bootstrap<RouterReady> {
    /// 绑定配置的监听地址；端口为零时可在下一阶段读取系统分配的实际端口。
    pub fn bind(self) -> Result<Bootstrap<ListenerReady>, BootstrapError> {
        let listener =
            HttpListener::bind(self.config.server().bind()).map_err(BootstrapError::Http)?;
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage,
        } = self;
        let RouterReady { runtime, router } = stage;
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage: ListenerReady {
                runtime,
                router,
                listener,
            },
        })
    }
}

impl Bootstrap<ListenerReady> {
    /// 返回已绑定监听器的实际地址。
    #[must_use]
    pub const fn local_addr(&self) -> std::net::SocketAddr {
        self.stage.listener.local_addr()
    }

    /// 运行 HTTP 服务，并按顺序排空请求、停止任务、关闭数据库。
    ///
    /// 若强制停止后仍无法确认 HTTP 连接或后台任务已经退出，本方法会记录固定脱敏
    /// 分类并直接以状态码 1 结束进程，不会返回 `Result`。该终态无法在嵌入宿主内
    /// 安全恢复；需要自定义策略的宿主不应调用本服务进程入口。
    pub async fn serve<F>(self, shutdown_signal: F) -> Result<ShutdownReport, BootstrapError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let Bootstrap {
            config: _,
            shutdown_timeout,
            shutdown,
            stage,
        } = self;
        let ListenerReady {
            runtime,
            router,
            listener,
        } = stage;
        let Supervised {
            infrastructure,
            supervisor,
            relay: _,
        } = runtime;
        let http =
            serve_with_graceful_shutdown(listener, router, shutdown_signal, shutdown_timeout);

        run_http_and_shutdown(
            http,
            shutdown,
            supervisor,
            infrastructure.database,
            shutdown_timeout,
        )
        .await
    }
}

async fn run_http_and_shutdown<H>(
    http: H,
    shutdown: ShutdownController,
    supervisor: BackgroundTaskSupervisor,
    database: DatabasePool,
    shutdown_timeout: Duration,
) -> Result<ShutdownReport, BootstrapError>
where
    H: Future<Output = Result<ServeOutcome, ServeError>>,
{
    let http_result = http.await;

    // HTTP 无论正常结束还是失败，都必须先封闭后台任务，再关闭数据库。
    let _ = shutdown.trigger();
    let background_result = supervisor.shutdown(shutdown_timeout).await;

    // 无法确认请求或后台任务已经终止时，禁止析构仍可能被使用的运行期资源。
    let http_result = match http_result {
        Err(ServeError::ForceStopTimeout) => {
            terminate_unconfirmed_shutdown("http_force_stop_timeout");
        }
        result => result,
    };
    let background = match background_result {
        Ok(outcome) => outcome,
        Err(SupervisorError::ForceStopTimeout) => {
            terminate_unconfirmed_shutdown("background_force_stop_timeout");
        }
    };
    let database_result = database.close().await;
    match (http_result, database_result) {
        (Ok(http), Ok(())) => Ok(ShutdownReport { http, background }),
        (Ok(_), Err(error)) => Err(BootstrapError::Database(error)),
        (Err(error), Ok(())) => Err(BootstrapError::Http(error)),
        (Err(error), Err(_)) => {
            // 主 HTTP 错误决定启动/服务失败原因，清理错误只记录固定分类。
            tracing::error!(
                error_kind = "database_close_after_http_error",
                "HTTP 失败后的数据库关闭也失败"
            );
            Err(BootstrapError::Http(error))
        }
    }
}

impl<S> Bootstrap<S> {
    fn transition<T>(self, stage: T) -> Bootstrap<T> {
        Bootstrap {
            config: self.config,
            shutdown_timeout: self.shutdown_timeout,
            shutdown: self.shutdown,
            stage,
        }
    }
}

fn database_options(config: &AppConfig) -> Result<DatabaseOptions, BootstrapError> {
    DatabaseOptions::new(config.database().url().expose().to_owned())
        .map_err(BootstrapError::DatabaseOptions)
}

fn migration_options(config: &AppConfig) -> Result<MigrationOptions, BootstrapError> {
    config.database().migration_timeout_secs().map_or_else(
        || Ok(MigrationOptions::default()),
        |seconds| {
            MigrationOptions::new(Duration::from_secs(seconds))
                .map_err(BootstrapError::DatabaseOptions)
        },
    )
}

fn build_relay(config: &AppConfig) -> Result<RelayService, BootstrapError> {
    let client = build_http_client(config)?;
    let upstream = config.openai_upstream();
    RelayService::new(
        client,
        upstream.base_url(),
        upstream.model(),
        upstream.api_key().expose(),
    )
    .map_err(BootstrapError::Relay)
}

fn build_http_client(config: &AppConfig) -> Result<PooledClient, BootstrapError> {
    let settings = config.http_client();
    let timeouts = HttpTimeouts::new(
        settings
            .connect_timeout_secs()
            .map_or(DEFAULT_CONNECT_TIMEOUT, Duration::from_secs),
        settings
            .read_timeout_secs()
            .map_or(DEFAULT_READ_TIMEOUT, Duration::from_secs),
        settings
            .request_timeout_secs()
            .map_or(DEFAULT_REQUEST_TIMEOUT, Duration::from_secs),
    )
    .map_err(BootstrapError::HttpClient)?;
    let proxy = settings
        .proxy_url()
        .map_or_else(
            || Ok(ProxyConfig::direct()),
            |url| ProxyConfig::parse(url.expose()),
        )
        .map_err(BootstrapError::HttpClient)?;
    let remote_dns_policy = if settings.trust_proxy_dns() {
        RemoteDnsPolicy::TrustProxy
    } else {
        RemoteDnsPolicy::Deny
    };
    let client_config =
        HttpClientConfig::new(proxy, timeouts).with_remote_dns_policy(remote_dns_policy);
    let pool = HttpClientPool::new(
        settings
            .max_cached_clients()
            .unwrap_or(DEFAULT_MAX_CACHED_CLIENTS),
    )
    .map_err(BootstrapError::HttpClient)?;
    pool.get(&client_config).map_err(BootstrapError::HttpClient)
}

fn terminate_unconfirmed_shutdown(error_kind: &'static str) -> ! {
    tracing::error!(
        error_kind,
        "无法确认运行期任务已经退出，立即终止进程以保护共享资源"
    );
    std::process::exit(1)
}

/// 加载默认配置并运行当前已实现的完整启动链。
pub async fn run() -> Result<(), BootstrapError> {
    let config = af_config::load().map_err(BootstrapError::Config)?;
    let report = Bootstrap::new(config)?
        .init_telemetry()?
        .connect_database()
        .await?
        .init_relay()?
        .start_supervisor()
        .build_router()
        .bind()?
        .serve(system_shutdown_signal())
        .await?;
    log_shutdown_report(report);
    Ok(())
}

fn log_shutdown_report(report: ShutdownReport) {
    match report.http {
        ServeOutcome::Drained => {
            tracing::info!(shutdown_kind = "http_drained", "HTTP 在途请求已排空");
        }
        ServeOutcome::DrainTimedOut => {
            tracing::warn!(
                shutdown_kind = "http_forced",
                "HTTP 排空超时，剩余连接已强制终止"
            );
        }
        ServeOutcome::Stopped => {
            tracing::warn!(shutdown_kind = "http_stopped", "HTTP 服务在关闭信号前停止");
        }
        _ => {
            tracing::warn!(shutdown_kind = "http_unknown", "HTTP 服务以未知状态停止");
        }
    }
    match report.background {
        SupervisorShutdown::Drained => {
            tracing::info!(shutdown_kind = "tasks_drained", "后台任务已完成收尾");
        }
        SupervisorShutdown::TimedOut => {
            tracing::warn!(
                shutdown_kind = "tasks_forced",
                "后台任务收尾超时，剩余任务已取消"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
        },
    };

    use tokio::sync::Notify;

    use super::*;

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    fn config_file(database_url: &str) -> PathBuf {
        let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-bootstrap-{}-{serial}.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            format!(
                "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{database_url}'\n[openai_upstream]\nbase_url = 'https://upstream.example'\nmodel = 'test-model'\napi_key = 'test-upstream-key'\n"
            ),
        )
        .unwrap();
        path
    }

    #[test]
    fn invalid_database_scheme_fails_before_any_connection_attempt() {
        let path = config_file("redis://private-host/secret");
        let config = af_config::load_from(&path).unwrap();
        let _ = fs::remove_file(path);
        let error = database_options(&config).unwrap_err();
        assert!(matches!(error, BootstrapError::DatabaseOptions(_)));
        let rendered = format!("{error:?}\n{error}");
        assert!(!rendered.contains("private-host"));
        assert!(!rendered.contains("secret"));
    }

    #[test]
    fn top_level_display_never_renders_nested_error_details() {
        const SECRET_CANARY: &str = "startup-error-secret-canary";
        let error = BootstrapError::Http(ServeError::Io(std::io::Error::other(SECRET_CANARY)));

        assert!(!error.to_string().contains(SECRET_CANARY));
        assert!(!error.requires_immediate_exit());
        assert!(BootstrapError::Http(ServeError::ForceStopTimeout).requires_immediate_exit());
        assert!(
            BootstrapError::Supervisor(SupervisorError::ForceStopTimeout).requires_immediate_exit()
        );
    }

    #[tokio::test]
    async fn current_stages_start_and_shutdown_in_order() {
        let path = config_file("sqlite::memory:");
        let config = af_config::load_from(&path).unwrap();
        let _ = fs::remove_file(path);

        let report = Bootstrap::new(config)
            .unwrap()
            .init_telemetry()
            .unwrap()
            .connect_database()
            .await
            .unwrap()
            .init_relay()
            .unwrap()
            .start_supervisor()
            .build_router()
            .bind()
            .unwrap()
            .serve(async {})
            .await
            .unwrap();
        assert_eq!(report.http, ServeOutcome::Drained);
        assert_eq!(report.background, SupervisorShutdown::Drained);
    }

    #[tokio::test]
    async fn http_finishes_before_background_shutdown_and_database_close() {
        let database = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let observer = database.clone();
        let shutdown = ShutdownController::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let started = Arc::new(Notify::new());
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown.clone());
        supervisor.spawn("order-test", {
            let events = Arc::clone(&events);
            let started = Arc::clone(&started);
            move |task_shutdown| {
                let events = Arc::clone(&events);
                let started = Arc::clone(&started);
                async move {
                    started.notify_one();
                    task_shutdown.cancelled().await;
                    events.lock().unwrap().push("background_stopped");
                }
            }
        });
        started.notified().await;

        let http_shutdown = shutdown.clone();
        let http_events = Arc::clone(&events);
        let result = run_http_and_shutdown(
            async move {
                assert!(!http_shutdown.is_triggered());
                http_events.lock().unwrap().push("http_finished");
                Err(ServeError::Io(std::io::Error::other("primary-http-secret")))
            },
            shutdown,
            supervisor,
            database,
            Duration::from_secs(1),
        )
        .await;

        assert!(matches!(result, Err(BootstrapError::Http(_))));
        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["http_finished", "background_stopped"]
        );
        assert!(observer.ping().await.is_err());
    }
}
