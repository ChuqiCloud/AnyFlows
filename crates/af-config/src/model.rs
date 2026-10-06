use std::{
    fmt,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use af_domain::{IpCidr, MAX_MODEL_NAME_BYTES};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use url::Url;
use zeroize::Zeroize as _;

use crate::{ConfigError, CorsOriginError, SecretString};

/// 服务优雅关闭的默认排空时间。
pub const DEFAULT_SHUTDOWN_TIMEOUT_SECS: u64 = 30;
/// 服务优雅关闭允许配置的硬上限。
pub const MAX_SHUTDOWN_TIMEOUT_SECS: u64 = 300;
/// 数据库健康检查允许配置的硬上限。
pub const MAX_DATABASE_HEALTH_CHECK_TIMEOUT_SECS: u64 = 30;
/// 令牌认证仓储查询的默认超时时间。
pub const DEFAULT_AUTH_LOOKUP_TIMEOUT_SECS: u64 = 2;
/// 令牌认证仓储查询允许配置的硬上限。
pub const MAX_AUTH_LOOKUP_TIMEOUT_SECS: u64 = 30;
/// 管理会话 JWT 的默认有效期。
pub const DEFAULT_AUTH_SESSION_TTL_SECS: u64 = 3_600;
/// 管理会话 JWT 允许配置的硬上限。
pub const MAX_AUTH_SESSION_TTL_SECS: u64 = 86_400;
/// 管理会话签名密钥要求的随机字节数。
pub const AUTH_SESSION_SIGNING_KEY_BYTES: usize = 32;
/// Turnstile 服务端验证请求的默认硬超时。
pub const DEFAULT_TURNSTILE_TIMEOUT_SECS: u64 = 5;
/// Turnstile 服务端验证请求允许的最大硬超时。
pub const MAX_TURNSTILE_TIMEOUT_SECS: u64 = 30;
/// Turnstile site key 允许的最大 ASCII 字节数。
pub const MAX_TURNSTILE_SITE_KEY_BYTES: usize = 256;
/// Turnstile secret key 允许的最大 ASCII 字节数。
pub const MAX_TURNSTILE_SECRET_KEY_BYTES: usize = 4 * 1_024;
/// 计费 WAL 批量落库的默认周期。
pub const DEFAULT_BILLING_FLUSH_INTERVAL_SECS: u64 = 5;
/// 计费 WAL 单次启动恢复或关闭收尾的默认截止时间。
pub const DEFAULT_BILLING_FLUSH_TIMEOUT_SECS: u64 = 30;
/// 计费 WAL 批量落库周期允许配置的硬上限。
pub const MAX_BILLING_FLUSH_INTERVAL_SECS: u64 = 3_600;
/// 计费 WAL 启动恢复或关闭收尾允许配置的硬上限。
pub const MAX_BILLING_FLUSH_TIMEOUT_SECS: u64 = 300;
/// Stripe webhook 签名时间容差默认值。
pub const DEFAULT_PAYMENT_SIGNATURE_TOLERANCE_SECS: u64 = 300;
/// Stripe webhook 签名时间容差允许的最大值。
pub const MAX_PAYMENT_SIGNATURE_TOLERANCE_SECS: u64 = 86_400;
/// Stripe API 密钥允许的最大字节数。
pub const MAX_PAYMENT_SECRET_KEY_BYTES: usize = 4 * 1_024;
/// Stripe publishable key 允许的最大字节数。
const MAX_PAYMENT_PUBLISHABLE_KEY_BYTES: usize = 512;
/// 支付 Provider webhook 密钥允许的最大字节数。
pub const MAX_PAYMENT_WEBHOOK_SECRET_BYTES: usize = 4 * 1_024;
/// 支付宝实名认证应用标识允许的最大字节数。
pub const MAX_ALIPAY_VERIFICATION_APP_ID_BYTES: usize = 128;
/// 支付宝实名认证密钥材料允许的最大字节数。
pub const MAX_ALIPAY_VERIFICATION_KEY_BYTES: usize = 16 * 1_024;
/// 支付宝实名认证网关地址允许的最大字节数。
pub const MAX_ALIPAY_VERIFICATION_GATEWAY_BYTES: usize = 2_048;
/// 支付宝实名认证业务码允许的最大字节数。
pub const MAX_ALIPAY_VERIFICATION_BIZ_CODE_BYTES: usize = 64;
/// 支付宝实名认证请求超时默认值。
pub const DEFAULT_ALIPAY_VERIFICATION_TIMEOUT_SECS: u64 = 8;
/// 支付宝实名认证请求超时允许的最大值。
pub const MAX_ALIPAY_VERIFICATION_TIMEOUT_SECS: u64 = 30;
/// 用量记录内存接收队列的默认容量。
pub const DEFAULT_USAGE_RECORD_QUEUE_CAPACITY: usize = 4_096;
/// 单实例用量记录队列允许配置的硬容量上限。
pub const MAX_USAGE_RECORD_QUEUE_CAPACITY: usize = 1_000_000;
/// 默认用量记录持久化 worker 数量。
pub const DEFAULT_USAGE_RECORD_WORKER_COUNT: usize = 1;
/// 单实例用量记录持久化 worker 数量硬上限。
pub const MAX_USAGE_RECORD_WORKER_COUNT: usize = 64;
/// 订阅生命周期周期任务默认开启，确保到期窗口和取消订阅能够自动收敛。
pub const DEFAULT_SUBSCRIPTION_CYCLE_ENABLED: bool = true;
/// 订阅生命周期周期任务默认单批记录数。
pub const DEFAULT_SUBSCRIPTION_CYCLE_BATCH_SIZE: usize = 64;
/// 订阅生命周期周期任务默认执行间隔。
pub const DEFAULT_SUBSCRIPTION_CYCLE_INTERVAL_SECS: u64 = 60;
/// 每类订阅状态单轮默认最多扫描的批次数。
pub const DEFAULT_SUBSCRIPTION_CYCLE_MAX_BATCHES_PER_RUN: usize = 8;
/// 订阅生命周期单批记录数硬上限，与数据库分页边界保持一致。
pub const MAX_SUBSCRIPTION_CYCLE_BATCH_SIZE: usize = 100;
/// 订阅生命周期执行间隔允许配置的硬上限。
pub const MAX_SUBSCRIPTION_CYCLE_INTERVAL_SECS: u64 = 3_600;
/// 每类订阅状态单轮扫描批次数硬上限。
pub const MAX_SUBSCRIPTION_CYCLE_BATCHES_PER_RUN: usize = 64;
/// 渠道探活后台任务默认关闭，避免凭据运行时尚未接线时产生无效调用。
pub const DEFAULT_CHANNEL_PROBE_ENABLED: bool = false;
/// 渠道探活默认单轮批次。
pub const DEFAULT_CHANNEL_PROBE_BATCH_SIZE: usize = 16;
/// 渠道探活默认执行周期。
pub const DEFAULT_CHANNEL_PROBE_INTERVAL_SECS: u64 = 60;
/// 单渠道探活默认硬超时。
pub const DEFAULT_CHANNEL_PROBE_TIMEOUT_SECS: u64 = 10;
/// 单轮渠道探活批次的仓储硬上限。
pub const MAX_CHANNEL_PROBE_BATCH_SIZE: usize = 64;
/// 渠道探活执行周期允许配置的硬上限。
pub const MAX_CHANNEL_PROBE_INTERVAL_SECS: u64 = 3_600;
/// 单渠道探活超时允许配置的硬上限。
pub const MAX_CHANNEL_PROBE_TIMEOUT_SECS: u64 = 300;
/// XChaCha20-Poly1305 使用的固定密钥长度。
pub const CREDENTIAL_ENCRYPTION_KEY_BYTES: usize = 32;
/// 凭据密钥标识允许配置的最大 ASCII 字节数。
pub const MAX_CREDENTIAL_ENCRYPTION_KEY_ID_BYTES: usize = 512;
/// 单实例允许配置的静态可信前置代理网络上限。
pub const MAX_TRUSTED_PROXY_CIDRS: usize = 64;
/// OpenAI-compatible 上游基础地址的最大 UTF-8 字节数。
pub const MAX_OPENAI_UPSTREAM_BASE_URL_BYTES: usize = 2_048;
/// 静态单上游模型标识的最大 UTF-8 字节数。
pub const MAX_OPENAI_UPSTREAM_MODEL_BYTES: usize = MAX_MODEL_NAME_BYTES;
/// 静态单上游 API Key 的最大 UTF-8 字节数。
pub const MAX_OPENAI_UPSTREAM_API_KEY_BYTES: usize = 16 * 1_024;
/// 单实例默认允许保留的待完成 OAuth 授权数量。
pub const DEFAULT_OAUTH_MAX_PENDING_AUTHORIZATIONS: usize = 256;
/// 单实例允许保留的待完成 OAuth 授权硬上限。
pub const MAX_OAUTH_PENDING_AUTHORIZATIONS: usize = 4_096;
/// OAuth 授权会话默认有效期。
pub const DEFAULT_OAUTH_SESSION_TTL_SECS: u64 = 10 * 60;
/// OAuth 授权会话允许的最短有效期。
pub const MIN_OAUTH_SESSION_TTL_SECS: u64 = 60;
/// OAuth 授权会话允许的最长有效期。
pub const MAX_OAUTH_SESSION_TTL_SECS: u64 = 15 * 60;
/// OAuth 公共客户端标识允许配置的最大 ASCII 字节数。
pub const MAX_OAUTH_CLIENT_ID_BYTES: usize = 256;
/// OAuth 客户端密钥允许配置的最大 ASCII 字节数。
pub const MAX_OAUTH_CLIENT_SECRET_BYTES: usize = 16 * 1_024;
/// OAuth token 周期刷新默认开启。
pub const DEFAULT_OAUTH_REFRESH_ENABLED: bool = true;
/// OAuth 到期候选的默认扫描周期。
pub const DEFAULT_OAUTH_REFRESH_INTERVAL_SECS: u64 = 60;
/// OAuth access token 默认提前刷新秒数。
pub const DEFAULT_OAUTH_REFRESH_BEFORE_EXPIRY_SECS: u64 = 5 * 60;
/// OAuth 单轮默认读取的候选数量。
pub const DEFAULT_OAUTH_REFRESH_BATCH_SIZE: usize = 64;
/// OAuth 单实例默认并发刷新数量。
pub const DEFAULT_OAUTH_REFRESH_CONCURRENCY: usize = 8;
/// OAuth 到期候选扫描周期硬上限。
pub const MAX_OAUTH_REFRESH_INTERVAL_SECS: u64 = 3_600;
/// OAuth token 提前刷新时间硬上限。
pub const MAX_OAUTH_REFRESH_BEFORE_EXPIRY_SECS: u64 = 24 * 60 * 60;
/// OAuth 单轮候选数量硬上限，与数据库查询边界保持一致。
pub const MAX_OAUTH_REFRESH_BATCH_SIZE: usize = 256;
/// OAuth 单实例刷新并发硬上限。
pub const MAX_OAUTH_REFRESH_CONCURRENCY: usize = 64;
/// ClickHouse 分析读取的默认查询超时。
pub const DEFAULT_CLICKHOUSE_ANALYTICS_TIMEOUT_SECS: u64 = 5;
/// ClickHouse 分析读取允许配置的最大查询超时。
pub const MAX_CLICKHOUSE_ANALYTICS_TIMEOUT_SECS: u64 = 300;
/// ClickHouse 单行分析快照的默认响应体上限。
pub const DEFAULT_CLICKHOUSE_ANALYTICS_RESPONSE_BYTES: usize = 1024 * 1024;
/// ClickHouse 单行分析快照允许配置的最大响应体上限。
pub const MAX_CLICKHOUSE_ANALYTICS_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
/// ClickHouse 异步事实投递的默认批量大小。
pub const DEFAULT_CLICKHOUSE_EXPORT_BATCH_SIZE: usize = 64;
/// ClickHouse 异步事实投递允许配置的最大批量大小。
pub const MAX_CLICKHOUSE_EXPORT_BATCH_SIZE: usize = 256;
/// ClickHouse 异步事实投递的默认扫描周期。
pub const DEFAULT_CLICKHOUSE_EXPORT_INTERVAL_SECS: u64 = 5;
/// ClickHouse 异步事实投递允许配置的最大扫描周期。
pub const MAX_CLICKHOUSE_EXPORT_INTERVAL_SECS: u64 = 3_600;
/// ClickHouse 异步事实投递的默认请求超时。
pub const DEFAULT_CLICKHOUSE_EXPORT_TIMEOUT_SECS: u64 = 5;
/// ClickHouse 异步事实投递允许配置的最大请求超时。
pub const MAX_CLICKHOUSE_EXPORT_TIMEOUT_SECS: u64 = 300;
/// ClickHouse 异步事实投递的默认请求体上限。
pub const DEFAULT_CLICKHOUSE_EXPORT_REQUEST_BYTES: usize = 4 * 1024 * 1024;
/// ClickHouse 异步事实投递允许配置的最大请求体上限。
pub const MAX_CLICKHOUSE_EXPORT_REQUEST_BYTES: usize = 16 * 1024 * 1024;
/// ClickHouse 历史事实单轮默认回填大小。
pub const DEFAULT_CLICKHOUSE_EXPORT_BACKFILL_BATCH_SIZE: usize = 128;
/// ClickHouse 历史事实单轮最大回填大小。
pub const MAX_CLICKHOUSE_EXPORT_BACKFILL_BATCH_SIZE: usize = 256;
const MAX_CLICKHOUSE_ANALYTICS_QUERY_BYTES: usize = 64 * 1024;
const MAX_CLICKHOUSE_ANALYTICS_USERNAME_BYTES: usize = 256;
const MAX_CLICKHOUSE_ANALYTICS_PASSWORD_BYTES: usize = 1024;

/// 经过规范化的 HTTP(S) CORS Origin，不包含路径、凭据或通配语义。
#[derive(Clone, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CorsOrigin(String);

impl CorsOrigin {
    /// 返回可直接用于 `Origin` 精确比较的规范 ASCII 文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for CorsOrigin {
    type Err = CorsOriginError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty()
            || value.trim() != value
            || matches!(value, "*" | "null")
            || !has_strict_origin_shape(value)
        {
            return Err(CorsOriginError);
        }
        let parsed = Url::parse(value).map_err(|_| CorsOriginError)?;
        let supported_scheme = matches!(parsed.scheme(), "http" | "https");
        let has_credentials = !parsed.username().is_empty() || parsed.password().is_some();
        if !supported_scheme
            || !parsed.has_host()
            || has_credentials
            || parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(CorsOriginError);
        }

        let canonical = parsed.origin().ascii_serialization();
        if canonical == "null" {
            return Err(CorsOriginError);
        }
        Ok(Self(canonical))
    }
}

