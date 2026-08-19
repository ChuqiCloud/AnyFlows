import { CheckCircle2, GitBranch, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'
import { DashboardFlowMap } from './dashboard-flow-map'

type DashboardOutcomesProps = {
  dashboard: AdminDashboardResponse
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
}

function ratio(count: number, total: number) {
  return total > 0 ? count / total : null
}

export function DashboardOutcomes({ dashboard, formatNumber, formatPercent }: DashboardOutcomesProps) {
  const { t } = useTranslation()
  const successRate = ratio(dashboard.successful_request_count, dashboard.outcome_request_count)
  const failureRate = ratio(dashboard.failed_request_count, dashboard.outcome_request_count)

  return (
    <section className="grid gap-3 xl:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)]" aria-label={t('dashboard.outcomes.label')}>
      <Card>
        <CardHeader className="border-b border-[var(--hairline)] px-4 py-3">
          <CardTitle className="text-sm">{t('dashboard.outcomes.title')}</CardTitle>
          <CardDescription className="mt-1 text-xs">{t('dashboard.outcomes.subtitle')}</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4 px-4 py-4">
          <div className="grid gap-3 sm:grid-cols-3">
            <OutcomeMetric
              icon={GitBranch}
              label={t('dashboard.outcomes.sampleCount')}
              value={formatNumber(dashboard.outcome_request_count)}
              hint={t('dashboard.outcomes.sampleHint')}
            />
            <OutcomeMetric
              icon={CheckCircle2}
              label={t('dashboard.outcomes.successRate')}
              value={successRate === null ? '--' : formatPercent(successRate)}
              hint={t('dashboard.outcomes.successHint', { count: formatNumber(dashboard.successful_request_count) })}
            />
            <OutcomeMetric
              icon={TriangleAlert}
              label={t('dashboard.outcomes.failureRate')}
              value={failureRate === null ? '--' : formatPercent(failureRate)}
              hint={t('dashboard.outcomes.failureHint', { count: formatNumber(dashboard.failed_request_count) })}
            />
          </div>

          {dashboard.outcome_request_count === 0 ? (
            <div className="rounded-lg border border-dashed border-[var(--hairline)] px-3 py-4 text-xs text-muted-foreground">
              {t('dashboard.outcomes.empty')}
            </div>
          ) : (
            <div className="grid gap-3">
              <h3 className="text-xs font-medium text-muted-foreground">{t('dashboard.outcomes.failuresTitle')}</h3>
              {dashboard.failures.length === 0 ? (
                <p className="text-xs text-muted-foreground">{t('dashboard.outcomes.noFailures')}</p>
              ) : (
                <div className="grid gap-2">
                  {dashboard.failures.map((failure) => {
                    const share = ratio(failure.request_count, dashboard.failed_request_count)
                    return (
                      <div key={failure.kind} className="grid gap-1.5">
                        <div className="flex items-center justify-between gap-3 text-xs">
                          <span>{t(`dashboard.outcomes.failures.${failure.kind}`)}</span>
                          <span className="tabular-nums text-muted-foreground">
                            {formatNumber(failure.request_count)} · {share === null ? '--' : formatPercent(share)}
                          </span>
                        </div>
                        <div className="h-1.5 overflow-hidden rounded-full bg-surface-2" role="presentation">
                          <div className="h-full bg-destructive/75" style={{ width: `${(share ?? 0) * 100}%` }} />
                        </div>
                      </div>
                    )
                  })}
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="border-b border-[var(--hairline)] px-4 py-3">
          <CardTitle className="text-sm">{t('dashboard.outcomes.flowsTitle')}</CardTitle>
          <CardDescription className="mt-1 text-xs">{t('dashboard.outcomes.flowsSubtitle')}</CardDescription>
        </CardHeader>
        <DashboardFlowMap dashboard={dashboard} formatNumber={formatNumber} formatPercent={formatPercent} />
      </Card>
    </section>
  )
}

function OutcomeMetric({
  icon: Icon,
  label,
  value,
  hint,
}: {
  icon: typeof GitBranch
  label: string
  value: string
  hint: string
}) {
  return (
    <div className="rounded-lg border border-[var(--hairline)] p-3">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <Icon className="size-3.5" aria-hidden="true" />
        <span>{label}</span>
      </div>
      <div className="mt-2 text-lg font-semibold tabular-nums">{value}</div>
      <div className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{hint}</div>
    </div>
  )
}
