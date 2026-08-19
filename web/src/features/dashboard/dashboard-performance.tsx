import { Gauge, TimerReset } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'

type DashboardPerformanceProps = {
  dashboard: AdminDashboardResponse
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
}

function ratio(value: number, total: number) {
  return total > 0 ? value / total : null
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
  sampleHint: string
  coverage: number | null
  slowHint: string
}

function SampleRow({ icon: Icon, label, average, sampleHint, coverage, slowHint }: SampleRowProps) {
  return (
    <div className="py-3 first:pt-0 last:pb-0">
      <div className="flex items-start justify-between gap-3">
        <div className="inline-flex min-w-0 items-center gap-2 text-xs font-medium">
          <Icon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
          <span>{label}</span>
        </div>
        <span className="shrink-0 text-sm font-semibold tabular-nums">{average}</span>
      </div>
      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-surface-3" aria-hidden="true">
        <div className="h-full rounded-full bg-success" style={{ width: `${(coverage ?? 0) * 100}%` }} />
      </div>
      <div className="mt-1.5 flex flex-wrap items-center justify-between gap-x-3 gap-y-1 text-[0.6875rem] leading-4 text-muted-foreground">
        <span>{sampleHint}</span>
        <span>{slowHint}</span>
      </div>
    </div>
  )
}

/** 仅呈现有真实持久化样本的耗时健康，不把缺失值解释为零。 */
export function DashboardPerformance({ dashboard, formatNumber, formatPercent }: DashboardPerformanceProps) {
  const { t } = useTranslation()
  const performance = dashboard.performance
  const firstTokenCoverage = ratio(performance.first_token_sample_count, dashboard.request_count)
  const durationCoverage = ratio(performance.duration_sample_count, dashboard.request_count)

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
          coverage={firstTokenCoverage}
          sampleHint={firstTokenCoverage === null
            ? t('dashboard.performance.noSamples')
            : t('dashboard.performance.coverage', {
                rate: formatPercent(firstTokenCoverage),
                count: formatNumber(performance.first_token_sample_count),
              })}
          slowHint={t('dashboard.performance.slow', {
            count: formatNumber(performance.slow_first_token_count),
            threshold: formatMilliseconds(performance.slow_first_token_threshold_ms, formatNumber),
          })}
        />
        <SampleRow
          icon={TimerReset}
          label={t('dashboard.performance.duration')}
          average={formatMilliseconds(performance.average_duration_ms, formatNumber)}
          coverage={durationCoverage}
          sampleHint={durationCoverage === null
            ? t('dashboard.performance.noSamples')
            : t('dashboard.performance.coverage', {
                rate: formatPercent(durationCoverage),
                count: formatNumber(performance.duration_sample_count),
              })}
          slowHint={t('dashboard.performance.slow', {
            count: formatNumber(performance.slow_request_count),
            threshold: formatMilliseconds(performance.slow_request_threshold_ms, formatNumber),
          })}
        />
      </CardContent>
    </Card>
  )
}
