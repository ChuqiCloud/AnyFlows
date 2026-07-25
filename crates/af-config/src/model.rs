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
/// 计费 WAL 批量落库的默认周期。
pub const DEFAULT_BILLING_FLUSH_INTERVAL_SECS: u64 = 5;
/// 计费 WAL 单次启动恢复或关闭收尾的默认截止时间。
pub const DEFAULT_BILLING_FLUSH_TIMEOUT_SECS: u64 = 30;
/// 计费 WAL 批量落库周期允许配置的硬上限。
pub const MAX_BILLING_FLUSH_INTERVAL_SECS: u64 = 3_600;
/// 计费 WAL 启动恢复或关闭收尾允许配置的硬上限。
pub const MAX_BILLING_FLUSH_TIMEOUT_SECS: u64 = 300;
/// 用量记录内存接收队列的默认容量。
pub const DEFAULT_USAGE_RECORD_QUEUE_CAPACITY: usize = 4_096;
/// 单实例用量记录队列允许配置的硬容量上限。
pub const MAX_USAGE_RECORD_QUEUE_CAPACITY: usize = 1_000_000;
/// 默认用量记录持久化 worker 数量。
pub const DEFAULT_USAGE_RECORD_WORKER_COUNT: usize = 1;
/// 单实例用量记录持久化 worker 数量硬上限。
pub const MAX_USAGE_RECORD_WORKER_COUNT: usize = 64;
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

    /// 返回单渠道探活允许占用的最大秒数。
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
}

impl RedisSettings {
    /// 返回受保护的 Redis URL；为 `None` 时不得尝试建立远程连接。
    #[must_use]
    pub fn url(&self) -> Option<&SecretString> {
        self.url.as_ref()
    }
}

/// 下游 API Key 认证的静态设置。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuthSettings {
    allow_query_api_key: bool,
    lookup_timeout_secs: u64,
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
}

impl Default for AuthSettings {
    fn default() -> Self {
        Self {
            allow_query_api_key: false,
            lookup_timeout_secs: DEFAULT_AUTH_LOOKUP_TIMEOUT_SECS,
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
    channel_probe: ChannelProbeSettings,
    #[serde(default)]
    credential_encryption: CredentialEncryptionSettings,
    #[serde(default)]
    redis: RedisSettings,
    #[serde(default)]
    auth: AuthSettings,
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

    /// 返回下游 API Key 认证设置。
    #[must_use]
    pub const fn auth(&self) -> &AuthSettings {
        &self.auth
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
}
