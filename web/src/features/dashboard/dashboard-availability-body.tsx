import { RadioTower } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'
import { DashboardReliability } from './dashboard-reliability'

export type DashboardAvailabilityProps = {
  dashboard: AdminDashboardResponse
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
}

export function DashboardAvailabilityBody({ dashboard, formatNumber }: DashboardAvailabilityProps) {
  const { t } = useTranslation()
  const channels = [
    { key: 'enabled', count: dashboard.enabled_channel_count, tone: 'var(--report-teal)' },
    { key: 'disabled', count: dashboard.disabled_channel_count, tone: 'var(--muted-foreground)' },
    { key: 'autoDisabled', count: dashboard.auto_disabled_channel_count, tone: 'var(--report-amber)' },
  ]
  const total = channels.reduce((sum, item) => sum + item.count, 0)
  return <>
    <header className="report-section-header"><div className="report-section-title"><RadioTower size={17} aria-hidden="true" /><h3>{t('dashboard.availability.title')}</h3></div></header>
    <div className="px-4 pb-4">
      <DashboardReliability dashboard={dashboard} formatNumber={formatNumber} />
      <div className="border-t border-[var(--hairline)] pt-4">
        <div className="flex items-center justify-between gap-3 text-xs"><span className="text-muted-foreground">{t('dashboard.availability.routable')}</span><strong className="font-medium tabular-nums">{t('dashboard.availability.channelValue', { enabled: formatNumber(dashboard.enabled_channel_count), total: formatNumber(total) })}</strong></div>
        <div className="mt-3 grid gap-2">{channels.map(item => <div key={item.key} className="flex items-center justify-between text-xs"><span className="inline-flex items-center gap-2 text-muted-foreground"><span className="size-1.5 rounded-full" style={{ background: item.tone }} aria-hidden="true" />{t(`dashboard.channels.${item.key}`)}</span><span className="tabular-nums">{formatNumber(item.count)}</span></div>)}</div>
      </div>
    </div>
  </>
}
