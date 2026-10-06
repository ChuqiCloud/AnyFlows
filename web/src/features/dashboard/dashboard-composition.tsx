import { useTranslation } from 'react-i18next'

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'

type DashboardCompositionProps = {
  dashboard: AdminDashboardResponse
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
}

type CompositionRowProps = {
  label: string
  count: number
  total: number
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
  tone: 'primary' | 'info' | 'muted'
}

function CompositionRow({ label, count, total, formatNumber, formatPercent, tone }: CompositionRowProps) {
  const ratio = total > 0 ? count / total : 0
  return (
    <div className="grid gap-1.5">
      <div className="flex items-center justify-between gap-3 text-xs">
        <span>{label}</span>
        <span className="shrink-0 tabular-nums text-muted-foreground">
          {formatNumber(count)} · {formatPercent(ratio)}
        </span>
      </div>
      <div
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={Math.max(total, 1)}
        aria-valuenow={count}
        className="h-1.5 overflow-hidden rounded-full bg-surface-2"
      >
        <div
          className={tone === 'primary'
            ? 'h-full bg-primary'
            : tone === 'info'
              ? 'h-full bg-info'
              : 'h-full bg-muted-foreground/45'}
          style={{ width: `${ratio * 100}%` }}
        />
      </div>
    </div>
  )
}

export function DashboardComposition({ dashboard, formatNumber, formatPercent }: DashboardCompositionProps) {
  const { t } = useTranslation()
  return (
    <Card>
      <CardHeader className="border-b border-[var(--hairline)] px-4 py-3">
        <CardTitle className="text-sm">{t('dashboard.composition.title')}</CardTitle>
        <CardDescription className="text-xs">{t('dashboard.composition.subtitle')}</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-5 px-4 py-4 md:grid-cols-2">
        <section className="grid content-start gap-3" aria-label={t('dashboard.composition.source.title')}>
          <h3 className="text-xs font-medium text-muted-foreground">{t('dashboard.composition.source.title')}</h3>
          <CompositionRow
            label={t('dashboard.composition.source.upstream')}
            count={dashboard.upstream_usage_count}
            total={dashboard.request_count}
            formatNumber={formatNumber}
            formatPercent={formatPercent}
            tone="primary"
          />
          <CompositionRow
            label={t('dashboard.composition.source.estimated')}
            count={dashboard.estimated_usage_count}
            total={dashboard.request_count}
            formatNumber={formatNumber}
            formatPercent={formatPercent}
            tone="muted"
          />
        </section>

        <section className="grid content-start gap-3 border-t border-[var(--hairline)] pt-4 md:border-l md:border-t-0 md:pl-5 md:pt-0" aria-label={t('dashboard.composition.billing.title')}>
          <h3 className="text-xs font-medium text-muted-foreground">{t('dashboard.composition.billing.title')}</h3>
          <CompositionRow
            label={t('dashboard.composition.billing.perToken')}
            count={dashboard.per_token_request_count}
            total={dashboard.request_count}
            formatNumber={formatNumber}
            formatPercent={formatPercent}
            tone="primary"
          />
          <CompositionRow
            label={t('dashboard.composition.billing.perCall')}
            count={dashboard.per_call_request_count}
            total={dashboard.request_count}
            formatNumber={formatNumber}
            formatPercent={formatPercent}
            tone="info"
          />
          <CompositionRow
            label={t('dashboard.composition.billing.free')}
            count={dashboard.free_request_count}
            total={dashboard.request_count}
            formatNumber={formatNumber}
            formatPercent={formatPercent}
            tone="muted"
          />
        </section>
      </CardContent>
    </Card>
  )
}
