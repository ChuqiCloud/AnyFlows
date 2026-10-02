import { ChevronLeft, ChevronRight, RefreshCw, Search, ShieldCheck } from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { ServiceLevelRow } from '@/lib/api/generated/types.gen'
import { useServiceLevels } from './dashboard-sla-api'
import { serviceLevelMetrics, serviceLevelTone } from './dashboard-sla-model'

export function DashboardSla() {
  const { t, i18n } = useTranslation()
  const queryClient = useQueryClient()
  const [dimension, setDimension] = useState<'model' | 'channel'>('model')
  const [search, setSearch] = useState('')
  const [filter, setFilter] = useState('')
  const [page, setPage] = useState(1)
  const [size, setSize] = useState(10)
  const [sort, setSort] = useState<'requests' | 'failures'>('requests')
  const [target, setTarget] = useState(0.999)
  const report = useServiceLevels(dimension, filter, page, size, sort)
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const number = new Intl.NumberFormat(locale)
  const percent = new Intl.NumberFormat(locale, { style: 'percent', minimumFractionDigits: 2, maximumFractionDigits: 3 })
  const formatRate = (value: number) => percent.format(Math.floor(value * 100_000) / 100_000)
  const time = new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' })
  const pages = Math.max(1, Math.ceil((report.data?.total ?? 0) / size))
  useEffect(() => {
    const timer = window.setTimeout(() => { setFilter(search.trim()); setPage(1) }, 300)
    return () => window.clearTimeout(timer)
  }, [search])
  useEffect(() => {
    if (report.data && !report.isFetching && page > pages) {
      void queryClient.invalidateQueries({ queryKey: ['admin-service-levels', dimension, filter, pages, size, sort], exact: true, refetchType: 'none' })
      setPage(pages)
    }
  }, [report.data, report.isFetching, page, pages, dimension, filter, size, sort, queryClient])

  return (
    <section className="report-sla" aria-label={t('dashboard.sla.title')}>
      <header className="report-section-header">
        <div className="report-section-title"><ShieldCheck size={18} aria-hidden="true" /><h3>{t('dashboard.sla.title')}</h3><span className="report-period">24h</span></div>
        <button type="button" className="report-icon-button" disabled={report.isFetching} onClick={() => void report.refetch()} title={t('dashboard.actions.refresh')} aria-label={t('dashboard.actions.refresh')}><RefreshCw size={15} className={report.isFetching ? 'animate-spin' : undefined} /></button>
      </header>
      <div className="report-sla-toolbar">
        <div className="report-segments" role="group" aria-label={t('dashboard.sla.dimension')}>
          {(['model', 'channel'] as const).map(value => <button key={value} type="button" aria-pressed={dimension === value} onClick={() => { setDimension(value); setPage(1) }}>{t(`dashboard.sla.${value}`)}</button>)}
        </div>
        <label className="report-search"><Search size={15} aria-hidden="true" /><input maxLength={128} aria-label={t('dashboard.sla.search')} placeholder={t('dashboard.sla.search')} value={search} onChange={event => setSearch(event.target.value)} /></label>
        <label className="report-filter"><span>{t('dashboard.sla.sort')}</span><select value={sort} onChange={event => { setSort(event.target.value as typeof sort); setPage(1) }}>{(['requests', 'failures'] as const).map(value => <option key={value} value={value}>{t(`dashboard.sla.sorts.${value}`)}</option>)}</select></label>
        <label className="report-filter"><span>{t('dashboard.sla.target')}</span><select value={target} onChange={event => setTarget(Number(event.target.value))}>{[0.99, 0.999, 0.9995, 0.9999].map(value => <option key={value} value={value}>{percent.format(value)}</option>)}</select></label>
      </div>
      <div className="report-sla-table" aria-busy={report.isFetching}>
        <div className="report-sla-head" aria-hidden="true"><span>{t(`dashboard.sla.${dimension}`)}</span><span>{t('dashboard.sla.rate')}</span><span>{t('dashboard.sla.requests')}</span><span>{t('dashboard.sla.duration')}</span><span>{t('dashboard.sla.history')}</span></div>
        {report.isPending ? <div className="report-loading" role="status" aria-label={t('dashboard.loading')}>{[0, 1, 2, 3, 4].map(value => <div key={value} className="report-loading-row motion-safe:animate-pulse" />)}</div>
          : report.isError ? <div className="report-message" role="alert"><p>{t('dashboard.error.body')}</p><button type="button" onClick={() => void report.refetch()}>{t('dashboard.actions.retry')}</button></div>
          : !report.data?.items.length ? <div className="report-message">{t(filter ? 'dashboard.sla.noMatches' : 'dashboard.sla.empty')}</div>
          : report.data.items.map(row => <SlaRow key={`${dimension}-${row.key}`} row={row} target={target} formatNumber={value => number.format(value)} formatPercent={formatRate} formatTime={value => time.format(value * 1000)} />)}
      </div>
      <footer className="report-sla-footer">
        <div className="report-legend">{(['healthy', 'degraded', 'critical', 'unknown', 'empty'] as const).map(value => <span key={value}><i className={`report-health-${value}`} aria-hidden="true" />{t(`dashboard.sla.states.${value}`)}</span>)}</div>
        <div className="report-pagination">
          <label>{t('dashboard.sla.perPage')}<select value={size} onChange={event => { setSize(Number(event.target.value)); setPage(1) }}>{[5, 10, 20].map(value => <option key={value}>{value}</option>)}</select></label>
          <span>{t('dashboard.sla.page', { page, pages, total: number.format(report.data?.total ?? 0) })}</span>
          <button type="button" className="report-icon-button" disabled={page === 1 || report.isFetching} onClick={() => setPage(page - 1)} aria-label={t('dashboard.sla.previous')} title={t('dashboard.sla.previous')}><ChevronLeft size={16} /></button>
          <button type="button" className="report-icon-button" disabled={page >= pages || report.isFetching} onClick={() => setPage(page + 1)} aria-label={t('dashboard.sla.next')} title={t('dashboard.sla.next')}><ChevronRight size={16} /></button>
        </div>
      </footer>
      <p className="report-sla-note">{t('dashboard.sla.basis')}{dimension === 'channel' && report.data && report.data.unattributed_request_count > 0 ? ` ${t('dashboard.sla.unattributed', { count: number.format(report.data.unattributed_request_count) })}` : ''}</p>
    </section>
  )
}