/// 在 URL 规范化前拒绝 userinfo、点路径等会被解析器消除的非 Origin 结构。
fn has_strict_origin_shape(value: &str) -> bool {
    let Some((_, scheme_relative)) = value.split_once("://") else {
        return false;
    };
    let authority_end = scheme_relative
        .find(['/', '?', '#'])
        .unwrap_or(scheme_relative.len());
    let authority = &scheme_relative[..authority_end];
    let suffix = &scheme_relative[authority_end..];

    !authority.is_empty()
        && !authority.contains('@')
        && !authority.contains('\\')
        && !authority.ends_with(':')
        && matches!(suffix, "" | "/")
}

impl<'de> Deserialize<'de> for CorsOrigin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

impl fmt::Debug for CorsOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CorsOrigin")
            .field(&"<已配置>")
            .finish()
    }
}

/// 鉴权使用的可信客户端 IP 来源。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClientIpSource {
    /// 只使用 TCP 连接对端，完全忽略转发地址请求头。
    #[default]
    Peer,
    /// 仅在 TCP 对端属于静态可信代理网络时解析 `X-Forwarded-For`。
    XForwardedFor,
}

/// 服务监听与入站网络信任配置。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    bind: SocketAddr,
    cors_allowed_origins: Vec<CorsOrigin>,
    client_ip_source: ClientIpSource,
    trusted_proxy_cidrs: Vec<IpCidr>,
    shutdown_timeout_secs: u64,
    /// 运行目录下可选的外部前端模板目录。
    frontend_template_directory: PathBuf,
}

impl ServerConfig {
    /// 返回服务监听地址。
    #[must_use]
    pub const fn bind(&self) -> SocketAddr {
        self.bind
    }

    /// 返回允许访问业务路由的规范 CORS Origin；空列表表示关闭跨域。
    #[must_use]
    pub fn cors_allowed_origins(&self) -> &[CorsOrigin] {
        &self.cors_allowed_origins
    }

    /// 返回鉴权使用的可信客户端 IP 来源。
    #[must_use]
    pub const fn client_ip_source(&self) -> ClientIpSource {
        self.client_ip_source
    }

    /// 返回允许提供转发地址链的静态可信 TCP 对端网络。
    #[must_use]
    pub fn trusted_proxy_cidrs(&self) -> &[IpCidr] {
        &self.trusted_proxy_cidrs
    }

    /// 返回收到关闭信号后等待在途请求完成的秒数。
    #[must_use]
    pub const fn shutdown_timeout_secs(&self) -> u64 {
        self.shutdown_timeout_secs
    }

    /// 返回外部前端模板目录。相对路径以进程运行目录为基准。
    #[must_use]
    pub fn frontend_template_directory(&self) -> &Path {
        &self.frontend_template_directory
    }

    /// 替换业务路由 CORS 白名单，供嵌入式启动与测试构造使用。
    #[must_use]
    pub fn with_cors_allowed_origins(
        mut self,
        origins: impl IntoIterator<Item = CorsOrigin>,
    ) -> Self {
        self.cors_allowed_origins = origins.into_iter().collect();
        self
    }

    /// 替换可信客户端 IP 来源与代理网络，供嵌入式启动和测试构造使用。
    #[must_use]
    pub fn with_client_ip_source(
        mut self,
        source: ClientIpSource,
        trusted_proxy_cidrs: impl IntoIterator<Item = IpCidr>,
    ) -> Self {
        self.client_ip_source = source;
        self.trusted_proxy_cidrs = trusted_proxy_cidrs.into_iter().collect();
        self
    }

    /// 替换外部前端模板目录，供嵌入式启动与测试构造使用。
    #[must_use]
    pub fn with_frontend_template_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.frontend_template_directory = directory.into();
        self
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080"
                .parse()
                .expect("内建监听地址必须是有效的 SocketAddr"),
            cors_allowed_origins: Vec::new(),
            client_ip_source: ClientIpSource::Peer,
            trusted_proxy_cidrs: Vec::new(),
            shutdown_timeout_secs: DEFAULT_SHUTDOWN_TIMEOUT_SECS,
            frontend_template_directory: PathBuf::from("public/templates"),
        }
    }
}

/// AnyFlows 自身结构化日志的最高详细级别。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// 完全关闭 tracing 输出。
    Off,
    /// 仅输出错误。
    Error,
    /// 输出警告与错误。
    Warn,
    /// 输出生产默认的信息、警告与错误。
    #[default]
    Info,
    /// 输出调试及以上级别。
    Debug,
    /// 输出全部 tracing 事件。
    Trace,
}

/// tracing 启动设置；运行期调整通过 telemetry reload handle 完成。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TelemetrySettings {
    level: LogLevel,
}

impl TelemetrySettings {
    /// 使用指定日志级别构造设置。
    #[must_use]
    pub const fn new(level: LogLevel) -> Self {
        Self { level }
    }

    /// 返回 AnyFlows 自身日志的最高详细级别。
    #[must_use]
    pub const fn level(&self) -> LogLevel {
        self.level
    }
}

/// 数据库启动配置。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    url: SecretString,
    migration_timeout_secs: Option<u64>,
    health_check_timeout_secs: Option<u64>,
}

impl DatabaseConfig {
    /// 返回受保护的数据库连接 URL；下层连接器必须显式读取明文。
    #[must_use]
    pub const fn url(&self) -> &SecretString {
        &self.url
    }

    /// 返回启动迁移截止时间覆盖（秒）；缺省时沿用数据库层默认值。
    #[must_use]
    pub const fn migration_timeout_secs(&self) -> Option<u64> {
        self.migration_timeout_secs
    }

    /// 返回数据库健康检查截止时间覆盖（秒）；缺省时沿用数据库层默认值。
    #[must_use]
    pub const fn health_check_timeout_secs(&self) -> Option<u64> {
        self.health_check_timeout_secs
    }
}

/// 计费增量 WAL 与落库节奏的启动期设置。
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BillingSettings {
    batch_enabled: bool,
    flush_interval_secs: u64,
    flush_timeout_secs: u64,
    usage_record_queue_capacity: usize,
    usage_record_worker_count: usize,
    wal_directory: PathBuf,
}

