export type RuntimeDependency = { kind: string; status: string; latency_ms: number | null }
export type RuntimeResourcePoint = { sampled_at: number; cpu_cores: number | null; memory_bytes: number | null }
export type RuntimeNode = {
  node_id: string
  instance_id: string
  version: string
  os: string
  started_at: number
  sampled_at: number
  uptime_seconds: number
  status: string
  cpu_cores: number | null
  cpu_capacity_cores: number | null
  memory_bytes: number | null
  host_memory_bytes: number | null
  host_memory_used_bytes: number | null
  container_memory_limit_bytes: number | null
  container_memory_bytes: number | null
  database: RuntimeDependency
  redis: RuntimeDependency
  history: RuntimeResourcePoint[]
}
export type RuntimeStatus = {
  sampled_at: number
  local_instance_id: string
  cluster_status: string
  cluster_observed_at: number | null
  nodes: RuntimeNode[]
  prompt_cache: {
    sampled_at: number | null
    period_start: number
    period_end: number
    status: string
    scope: string
    upstream_requests: number
    estimated_requests: number
    positive_cache_requests: number
    input_tokens: number
    cache_read_tokens: number
    cache_write_tokens: number
  }
}

export function resourcePath(values: (number | null)[]) {
  const maximum = Math.max(1, ...values.filter((value): value is number => value !== null && Number.isFinite(value)))
  let connected = false
  return values.map((value, index) => {
    if (value === null || !Number.isFinite(value)) { connected = false; return '' }
    const command = connected ? 'L' : 'M'
    connected = true
    return `${command}${(index / Math.max(1, values.length - 1) * 600).toFixed(2)},${(100 - value / maximum * 90).toFixed(2)}${command === 'M' ? 'l0.01,0' : ''}`
  }).join(' ')
}
