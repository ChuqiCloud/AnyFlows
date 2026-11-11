import { Button, Skeleton } from '@heroui/react'
import { Activity, ArrowLeft, Clock3, RefreshCw, ShieldCheck } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

import { PublicSiteHeader } from '@/components/layout/public-site-header'
import { SiteFooter } from '@/components/layout/site-footer'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { usePublicServiceStatus } from './public-status-api'

function StatusBadge({ status }: { status: string }) {
  const { t } = useTranslation()
  const tone = status === 'operational' ? 'text-success' : status === 'degraded' || status === 'unavailable' ? 'text-danger' : 'text-warning'
  return <span className={`inline-flex items-center gap-2 text-sm font-semibold ${tone}`}><span className="size-2 rounded-full bg-current" aria-hidden="true" />{t(`publicStatus.states.${status}`, { defaultValue: t('publicStatus.states.unknown') })}</span>
}

export function PublicStatusPage() {
  const { t, i18n } = useTranslation()
  const query = usePublicServiceStatus()
  const site = usePublicSiteSettings()
  const data = query.data
  const formatNumber = (value: number | null | undefined, digits = 0) => value == null ? '—' : new Intl.NumberFormat(i18n.language, { maximumFractionDigits: digits }).format(value)
  const formatTime = (value?: number) => value == null ? '—' : new Date(value * 1000).toLocaleString(i18n.language)
  const maxCount = Math.max(1, ...(data?.hourly ?? []).map(point => point.successful_request_count + point.failed_request_count + point.unknown_request_count))
  const points = data?.hourly ?? []
  return <div className="min-h-screen bg-background text-foreground">
    <PublicSiteHeader logoUrl={site.data?.brand.logo_url} siteName={site.data?.site_name ?? 'AnyFlows'} navigation={site.data?.navigation} />
    <main className="mx-auto grid w-full max-w-5xl gap-8 px-4 pb-16 pt-24 sm:px-6">
      <header className="flex flex-wrap items-start justify-between gap-4 border-b border-[var(--hairline)] pb-5">
        <div><div className="flex items-center gap-2 text-xs font-medium text-muted-foreground"><Activity className="size-4" aria-hidden="true" />{t('publicStatus.eyebrow')}</div><h1 className="mt-2 text-2xl font-semibold">{t('publicStatus.title')}</h1><p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">{t('publicStatus.subtitle')}</p></div>
        <div className="flex items-center gap-3"><Button as={Link} to="/" size="sm" variant="bordered"><ArrowLeft className="size-3.5" aria-hidden="true" />{t('publicStatus.home')}</Button><Button type="button" size="sm" variant="bordered" isDisabled={query.isFetching} onPress={() => void query.refetch()}><RefreshCw className={`size-3.5 ${query.isFetching ? 'animate-spin' : ''}`} aria-hidden="true" />{t('publicStatus.refresh')}</Button></div>
      </header>

      {query.isPending ? <div className="grid gap-3 sm:grid-cols-3" role="status" aria-label={t('publicStatus.loading')}>{[0, 1, 2].map(item => <Skeleton key={item} className="h-24 rounded-lg" />)}<Skeleton className="h-48 rounded-lg sm:col-span-3" /></div> : query.isError || !data ? <section className="grid gap-3 border-b border-[var(--hairline)] pb-6" role="alert"><StatusBadge status="unknown" /><p className="text-sm text-muted-foreground">{t('publicStatus.error')}</p><Button className="w-fit" size="sm" variant="bordered" onPress={() => void query.refetch()}>{t('publicStatus.retry')}</Button></section> : <>
        <section className="grid gap-3 border-b border-[var(--hairline)] pb-6 sm:grid-cols-3">
          <article className="min-w-0 border-r border-[var(--hairline)] pr-4"><span className="text-xs text-muted-foreground">{t('publicStatus.api')}</span><div className="mt-2"><StatusBadge status={!data.api_ready ? 'unavailable' : data.status} /></div></article>
          <article className="min-w-0 border-r border-[var(--hairline)] pr-4"><span className="text-xs text-muted-foreground">{t('publicStatus.availability')}</span><strong className="mt-2 block text-xl tabular-nums">{data.availability_percent == null ? '—' : `${formatNumber(data.availability_percent, 2)}%`}</strong></article>
          <article className="min-w-0"><span className="text-xs text-muted-foreground">{t('publicStatus.averageLatency')}</span><strong className="mt-2 block text-xl tabular-nums">{data.average_success_duration_ms == null ? '—' : `${formatNumber(data.average_success_duration_ms)} ms`}</strong></article>
          <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground sm:col-span-3"><span>{t('publicStatus.period', { start: formatTime(data.period_start), end: formatTime(data.period_end) })}</span><span className="inline-flex items-center gap-1.5"><Clock3 className="size-3.5" aria-hidden="true" />{t('publicStatus.updated', { time: formatTime(data.sampled_at) })}</span></div>
        </section>

        {data.status === 'insufficient_data' || data.status === 'unknown' ? <section className="flex items-start gap-3 border-b border-[var(--hairline)] py-5"><ShieldCheck className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden="true" /><p className="text-sm leading-6 text-muted-foreground">{t('publicStatus.insufficientData')}</p></section> : null}
        {data.status === 'monitoring' ? <section className="border-b border-warning/30 py-4 text-sm leading-6 text-warning">{t('publicStatus.monitoring')}</section> : null}

        <section className="grid gap-4 border-b border-[var(--hairline)] py-4" aria-labelledby="status-history">
          <div className="flex items-start justify-between gap-4"><div><h2 id="status-history" className="text-base font-semibold">{t('publicStatus.history')}</h2><p className="mt-1 text-xs text-muted-foreground">{t('publicStatus.historyHint')}</p></div>{data.sample_count != null ? <span className="text-xs tabular-nums text-muted-foreground">{t('publicStatus.requests', { count: formatNumber(data.sample_count) })}</span> : null}</div>
          {points.length ? <ol className="grid min-h-28 grid-cols-12 items-end gap-1 sm:grid-cols-24" aria-label={t('publicStatus.hourlyHistory')}>
            {points.map((point, index) => {
              const total = point.successful_request_count + point.failed_request_count + point.unknown_request_count
              const height = Math.max(4, Math.round((total / maxCount) * 100))
              const failedHeight = total ? Math.max(0, Math.round((point.failed_request_count / total) * height)) : 0
              const unknownHeight = total ? Math.max(0, Math.round((point.unknown_request_count / total) * height)) : 0
              return <li key={point.period_start} title={`${formatTime(point.period_start)} · ${formatNumber(total)} ${t('publicStatus.requestsLabel')}`} className="flex h-28 flex-col items-stretch justify-end" aria-label={`${formatTime(point.period_start)}: ${formatNumber(total)} ${t('publicStatus.requestsLabel')}`}><span className="flex min-h-1 flex-col justify-end overflow-hidden rounded-sm bg-success/70" style={{ height: `${height}%` }}><span className="w-full shrink-0 bg-danger" style={{ height: `${failedHeight}%` }} /><span className="w-full shrink-0 bg-warning" style={{ height: `${unknownHeight}%` }} /></span><span className="sr-only">{index}</span></li>
            })}
          </ol> : <p className="py-8 text-center text-sm text-muted-foreground">{t('publicStatus.noHistory')}</p>}
          <div className="flex flex-wrap gap-x-5 gap-y-2 text-xs text-muted-foreground"><span className="inline-flex items-center gap-1.5"><span className="size-2 rounded-full bg-success/70" />{t('publicStatus.success')}</span><span className="inline-flex items-center gap-1.5"><span className="size-2 rounded-full bg-danger" />{t('publicStatus.failure')}</span><span className="inline-flex items-center gap-1.5"><span className="size-2 rounded-full bg-warning" />{t('publicStatus.unknown')}</span></div>
        </section>

        <section className="grid gap-2 py-3"><h2 className="text-sm font-semibold">{t('publicStatus.scopeTitle')}</h2><p className="text-xs leading-5 text-muted-foreground">{t('publicStatus.scope')}</p></section>
      </>}
    </main>
    <SiteFooter siteName={site.data?.site_name ?? 'AnyFlows'} navigation={site.data?.navigation} />
  </div>
}
