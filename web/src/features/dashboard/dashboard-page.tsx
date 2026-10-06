import { Activity, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { useAdminDashboard } from './dashboard-api'
import { DashboardAvailability } from './dashboard-availability'
import { DashboardAnalyticsExport } from './dashboard-analytics-export'
import { DashboardComposition } from './dashboard-composition'
import { DashboardPerformance } from './dashboard-performance'
import { DashboardOutcomes } from './dashboard-outcomes'
import { DashboardSummary } from './dashboard-summary'
import { DashboardTrend } from './dashboard-trend'
import { DashboardSla } from './dashboard-sla'
import './dashboard-report.css'

function isEmptyDashboard(dashboard: AdminDashboardResponse) {
  return dashboard.request_count === 0
    && dashboard.outcome_request_count === 0
    && dashboard.enabled_channel_count === 0
    && dashboard.disabled_channel_count === 0
    && dashboard.auto_disabled_channel_count === 0
}

export function DashboardPage() {
  const { t, i18n } = useTranslation()
  const dashboardQuery = useAdminDashboard()
  const { formatQuota } = useBalanceDisplay()
  const dashboard = dashboardQuery.data
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const formatNumber = (value: number) => new Intl.NumberFormat(locale).format(value)
  const formatPercent = (value: number) => new Intl.NumberFormat(locale, {
    style: 'percent',
    maximumFractionDigits: 1,
  }).format(value)
  const formatDateTime = (value: number) => new Intl.DateTimeFormat(locale, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(value * 1000)
  const formatHour = (value: number) => new Intl.DateTimeFormat(locale, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(value * 1000)

  return (
    <div className="report-page flex flex-col gap-5" data-report-theme="classic">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('dashboard.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {dashboard
              ? t('dashboard.period', {
                start: formatDateTime(dashboard.period_start),
                end: formatDateTime(dashboard.period_end),
              })
              : t('dashboard.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          className="self-start sm:self-auto"
          disabled={dashboardQuery.isFetching}
          onClick={() => dashboardQuery.refetch()}
        >
          <RefreshCw className={dashboardQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('dashboard.actions.refresh')}
        </Button>
      </header>

      {dashboardQuery.isPending ? (
        <div className="grid gap-3" aria-label={t('dashboard.loading')}>
          <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
            {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-28 rounded-xl" />)}
          </div>
          <div className="grid gap-3 xl:grid-cols-[minmax(0,1.55fr)_minmax(15rem,0.75fr)]">
            <Skeleton className="h-52 rounded-xl" />
            <Skeleton className="h-52 rounded-xl" />
          </div>
          <div className="grid gap-3 xl:grid-cols-[minmax(0,1.55fr)_minmax(15rem,0.75fr)]">
            <Skeleton className="h-52 rounded-xl" />
            <Skeleton className="h-52 rounded-xl" />
          </div>
        </div>
      ) : dashboardQuery.isError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('dashboard.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('dashboard.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => dashboardQuery.refetch()}>
            {t('dashboard.actions.retry')}
          </Button>
        </div>
      ) : dashboard && isEmptyDashboard(dashboard) ? (
        <Card>
          <CardContent className="grid min-h-64 place-items-center p-6">
            <div className="max-w-sm text-center">
              <div className="mx-auto grid size-9 place-items-center rounded-lg border border-[var(--hairline)] text-muted-foreground">
                <Activity className="size-4" aria-hidden="true" />
              </div>
              <h3 className="mt-3 text-sm font-semibold">{t('dashboard.empty.title')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('dashboard.empty.body')}</p>
            </div>
          </CardContent>
        </Card>
      ) : dashboard ? (
        <>
          <DashboardSummary dashboard={dashboard} formatNumber={formatNumber} formatPercent={formatPercent} formatQuota={formatQuota} />
          <section className="grid gap-3 xl:grid-cols-[minmax(0,1.55fr)_minmax(15rem,0.75fr)]">
            <DashboardTrend dashboard={dashboard} formatHour={formatHour} formatNumber={formatNumber} formatQuota={formatQuota} />
            <DashboardPerformance dashboard={dashboard} formatNumber={formatNumber} formatPercent={formatPercent} />
          </section>
          <DashboardSla />
          <section className="grid gap-3 xl:grid-cols-[minmax(0,1.55fr)_minmax(15rem,0.75fr)]">
            <DashboardComposition dashboard={dashboard} formatNumber={formatNumber} formatPercent={formatPercent} />
            <DashboardAvailability
              dashboard={dashboard}
              formatNumber={formatNumber}
              formatPercent={formatPercent}
            />
          </section>
          <DashboardOutcomes dashboard={dashboard} formatNumber={formatNumber} formatPercent={formatPercent} formatQuota={formatQuota} />
        </>
      ) : null}
      <DashboardAnalyticsExport formatNumber={formatNumber} />
    </div>
  )
}
