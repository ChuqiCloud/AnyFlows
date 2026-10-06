import { AlertTriangle, CircleCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { getDashboardAvailabilityMetrics } from './dashboard-availability-metrics'

const FAILURE_ALERT_RATE = 0.05

/** 把可用性、自动停用渠道和失败率合成一条状态，让管理员先知道要不要操心。 */
export function DashboardStatusBar({
  dashboard,
  formatPercent,
}: {
  dashboard: AdminDashboardResponse
  formatPercent: (value: number) => string
}) {
  const { t } = useTranslation()
  const metrics = getDashboardAvailabilityMetrics(dashboard)
  const issues: string[] = []
  if (dashboard.enabled_channel_count === 0) issues.push(t('dashboard.status.noChannels'))
  if (dashboard.auto_disabled_channel_count > 0) {
    issues.push(t('dashboard.status.autoDisabled', { count: dashboard.auto_disabled_channel_count }))
  }
  if (metrics.failureRate !== null && metrics.failureRate >= FAILURE_ALERT_RATE) {
    issues.push(t('dashboard.status.failureRate', { rate: formatPercent(metrics.failureRate) }))
  }
  const healthy = issues.length === 0
  const Icon = healthy ? CircleCheck : AlertTriangle

  return (
    <section
      role="status"
      className={cn(
        'flex flex-wrap items-center gap-x-4 gap-y-2 rounded-xl border px-4 py-3 text-xs',
        healthy ? 'border-success/25 bg-success/8' : 'border-warning/30 bg-warning/8',
      )}
    >
      <span className={cn('flex items-center gap-2 font-medium', healthy ? 'text-success' : 'text-warning')}>
        <Icon className="size-4" aria-hidden="true" />
        {t(healthy ? 'dashboard.status.healthy' : 'dashboard.status.attention')}
      </span>
      {healthy ? (
        <span className="text-muted-foreground">
          {t('dashboard.status.summary', {
            rate: metrics.successRate === null ? '—' : formatPercent(metrics.successRate),
            enabled: dashboard.enabled_channel_count,
          })}
        </span>
      ) : (
        <ul className="flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground">
          {issues.map((issue) => <li key={issue}>{issue}</li>)}
        </ul>
      )}
    </section>
  )
}
