import { useTranslation } from 'react-i18next'

import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type DashboardChannelStatusProps = {
  dashboard: AdminDashboardResponse
  formatNumber: (value: number) => string
}

export function DashboardChannelStatus({ dashboard, formatNumber }: DashboardChannelStatusProps) {
  const { t } = useTranslation()
  const statuses = [
    { key: 'enabled', count: dashboard.enabled_channel_count, tone: 'bg-success' },
    { key: 'disabled', count: dashboard.disabled_channel_count, tone: 'bg-muted-foreground/55' },
    { key: 'autoDisabled', count: dashboard.auto_disabled_channel_count, tone: 'bg-warning' },
  ] as const
  const total = statuses.reduce((sum, status) => sum + status.count, 0)

  return (
    <Card>
      <CardHeader className="border-b border-[var(--hairline)] px-4 py-3">
        <div className="flex items-center justify-between gap-3">
          <CardTitle className="text-sm">{t('dashboard.channels.title')}</CardTitle>
          <span className="text-xs tabular-nums text-muted-foreground">
            {t('dashboard.channels.total', { count: formatNumber(total) })}
          </span>
        </div>
      </CardHeader>
      <CardContent className="px-4 py-2">
        <div className="divide-y divide-[var(--hairline)]">
          {statuses.map((status) => (
            <div key={status.key} className="flex min-h-10 items-center justify-between gap-3 text-xs">
              <span className="inline-flex min-w-0 items-center gap-2">
                <span className={cn('size-1.5 shrink-0 rounded-full', status.tone)} aria-hidden="true" />
                <span>{t(`dashboard.channels.${status.key}`)}</span>
              </span>
              <span className="shrink-0 font-medium tabular-nums">{formatNumber(status.count)}</span>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  )
}
