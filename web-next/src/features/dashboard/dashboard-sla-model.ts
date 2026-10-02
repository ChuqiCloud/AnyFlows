export type OutcomeCounts = {
  successful_request_count: number
  failed_request_count: number
  unknown_request_count: number
}

export function serviceLevelMetrics(counts: OutcomeCounts, target: number) {
  const resolved = counts.successful_request_count + counts.failed_request_count
  const total = resolved + counts.unknown_request_count
  const rate = resolved > 0 ? counts.successful_request_count / resolved : null
  const allowedFailures = Math.floor(resolved * (1 - target) + 1e-9)
  return {
    resolved,
    rate,
    coverage: total > 0 ? resolved / total : null,
    remainingFailures: Math.max(0, allowedFailures - counts.failed_request_count),
    exceededFailures: Math.max(0, counts.failed_request_count - allowedFailures),
    state: rate === null ? 'unknown' : rate + Number.EPSILON >= target ? 'healthy' : 'degraded',
  } as const
}

export function serviceLevelTone(counts: OutcomeCounts, target: number) {
  const total = counts.successful_request_count + counts.failed_request_count + counts.unknown_request_count
  if (total === 0) return 'empty'
  const { rate } = serviceLevelMetrics(counts, target)
  if (rate === null) return 'unknown'
  if (rate + Number.EPSILON >= target) return 'healthy'
  return rate >= 0.95 ? 'degraded' : 'critical'
}
