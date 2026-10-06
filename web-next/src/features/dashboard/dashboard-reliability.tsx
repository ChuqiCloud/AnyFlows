import { useTranslation } from 'react-i18next'

import type { AdminDashboardResponse } from '@/lib/api/generated/types.gen'
import { getDashboardAvailabilityMetrics } from './dashboard-availability-metrics'

export function DashboardReliability({ dashboard, formatNumber }: { dashboard: AdminDashboardResponse; formatNumber: (value: number) => string }) {
  const { t, i18n } = useTranslation()
  const metrics = getDashboardAvailabilityMetrics(dashboard)
  const percent = new Intl.NumberFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', { style: 'percent', minimumFractionDigits: 2, maximumFractionDigits: 2 })
  const segments = [
    { key: 'success', count: dashboard.successful_request_count, color: 'var(--report-teal)' },
    { key: 'failure', count: metrics.confirmedFailureCount, color: 'var(--report-coral)' },
    { key: 'unknown', count: metrics.unknownRequestCount, color: 'var(--report-amber)' },
  ]
  const circumference = 2 * Math.PI * 48
  let offset = 0
  return (
    <div className="report-reliability">
      <div className="report-ring">
        <svg viewBox="0 0 120 120" role="img" aria-label={t('dashboard.availability.distributionLabel', { success: formatNumber(segments[0].count), failure: formatNumber(segments[1].count), unknown: formatNumber(segments[2].count) })}>
          <circle cx="60" cy="60" r="48" fill="none" stroke="var(--hairline)" strokeWidth="7" />
          {segments.map(segment => {
            const length = dashboard.outcome_request_count > 0 ? segment.count / dashboard.outcome_request_count * circumference : 0
            const segmentOffset = offset
            offset += length
            return <circle key={segment.key} cx="60" cy="60" r="48" fill="none" stroke={segment.color} strokeWidth="7" strokeDasharray={`${length} ${circumference - length}`} strokeDashoffset={-segmentOffset} transform="rotate(-90 60 60)" />
          })}
        </svg>
        <div><strong>{metrics.successRate === null ? '--' : percent.format(metrics.successRate)}</strong><span>{t('dashboard.sla.rate')}</span></div>
      </div>
      <div className="report-ring-legend">{segments.map(segment => <div key={segment.key}><span><i style={{ background: segment.color }} aria-hidden="true" />{t(`dashboard.availability.${segment.key}`)}</span><strong>{formatNumber(segment.count)}</strong></div>)}<small>{t('dashboard.sla.basis')}</small></div>
    </div>
  )
}
