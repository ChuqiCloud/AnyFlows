use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use af_cache::{RedisConfig, RedisNodeRegistry};
use af_db::{DatabaseDialect, DatabasePool};
use af_http::{
    RuntimeDependency, RuntimeNode, RuntimePromptCache, RuntimeResourcePoint, RuntimeStatus,
    RuntimeStatusFuture, RuntimeStatusReader,
};
use tokio::sync::RwLock;

use crate::ShutdownController;

pub(crate) struct RuntimeMonitor {
    state: RwLock<RuntimeStatus>,
    database: DatabasePool,
    redis_config: Option<RedisConfig>,
    namespace: String,
}

impl RuntimeMonitor {
    pub(crate) fn new(
        database: DatabasePool,
        redis_config: Option<RedisConfig>,
        node_id: Option<&str>,
        namespace: &str,
    ) -> Arc<Self> {
        let now = timestamp();
        let instance_id = uuid::Uuid::new_v4().simple().to_string();
        let node = RuntimeNode {
            node_id: node_id
                .filter(|value| {
                    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
                })
                .map(str::to_owned)
                .unwrap_or_else(|| format!("node-{}", &instance_id[..8])),
            instance_id: instance_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            os: std::env::consts::OS.to_owned(),
            started_at: now,
            sampled_at: now,
            uptime_seconds: 0,
            status: "starting".to_owned(),
            cpu_cores: None,
            memory_bytes: None,
            cpu_capacity_cores: None,
            host_memory_bytes: None,
            host_memory_used_bytes: None,
            container_memory_limit_bytes: None,
            container_memory_bytes: None,
            database: dependency(database_kind(&database), "starting", None),
            redis: dependency(
                "Redis",
                if redis_config.is_some() {
                    "starting"
                } else {
                    "not_configured"
                },
                None,
            ),
            history: Vec::new(),
        };
        Arc::new(Self {
            state: RwLock::new(RuntimeStatus {
                sampled_at: now,
                local_instance_id: instance_id,
                cluster_status: if redis_config.is_some() {
                    "starting"
                } else {
                    "local"
                }
                .to_owned(),
                cluster_observed_at: None,
                nodes: vec![node],
                prompt_cache: RuntimePromptCache {
                    status: "starting".to_owned(),
                    scope: "public_usage_ledger".to_owned(),
                    ..Default::default()
                },
            }),
            database,
            redis_config,
            namespace: namespace.to_owned(),
        })
    }

