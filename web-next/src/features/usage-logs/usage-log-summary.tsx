import { Coins, Gauge, ScrollText, Zap } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { formatUsageLatency, formatUsageNumber } from './usage-log-format'
import type { UsageLogSummary as UsageLogSummaryValue } from './usage-log-model'

type UsageLogSummaryProps = {
  summary: UsageLogSummaryValue
  formatQuota: (value: number) => string
}

/** 用单条指标带呈现当前页事实，避免管理台出现重复卡片墙。 */
export function UsageLogSummary({ summary, formatQuota }: UsageLogSummaryProps) {
  const { t, i18n } = useTranslation()
  const items = [
    {
      key: 'requests',
      icon: ScrollText,
      label: t('usageLogs.summary.requests'),
      value: formatUsageNumber(summary.requestCount, i18n.language),
    },
    {
      key: 'tokens',
      icon: Zap,
      label: t('usageLogs.summary.tokens'),
      value: formatUsageNumber(summary.tokenCount, i18n.language),
    },
    {
      key: 'firstToken',
      icon: Gauge,
      label: t('usageLogs.summary.averageFirstToken'),
      value: formatUsageLatency(summary.averageFirstTokenMs, i18n.language),
    },
    {
      key: 'quota',
      icon: Coins,
      label: t('usageLogs.summary.quota'),
      value: formatQuota(summary.quota),
    },
  ]

  return (
    <section
      className="grid grid-cols-2 divide-x divide-y divide-[var(--hairline)] overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1/45 md:grid-cols-4 md:divide-y-0"
      aria-label={t('usageLogs.summary.label')}
    >
      {items.map(({ key, icon: Icon, label, value }) => (
        <div key={key} className="flex min-w-0 items-center gap-3 px-3 py-3 sm:px-4">
          <Icon className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <div className="min-w-0">
            <div className="truncate text-[0.6875rem] text-muted-foreground">{label}</div>
            <div className="mt-0.5 truncate text-sm font-semibold tabular-nums" title={value}>{value}</div>
          </div>
        </div>
      ))}
    </section>
  )
}
