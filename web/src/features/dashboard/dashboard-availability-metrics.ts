import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'

export type DashboardAvailabilityMetrics = {
  unknownRequestCount: number
  confirmedFailureCount: number
  resolvedRequestCount: number
  successRate: number | null
  failureRate: number | null
  resolutionRate: number | null
}

function ratio(count: number, total: number) {
  return total > 0 ? count / total : null
}

/** 从终态事实中拆出可判定结果，结果未知不进入成功率分母。 */
export function getDashboardAvailabilityMetrics(
  dashboard: AdminDashboardResponse,
): DashboardAvailabilityMetrics {
  const unknownRequestCount = dashboard.failures.find(
    (failure) => failure.kind === 'outcome_unknown',
  )?.request_count ?? 0
  const confirmedFailureCount = Math.max(
    dashboard.failed_request_count - unknownRequestCount,
    0,
  )
  const resolvedRequestCount = dashboard.successful_request_count + confirmedFailureCount

  return {
    unknownRequestCount,
    confirmedFailureCount,
    resolvedRequestCount,
    successRate: ratio(dashboard.successful_request_count, resolvedRequestCount),
    failureRate: ratio(confirmedFailureCount, resolvedRequestCount),
    resolutionRate: ratio(resolvedRequestCount, dashboard.outcome_request_count),
  }
}