impl BillingSettings {
    /// 返回是否由后台任务按周期合并落库。
    #[must_use]
    pub const fn batch_enabled(&self) -> bool {
        self.batch_enabled
    }

    /// 返回批量模式的周期 flush 秒数。
    #[must_use]
    pub const fn flush_interval_secs(&self) -> u64 {
        self.flush_interval_secs
    }

    /// 返回启动恢复与关闭最终 flush 共用的硬截止秒数。
    #[must_use]
    pub const fn flush_timeout_secs(&self) -> u64 {
        self.flush_timeout_secs
    }

    /// 返回用量记录非阻塞接收队列的最大未确认事实数。
    #[must_use]
    pub const fn usage_record_queue_capacity(&self) -> usize {
        self.usage_record_queue_capacity
    }

    /// 返回并发运行的用量记录持久化 worker 数量。
    #[must_use]
    pub const fn usage_record_worker_count(&self) -> usize {
        self.usage_record_worker_count
    }

    /// 返回本实例独占的 WAL 目录；调用方不得将路径写入日志或错误。
    #[must_use]
    pub fn wal_directory(&self) -> &Path {
        &self.wal_directory
    }
}

impl Default for BillingSettings {
    fn default() -> Self {
        Self {
            batch_enabled: false,
            flush_interval_secs: DEFAULT_BILLING_FLUSH_INTERVAL_SECS,
            flush_timeout_secs: DEFAULT_BILLING_FLUSH_TIMEOUT_SECS,
            usage_record_queue_capacity: DEFAULT_USAGE_RECORD_QUEUE_CAPACITY,
            usage_record_worker_count: DEFAULT_USAGE_RECORD_WORKER_COUNT,
            wal_directory: PathBuf::from("data/billing-wal"),
        }
    }
}

/// 订阅窗口推进与到期迁移的启动期周期任务设置。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SubscriptionSettings {
    enabled: bool,
    batch_size: usize,
    interval_secs: u64,
    max_batches_per_run: usize,
}

impl SubscriptionSettings {
    /// 返回是否注册订阅生命周期周期任务。
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// 返回单次数据库分页最多读取的订阅数量。
    #[must_use]
    pub const fn batch_size(self) -> usize {
        self.batch_size
    }

    /// 返回两轮订阅生命周期扫描之间的等待秒数。
    #[must_use]
    pub const fn interval_secs(self) -> u64 {
        self.interval_secs
    }

    /// 返回每类订阅状态单轮最多扫描的批次数。
    #[must_use]
    pub const fn max_batches_per_run(self) -> usize {
        self.max_batches_per_run
    }
}

impl Default for SubscriptionSettings {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_SUBSCRIPTION_CYCLE_ENABLED,
            batch_size: DEFAULT_SUBSCRIPTION_CYCLE_BATCH_SIZE,
            interval_secs: DEFAULT_SUBSCRIPTION_CYCLE_INTERVAL_SECS,
            max_batches_per_run: DEFAULT_SUBSCRIPTION_CYCLE_MAX_BATCHES_PER_RUN,
        }
    }
}

/// 支付 Provider 的启动期安全配置。
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PaymentSettings {
    stripe_secret_key: Option<SecretString>,
    stripe_publishable_key: Option<String>,
    stripe_webhook_secret: Option<SecretString>,
    stripe_signature_tolerance_secs: u64,
}

/// 支付宝实名认证的启动期安全配置。
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AlipayVerificationSettings {
    enabled: bool,
    app_id: Option<SecretString>,
    private_key: Option<SecretString>,
    public_key: Option<SecretString>,
    gateway_url: String,
    biz_code: String,
    timeout_secs: u64,
}

impl AlipayVerificationSettings {
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn app_id(&self) -> Option<&SecretString> {
        self.app_id.as_ref()
    }

    #[must_use]
    pub const fn private_key(&self) -> Option<&SecretString> {
        self.private_key.as_ref()
    }

    #[must_use]
    pub const fn public_key(&self) -> Option<&SecretString> {
        self.public_key.as_ref()
    }

    #[must_use]
    pub fn gateway_url(&self) -> &str {
        &self.gateway_url
    }

    #[must_use]
    pub fn biz_code(&self) -> &str {
        &self.biz_code
    }

    #[must_use]
    pub const fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }
}

impl Default for AlipayVerificationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            app_id: None,
            private_key: None,
            public_key: None,
            gateway_url: "https://openapi.alipay.com/gateway.do".to_owned(),
            biz_code: "FACE".to_owned(),
            timeout_secs: DEFAULT_ALIPAY_VERIFICATION_TIMEOUT_SECS,
        }
    }
}

/// 账号实名认证 provider 的启动期配置。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccountVerificationSettings {
    alipay: AlipayVerificationSettings,
}

impl AccountVerificationSettings {
    #[must_use]
    pub const fn alipay(&self) -> &AlipayVerificationSettings {
        &self.alipay
    }
}

impl PaymentSettings {
    /// 返回 Stripe 服务端 API 密钥；未配置时禁止创建 PaymentIntent。
    #[must_use]
    pub fn stripe_secret_key(&self) -> Option<&SecretString> {
        self.stripe_secret_key.as_ref()
    }

    /// 返回可安全交给 Stripe.js 的 publishable key。
    #[must_use]
    pub fn stripe_publishable_key(&self) -> Option<&str> {
        self.stripe_publishable_key.as_deref()
    }

    /// 返回 Stripe webhook 密钥；未配置时回调路由保持禁用。
    #[must_use]
    pub fn stripe_webhook_secret(&self) -> Option<&SecretString> {
        self.stripe_webhook_secret.as_ref()
    }

    /// 返回 Stripe webhook 的重放保护时间容差。
    #[must_use]
    pub const fn stripe_signature_tolerance_secs(&self) -> u64 {
        self.stripe_signature_tolerance_secs
    }
}

impl Default for PaymentSettings {
    fn default() -> Self {
        Self {
            stripe_secret_key: None,
            stripe_publishable_key: None,
            stripe_webhook_secret: None,
            stripe_signature_tolerance_secs: DEFAULT_PAYMENT_SIGNATURE_TOLERANCE_SECS,
        }
    }
}

/// 自动禁用渠道的周期探活设置。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChannelProbeSettings {
    enabled: bool,
    batch_size: usize,
    interval_secs: u64,
    probe_timeout_secs: u64,
}

impl ChannelProbeSettings {
    /// 返回是否注册渠道探活后台任务。
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// 返回单轮读取的最大自动禁用渠道数量。
    #[must_use]
    pub const fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// 返回两轮探活之间的等待秒数。
    #[must_use]
    pub const fn interval_secs(&self) -> u64 {
        self.interval_secs
    }

    /// 返回未设置渠道超时时，单次真实探活使用的默认秒数。
    #[must_use]
    pub const fn probe_timeout_secs(&self) -> u64 {
        self.probe_timeout_secs
    }
}

impl Default for ChannelProbeSettings {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_CHANNEL_PROBE_ENABLED,
            batch_size: DEFAULT_CHANNEL_PROBE_BATCH_SIZE,
            interval_secs: DEFAULT_CHANNEL_PROBE_INTERVAL_SECS,
            probe_timeout_secs: DEFAULT_CHANNEL_PROBE_TIMEOUT_SECS,
        }
    }
}

/// 凭据密文解密使用的单密钥 v1 启动配置。
///
/// 探活关闭时允许保持未配置；启用真实探活时 `key_id` 与 `key` 必须同时存在，
/// 其中 `key` 是 32 字节原始密钥的 Base64URL 无填充文本。
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CredentialEncryptionSettings {
    key_id: Option<String>,
    key: Option<SecretString>,
}

impl CredentialEncryptionSettings {
    /// 返回密文封套选择密钥时使用的稳定标识。
    #[must_use]
    pub fn key_id(&self) -> Option<&str> {
        self.key_id.as_deref()
    }

    /// 返回受保护的 Base64URL 密钥文本。
    #[must_use]
    pub fn key(&self) -> Option<&SecretString> {
        self.key.as_ref()
    }

    /// 返回当前是否已完整配置密钥标识和密钥材料。
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        self.key_id.is_some() && self.key.is_some()
    }
}

impl fmt::Debug for CredentialEncryptionSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialEncryptionSettings")
            .field("configured", &self.is_configured())
            .finish()
    }
}

impl fmt::Debug for BillingSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingSettings")
            .field("batch_enabled", &self.batch_enabled)
            .field("flush_interval_secs", &self.flush_interval_secs)
            .field("flush_timeout_secs", &self.flush_timeout_secs)
            .field(
                "usage_record_queue_capacity",
                &self.usage_record_queue_capacity,
            )
            .field("usage_record_worker_count", &self.usage_record_worker_count)
            .field("wal_directory", &"<已配置>")
            .finish()
    }
}

/// 可选 Redis 连接配置；缺省时由缓存层运行在本地模式。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RedisSettings {
    url: Option<SecretString>,
    /// 请求限流使用的 Redis 键空间；缺省值由服务端保持稳定兼容命名。
    request_rate_limit_namespace: Option<String>,
}

impl RedisSettings {
    /// 返回受保护的 Redis URL；为 `None` 时不得尝试建立远程连接。
    #[must_use]
    pub fn url(&self) -> Option<&SecretString> {
        self.url.as_ref()
    }

    /// 返回请求限流键空间；未配置时由服务端使用默认命名。
    #[must_use]
    pub fn request_rate_limit_namespace(&self) -> Option<&str> {
        self.request_rate_limit_namespace.as_deref()
    }
}

/// 可选的 ClickHouse 管理看板分析读取配置；整个配置段缺失时保持主库读取。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClickHouseAnalyticsSettings {
    endpoint: SecretString,
    query: SecretString,
    username: SecretString,
    password: SecretString,
    #[serde(default = "default_clickhouse_analytics_timeout_secs")]
    timeout_secs: u64,
    #[serde(default = "default_clickhouse_analytics_response_bytes")]
    max_response_bytes: usize,
}

impl ClickHouseAnalyticsSettings {
    /// 返回仅供 ClickHouse HTTP 适配器使用的受保护端点。
    #[must_use]
    pub const fn endpoint(&self) -> &SecretString {
        &self.endpoint
    }

    /// 返回固定的参数化分析查询；调用方不得记录其正文。
    #[must_use]
    pub const fn query(&self) -> &SecretString {
        &self.query
    }

