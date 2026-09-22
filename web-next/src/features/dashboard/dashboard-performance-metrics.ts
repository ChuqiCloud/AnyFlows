import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'

export type DashboardTimingMetric = {
  sampleCount: number
  slowCount: number
  belowThresholdCount: number
  unsampledCount: number
  sampleCoverage: number | null
  belowThresholdRate: number | null
  belowThresholdShare: number
  slowShare: number
}

function ratio(value: number, total: number) {
  return total > 0 ? value / total : null
}

/** 将耗时样本拆成阈值内、慢请求和未采样三类，避免把样本覆盖率误作性能健康度。 */
function getTimingMetric(requestCount: number, sampleCount: number, slowCount: number): DashboardTimingMetric {
  const belowThresholdCount = Math.max(0, sampleCount - slowCount)

  return {
    sampleCount,
    slowCount,
    belowThresholdCount,
    unsampledCount: Math.max(0, requestCount - sampleCount),
    sampleCoverage: ratio(sampleCount, requestCount),
    belowThresholdRate: ratio(belowThresholdCount, sampleCount),
    belowThresholdShare: ratio(belowThresholdCount, requestCount) ?? 0,
    slowShare: ratio(slowCount, requestCount) ?? 0,
  }
}

export function getDashboardPerformanceMetrics(dashboard: AdminDashboardResponse) {
  return {
    firstToken: getTimingMetric(
      dashboard.request_count,
      dashboard.performance.first_token_sample_count,
      dashboard.performance.slow_first_token_count,
    ),
    duration: getTimingMetric(
      dashboard.request_count,
      dashboard.performance.duration_sample_count,
      dashboard.performance.slow_request_count,
    ),
  }
}
