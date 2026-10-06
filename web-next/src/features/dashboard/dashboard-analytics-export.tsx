import { Button, Card, CardBody, CardHeader } from '@heroui/react'
import { Activity, CheckCircle2, CircleAlert, Database, LoaderCircle, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AnalyticsExportStatusResponse } from '@/lib/api/generated/types.gen'
import { useAnalyticsExportStatus, useReplayAnalyticsExport } from './dashboard-analytics-export-api'

type Props = {
  formatNumber: (value: number) => string
}
function statusIcon(status: AnalyticsExportStatusResponse['state']) {
  if (status === 'healthy') return CheckCircle2
  if (status === 'backlog' || status === 'unavailable') return CircleAlert
  if (status === 'disabled') return Database
  return Activity
}

/** 管理员可见的分析导出健康与有界重放入口。 */
export function DashboardAnalyticsExport({ formatNumber }: Props) {
  const { t } = useTranslation()
  const query = useAnalyticsExportStatus()
  const replay = useReplayAnalyticsExport()
  const status = query.data
  const Icon = statusIcon(status?.state ?? 'disabled')
  const replaying = replay.isPending

  return (
    <Card className="border border-[var(--hairline)] bg-card" shadow="none">
      <CardHeader className="block border-b border-[var(--hairline)] px-4 py-3">
        <div className="flex items-start justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold">{t('dashboard.analyticsExport.title')}</h3>
            <p className="mt-1 text-xs text-muted-foreground">{t('dashboard.analyticsExport.subtitle')}</p>
          </div>
          <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
        </div>
      </CardHeader>
      <CardBody className="space-y-3 px-4 py-3">
        {query.isPending ? <p className="text-xs text-muted-foreground">{t('dashboard.analyticsExport.loading')}</p> : null}
        {query.isError ? <p role="alert" className="text-xs text-destructive">{t('dashboard.analyticsExport.error')}</p> : null}
        {status ? (
          <>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-xs font-medium">{t(`dashboard.analyticsExport.states.${status.state}`)}</span>
              <span className="text-xs tabular-nums text-muted-foreground">
                {status.backlog_count === null || status.backlog_count === undefined
                  ? t('dashboard.analyticsExport.noCount')
                  : t('dashboard.analyticsExport.backlog', { count: formatNumber(status.backlog_count) })}
              </span>
            </div>
            {status.enabled && status.state === 'backlog' ? (
              <Button
                type="button"
                size="sm"
                variant="bordered"
                isDisabled={replaying}
                onClick={() => replay.mutate(64)}
              >
                {replaying ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <RefreshCw className="size-3.5" aria-hidden="true" />}
                {t('dashboard.analyticsExport.replay')}
              </Button>
            ) : null}
            {replay.isError ? <p role="alert" className="text-xs text-destructive">{t('dashboard.analyticsExport.replayError')}</p> : null}
            {replay.data ? <p className="text-xs text-muted-foreground">{t('dashboard.analyticsExport.replayed', { count: formatNumber(replay.data.replayed_count) })}</p> : null}
          </>
        ) : null}
      </CardBody>
    </Card>
  )
}