    /// 返回 ClickHouse 只读用户名称。
    #[must_use]
    pub const fn username(&self) -> &SecretString {
        &self.username
    }

    /// 返回 ClickHouse 只读用户密码。
    #[must_use]
    pub const fn password(&self) -> &SecretString {
        &self.password
    }

    /// 返回单次分析查询的硬超时秒数。
    #[must_use]
    pub const fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }

    /// 返回单行分析快照允许读取的最大响应字节数。
    #[must_use]
    pub const fn max_response_bytes(&self) -> usize {
        self.max_response_bytes
    }
}

impl fmt::Debug for ClickHouseAnalyticsSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClickHouseAnalyticsSettings")
            .field("endpoint", &"<redacted>")
            .field("query", &"<redacted>")
            .field("username", &"<redacted>")
            .field("password", &"<redacted>")
            .field("timeout_secs", &self.timeout_secs)
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

/// 可选的 ClickHouse 异步事实投递配置；与只读分析账号严格分离。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClickHouseExportSettings {
    endpoint: SecretString,
    usage_insert_query: SecretString,
    outcome_insert_query: SecretString,
    username: SecretString,
    password: SecretString,
    #[serde(default = "default_clickhouse_export_batch_size")]
    batch_size: usize,
    #[serde(default = "default_clickhouse_export_interval_secs")]
    interval_secs: u64,
    #[serde(default = "default_clickhouse_export_timeout_secs")]
    timeout_secs: u64,
    #[serde(default = "default_clickhouse_export_request_bytes")]
    max_request_bytes: usize,
    #[serde(default = "default_clickhouse_export_backfill_batch_size")]
    backfill_batch_size: usize,
}

impl ClickHouseExportSettings {
    /// 返回事实投递端点。
    #[must_use]
    pub const fn endpoint(&self) -> &SecretString {
        &self.endpoint
    }
    /// 返回用量事实 INSERT 查询。
    #[must_use]
    pub const fn usage_insert_query(&self) -> &SecretString {
        &self.usage_insert_query
    }
    /// 返回请求终态事实 INSERT 查询。
    #[must_use]
    pub const fn outcome_insert_query(&self) -> &SecretString {
        &self.outcome_insert_query
    }
    /// 返回独立的 ClickHouse 写入用户名。
    #[must_use]
    pub const fn username(&self) -> &SecretString {
        &self.username
    }
    /// 返回独立的 ClickHouse 写入密码。
    #[must_use]
    pub const fn password(&self) -> &SecretString {
        &self.password
    }
    /// 返回单轮投递批量大小。
    #[must_use]
    pub const fn batch_size(&self) -> usize {
        self.batch_size
    }
    /// 返回后台扫描周期秒数。
    #[must_use]
    pub const fn interval_secs(&self) -> u64 {
        self.interval_secs
    }
    /// 返回单次 ClickHouse 请求超时秒数。
    #[must_use]
    pub const fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }
    /// 返回单次批量请求体硬上限。
    #[must_use]
    pub const fn max_request_bytes(&self) -> usize {
        self.max_request_bytes
    }
    /// 返回历史事实单轮回填大小。
    #[must_use]
    pub const fn backfill_batch_size(&self) -> usize {
        self.backfill_batch_size
    }
}

impl fmt::Debug for ClickHouseExportSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClickHouseExportSettings")
            .field("endpoint", &"<redacted>")
            .field("usage_insert_query", &"<redacted>")
            .field("outcome_insert_query", &"<redacted>")
            .field("username", &"<redacted>")
            .field("password", &"<redacted>")
            .field("batch_size", &self.batch_size)
            .field("interval_secs", &self.interval_secs)
            .field("timeout_secs", &self.timeout_secs)
            .field("max_request_bytes", &self.max_request_bytes)
            .field("backfill_batch_size", &self.backfill_batch_size)
            .finish()
    }
}

const fn default_clickhouse_export_batch_size() -> usize {
    DEFAULT_CLICKHOUSE_EXPORT_BATCH_SIZE
}
const fn default_clickhouse_export_interval_secs() -> u64 {
    DEFAULT_CLICKHOUSE_EXPORT_INTERVAL_SECS
}
const fn default_clickhouse_export_timeout_secs() -> u64 {
    DEFAULT_CLICKHOUSE_EXPORT_TIMEOUT_SECS
}
const fn default_clickhouse_export_request_bytes() -> usize {
    DEFAULT_CLICKHOUSE_EXPORT_REQUEST_BYTES
}
const fn default_clickhouse_export_backfill_batch_size() -> usize {
    DEFAULT_CLICKHOUSE_EXPORT_BACKFILL_BATCH_SIZE
}

const fn default_clickhouse_analytics_timeout_secs() -> u64 {
    DEFAULT_CLICKHOUSE_ANALYTICS_TIMEOUT_SECS
}

const fn default_clickhouse_analytics_response_bytes() -> usize {
    DEFAULT_CLICKHOUSE_ANALYTICS_RESPONSE_BYTES
}

/// 下游 API Key 认证的静态设置。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AuthSettings {
    allow_query_api_key: bool,
    lookup_timeout_secs: u64,
    session_signing_key: Option<SecretString>,
    session_ttl_secs: u64,
}

impl AuthSettings {
    /// 返回是否允许通过查询参数传递 API Key。
    #[must_use]
    pub const fn allow_query_api_key(&self) -> bool {
        self.allow_query_api_key
    }

    /// 返回认证仓储查询的硬超时秒数。
    #[must_use]
    pub const fn lookup_timeout_secs(&self) -> u64 {
        self.lookup_timeout_secs
    }

    /// 返回管理会话签名密钥；缺省时由 Bootstrap 拒绝启动监听。
    #[must_use]
    pub fn session_signing_key(&self) -> Option<&SecretString> {
        self.session_signing_key.as_ref()
    }

    /// 返回管理会话 JWT 的有效期秒数。
    #[must_use]
    pub const fn session_ttl_secs(&self) -> u64 {
        self.session_ttl_secs
    }
}

impl Default for AuthSettings {
    fn default() -> Self {
        Self {
            allow_query_api_key: false,
            lookup_timeout_secs: DEFAULT_AUTH_LOOKUP_TIMEOUT_SECS,
            session_signing_key: None,
            session_ttl_secs: DEFAULT_AUTH_SESSION_TTL_SECS,
        }
    }
}

/// 登录与注册使用的 Turnstile 启动期配置。
///
/// 只有同时提供 site key 与 secret key 时才启用；任意半配置都会在启动期拒绝，
/// 避免前端显示挑战但服务端无法完成验证。
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TurnstileSettings {
    site_key: Option<String>,
    secret_key: Option<SecretString>,
    timeout_secs: u64,
}

impl TurnstileSettings {
    /// 返回公开给浏览器的 site key；未配置时 Turnstile 关闭。
    #[must_use]
    pub fn site_key(&self) -> Option<&str> {
        self.site_key.as_deref()
    }

    /// 返回仅供服务端验证使用的 Turnstile secret key。
    #[must_use]
    pub fn secret_key(&self) -> Option<&SecretString> {
        self.secret_key.as_ref()
    }

    /// 返回服务端验证请求的硬超时。
    #[must_use]
    pub const fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }

    /// 返回是否已完成成对配置。
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.site_key.is_some() && self.secret_key.is_some()
    }
}

impl Default for TurnstileSettings {
    fn default() -> Self {
        Self {
            site_key: None,
            secret_key: None,
            timeout_secs: DEFAULT_TURNSTILE_TIMEOUT_SECS,
        }
    }
}

/// 只使用公共客户端标识的 OAuth provider 配置。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OAuthPublicClientSettings {
    client_id: SecretString,
}

impl OAuthPublicClientSettings {
    /// 返回只应交给闭合 provider profile 的客户端标识。
    #[must_use]
    pub const fn client_id(&self) -> &SecretString {
        &self.client_id
    }
}

/// 同时要求客户端标识与密钥的 OAuth provider 配置。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OAuthConfidentialClientSettings {
    client_id: SecretString,
    client_secret: SecretString,
}

impl OAuthConfidentialClientSettings {
    /// 返回只应交给闭合 provider profile 的客户端标识。
    #[must_use]
    pub const fn client_id(&self) -> &SecretString {
        &self.client_id
    }

    /// 返回只应移动到受控 token 交换器的客户端密钥。
    #[must_use]
    pub const fn client_secret(&self) -> &SecretString {
        &self.client_secret
    }
}

/// 上游订阅账号 OAuth 的启动期闭合配置。
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OAuthSettings {
    max_pending_authorizations: usize,
    session_ttl_secs: u64,
    refresh_enabled: bool,
    refresh_interval_secs: u64,
    refresh_before_expiry_secs: u64,
    refresh_batch_size: usize,
    refresh_concurrency: usize,
    claude_code: Option<OAuthPublicClientSettings>,
    codex: Option<OAuthPublicClientSettings>,
    gemini: Option<OAuthConfidentialClientSettings>,
    antigravity: Option<OAuthConfidentialClientSettings>,
}

impl OAuthSettings {
    /// 返回单实例待授权会话容量。
    #[must_use]
    pub const fn max_pending_authorizations(&self) -> usize {
        self.max_pending_authorizations
    }

    /// 返回授权会话有效期秒数。
    #[must_use]
    pub const fn session_ttl_secs(&self) -> u64 {
        self.session_ttl_secs
    }

    /// 返回是否启用 OAuth token 周期刷新任务。
    #[must_use]
    pub const fn refresh_enabled(&self) -> bool {
        self.refresh_enabled
    }

    /// 返回两轮 OAuth 到期候选扫描之间的秒数。
    #[must_use]
    pub const fn refresh_interval_secs(&self) -> u64 {
        self.refresh_interval_secs
    }

    /// 返回 access token 到期前进入刷新候选集的秒数。
    #[must_use]
    pub const fn refresh_before_expiry_secs(&self) -> u64 {
        self.refresh_before_expiry_secs
    }

    /// 返回单轮最多读取的 OAuth 刷新候选数。
    #[must_use]
    pub const fn refresh_batch_size(&self) -> usize {
        self.refresh_batch_size
    }

    /// 返回单实例同时执行的 OAuth 刷新上限。
    #[must_use]
    pub const fn refresh_concurrency(&self) -> usize {
        self.refresh_concurrency
    }

