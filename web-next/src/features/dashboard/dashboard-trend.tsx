import { Card, CardBody, CardHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'

type DashboardTrendProps = {
  dashboard: AdminDashboardResponse
  formatHour: (value: number) => string
  formatNumber: (value: number) => string
  formatQuota: (value: number) => string
}

function barHeight(value: number, maximum: number) {
  if (value <= 0 || maximum <= 0)
    return '0%'
  return `${Math.max(4, (value / maximum) * 100)}%`
}

/** 用固定 24 列展示服务端分桶，零样本也保留列宽和时间位置。 */
export function DashboardTrend({ dashboard, formatHour, formatNumber, formatQuota }: DashboardTrendProps) {
  const { t } = useTranslation()
  const maximumRequests = Math.max(0, ...dashboard.hourly.map(point => point.request_count))
  const maximumQuota = Math.max(0, ...dashboard.hourly.map(point => point.quota_consumed))

  return (
    <Card className="border border-[var(--hairline)] bg-card" shadow="none">
      <CardHeader className="block border-b border-[var(--hairline)] px-4 py-3">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <h3 className="block text-sm font-semibold">{t('dashboard.trend.title')}</h3>
            <p className="mt-1 block text-xs text-muted-foreground">{t('dashboard.trend.subtitle')}</p>
          </div>
          <div className="flex shrink-0 items-center gap-3 text-[0.6875rem] text-muted-foreground" aria-label={t('dashboard.trend.legend')}>
            <span className="inline-flex items-center gap-1.5"><span className="size-1.5 rounded-full bg-primary" />{t('dashboard.trend.requests')}</span>
            <span className="inline-flex items-center gap-1.5"><span className="size-1.5 rounded-full bg-info" />{t('dashboard.trend.quota')}</span>
          </div>
        </div>
      </CardHeader>
      <CardBody className="px-4 pb-3 pt-4">
        <div
          className="grid h-36 grid-cols-[repeat(24,minmax(0,1fr))] items-end gap-0.5 sm:gap-1"
          role="img"
          aria-label={t('dashboard.trend.chartLabel')}
        >
          {dashboard.hourly.map(point => (
            <div
              key={point.period_start}
              className="flex h-full min-w-0 items-end justify-center gap-px"
              title={t('dashboard.trend.point', {
                time: formatHour(point.period_start),
                requests: formatNumber(point.request_count),
                quota: formatQuota(point.quota_consumed),
              })}
            >
              <span className="w-[42%] max-w-2 rounded-t-[2px] bg-primary/85" style={{ height: barHeight(point.request_count, maximumRequests) }} />
              <span className="w-[42%] max-w-2 rounded-t-[2px] bg-info/75" style={{ height: barHeight(point.quota_consumed, maximumQuota) }} />
            </div>
          ))}
        </div>
        <div className="mt-2 grid grid-cols-4 text-[0.625rem] tabular-nums text-muted-foreground">
          {[0, 8, 16, 23].map((index, position) => (
            <span key={index} className={position === 3 ? 'text-right' : undefined}>
              {dashboard.hourly[index] ? formatHour(dashboard.hourly[index].period_start) : '--'}
            </span>
          ))}
        </div>
      </CardBody>
    </Card>
  )
}