function SlaRow({ row, target, formatNumber, formatPercent, formatTime }: {
  row: ServiceLevelRow
  target: number
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
  formatTime: (value: number) => string
}) {
  const { t } = useTranslation()
  const [active, setActive] = useState<number | null>(null)
  const metrics = serviceLevelMetrics(row, target)
  const selected = active === null ? undefined : row.hourly[active]
  return (
    <div className="report-sla-row">
      <div className="report-sla-name"><span className={`report-status-dot report-health-${metrics.state}`} aria-hidden="true" /><div><strong title={row.name}>{row.name}</strong><small>{t(`dashboard.sla.status.${metrics.state}`)}{metrics.coverage !== null && metrics.coverage < 1 ? ` · ${t('dashboard.sla.coverage', { rate: formatPercent(metrics.coverage) })}` : ''}</small></div></div>
      <div className="report-sla-rate"><strong className={`report-rate-${metrics.state}`}>{metrics.rate === null ? '--' : formatPercent(metrics.rate)}</strong><small title={t('dashboard.sla.budgetDefinition')}>{metrics.rate === null ? t('dashboard.sla.states.unknown') : metrics.exceededFailures > 0 ? t('dashboard.sla.exceeded', { count: formatNumber(metrics.exceededFailures) }) : t('dashboard.sla.budget', { count: formatNumber(metrics.remainingFailures) })}</small></div>
      <div className="report-sla-count"><strong>{formatNumber(row.request_count)}</strong><small>{t('dashboard.sla.failed', { count: formatNumber(row.failed_request_count) })}</small></div>
      <div className="report-sla-duration"><strong>{row.average_duration_ms === null || row.average_duration_ms === undefined ? '--' : `${formatNumber(row.average_duration_ms)} ms`}</strong><small>{t('dashboard.sla.successDuration')}</small></div>
      <div className="report-sla-history">
        <div className="report-health-grid" role="group" aria-label={`${row.name} ${t('dashboard.sla.history')}`} onPointerLeave={() => setActive(null)}>
          {row.hourly.map((point, index) => <button key={point.period_start} type="button" className={`report-health-${serviceLevelTone(point, target)}`} tabIndex={index === (active ?? row.hourly.length - 1) ? 0 : -1}
            aria-label={`${formatTime(point.period_start)} ${t('dashboard.sla.hour', { success: formatNumber(point.successful_request_count), failed: formatNumber(point.failed_request_count), unknown: formatNumber(point.unknown_request_count) })}`}
            onPointerEnter={() => setActive(index)} onFocus={() => setActive(index)} onClick={() => setActive(index)}
            onKeyDown={event => {
              if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
                event.preventDefault()
                const next = Math.min(row.hourly.length - 1, Math.max(0, index + (event.key === 'ArrowRight' ? 1 : -1)))
                setActive(next)
                ;(event.currentTarget.parentElement?.children[next] as HTMLButtonElement | undefined)?.focus()
              }
            }} />)}
        </div>
        <small className="report-health-caption" aria-live="polite">{selected ? `${formatTime(selected.period_start)} · ${t('dashboard.sla.hour', { success: formatNumber(selected.successful_request_count), failed: formatNumber(selected.failed_request_count), unknown: formatNumber(selected.unknown_request_count) })}` : t('dashboard.sla.historyRange')}</small>
      </div>
    </div>
  )
}