    /// 返回可选 Claude Code 公共客户端配置。
    #[must_use]
    pub const fn claude_code(&self) -> Option<&OAuthPublicClientSettings> {
        self.claude_code.as_ref()
    }

    /// 返回可选 Codex 公共客户端配置。
    #[must_use]
    pub const fn codex(&self) -> Option<&OAuthPublicClientSettings> {
        self.codex.as_ref()
    }

    /// 返回可选 Gemini CLI 客户端配置。
    #[must_use]
    pub const fn gemini(&self) -> Option<&OAuthConfidentialClientSettings> {
        self.gemini.as_ref()
    }

    /// 返回可选 Antigravity 客户端配置。
    #[must_use]
    pub const fn antigravity(&self) -> Option<&OAuthConfidentialClientSettings> {
        self.antigravity.as_ref()
    }

    /// 返回当前实际配置的 provider 数量。
    #[must_use]
    pub fn configured_provider_count(&self) -> usize {
        [
            self.claude_code.is_some(),
            self.codex.is_some(),
            self.gemini.is_some(),
            self.antigravity.is_some(),
        ]
        .into_iter()
        .filter(|configured| *configured)
        .count()
    }
}

impl Default for OAuthSettings {
    fn default() -> Self {
        Self {
            max_pending_authorizations: DEFAULT_OAUTH_MAX_PENDING_AUTHORIZATIONS,
            session_ttl_secs: DEFAULT_OAUTH_SESSION_TTL_SECS,
            refresh_enabled: DEFAULT_OAUTH_REFRESH_ENABLED,
            refresh_interval_secs: DEFAULT_OAUTH_REFRESH_INTERVAL_SECS,
            refresh_before_expiry_secs: DEFAULT_OAUTH_REFRESH_BEFORE_EXPIRY_SECS,
            refresh_batch_size: DEFAULT_OAUTH_REFRESH_BATCH_SIZE,
            refresh_concurrency: DEFAULT_OAUTH_REFRESH_CONCURRENCY,
            claude_code: None,
            codex: None,
            gemini: None,
            antigravity: None,
        }
    }
}

/// M0 静态 OpenAI-compatible 上游的兼容配置。
///
/// M1 生产 Chat 已改用数据库运行时渠道目录，本配置不参与生产选路，仅暂时保留旧配置文件
/// 的解析兼容。URL、模型和密钥仍按敏感运行参数处理。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiUpstreamConfig {
    base_url: String,
    model: String,
    api_key: SecretString,
}

impl OpenAiUpstreamConfig {
    /// 返回 OpenAI-compatible 服务根或反向代理前缀；调用方不得写入日志。
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// 返回当前唯一允许转发的模型；调用方不得写入日志。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回上游 API Key；只应在构造适配器凭据时短暂读取明文。
    #[must_use]
    pub const fn api_key(&self) -> &SecretString {
        &self.api_key
    }
}

impl fmt::Debug for OpenAiUpstreamConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiUpstreamConfig")
            .field("base_url", &"<已配置>")
            .field("model", &"<已配置>")
            .field("api_key", &"<已脱敏>")
            .finish()
    }
}

/// HTTP 客户端池与三段超时配置。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HttpClientSettings {
    max_cached_clients: Option<usize>,
    connect_timeout_secs: Option<u64>,
    read_timeout_secs: Option<u64>,
    request_timeout_secs: Option<u64>,
    proxy_url: Option<SecretString>,
    trust_proxy_dns: bool,
}

impl HttpClientSettings {
    /// 返回 Client LRU 最大容量覆盖；缺省时沿用 HTTP 客户端层默认值。
    #[must_use]
    pub const fn max_cached_clients(&self) -> Option<usize> {
        self.max_cached_clients
    }

    /// 返回连接阶段超时覆盖（秒）。
    #[must_use]
    pub const fn connect_timeout_secs(&self) -> Option<u64> {
        self.connect_timeout_secs
    }

    /// 返回读取阶段超时覆盖（秒）。
    #[must_use]
    pub const fn read_timeout_secs(&self) -> Option<u64> {
        self.read_timeout_secs
    }

    /// 返回单请求总超时覆盖（秒）。
    #[must_use]
    pub const fn request_timeout_secs(&self) -> Option<u64> {
        self.request_timeout_secs
    }

    /// 返回显式强制代理地址；缺省时直连且忽略系统代理环境变量。
    #[must_use]
    pub fn proxy_url(&self) -> Option<&SecretString> {
        self.proxy_url.as_ref()
    }

    /// 返回是否将目标域名解析与私网阻断责任委托给受信代理。
    #[must_use]
    pub const fn trust_proxy_dns(&self) -> bool {
        self.trust_proxy_dns
    }
}

/// 应用启动期配置。运营期 Option 热更新不属于此类型。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    #[serde(default)]
    server: ServerConfig,
    #[serde(default)]
    telemetry: TelemetrySettings,
    database: DatabaseConfig,
    #[serde(default)]
    billing: BillingSettings,
    #[serde(default)]
    subscription: SubscriptionSettings,
    #[serde(default)]
    payment: PaymentSettings,
    #[serde(default)]
    account_verification: AccountVerificationSettings,
    #[serde(default)]
    channel_probe: ChannelProbeSettings,
    #[serde(default)]
    credential_encryption: CredentialEncryptionSettings,
    #[serde(default)]
    redis: RedisSettings,
    #[serde(default)]
    clickhouse_analytics: Option<ClickHouseAnalyticsSettings>,
    #[serde(default)]
    clickhouse_export: Option<ClickHouseExportSettings>,
    #[serde(default)]
    auth: AuthSettings,
    #[serde(default)]
    turnstile: TurnstileSettings,
    #[serde(default)]
    oauth: OAuthSettings,
    #[serde(default)]
    openai_upstream: Option<OpenAiUpstreamConfig>,
    #[serde(default)]
    http_client: HttpClientSettings,
}

impl AppConfig {
    /// 返回服务监听配置。
    #[must_use]
    pub const fn server(&self) -> &ServerConfig {
        &self.server
    }

    /// 返回 tracing 启动设置。
    #[must_use]
    pub const fn telemetry(&self) -> &TelemetrySettings {
        &self.telemetry
    }

    /// 返回数据库配置。
    #[must_use]
    pub const fn database(&self) -> &DatabaseConfig {
        &self.database
    }

    /// 返回计费 WAL 与落库节奏设置。
    #[must_use]
    pub const fn billing(&self) -> &BillingSettings {
        &self.billing
    }

    /// 返回订阅生命周期周期任务设置。
    #[must_use]
    pub const fn subscription(&self) -> &SubscriptionSettings {
        &self.subscription
    }

    /// 返回支付 Provider 的启动期安全配置。
    #[must_use]
    pub const fn payment(&self) -> &PaymentSettings {
        &self.payment
    }

    /// 返回账号实名认证 provider 的启动期配置。
    #[must_use]
    pub const fn account_verification(&self) -> &AccountVerificationSettings {
        &self.account_verification
    }

    /// 返回自动禁用渠道的周期探活设置。
    #[must_use]
    pub const fn channel_probe(&self) -> &ChannelProbeSettings {
        &self.channel_probe
    }

    /// 返回凭据密文解密使用的启动密钥设置。
    #[must_use]
    pub const fn credential_encryption(&self) -> &CredentialEncryptionSettings {
        &self.credential_encryption
    }

    /// 返回可选 Redis 配置。
    #[must_use]
    pub const fn redis(&self) -> &RedisSettings {
        &self.redis
    }

    /// 返回可选的 ClickHouse 分析事实源；缺失时继续使用事务主库。
    #[must_use]
    pub const fn clickhouse_analytics(&self) -> Option<&ClickHouseAnalyticsSettings> {
        self.clickhouse_analytics.as_ref()
    }

    /// 返回可选的 ClickHouse 异步事实投递配置；缺失时导出保持关闭。
    #[must_use]
    pub const fn clickhouse_export(&self) -> Option<&ClickHouseExportSettings> {
        self.clickhouse_export.as_ref()
    }

    /// 返回下游 API Key 认证设置。
    #[must_use]
    pub const fn auth(&self) -> &AuthSettings {
        &self.auth
    }

    /// 返回登录与注册使用的 Turnstile 启动配置。
    #[must_use]
    pub const fn turnstile(&self) -> &TurnstileSettings {
        &self.turnstile
    }

    /// 返回上游订阅账号 OAuth 启动配置。
    #[must_use]
    pub const fn oauth(&self) -> &OAuthSettings {
        &self.oauth
    }

    /// 返回可选的 M0 静态上游兼容配置；生产转发不得使用该值作为回退。
    #[must_use]
    pub const fn openai_upstream(&self) -> Option<&OpenAiUpstreamConfig> {
        self.openai_upstream.as_ref()
    }

    /// 返回 HTTP 客户端配置。
    #[must_use]
    pub const fn http_client(&self) -> &HttpClientSettings {
        &self.http_client
    }

