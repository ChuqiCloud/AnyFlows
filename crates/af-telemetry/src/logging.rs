use tracing::{Level, Metadata, Subscriber};
use tracing_subscriber::{
    filter::{
        FilterExt, FilterFn, LevelFilter, Targets,
        combinator::{And, Or},
        filter_fn,
    },
    fmt::{self, writer::MakeWriter},
    layer::{Layer as _, SubscriberExt},
    registry::Registry,
    reload,
};

use crate::{
    LogLevel, TelemetryError, TelemetrySettings,
    fields::{FailClosedJsonFields, RedactingJsonFormat},
    request::REQUEST_SPAN_TARGET,
};

const ANYFLOWS_TARGET_PREFIX: &str = "af_";
/// 允许输出日志的产品 crate；新增 workspace crate 时必须显式审查后加入。
const ANYFLOWS_TARGETS: &[&str] = &[
    "af_account",
    "af_adapter",
    "af_admin",
    "af_analytics",
    "af_billing",
    "af_cache",
    "af_config",
    "af_db",
    "af_domain",
    "af_http",
    "af_httpclient",
    "af_protocol",
    "af_relay",
    "af_scheduler",
    "af_server",
    "af_telemetry",
];
type OwnedTargetFilter = FilterFn<fn(&Metadata<'_>) -> bool>;
type ApplicationFilter = And<Targets, OwnedTargetFilter, Registry>;
type RequestSpanFilter = FilterFn<fn(&Metadata<'_>) -> bool>;
type TelemetryFilter = Or<ApplicationFilter, RequestSpanFilter, Registry>;
type FilterHandle = reload::Handle<TelemetryFilter, Registry>;

/// tracing 运行期控制句柄；克隆值共享同一个过滤器。
#[derive(Clone, Debug)]
pub struct TracingHandle {
    filter: FilterHandle,
}

impl TracingHandle {
    /// 原子替换 AnyFlows 自身日志级别；失败时不保留底层错误文本。
    pub fn set_level(&self, level: LogLevel) -> Result<(), TelemetryError> {
        self.filter
            .reload(filter_for(level))
            .map_err(|_| TelemetryError::ReloadFailed)
    }
}

/// 安装 stdout JSON 全局 subscriber，并返回运行期调级句柄。
///
/// 本函数只安装 tracing subscriber，不接管 `log` facade 的旧式日志调用。
///
/// 字段名脱敏只提供纵深防御。调用方仍禁止把 `SecretString::expose()`、原始 URI、
/// Header、Body 或 reqwest/sqlx/SeaORM 底层错误链写入 tracing 字段。
/// 非敏感字段的 Debug/Display 会实际执行，只允许记录无密钥且不会 panic 的受控值。
pub fn init_tracing(settings: &TelemetrySettings) -> Result<TracingHandle, TelemetryError> {
    let (subscriber, handle) = build_subscriber(settings.level(), std::io::stdout);
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|_| TelemetryError::InitializationConflict)?;
    Ok(handle)
}

pub(crate) fn build_subscriber<W>(
    level: LogLevel,
    writer: W,
) -> (impl Subscriber + Send + Sync, TracingHandle)
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let (filter_layer, filter) = reload::Layer::new(filter_for(level));
    let output_layer = fmt::layer()
        // 事件字段固定嵌套在 fields，span 仅输出完整根到叶链，避免重复与键碰撞。
        .event_format(RedactingJsonFormat)
        .fmt_fields(FailClosedJsonFields)
        .with_ansi(false)
        // formatter 失败时静默丢弃，禁止内部诊断重新打印原始事件字段。
        .log_internal_errors(false)
        .with_writer(writer);
    let subscriber = tracing_subscriber::registry().with(output_layer.with_filter(filter_layer));

    (subscriber, TracingHandle { filter })
}

fn filter_for(level: LogLevel) -> TelemetryFilter {
    let application_level = match level {
        LogLevel::Off => LevelFilter::OFF,
        LogLevel::Error => LevelFilter::ERROR,
        LogLevel::Warn => LevelFilter::WARN,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Debug => LevelFilter::DEBUG,
        LogLevel::Trace => LevelFilter::TRACE,
    };
    let request_span_filter = match level {
        LogLevel::Off => reject_request_span as fn(&Metadata<'_>) -> bool,
        _ => allow_request_span as fn(&Metadata<'_>) -> bool,
    };

    Targets::new()
        .with_default(LevelFilter::OFF)
        .with_target(ANYFLOWS_TARGET_PREFIX, application_level)
        .and(filter_fn(
            is_anyflows_target as fn(&Metadata<'_>) -> bool,
        ))
        // 只为请求根 span 放宽级别，普通事件仍必须遵守应用级过滤。
        .or(filter_fn(request_span_filter))
}

fn is_anyflows_target(metadata: &Metadata<'_>) -> bool {
    ANYFLOWS_TARGETS.iter().any(|crate_name| {
        metadata.target() == *crate_name
            || metadata
                .target()
                .strip_prefix(crate_name)
                .is_some_and(|suffix| suffix.starts_with("::"))
    })
}

fn allow_request_span(metadata: &Metadata<'_>) -> bool {
    metadata.is_span()
        && metadata.target() == REQUEST_SPAN_TARGET
        && metadata.name() == "request"
        && metadata.level() == &Level::INFO
}

fn reject_request_span(_: &Metadata<'_>) -> bool {
    false
}
