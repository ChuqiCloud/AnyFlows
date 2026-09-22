import { Card, CardBody, CardHeader } from '@heroui/react'
import { CircleCheck, CircleHelp, CircleX, RadioTower } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { getDashboardAvailabilityMetrics } from './dashboard-availability-metrics'

type DashboardAvailabilityProps = {
  dashboard: AdminDashboardResponse
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
}

export function DashboardAvailability({
  dashboard,
  formatNumber,
  formatPercent,
}: DashboardAvailabilityProps) {
  const { t } = useTranslation()
  const metrics = getDashboardAvailabilityMetrics(dashboard)
  const channels = [
    { key: 'enabled', count: dashboard.enabled_channel_count, tone: 'bg-success' },
    { key: 'disabled', count: dashboard.disabled_channel_count, tone: 'bg-muted-foreground/55' },
    { key: 'autoDisabled', count: dashboard.auto_disabled_channel_count, tone: 'bg-warning' },
  ] as const
  const channelCount = channels.reduce((sum, channel) => sum + channel.count, 0)
  const sampleState = dashboard.outcome_request_count === 0
    ? 'noSamples'
    : metrics.resolvedRequestCount === 0
      ? 'unknownOnly'
      : 'resolved'
  const channelState = channelCount === 0
    ? 'noChannels'
    : dashboard.enabled_channel_count === 0
      ? 'noEnabledChannels'
      : 'configured'

  return (
    <Card className="border border-[var(--hairline)] bg-card" shadow="none">
      <CardHeader className="block border-b border-[var(--hairline)] px-4 py-3">
        <h3 className="text-sm font-semibold">{t('dashboard.availability.title')}</h3>
        <p className="text-xs text-muted-foreground">{t('dashboard.availability.subtitle')}</p>
      </CardHeader>
      <CardBody className="grid gap-4 px-4 py-4">
        <section aria-label={t('dashboard.availability.factsLabel')}>
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <div className="text-xs text-muted-foreground">
                {t('dashboard.availability.observedRate')}
              </div>
              <div className="mt-1 text-2xl font-semibold tabular-nums">
                {metrics.successRate === null ? '--' : formatPercent(metrics.successRate)}
              </div>
            </div>
            <CircleCheck className="mt-0.5 size-4 shrink-0 text-success" aria-hidden="true" />
          </div>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
            {sampleState === 'resolved'
              ? t('dashboard.availability.resolved', {
                resolved: formatNumber(metrics.resolvedRequestCount),
                total: formatNumber(dashboard.outcome_request_count),
                rate: formatPercent(metrics.resolutionRate ?? 0),
              })
              : t(`dashboard.availability.${sampleState}`)}
          </p>

          <div
            className="mt-3 flex h-1.5 overflow-hidden rounded-full bg-surface-2"
            role="img"
            aria-label={t('dashboard.availability.distributionLabel', {
              success: formatNumber(dashboard.successful_request_count),
              failure: formatNumber(metrics.confirmedFailureCount),
              unknown: formatNumber(metrics.unknownRequestCount),
            })}
          >
            <AvailabilitySegment
              count={dashboard.successful_request_count}
              total={dashboard.outcome_request_count}
              className="bg-success"
            />
            <AvailabilitySegment
              count={metrics.confirmedFailureCount}
              total={dashboard.outcome_request_count}
              className="bg-destructive/75"
            />
            <AvailabilitySegment
              count={metrics.unknownRequestCount}
              total={dashboard.outcome_request_count}
              className="bg-warning"
            />
          </div>

          <div className="mt-3 grid gap-2 text-xs">
            <AvailabilityFact
              icon={CircleCheck}
              label={t('dashboard.availability.success')}
              value={formatNumber(dashboard.successful_request_count)}
              tone="text-success"
            />
            <AvailabilityFact
              icon={CircleX}
              label={t('dashboard.availability.failure')}
              value={formatNumber(metrics.confirmedFailureCount)}
              tone="text-destructive"
            />
            <AvailabilityFact
              icon={CircleHelp}
              label={t('dashboard.availability.unknown')}
              value={formatNumber(metrics.unknownRequestCount)}
              tone="text-warning"
            />
          </div>
        </section>

        <section className="border-t border-[var(--hairline)] pt-4" aria-label={t('dashboard.availability.channelsLabel')}>
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <div className="text-xs text-muted-foreground">{t('dashboard.availability.routable')}</div>
              <div className="mt-1 text-lg font-semibold tabular-nums">
                {t('dashboard.availability.channelValue', {
                  enabled: formatNumber(dashboard.enabled_channel_count),
                  total: formatNumber(channelCount),
                })}
              </div>
            </div>
            <RadioTower className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          </div>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
            {t(`dashboard.availability.${channelState}`)}
          </p>
          <div className="mt-3 divide-y divide-[var(--hairline)]">
            {channels.map((channel) => (
              <div key={channel.key} className="flex min-h-8 items-center justify-between gap-3 text-xs">
                <span className="inline-flex min-w-0 items-center gap-2">
                  <span className={cn('size-1.5 shrink-0 rounded-full', channel.tone)} aria-hidden="true" />
                  <span>{t(`dashboard.channels.${channel.key}`)}</span>
                </span>
                <span className="shrink-0 font-medium tabular-nums">{formatNumber(channel.count)}</span>
              </div>
            ))}
          </div>
        </section>
      </CardBody>
    </Card>
  )
}

function AvailabilitySegment({ count, total, className }: { count: number, total: number, className: string }) {
  const width = total > 0 ? count / total * 100 : 0
  return <span className={className} style={{ width: `${width}%` }} aria-hidden="true" />
}

function AvailabilityFact({
  icon: Icon,
  label,
  value,
  tone,
}: {
  icon: typeof CircleCheck
  label: string
  value: string
  tone: string
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="inline-flex min-w-0 items-center gap-2">
        <Icon className={cn('size-3.5 shrink-0', tone)} aria-hidden="true" />
        <span>{label}</span>
      </span>
      <span className="shrink-0 font-medium tabular-nums">{value}</span>
    </div>
  )
}