    /// 校验敏感文本与所有受控数值，确保错误在启动期暴露。
    pub fn validate(&self) -> Result<(), ConfigError> {
        // 基础鉴权不代表公网安全闭环已完成，当前仍只允许本机监听。
        if !self.server.bind.ip().is_loopback() {
            return Err(ConfigError::InvalidField {
                field: "server.bind",
            });
        }
        if self.server.shutdown_timeout_secs == 0 {
            return Err(ConfigError::NonPositive {
                field: "server.shutdown_timeout_secs",
            });
        }
        if self.server.shutdown_timeout_secs > MAX_SHUTDOWN_TIMEOUT_SECS {
            return Err(ConfigError::OutOfRange {
                field: "server.shutdown_timeout_secs",
            });
        }
        if !valid_frontend_template_directory(&self.server.frontend_template_directory) {
            return Err(ConfigError::InvalidField {
                field: "server.frontend_template_directory",
            });
        }
        if self.server.trusted_proxy_cidrs.len() > MAX_TRUSTED_PROXY_CIDRS {
            return Err(ConfigError::OutOfRange {
                field: "server.trusted_proxy_cidrs",
            });
        }
        if self
            .server
            .trusted_proxy_cidrs
            .iter()
            .any(|network| network.is_universal())
        {
            return Err(ConfigError::InvalidField {
                field: "server.trusted_proxy_cidrs",
            });
        }
        match self.server.client_ip_source {
            ClientIpSource::Peer if !self.server.trusted_proxy_cidrs.is_empty() => {
                return Err(ConfigError::InvalidField {
                    field: "server.trusted_proxy_cidrs",
                });
            }
            ClientIpSource::XForwardedFor if self.server.trusted_proxy_cidrs.is_empty() => {
                return Err(ConfigError::InvalidField {
                    field: "server.client_ip_source",
                });
            }
            ClientIpSource::Peer | ClientIpSource::XForwardedFor => {}
        }
        if self.database.url.expose().trim().is_empty() {
            return Err(ConfigError::EmptyValue {
                field: "database.url",
            });
        }
        validate_turnstile_settings(&self.turnstile)?;
        if self.database.migration_timeout_secs == Some(0) {
            return Err(ConfigError::NonPositive {
                field: "database.migration_timeout_secs",
            });
        }
        if self.database.health_check_timeout_secs == Some(0) {
            return Err(ConfigError::NonPositive {
                field: "database.health_check_timeout_secs",
            });
        }
        if self
            .database
            .health_check_timeout_secs
            .is_some_and(|seconds| seconds > MAX_DATABASE_HEALTH_CHECK_TIMEOUT_SECS)
        {
            return Err(ConfigError::OutOfRange {
                field: "database.health_check_timeout_secs",
            });
        }
        if self.billing.flush_interval_secs == 0 {
            return Err(ConfigError::NonPositive {
                field: "billing.flush_interval_secs",
            });
        }
        if self.billing.flush_interval_secs > MAX_BILLING_FLUSH_INTERVAL_SECS {
            return Err(ConfigError::OutOfRange {
                field: "billing.flush_interval_secs",
            });
        }
        if self.billing.flush_timeout_secs == 0 {
            return Err(ConfigError::NonPositive {
                field: "billing.flush_timeout_secs",
            });
        }
        if self.billing.flush_timeout_secs > MAX_BILLING_FLUSH_TIMEOUT_SECS {
            return Err(ConfigError::OutOfRange {
                field: "billing.flush_timeout_secs",
            });
        }
        if self.billing.usage_record_queue_capacity == 0 {
            return Err(ConfigError::NonPositive {
                field: "billing.usage_record_queue_capacity",
            });
        }
        if self.billing.usage_record_queue_capacity > MAX_USAGE_RECORD_QUEUE_CAPACITY {
            return Err(ConfigError::OutOfRange {
                field: "billing.usage_record_queue_capacity",
            });
        }
        if self.billing.usage_record_worker_count == 0 {
            return Err(ConfigError::NonPositive {
                field: "billing.usage_record_worker_count",
            });
        }
        if self.billing.usage_record_worker_count > MAX_USAGE_RECORD_WORKER_COUNT {
            return Err(ConfigError::OutOfRange {
                field: "billing.usage_record_worker_count",
            });
        }
        if self.billing.wal_directory.as_os_str().is_empty() {
            return Err(ConfigError::EmptyValue {
                field: "billing.wal_directory",
            });
        }
        if self.subscription.batch_size == 0 {
            return Err(ConfigError::NonPositive {
                field: "subscription.batch_size",
            });
        }
        if self.subscription.batch_size > MAX_SUBSCRIPTION_CYCLE_BATCH_SIZE {
            return Err(ConfigError::OutOfRange {
                field: "subscription.batch_size",
            });
        }
        if self.subscription.interval_secs == 0 {
            return Err(ConfigError::NonPositive {
                field: "subscription.interval_secs",
            });
        }
        if self.subscription.interval_secs > MAX_SUBSCRIPTION_CYCLE_INTERVAL_SECS {
            return Err(ConfigError::OutOfRange {
                field: "subscription.interval_secs",
            });
        }
        if self.subscription.max_batches_per_run == 0 {
            return Err(ConfigError::NonPositive {
                field: "subscription.max_batches_per_run",
            });
        }
        if self.subscription.max_batches_per_run > MAX_SUBSCRIPTION_CYCLE_BATCHES_PER_RUN {
            return Err(ConfigError::OutOfRange {
                field: "subscription.max_batches_per_run",
            });
        }
        if self.payment.stripe_signature_tolerance_secs == 0 {
            return Err(ConfigError::NonPositive {
                field: "payment.stripe_signature_tolerance_secs",
            });
        }
        if self.payment.stripe_signature_tolerance_secs > MAX_PAYMENT_SIGNATURE_TOLERANCE_SECS {
            return Err(ConfigError::OutOfRange {
                field: "payment.stripe_signature_tolerance_secs",
            });
        }
        if let Some(secret) = self.payment.stripe_secret_key.as_ref() {
            validate_nonempty_text(
                secret.expose(),
                "payment.stripe_secret_key",
                MAX_PAYMENT_SECRET_KEY_BYTES,
            )?;
            if !secret
                .expose()
                .bytes()
                .all(|byte| (0x21..=0x7e).contains(&byte))
            {
                return Err(ConfigError::InvalidField {
                    field: "payment.stripe_secret_key",
                });
            }
            // 禁止创建无法通过可信 webhook 原子到账的真实支付订单。
            if self.payment.stripe_webhook_secret.is_none() {
                return Err(ConfigError::EmptyValue {
                    field: "payment.stripe_webhook_secret",
                });
            }
            if self.payment.stripe_publishable_key.is_none() {
                return Err(ConfigError::EmptyValue {
                    field: "payment.stripe_publishable_key",
                });
            }
        }
        if let Some(key) = self.payment.stripe_publishable_key.as_deref() {
            validate_nonempty_text(
                key,
                "payment.stripe_publishable_key",
                MAX_PAYMENT_PUBLISHABLE_KEY_BYTES,
            )?;
            if !key.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
                || !(key.starts_with("pk_test_") || key.starts_with("pk_live_"))
            {
                return Err(ConfigError::InvalidField {
                    field: "payment.stripe_publishable_key",
                });
            }
            // 单独公开客户端密钥无法创建订单，启动期直接拒绝半配置状态。
            if self.payment.stripe_secret_key.is_none() {
                return Err(ConfigError::EmptyValue {
                    field: "payment.stripe_secret_key",
                });
            }
        }
        if let Some(secret) = self.payment.stripe_webhook_secret.as_ref() {
            validate_nonempty_text(
                secret.expose(),
                "payment.stripe_webhook_secret",
                MAX_PAYMENT_WEBHOOK_SECRET_BYTES,
            )?;
            if secret
                .expose()
                .bytes()
                .any(|byte| byte < b' ' || byte == 0x7f)
            {
                return Err(ConfigError::InvalidField {
                    field: "payment.stripe_webhook_secret",
                });
            }
        }
        let alipay = self.account_verification.alipay();
        if alipay.timeout_secs() == 0 {
            return Err(ConfigError::NonPositive {
                field: "account_verification.alipay.timeout_secs",
            });
        }
        if alipay.timeout_secs() > MAX_ALIPAY_VERIFICATION_TIMEOUT_SECS {
            return Err(ConfigError::OutOfRange {
                field: "account_verification.alipay.timeout_secs",
            });
        }
        let gateway = Url::parse(alipay.gateway_url()).map_err(|_| ConfigError::InvalidField {
            field: "account_verification.alipay.gateway_url",
        })?;
        if gateway.scheme() != "https"
            || gateway.host_str().is_none()
            || gateway.username() != ""
            || gateway.password().is_some()
            || gateway.query().is_some()
            || gateway.fragment().is_some()
        {
            return Err(ConfigError::InvalidField {
                field: "account_verification.alipay.gateway_url",
            });
        }
        validate_nonempty_text(
            alipay.gateway_url(),
            "account_verification.alipay.gateway_url",
            MAX_ALIPAY_VERIFICATION_GATEWAY_BYTES,
        )?;
        validate_nonempty_text(
            alipay.biz_code(),
            "account_verification.alipay.biz_code",
            MAX_ALIPAY_VERIFICATION_BIZ_CODE_BYTES,
        )?;
        if !alipay
            .biz_code()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return Err(ConfigError::InvalidField {
                field: "account_verification.alipay.biz_code",
            });
        }
        for (value, field, max) in [
            (
                alipay.app_id().map(SecretString::expose),
                "account_verification.alipay.app_id",
                MAX_ALIPAY_VERIFICATION_APP_ID_BYTES,
            ),
            (
                alipay.private_key().map(SecretString::expose),
                "account_verification.alipay.private_key",
                MAX_ALIPAY_VERIFICATION_KEY_BYTES,
            ),
            (
                alipay.public_key().map(SecretString::expose),
                "account_verification.alipay.public_key",
                MAX_ALIPAY_VERIFICATION_KEY_BYTES,
            ),
        ] {
            if let Some(value) = value
                && (value.is_empty() || value.len() > max || value.trim() != value)
            {
                return Err(ConfigError::InvalidField { field });
            }
        }
        if alipay.enabled() {
            if alipay.app_id().is_none() {
                return Err(ConfigError::EmptyValue {
                    field: "account_verification.alipay.app_id",
                });
            }
            if alipay.private_key().is_none() {
                return Err(ConfigError::EmptyValue {
                    field: "account_verification.alipay.private_key",
                });
            }
            if alipay.public_key().is_none() {
                return Err(ConfigError::EmptyValue {
                    field: "account_verification.alipay.public_key",
                });
            }
        }
        if self.channel_probe.batch_size == 0 {
            return Err(ConfigError::NonPositive {
                field: "channel_probe.batch_size",
            });
        }
        if self.channel_probe.batch_size > MAX_CHANNEL_PROBE_BATCH_SIZE {
            return Err(ConfigError::OutOfRange {
                field: "channel_probe.batch_size",
            });
        }
        if self.channel_probe.interval_secs == 0 {
            return Err(ConfigError::NonPositive {
                field: "channel_probe.interval_secs",
            });
        }
        if self.channel_probe.interval_secs > MAX_CHANNEL_PROBE_INTERVAL_SECS {
            return Err(ConfigError::OutOfRange {
                field: "channel_probe.interval_secs",
            });
        }
        if self.channel_probe.probe_timeout_secs == 0 {
            return Err(ConfigError::NonPositive {
                field: "channel_probe.probe_timeout_secs",
            });
        }
        if self.channel_probe.probe_timeout_secs > MAX_CHANNEL_PROBE_TIMEOUT_SECS {
            return Err(ConfigError::OutOfRange {
                field: "channel_probe.probe_timeout_secs",
            });
        }
        validate_credential_encryption(&self.credential_encryption, self.channel_probe.enabled)?;
        if let Some(url) = self.redis.url()
            && url.expose().trim().is_empty()
        {
            return Err(ConfigError::EmptyValue { field: "redis.url" });
        }
        if let Some(settings) = self.clickhouse_analytics.as_ref() {
            validate_clickhouse_analytics(settings)?;
        }
        if let Some(settings) = self.clickhouse_export.as_ref() {
            validate_clickhouse_export(settings)?;
        }
        if self.auth.lookup_timeout_secs == 0 {
            return Err(ConfigError::NonPositive {
                field: "auth.lookup_timeout_secs",
            });
        }
        if self.auth.lookup_timeout_secs > MAX_AUTH_LOOKUP_TIMEOUT_SECS {
            return Err(ConfigError::OutOfRange {
                field: "auth.lookup_timeout_secs",
            });
        }
        if self.auth.session_ttl_secs == 0 {
            return Err(ConfigError::NonPositive {
                field: "auth.session_ttl_secs",
            });
        }
        if self.auth.session_ttl_secs > MAX_AUTH_SESSION_TTL_SECS {
            return Err(ConfigError::OutOfRange {
                field: "auth.session_ttl_secs",
            });
        }
        if let Some(key) = self.auth.session_signing_key.as_ref() {
            let mut decoded = [0_u8; AUTH_SESSION_SIGNING_KEY_BYTES];
            let valid = URL_SAFE_NO_PAD
                .decode_slice(key.expose(), &mut decoded)
                .is_ok_and(|length| length == AUTH_SESSION_SIGNING_KEY_BYTES);
            decoded.zeroize();
            if !valid {
                return Err(ConfigError::InvalidField {
                    field: "auth.session_signing_key",
                });
            }
        }
        validate_oauth_settings(&self.oauth)?;
        if let Some(openai_upstream) = &self.openai_upstream {
            validate_openai_upstream(openai_upstream)?;
        }
        if self.http_client.max_cached_clients == Some(0) {
            return Err(ConfigError::NonPositive {
                field: "http_client.max_cached_clients",
            });
        }
        for (value, field) in [
            (
                self.http_client.connect_timeout_secs,
                "http_client.connect_timeout_secs",
            ),
            (
                self.http_client.read_timeout_secs,
                "http_client.read_timeout_secs",
            ),
            (
                self.http_client.request_timeout_secs,
                "http_client.request_timeout_secs",
            ),
        ] {
            if value == Some(0) {
                return Err(ConfigError::NonPositive { field });
            }
        }
        validate_proxy_settings(&self.http_client)?;
        Ok(())
    }
}

