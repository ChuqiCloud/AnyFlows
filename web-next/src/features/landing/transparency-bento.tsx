import { Activity, ArrowUpRight, RefreshCw } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

import { Button } from '@heroui/react'
import { usePublicServiceStatus } from '@/features/public-status/public-status-api'

export function TransparencyBento() {
  const { t, i18n } = useTranslation()
  const query = usePublicServiceStatus()
  const data = query.data
  const status = !data ? 'unknown' : !data.api_ready ? 'unavailable' : data.status
  const tone = status === 'operational' ? 'text-success' : status === 'degraded' || status === 'unavailable' ? 'text-danger' : 'text-warning'
  const time = data ? new Date(data.sampled_at * 1000).toLocaleString(i18n.language) : undefined
  return <section className="mx-auto w-full max-w-[1240px] border-t border-[var(--hairline)] px-4 py-10 sm:px-6 lg:px-8" aria-labelledby="public-status-heading">
    <div className="flex flex-wrap items-center justify-between gap-4">
      <div className="flex min-w-0 items-start gap-3">
        <Activity className="mt-0.5 size-5 shrink-0 text-muted-foreground" aria-hidden="true" />
        <div className="min-w-0">
          <h2 id="public-status-heading" className="text-base font-semibold">{t('publicStatus.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{data ? t('publicStatus.updated', { time }) : t('publicStatus.subtitle')}</p>
          <div className={`mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm font-medium ${tone}`}>
            <span className="inline-flex items-center gap-2"><span className="size-2 rounded-full bg-current" aria-hidden="true" />{t(`publicStatus.states.${status}`, { defaultValue: t('publicStatus.states.unknown') })}</span>
            {data?.availability_percent != null ? <span className="text-foreground tabular-nums">{t('publicStatus.availability')}: {data.availability_percent.toFixed(2)}%</span> : null}
            {data?.sample_count != null ? <span className="text-muted-foreground tabular-nums">{t('publicStatus.requests', { count: new Intl.NumberFormat(i18n.language).format(data.sample_count) })}</span> : null}
          </div>
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {query.isError ? <Button type="button" size="sm" variant="light" isDisabled={query.isFetching} onPress={() => void query.refetch()}><RefreshCw className={`size-3.5 ${query.isFetching ? 'animate-spin' : ''}`} aria-hidden="true" />{t('publicStatus.retry')}</Button> : null}
        <Button as={Link} to="/status" size="sm" variant="bordered">{t('publicStatus.history')}<ArrowUpRight className="size-3.5" aria-hidden="true" /></Button>
      </div>
    </div>
  </section>
}
