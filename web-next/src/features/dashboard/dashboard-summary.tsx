import { Card } from '@heroui/react'
import { Activity, BadgeCheck, Coins, RadioTower, type LucideIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'

type DashboardSummaryProps = {
  dashboard: AdminDashboardResponse
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
  formatQuota: (value: number) => string
}

type SummaryItem = {
  key: string
  icon: LucideIcon
  value: string
  hint: string
}

export function DashboardSummary({ dashboard, formatNumber, formatPercent, formatQuota }: DashboardSummaryProps) {
  const { t } = useTranslation()
  const totalChannels = dashboard.enabled_channel_count
    + dashboard.disabled_channel_count
    + dashboard.auto_disabled_channel_count
  const confirmedRate = dashboard.request_count > 0
    ? dashboard.upstream_usage_count / dashboard.request_count
    : null
  const items: SummaryItem[] = [
    {
      key: 'requests',
      icon: Activity,
      value: formatNumber(dashboard.request_count),
      hint: t('dashboard.summary.requests.hint'),
    },
    {
      key: 'quota',
      icon: Coins,
      value: formatQuota(dashboard.quota_consumed),
      hint: t('dashboard.summary.quota.hint'),
    },
    {
      key: 'channels',
      icon: RadioTower,
      value: formatNumber(dashboard.enabled_channel_count),
      hint: t('dashboard.summary.channels.hint', { total: formatNumber(totalChannels) }),
    },
    {
      key: 'confirmed',
      icon: BadgeCheck,
      value: confirmedRate === null ? '--' : formatPercent(confirmedRate),
      hint: confirmedRate === null
        ? t('dashboard.summary.confirmed.empty')
        : t('dashboard.summary.confirmed.hint', {
          count: formatNumber(dashboard.upstream_usage_count),
        }),
    },
  ]

  return (
    <section className="grid grid-cols-2 gap-3 xl:grid-cols-4" aria-label={t('dashboard.summary.label')}>
      {items.map(({ key, icon: Icon, value, hint }) => (
        <Card key={key} data-report-metric={key} className="report-summary-metric min-h-28 border border-[var(--hairline)] bg-card p-3.5" shadow="none">
          <div className="flex items-start justify-between gap-3">
            <span className="text-xs text-muted-foreground">{t(`dashboard.summary.${key}.label`)}</span>
            <span className="grid size-7 shrink-0 place-items-center rounded-lg border border-[var(--hairline)] bg-surface-2/45 text-muted-foreground">
              <Icon className="size-3.5" aria-hidden="true" />
            </span>
          </div>
          <div className="report-summary-value mt-3 text-xl font-semibold tabular-nums">{value}</div>
          <div className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{hint}</div>
        </Card>
      ))}
    </section>
  )
}
