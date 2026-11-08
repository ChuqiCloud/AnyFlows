import { Link, useParams, useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useMerchantCatalogDetail, useMerchantStatistics } from './merchant-pool-api'
import { MerchantHealth, MerchantModelPrice } from './merchant-catalog-card'
import { positiveInteger } from './merchant-pool-model'
import { PoolEmpty, PoolError, PoolHeader, PoolLoading, PoolNavigation, PoolStatus } from './merchant-pool-ui'

export function MerchantDetailPage() {
  const { t } = useTranslation()
  const { id } = useParams()
  const [params] = useSearchParams()
  const poolId = positiveInteger(params.get('pool') ?? '')
  const serviceId = positiveInteger(params.get('service') ?? '')
  const merchantId = positiveInteger(id ?? '')
  const serviceQuery = useMerchantCatalogDetail(poolId, serviceId)
  const stats = useMerchantStatistics(poolId, '', 86400)
  const service = serviceQuery.data
  return <div className="flex flex-col gap-4"><PoolHeader title={service?.merchant_name ?? t('merchantPool.details')} description={service?.description ?? t('merchantPool.detailHint')} refreshing={serviceQuery.isFetching || stats.isFetching} onRefresh={() => { if (poolId && serviceId) void Promise.all([serviceQuery.refetch(), stats.refetch()]) }} /><PoolNavigation />{!poolId || !serviceId || !merchantId ? <PoolEmpty>{t('merchantPool.invalidDetail')}</PoolEmpty> : serviceQuery.isPending ? <PoolLoading /> : serviceQuery.isError ? <PoolError onRetry={() => void serviceQuery.refetch()} /> : !service || service.merchant_id !== merchantId ? <PoolEmpty /> : <><section className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[var(--hairline)] p-4"><div className="flex items-center gap-3"><h3 className="text-sm font-semibold">{service.name}</h3><PoolStatus status={service.status} /></div><Link className="text-xs text-primary hover:underline" to={`/console/merchant-pool/routes?pool=${poolId}&merchant=${service.merchant_id}`}>{t('merchantPool.onlyMerchantKey')}</Link></section><section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">{service.models.map((offer) => <article key={offer.model} className="grid content-start gap-3 rounded-xl border border-[var(--hairline)] p-4"><h3 className="break-all text-sm font-semibold">{offer.model}</h3><MerchantModelPrice offer={offer} /><dl className="grid gap-1 text-xs text-muted-foreground"><div className="flex justify-between gap-3"><dt>{t('merchantPool.contextWindow')}</dt><dd>{offer.context_window?.toLocaleString() ?? '—'}</dd></div><div className="flex justify-between gap-3"><dt>{t('merchantPool.maxOutput')}</dt><dd>{offer.max_output_tokens?.toLocaleString() ?? '—'}</dd></div></dl><div className="flex flex-wrap gap-2 text-[0.6875rem]">{([['supports_streaming', 'streaming'], ['supports_tools', 'tools'], ['supports_vision', 'vision']] as const).filter(([key]) => offer[key]).map(([, label]) => <span key={label} className="rounded bg-surface-2 px-2 py-1">{t(`merchantPool.${label}`)}</span>)}</div></article>)}</section><section className="rounded-xl border border-[var(--hairline)] p-4"><h3 className="mb-3 text-sm font-semibold">{t('merchantPool.recentHealth')}</h3>{stats.isError ? <PoolError message={t('merchantPool.statsError')} onRetry={() => void stats.refetch()} /> : <MerchantHealth stats={stats.data?.items.find((item) => item.service_id === service.id)} />}<p className="mt-3 text-xs leading-5 text-muted-foreground">{t('merchantPool.statisticsHint')}</p></section></>}</div>
}