fn validate_clickhouse_analytics(
    settings: &ClickHouseAnalyticsSettings,
) -> Result<(), ConfigError> {
    let endpoint = settings.endpoint.expose();
    let parsed = Url::parse(endpoint).map_err(|_| ConfigError::InvalidField {
        field: "clickhouse_analytics.endpoint",
    })?;
    if endpoint.trim() != endpoint
        || !matches!(parsed.scheme(), "http" | "https")
        || !parsed.has_host()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.port() == Some(0)
    {
        return Err(ConfigError::InvalidField {
            field: "clickhouse_analytics.endpoint",
        });
    }

    let query = settings.query.expose();
    let trimmed = query.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_CLICKHOUSE_ANALYTICS_QUERY_BYTES
        || trimmed.contains(';')
        || trimmed
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        || !trimmed.contains("{period_start:Int64}")
        || !trimmed.contains("{period_end:Int64}")
    {
        return Err(ConfigError::InvalidField {
            field: "clickhouse_analytics.query",
        });
    }
    validate_clickhouse_header_value(
        settings.username.expose(),
        MAX_CLICKHOUSE_ANALYTICS_USERNAME_BYTES,
        false,
        "clickhouse_analytics.username",
    )?;
    validate_clickhouse_header_value(
        settings.password.expose(),
        MAX_CLICKHOUSE_ANALYTICS_PASSWORD_BYTES,
        true,
        "clickhouse_analytics.password",
    )?;
    if settings.timeout_secs == 0 {
        return Err(ConfigError::NonPositive {
            field: "clickhouse_analytics.timeout_secs",
        });
    }
    if settings.timeout_secs > MAX_CLICKHOUSE_ANALYTICS_TIMEOUT_SECS {
        return Err(ConfigError::OutOfRange {
            field: "clickhouse_analytics.timeout_secs",
        });
    }
    if settings.max_response_bytes == 0 {
        return Err(ConfigError::NonPositive {
            field: "clickhouse_analytics.max_response_bytes",
        });
    }
    if settings.max_response_bytes > MAX_CLICKHOUSE_ANALYTICS_RESPONSE_BYTES {
        return Err(ConfigError::OutOfRange {
            field: "clickhouse_analytics.max_response_bytes",
        });
    }
    Ok(())
}

fn validate_clickhouse_export(settings: &ClickHouseExportSettings) -> Result<(), ConfigError> {
    let endpoint = settings.endpoint.expose();
    let parsed = Url::parse(endpoint).map_err(|_| ConfigError::InvalidField {
        field: "clickhouse_export.endpoint",
    })?;
    if endpoint.trim() != endpoint
        || !matches!(parsed.scheme(), "http" | "https")
        || !parsed.has_host()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.port() == Some(0)
    {
        return Err(ConfigError::InvalidField {
            field: "clickhouse_export.endpoint",
        });
    }

    for (query, field) in [
        (
            &settings.usage_insert_query,
            "clickhouse_export.usage_insert_query",
        ),
        (
            &settings.outcome_insert_query,
            "clickhouse_export.outcome_insert_query",
        ),
    ] {
        let trimmed = query.expose().trim();
        if trimmed.is_empty()
            || trimmed.len() > MAX_CLICKHOUSE_ANALYTICS_QUERY_BYTES
            || trimmed.contains(';')
            || trimmed.chars().any(char::is_control)
            || !is_clickhouse_insert_query(trimmed)
        {
            return Err(ConfigError::InvalidField { field });
        }
    }
    validate_clickhouse_header_value(
        settings.username.expose(),
        MAX_CLICKHOUSE_ANALYTICS_USERNAME_BYTES,
        false,
        "clickhouse_export.username",
    )?;
    validate_clickhouse_header_value(
        settings.password.expose(),
        MAX_CLICKHOUSE_ANALYTICS_PASSWORD_BYTES,
        true,
        "clickhouse_export.password",
    )?;
    if settings.batch_size == 0 {
        return Err(ConfigError::NonPositive {
            field: "clickhouse_export.batch_size",
        });
    }
    if settings.batch_size > MAX_CLICKHOUSE_EXPORT_BATCH_SIZE {
        return Err(ConfigError::OutOfRange {
            field: "clickhouse_export.batch_size",
        });
    }
    if settings.interval_secs == 0 {
        return Err(ConfigError::NonPositive {
            field: "clickhouse_export.interval_secs",
        });
    }
    if settings.interval_secs > MAX_CLICKHOUSE_EXPORT_INTERVAL_SECS {
        return Err(ConfigError::OutOfRange {
            field: "clickhouse_export.interval_secs",
        });
    }
    if settings.timeout_secs == 0 {
        return Err(ConfigError::NonPositive {
            field: "clickhouse_export.timeout_secs",
        });
    }
    if settings.timeout_secs > MAX_CLICKHOUSE_EXPORT_TIMEOUT_SECS {
        return Err(ConfigError::OutOfRange {
            field: "clickhouse_export.timeout_secs",
        });
    }
    if settings.max_request_bytes == 0 {
        return Err(ConfigError::NonPositive {
            field: "clickhouse_export.max_request_bytes",
        });
    }
    if settings.max_request_bytes > MAX_CLICKHOUSE_EXPORT_REQUEST_BYTES {
        return Err(ConfigError::OutOfRange {
            field: "clickhouse_export.max_request_bytes",
        });
    }
    if settings.backfill_batch_size == 0 {
        return Err(ConfigError::NonPositive {
            field: "clickhouse_export.backfill_batch_size",
        });
    }
    if settings.backfill_batch_size > MAX_CLICKHOUSE_EXPORT_BACKFILL_BATCH_SIZE {
        return Err(ConfigError::OutOfRange {
            field: "clickhouse_export.backfill_batch_size",
        });
    }
    Ok(())
}

fn is_clickhouse_insert_query(query: &str) -> bool {
    let mut words = query.split_ascii_whitespace();
    matches!(
        (words.next(), words.next()),
        (Some(insert), Some(into))
            if insert.eq_ignore_ascii_case("insert") && into.eq_ignore_ascii_case("into")
    )
}

fn validate_clickhouse_header_value(
    value: &str,
    max_bytes: usize,
    allow_empty: bool,
    field: &'static str,
) -> Result<(), ConfigError> {
    if (!allow_empty && value.is_empty())
        || value.len() > max_bytes
        || value.trim() != value
        || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
    {
        return Err(ConfigError::InvalidField { field });
    }
    Ok(())
}

fn validate_oauth_settings(settings: &OAuthSettings) -> Result<(), ConfigError> {
    if settings.max_pending_authorizations == 0 {
        return Err(ConfigError::NonPositive {
            field: "oauth.max_pending_authorizations",
        });
    }
    if settings.max_pending_authorizations > MAX_OAUTH_PENDING_AUTHORIZATIONS {
        return Err(ConfigError::OutOfRange {
            field: "oauth.max_pending_authorizations",
        });
    }
    if settings.session_ttl_secs < MIN_OAUTH_SESSION_TTL_SECS
        || settings.session_ttl_secs > MAX_OAUTH_SESSION_TTL_SECS
    {
        return Err(ConfigError::OutOfRange {
            field: "oauth.session_ttl_secs",
        });
    }
    if settings.refresh_interval_secs == 0 {
        return Err(ConfigError::NonPositive {
            field: "oauth.refresh_interval_secs",
        });
    }
    if settings.refresh_interval_secs > MAX_OAUTH_REFRESH_INTERVAL_SECS {
        return Err(ConfigError::OutOfRange {
            field: "oauth.refresh_interval_secs",
        });
    }
    if settings.refresh_before_expiry_secs == 0 {
        return Err(ConfigError::NonPositive {
            field: "oauth.refresh_before_expiry_secs",
        });
    }
    if settings.refresh_before_expiry_secs > MAX_OAUTH_REFRESH_BEFORE_EXPIRY_SECS {
        return Err(ConfigError::OutOfRange {
            field: "oauth.refresh_before_expiry_secs",
        });
    }
    if settings.refresh_batch_size == 0 {
        return Err(ConfigError::NonPositive {
            field: "oauth.refresh_batch_size",
        });
    }
    if settings.refresh_batch_size > MAX_OAUTH_REFRESH_BATCH_SIZE {
        return Err(ConfigError::OutOfRange {
            field: "oauth.refresh_batch_size",
        });
    }
    if settings.refresh_concurrency == 0 {
        return Err(ConfigError::NonPositive {
            field: "oauth.refresh_concurrency",
        });
    }
    if settings.refresh_concurrency > MAX_OAUTH_REFRESH_CONCURRENCY
        || settings.refresh_concurrency > settings.refresh_batch_size
    {
        return Err(ConfigError::OutOfRange {
            field: "oauth.refresh_concurrency",
        });
    }
    for (client, field) in [
        (settings.claude_code(), "oauth.claude_code.client_id"),
        (settings.codex(), "oauth.codex.client_id"),
    ] {
        if let Some(client) = client {
            validate_oauth_client_id(client.client_id(), field)?;
        }
    }
    for (client, id_field, secret_field) in [
        (
            settings.gemini(),
            "oauth.gemini.client_id",
            "oauth.gemini.client_secret",
        ),
        (
            settings.antigravity(),
            "oauth.antigravity.client_id",
            "oauth.antigravity.client_secret",
        ),
    ] {
        if let Some(client) = client {
            validate_oauth_client_id(client.client_id(), id_field)?;
            validate_oauth_client_secret(client.client_secret(), secret_field)?;
        }
    }
    Ok(())
}

