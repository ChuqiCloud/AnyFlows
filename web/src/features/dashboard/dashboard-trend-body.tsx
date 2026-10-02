import { Activity, Coins } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'
import { DashboardChart } from './dashboard-chart'

export type DashboardTrendProps = {
  dashboard: AdminDashboardResponse
  formatHour: (value: number) => string
  formatNumber: (value: number) => string
  formatQuota: (value: number) => string
}

export function DashboardTrendBody({ dashboard, formatHour, formatNumber, formatQuota }: DashboardTrendProps) {
  const { t } = useTranslation()
  const [metric, setMetric] = useState<'requests' | 'quota'>('requests')
  const requestMetric = metric === 'requests'
  const values = dashboard.hourly.map(point => ({ time: point.period_start, value: requestMetric ? point.request_count : point.quota_consumed }))
  const maximum = Math.max(0, ...values.map(point => point.value))
  const total = values.reduce((sum, point) => sum + point.value, 0)
  const format = requestMetric ? formatNumber : (value: number) => formatQuota(Math.round(value))
  return (
    <>
      <header className="report-section-header">
        <div className="report-section-title"><Activity size={17} aria-hidden="true" /><h3>{t('dashboard.trend.title')}</h3></div>
        <div className="report-segments" role="group" aria-label={t('dashboard.trend.legend')}>
          <button type="button" aria-pressed={requestMetric} onClick={() => setMetric('requests')}><Activity size={13} aria-hidden="true" />{t('dashboard.trend.requests')}</button>
          <button type="button" aria-pressed={!requestMetric} onClick={() => setMetric('quota')}><Coins size={13} aria-hidden="true" />{t('dashboard.trend.quota')}</button>
        </div>
      </header>
      <div className="report-trend-body">
        <div className="report-trend-stats"><div><span>{t('dashboard.charts.total')}</span><strong>{format(total)}</strong></div><div><span>{t('dashboard.charts.peak')}</span><strong>{format(maximum)}</strong></div><div><span>{t('dashboard.charts.average')}</span><strong>{format(values.length ? Math.round(total / values.length * 10) / 10 : 0)}</strong></div></div>
        <DashboardChart points={values} label={t(`dashboard.trend.${metric}`)} color={requestMetric ? 'var(--report-teal)' : 'var(--report-blue)'} formatTime={formatHour} formatValue={format} />
      </div>
    </>
  )
}