    pub(crate) async fn run(self: Arc<Self>, shutdown: ShutdownController) {
        let mut registry = None;
        let mut cpu_sample = None;
        let started = Instant::now();
        let mut first_sample = true;
        let mut usage_refresh = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);
        let mut ticker = tokio::time::interval(Duration::from_secs(10));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => return,
                _ = ticker.tick() => {}
            }
            tokio::select! {
                () = shutdown.cancelled() => return,
                result = self.sample(&mut registry, started, first_sample, &mut usage_refresh, &mut cpu_sample) => {
                    if result.is_none() { return; }
                    first_sample = false;
                }
            }
        }
    }

    async fn sample(
        &self,
        registry: &mut Option<RedisNodeRegistry>,
        started: Instant,
        first: bool,
        usage_refresh: &mut Instant,
        cpu_sample: &mut Option<CpuSample>,
    ) -> Option<()> {
        let previous_cpu = *cpu_sample;
        let (resources, current_cpu) =
            tokio::task::spawn_blocking(move || collect_resources(previous_cpu, first))
                .await
                .ok()?;
        *cpu_sample = current_cpu;
        let now = timestamp();
        let mut node = {
            let state = self.state.read().await;
            state
                .nodes
                .iter()
                .find(|n| n.instance_id == state.local_instance_id)?
                .clone()
        };
        node.sampled_at = now;
        node.uptime_seconds = started.elapsed().as_secs();
        node.status = "online".to_owned();
        node.cpu_cores = resources.cpu_cores;
        node.memory_bytes = resources.memory_bytes;
        node.cpu_capacity_cores = resources.cpu_capacity_cores;
        node.host_memory_bytes = resources.host_memory_bytes;
        node.host_memory_used_bytes = resources.host_memory_used_bytes;
        node.container_memory_bytes = resources.container_memory_bytes;
        node.container_memory_limit_bytes = resources.container_memory_limit_bytes;
        node.history.push(RuntimeResourcePoint {
            sampled_at: now,
            cpu_cores: node.cpu_cores,
            memory_bytes: node.memory_bytes,
        });
        if node.history.len() > 60 {
            node.history.remove(0);
        }

        let ping_started = Instant::now();
        let db_ready = tokio::time::timeout(Duration::from_secs(2), self.database.ping())
            .await
            .is_ok_and(|r| r.is_ok());
        node.database = dependency(
            database_kind(&self.database),
            if db_ready { "healthy" } else { "unavailable" },
            Some(elapsed_ms(ping_started)),
        );

        let mut cluster_status = if self.redis_config.is_some() {
            "unavailable"
        } else {
            "local"
        };
        let mut observation = None;
        let mut peers = None;
        if let Some(config) = &self.redis_config {
            let ping_started = Instant::now();
            if registry.is_none() {
                *registry = RedisNodeRegistry::connect(config.clone(), &self.namespace)
                    .await
                    .ok();
            }
            if let Some(store) = registry.as_ref() {
                // Successful exchange verifies read/write coordination, not just TCP reachability.
                node.redis = dependency("Redis", "healthy", Some(elapsed_ms(ping_started)));
                if let Ok(payload) = serde_json::to_vec(&node)
                    && let Ok(exchange) = store.exchange(&node.instance_id, &payload).await
                {
                    node.redis.latency_ms = Some(elapsed_ms(ping_started));
                    cluster_status = if exchange.admitted {
                        "healthy"
                    } else {
                        "capacity_reached"
                    };
                    observation = Some(now);
                    peers = Some(
                        exchange
                            .entries
                            .into_iter()
                            .filter_map(|entry| {
                                let mut peer: RuntimeNode =
                                    serde_json::from_slice(&entry.payload).ok()?;
                                if peer.instance_id != entry.instance_id {
                                    return None;
                                }
                                peer.status = if entry.age_seconds > 30 {
                                    "stale"
                                } else {
                                    "online"
                                }
                                .to_owned();
                                Some(peer)
                            })
                            .collect::<Vec<_>>(),
                    );
                }
            }
            if peers.is_none() {
                node.redis = dependency("Redis", "unavailable", Some(elapsed_ms(ping_started)));
                *registry = None;
            }
        }

        if usage_refresh.elapsed() >= Duration::from_secs(60) {
            *usage_refresh = Instant::now();
            // A shared lease keeps cluster nodes from each scanning the same ledger.
            let refresh = if let Some(store) = registry.as_ref() {
                store.claim_usage_refresh().await.unwrap_or(false)
            } else {
                self.redis_config.is_none()
            };
            if refresh {
                let result = self.database.runtime_usage_totals(now).await;
                let mut cache = self.state.read().await.prompt_cache.clone();
                if let Ok(totals) = result {
                    cache = RuntimePromptCache {
                        sampled_at: Some(now),
                        period_start: now.saturating_sub(86_400),
                        period_end: now,
                        status: "healthy".to_owned(),
                        scope: "public_usage_ledger".to_owned(),
                        upstream_requests: totals.upstream_requests,
                        estimated_requests: totals.estimated_requests,
                        positive_cache_requests: totals.positive_cache_requests,
                        input_tokens: totals.input_tokens,
                        cache_read_tokens: totals.cache_read_tokens,
                        cache_write_tokens: totals.cache_write_tokens,
                    };
                } else {
                    cache.status = "unavailable".to_owned();
                }
                if let Some(store) = registry.as_ref()
                    && let Ok(value) = serde_json::to_vec(&cache)
                {
                    let _ = store.publish_usage(&value).await;
                }
                self.state.write().await.prompt_cache = cache;
            }
        }
        if let Some(store) = registry.as_ref()
            && let Ok(Some(value)) = store.read_usage().await
            && let Ok(cache) = serde_json::from_slice::<RuntimePromptCache>(&value)
        {
            self.state.write().await.prompt_cache = cache;
        }
        {
            let mut state = self.state.write().await;
            if self.redis_config.is_some() && registry.is_none() {
                state.prompt_cache.status = "unavailable".to_owned();
            } else if state
                .prompt_cache
                .sampled_at
                .is_some_and(|at| now.saturating_sub(at) > 130)
            {
                state.prompt_cache.status = "stale".to_owned();
            }
        }

        let mut state = self.state.write().await;
        let mut nodes = peers.unwrap_or_else(|| {
            state
                .nodes
                .iter()
                .filter(|n| n.instance_id != node.instance_id)
                .cloned()
                .map(|mut n| {
                    n.status = "unknown".to_owned();
                    n
                })
                .collect()
        });
        nodes.retain(|n| n.instance_id != node.instance_id);
        nodes.push(node);
        nodes.sort_by(|a, b| {
            a.node_id
                .cmp(&b.node_id)
                .then(a.instance_id.cmp(&b.instance_id))
        });
        state.nodes = nodes;
        state.sampled_at = now;
        state.cluster_status = cluster_status.to_owned();
        if observation.is_some() {
            state.cluster_observed_at = observation;
        }
        Some(())
    }
}