fn validate_oauth_client_id(value: &SecretString, field: &'static str) -> Result<(), ConfigError> {
    let value = value.expose();
    if value.is_empty()
        || value.len() > MAX_OAUTH_CLIENT_ID_BYTES
        || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err(ConfigError::InvalidField { field });
    }
    Ok(())
}

fn validate_oauth_client_secret(
    value: &SecretString,
    field: &'static str,
) -> Result<(), ConfigError> {
    let value = value.expose();
    if value.is_empty()
        || value.len() > MAX_OAUTH_CLIENT_SECRET_BYTES
        || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err(ConfigError::InvalidField { field });
    }
    Ok(())
}

fn validate_credential_encryption(
    settings: &CredentialEncryptionSettings,
    required: bool,
) -> Result<(), ConfigError> {
    match (settings.key_id(), settings.key()) {
        (None, None) if required => Err(ConfigError::EmptyValue {
            field: "credential_encryption.key",
        }),
        (None, None) => Ok(()),
        (Some(_), None) | (None, Some(_)) => Err(ConfigError::InvalidField {
            field: "credential_encryption",
        }),
        (Some(key_id), Some(key)) => {
            if key_id.is_empty()
                || key_id.len() > MAX_CREDENTIAL_ENCRYPTION_KEY_ID_BYTES
                || key_id.trim() != key_id
                || !key_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                return Err(ConfigError::InvalidField {
                    field: "credential_encryption.key_id",
                });
            }

            let mut decoded = [0_u8; CREDENTIAL_ENCRYPTION_KEY_BYTES];
            let valid = URL_SAFE_NO_PAD
                .decode_slice(key.expose(), &mut decoded)
                .is_ok_and(|length| length == CREDENTIAL_ENCRYPTION_KEY_BYTES);
            decoded.zeroize();
            if !valid {
                return Err(ConfigError::InvalidField {
                    field: "credential_encryption.key",
                });
            }
            Ok(())
        }
    }
}

fn validate_openai_upstream(config: &OpenAiUpstreamConfig) -> Result<(), ConfigError> {
    validate_nonempty_text(
        &config.base_url,
        "openai_upstream.base_url",
        MAX_OPENAI_UPSTREAM_BASE_URL_BYTES,
    )?;
    let parsed = Url::parse(&config.base_url).map_err(|_| ConfigError::InvalidField {
        field: "openai_upstream.base_url",
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.has_host()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.port() == Some(0)
    {
        return Err(ConfigError::InvalidField {
            field: "openai_upstream.base_url",
        });
    }
    validate_nonempty_text(
        &config.model,
        "openai_upstream.model",
        MAX_OPENAI_UPSTREAM_MODEL_BYTES,
    )?;
    validate_nonempty_text(
        config.api_key.expose(),
        "openai_upstream.api_key",
        MAX_OPENAI_UPSTREAM_API_KEY_BYTES,
    )?;
    if !config.api_key.expose().is_ascii() {
        return Err(ConfigError::InvalidField {
            field: "openai_upstream.api_key",
        });
    }
    Ok(())
}

fn validate_proxy_settings(settings: &HttpClientSettings) -> Result<(), ConfigError> {
    let Some(proxy_url) = settings.proxy_url() else {
        if settings.trust_proxy_dns {
            return Err(ConfigError::InvalidField {
                field: "http_client.trust_proxy_dns",
            });
        }
        return Ok(());
    };
    let value = proxy_url.expose();
    validate_nonempty_text(value, "http_client.proxy_url", 2_048)?;
    let parsed = Url::parse(value).map_err(|_| ConfigError::InvalidField {
        field: "http_client.proxy_url",
    })?;
    let supported_scheme = matches!(parsed.scheme(), "http" | "https" | "socks5" | "socks5h");
    if !supported_scheme
        || !parsed.has_host()
        || !matches!(parsed.path(), "" | "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.port() == Some(0)
    {
        return Err(ConfigError::InvalidField {
            field: "http_client.proxy_url",
        });
    }

    let proxy_resolves_target = matches!(parsed.scheme(), "http" | "https" | "socks5h");
    if settings.trust_proxy_dns != proxy_resolves_target {
        return Err(ConfigError::InvalidField {
            field: "http_client.trust_proxy_dns",
        });
    }
    Ok(())
}

fn validate_turnstile_settings(settings: &TurnstileSettings) -> Result<(), ConfigError> {
    if settings.timeout_secs == 0 {
        return Err(ConfigError::NonPositive {
            field: "turnstile.timeout_secs",
        });
    }
    if settings.timeout_secs > MAX_TURNSTILE_TIMEOUT_SECS {
        return Err(ConfigError::OutOfRange {
            field: "turnstile.timeout_secs",
        });
    }
    match (settings.site_key.as_deref(), settings.secret_key.as_ref()) {
        (None, None) => Ok(()),
        (Some(site_key), Some(secret_key)) => {
            validate_turnstile_text(site_key, "turnstile.site_key", MAX_TURNSTILE_SITE_KEY_BYTES)?;
            validate_turnstile_text(
                secret_key.expose(),
                "turnstile.secret_key",
                MAX_TURNSTILE_SECRET_KEY_BYTES,
            )
        }
        (None, Some(_)) => Err(ConfigError::InvalidField {
            field: "turnstile.site_key",
        }),
        (Some(_), None) => Err(ConfigError::InvalidField {
            field: "turnstile.secret_key",
        }),
    }
}

fn validate_turnstile_text(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<(), ConfigError> {
    if value.is_empty() {
        return Err(ConfigError::EmptyValue { field });
    }
    if value.len() > max_bytes
        || value.trim() != value
        || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err(ConfigError::InvalidField { field });
    }
    Ok(())
}

fn validate_nonempty_text(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<(), ConfigError> {
    if value.is_empty() {
        return Err(ConfigError::EmptyValue { field });
    }
    if value.len() > max_bytes || value.trim() != value || value.chars().any(char::is_control) {
        return Err(ConfigError::InvalidField { field });
    }
    Ok(())
}

/// 校验外部前端模板目录必须是运行目录下的相对路径。
///
/// 目录最终由服务端读取；拒绝绝对路径、父目录跳转和控制字符，避免配置
/// 意外把模板加载边界扩展到运行目录之外。
fn valid_frontend_template_directory(path: &Path) -> bool {
    let Some(value) = path.to_str() else {
        return false;
    };
    if value.is_empty()
        || value.len() > 512
        || value.trim() != value
        || value.chars().any(char::is_control)
        || path.is_absolute()
    {
        return false;
    }
    let mut has_normal = false;
    for component in path.components() {
        match component {
            std::path::Component::Normal(_) => has_normal = true,
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return false,
        }
    }
    has_normal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_origin_normalizes_safe_http_origins() {
        assert_eq!(
            "https://Console.Example:443/"
                .parse::<CorsOrigin>()
                .unwrap()
                .as_str(),
            "https://console.example"
        );
        assert_eq!(
            "http://127.0.0.1:5173"
                .parse::<CorsOrigin>()
                .unwrap()
                .as_str(),
            "http://127.0.0.1:5173"
        );
    }

    #[test]
    fn frontend_template_directory_stays_below_runtime_directory() {
        assert!(valid_frontend_template_directory(Path::new(
            "public/templates"
        )));
        assert!(valid_frontend_template_directory(Path::new("./templates")));
        for value in [
            "",
            "/tmp/templates",
            "../templates",
            "public/../templates",
            " public/templates",
        ] {
            assert!(
                !valid_frontend_template_directory(Path::new(value)),
                "{value}"
            );
        }
    }

    #[test]
    fn cors_origin_rejects_ambient_or_non_origin_values_without_echoing_them() {
        for value in [
            "*",
            "null",
            "file:///tmp/index.html",
            "https://user:secret@console.example",
            "https://@console.example",
            "https://console.example/path",
            "https://console.example/a/..",
            "https://console.example/%2e",
            "https://console.example?secret=value",
            "https://console.example#fragment",
            " https://console.example",
        ] {
            let error = value.parse::<CorsOrigin>().unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            assert_eq!(error, CorsOriginError);
            assert!(!rendered.contains(value));
        }
    }

    #[test]
    fn cors_origin_debug_does_not_expose_internal_hosts() {
        let origin = "https://internal-console.example"
            .parse::<CorsOrigin>()
            .unwrap();
        let debug = format!("{origin:?}");
        assert!(!debug.contains("internal-console.example"));
    }

    #[test]
    fn clickhouse_export_query_requires_insert_into_tokens() {
        assert!(is_clickhouse_insert_query(
            "INSERT INTO usage_facts (fact_id)"
        ));
        assert!(is_clickhouse_insert_query(
            "insert\ninto outcome_facts (fact_id)"
        ));
        for query in [
            "INSERT usage_facts (fact_id)",
            "INSERTED INTO usage_facts (fact_id)",
            "SELECT fact_id FROM usage_facts",
        ] {
            assert!(!is_clickhouse_insert_query(query));
        }
    }
}
