import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { cn } from '@/lib/utils'
import type { AdminGroup } from '@/lib/api/generated/types.gen'

const GROUP_WINDOWS = [
  { label: 'groups.values.daily', limit: 'daily_limit', snapshot: 'daily_window' },
  { label: 'groups.values.weekly', limit: 'weekly_limit', snapshot: 'weekly_window' },
  { label: 'groups.values.monthly', limit: 'monthly_limit', snapshot: 'monthly_window' },
] as const

export function GroupQuotaWindows({ group }: { group: AdminGroup }) {
  const { i18n, t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const numberFormat = new Intl.NumberFormat(i18n.language)
  const resetFormat = new Intl.DateTimeFormat(i18n.language, {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })

  return (
    <div className="grid gap-1.5">
      {GROUP_WINDOWS.map((window) => {
        const limit = group[window.limit]
        const snapshot = group[window.snapshot]
        const exhausted = limit !== null && snapshot.usage >= limit
        const remaining = limit === null ? null : Math.max(0, limit - snapshot.usage)
        const resetText = resetFormat.format(snapshot.resets_at * 1000)
        return (
          <div key={window.snapshot} className="grid grid-cols-[auto_minmax(0,1fr)] items-start gap-x-1.5 text-[0.6875rem] leading-4">
            <Badge className="h-4 rounded px-1 text-[0.625rem] font-medium text-muted-foreground">
              {t(window.label)}
            </Badge>
            <div className="min-w-0">
              <div className={cn('truncate font-medium tabular-nums', exhausted && 'text-warning')}>
                {limit === null
                  ? t('groups.values.windowUsageUnlimited', { used: formatQuota(snapshot.usage) })
                  : t('groups.values.windowUsage', {
                      used: formatQuota(snapshot.usage),
                      limit: formatQuota(limit),
                    })}
              </div>
              <time
                className="block truncate text-[0.625rem] text-muted-foreground"
                dateTime={new Date(snapshot.resets_at * 1000).toISOString()}
                title={resetText}
              >
                {remaining === null
                  ? t('groups.values.windowResetsAt', { value: resetText })
                  : t('groups.values.windowRemainingResetsAt', {
                      remaining: formatQuota(remaining),
                      value: resetText,
                    })}
              </time>
            </div>
          </div>
        )
      })}
      <div className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-1.5 border-t border-[var(--hairline)] pt-1.5 text-[0.6875rem]">
        <Badge className="h-4 rounded px-1 text-[0.625rem] font-medium text-muted-foreground">RPM</Badge>
        <span className="truncate font-medium tabular-nums">
          {group.rpm_limit === null ? t('groups.values.unlimited') : numberFormat.format(group.rpm_limit)}
        </span>
      </div>
    </div>
  )
}