impl RuntimeStatusReader for RuntimeMonitor {
    fn snapshot(&self) -> RuntimeStatusFuture<'_> {
        Box::pin(async {
            let mut state = self.state.read().await.clone();
            if timestamp().saturating_sub(state.sampled_at) > 30 {
                state.cluster_status = "unavailable".to_owned();
                for node in &mut state.nodes {
                    node.status = "unknown".to_owned();
                }
                state.prompt_cache.status = "stale".to_owned();
            }
            state
        })
    }
}

#[derive(Clone, Copy)]
struct CpuSample {
    process_ticks: u64,
    total_ticks: u64,
    logical_cpus: usize,
}

#[derive(Default)]
struct Resources {
    cpu_cores: Option<f64>,
    memory_bytes: Option<u64>,
    cpu_capacity_cores: Option<f64>,
    host_memory_bytes: Option<u64>,
    host_memory_used_bytes: Option<u64>,
    container_memory_limit_bytes: Option<u64>,
    container_memory_bytes: Option<u64>,
}

fn collect_resources(previous: Option<CpuSample>, first: bool) -> (Resources, Option<CpuSample>) {
    let mut result = Resources::default();
    let current = proc_cpu_sample();
    if let Some(sample) = current
        && !first
        && let Some(previous) = previous
    {
        let process_delta = sample.process_ticks.saturating_sub(previous.process_ticks);
        let total_delta = sample.total_ticks.saturating_sub(previous.total_ticks);
        if total_delta > 0 {
            result.cpu_cores =
                Some((process_delta as f64 / total_delta as f64) * sample.logical_cpus as f64)
                    .filter(|n| n.is_finite());
        }
    }
    result.memory_bytes = process_memory_bytes();
    (result.host_memory_bytes, result.host_memory_used_bytes) = host_memory();
    result.cpu_capacity_cores = std::thread::available_parallelism()
        .ok()
        .map(|n| n.get() as f64);
    #[cfg(target_os = "linux")]
    {
        // cgroup v2 and the common v1 layout; absent files remain explicitly unavailable.
        let read_number = |paths: &[&str]| {
            paths
                .iter()
                .find_map(|path| {
                    std::fs::read_to_string(path)
                        .ok()?
                        .trim()
                        .parse::<u64>()
                        .ok()
                })
                .filter(|n| *n < (1_u64 << 60))
        };
        result.container_memory_limit_bytes = read_number(&[
            "/sys/fs/cgroup/memory.max",
            "/sys/fs/cgroup/memory/memory.limit_in_bytes",
        ]);
        result.container_memory_bytes = read_number(&[
            "/sys/fs/cgroup/memory.current",
            "/sys/fs/cgroup/memory/memory.usage_in_bytes",
        ]);
        let quota = std::fs::read_to_string("/sys/fs/cgroup/cpu.max")
            .ok()
            .and_then(|v| cpu_quota(&v));
        let quota = quota.or_else(|| {
            let q = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_quota_us").ok()?;
            let p = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_period_us").ok()?;
            cpu_quota(&format!("{} {}", q.trim(), p.trim()))
        });
        if let Some(quota) = quota {
            result.cpu_capacity_cores =
                Some(result.cpu_capacity_cores.map_or(quota, |n| n.min(quota)));
        }
    }
    (result, current)
}

