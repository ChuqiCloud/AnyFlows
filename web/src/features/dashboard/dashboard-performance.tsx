import { Gauge, TimerReset } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'
import { getDashboardPerformanceMetrics, type DashboardTimingMetric } from './dashboard-performance-metrics'

type DashboardPerformanceProps = {
  dashboard: AdminDashboardResponse
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
}

function formatMilliseconds(value: number | null | undefined, formatNumber: (value: number) => string) {
  if (value === null || value === undefined)
    return '--'
  return `${formatNumber(value)} ms`
}

type SampleRowProps = {
  icon: typeof Gauge
  label: string
  average: string
  threshold: string
  metric: DashboardTimingMetric
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
}

function SampleRow({ icon: Icon, label, average, threshold, metric, formatNumber, formatPercent }: SampleRowProps) {
  const { t } = useTranslation()
  const distributionLabel = metric.sampleCount === 0
    ? t('dashboard.performance.noSamples')
    : t('dashboard.performance.distribution', {
        below: formatNumber(metric.belowThresholdCount),
        slow: formatNumber(metric.slowCount),
        unsampled: formatNumber(metric.unsampledCount),
      })

  return (
    <div className="py-3 first:pt-0 last:pb-0">
      <div className="flex items-start justify-between gap-3">
        <div className="inline-flex min-w-0 items-center gap-2 text-xs font-medium">
          <Icon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
          <span>{label}</span>
        </div>
        <span className="shrink-0 text-sm font-semibold tabular-nums">{average}</span>
      </div>
      <div
        className="mt-2 flex h-1.5 overflow-hidden rounded-full bg-surface-3"
        role="img"
        aria-label={distributionLabel}
      >
        <span className="h-full bg-success" style={{ width: `${metric.belowThresholdShare * 100}%` }} />
        <span className="h-full bg-warning" style={{ width: `${metric.slowShare * 100}%` }} />
      </div>
      <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[0.6875rem] leading-4 text-muted-foreground">
        <span className="inline-flex items-center gap-1.5">
          <span className="size-1.5 rounded-full bg-success" aria-hidden="true" />
          {metric.belowThresholdRate === null
            ? t('dashboard.performance.noSamples')
            : t('dashboard.performance.belowThreshold', { rate: formatPercent(metric.belowThresholdRate) })}
        </span>
        <span className="inline-flex items-center gap-1.5">
          <span className="size-1.5 rounded-full bg-warning" aria-hidden="true" />
          {t('dashboard.performance.slow', { count: formatNumber(metric.slowCount), threshold })}
        </span>
        {metric.unsampledCount > 0 && (
          <span className="inline-flex items-center gap-1.5">
            <span className="size-1.5 rounded-full bg-surface-3" aria-hidden="true" />
            {t('dashboard.performance.unsampled', { count: formatNumber(metric.unsampledCount) })}
          </span>
        )}
      </div>
      <div className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
        {metric.sampleCoverage === null
          ? t('dashboard.performance.noRequests')
          : t('dashboard.performance.coverage', {
              rate: formatPercent(metric.sampleCoverage),
              count: formatNumber(metric.sampleCount),
            })}
      </div>
    </div>
  )
}

/** 仅呈现有真实持久化样本的耗时健康，不把缺失值解释为零。 */
export function DashboardPerformance({ dashboard, formatNumber, formatPercent }: DashboardPerformanceProps) {
  const { t } = useTranslation()
  const performance = dashboard.performance
  const metrics = getDashboardPerformanceMetrics(dashboard)

  return (
    <Card>
      <CardHeader className="border-b border-[var(--hairline)] px-4 py-3">
        <CardTitle className="text-sm">{t('dashboard.performance.title')}</CardTitle>
        <CardDescription className="mt-1 text-xs">{t('dashboard.performance.subtitle')}</CardDescription>
      </CardHeader>
      <CardContent className="divide-y divide-[var(--hairline)] px-4 py-3">
        <SampleRow
          icon={Gauge}
          label={t('dashboard.performance.firstToken')}
          average={formatMilliseconds(performance.average_first_token_ms, formatNumber)}
          threshold={formatMilliseconds(performance.slow_first_token_threshold_ms, formatNumber)}
          metric={metrics.firstToken}
          formatNumber={formatNumber}
          formatPercent={formatPercent}
        />
        <SampleRow
          icon={TimerReset}
          label={t('dashboard.performance.duration')}
          average={formatMilliseconds(performance.average_duration_ms, formatNumber)}
          threshold={formatMilliseconds(performance.slow_request_threshold_ms, formatNumber)}
          metric={metrics.duration}
          formatNumber={formatNumber}
          formatPercent={formatPercent}
        />
      </CardContent>
    </Card>
  )
}