#[cfg(target_os = "linux")]
fn proc_cpu_sample() -> Option<CpuSample> {
    let process = std::fs::read_to_string("/proc/self/stat").ok()?;
    let process_end = process.rfind(") ")? + 2;
    let fields: Vec<&str> = process[process_end..].split_whitespace().collect();
    let process_ticks = fields
        .get(11)?
        .parse::<u64>()
        .ok()?
        .checked_add(fields.get(12)?.parse::<u64>().ok()?)?;
    let host = std::fs::read_to_string("/proc/stat").ok()?;
    // guest and guest_nice are already included in user and nice.
    let total_ticks = host
        .lines()
        .next()?
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?
        .into_iter()
        .sum();
    let logical_cpus = host
        .lines()
        .filter(|line| {
            line.strip_prefix("cpu")
                .is_some_and(|suffix| suffix.starts_with(|c: char| c.is_ascii_digit()))
        })
        .count();
    (logical_cpus > 0).then_some(CpuSample {
        process_ticks,
        total_ticks,
        logical_cpus,
    })
}

#[cfg(not(target_os = "linux"))]
fn proc_cpu_sample() -> Option<CpuSample> {
    None
}

#[cfg(target_os = "linux")]
fn process_memory_bytes() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            let value = line
                .strip_prefix("VmRSS:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()?;
            value.checked_mul(1024)
        })
}

#[cfg(not(target_os = "linux"))]
fn process_memory_bytes() -> Option<u64> {
    None
}

#[cfg(target_os = "linux")]
fn host_memory() -> (Option<u64>, Option<u64>) {
    let Ok(content) = std::fs::read_to_string("/proc/meminfo") else {
        return (None, None);
    };
    let read = |name: &str| {
        content.lines().find_map(|line| {
            line.strip_prefix(name)?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()?
                .checked_mul(1024)
        })
    };
    let total = read("MemTotal:");
    (
        total,
        total
            .zip(read("MemAvailable:"))
            .map(|(total, available)| total.saturating_sub(available)),
    )
}

#[cfg(not(target_os = "linux"))]
fn host_memory() -> (Option<u64>, Option<u64>) {
    (None, None)
}

fn cpu_quota(value: &str) -> Option<f64> {
    let mut parts = value.split_whitespace();
    let quota: f64 = parts.next()?.parse().ok()?;
    let period: f64 = parts.next()?.parse().ok()?;
    (quota.is_finite() && period.is_finite() && quota > 0.0 && period > 0.0)
        .then_some(quota / period)
}

fn database_kind(database: &DatabasePool) -> &'static str {
    match database.dialect() {
        DatabaseDialect::Sqlite => "SQLite",
        DatabaseDialect::Postgres => "PostgreSQL",
        DatabaseDialect::MySql => "MySQL",
    }
}

fn dependency(kind: &str, status: &str, latency_ms: Option<u64>) -> RuntimeDependency {
    RuntimeDependency {
        kind: kind.to_owned(),
        status: status.to_owned(),
        latency_ms,
    }
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}
fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|n| i64::try_from(n.as_secs()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cgroup_cpu_capacity_handles_fractional_and_unlimited_quotas() {
        assert_eq!(cpu_quota("50000 100000"), Some(0.5));
        for value in ["max 100000", "-1 100000", "100 0", "NaN 100", "", "inf 1"] {
            assert_eq!(cpu_quota(value), None);
        }
    }
}
