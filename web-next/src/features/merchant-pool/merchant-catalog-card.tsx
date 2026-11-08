import { Checkbox } from '@heroui/react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import type { MerchantCatalogService, MerchantModelOffer, MerchantStatistics } from './merchant-pool-model'
import { poolRatio, poolTimestamp } from './merchant-pool-model'
import { PoolStatus } from './merchant-pool-ui'

export function MerchantModelPrice({ offer }: { offer: MerchantModelOffer }) {
  const { t } = useTranslation()
  return <div className="grid gap-1 text-xs tabular-nums"><span className="font-medium">{t('merchantPool.ratioValue', { value: poolRatio(offer.sales_ratio_micros) })}</span>{offer.prices ? <><span>{t('merchantPool.priceInput', { value: offer.prices.input })}</span><span>{t('merchantPool.priceOutput', { value: offer.prices.output })}</span><details className="text-muted-foreground"><summary className="cursor-pointer">{t('merchantPool.cachePrices')}</summary><p className="mt-1">{t('merchantPool.cacheRead', { value: offer.prices.cache_read })}</p><p>{t('merchantPool.cacheWrite5m', { value: offer.prices.cache_write_5m })}</p><p>{t('merchantPool.cacheWrite1h', { value: offer.prices.cache_write_1h })}</p></details></> : <span className="text-warning">{t('merchantPool.noPrice')}</span>}</div>
}

export function MerchantHealth({ stats }: { stats?: MerchantStatistics }) {
  const { t } = useTranslation()
  const millis = (value: number | null | undefined) => value == null ? '—' : `${Math.round(value).toLocaleString()} ms`
  return <div className="grid gap-1 text-xs tabular-nums"><div className="flex flex-wrap items-center gap-2"><span className="font-medium">{stats?.sample_count && stats.success_rate !== null ? `${stats.success_rate.toFixed(1)}%` : t('merchantPool.noData')}</span>{stats?.sample_count ? <span className="text-muted-foreground">{t('merchantPool.samples', { count: stats.sample_count })}</span> : null}</div>{stats?.sample_count && stats.sample_count < 100 ? <span className="text-warning">{t('merchantPool.lowSamples')}</span> : null}<span className="text-muted-foreground">{t('merchantPool.ttft', { p50: millis(stats?.ttft_p50_ms), p95: millis(stats?.ttft_p95_ms) })}</span><span className="text-muted-foreground">{t('merchantPool.latency', { value: millis(stats?.latency_p95_ms) })}</span>{stats ? <PoolStatus status={stats.status} /> : null}</div>
}

export function MerchantCatalogCard({ service, model, stats, selected, disabled, onSelect }: { service: MerchantCatalogService; model: string; stats?: MerchantStatistics; selected?: boolean; disabled?: boolean; onSelect?: () => void }) {
  const { t, i18n } = useTranslation()
  const offer = service.models.find((item) => item.model === model)
  return <article className="grid gap-4 rounded-xl border border-[var(--hairline)] bg-surface-1 p-4 lg:grid-cols-[minmax(12rem,1.3fr)_minmax(12rem,1fr)_minmax(13rem,1fr)_auto]">
    <div className="min-w-0"><div className="flex items-center gap-2">{onSelect ? <Checkbox size="sm" aria-label={t('merchantPool.compareService', { name: service.name })} isSelected={Boolean(selected)} isDisabled={disabled && !selected} onValueChange={onSelect} /> : null}<h3 className="truncate text-sm font-semibold">{service.merchant_name}</h3><PoolStatus status={service.status} /></div><p className="mt-1 text-xs">{service.name}</p><p className="mt-2 line-clamp-2 text-xs leading-5 text-muted-foreground">{service.description}</p><div className="mt-2 flex flex-wrap gap-1">{service.models.slice(0, 5).map((item) => <span key={item.model} className="max-w-full truncate rounded bg-surface-2 px-1.5 py-0.5 text-[0.6875rem]">{item.model}</span>)}{service.models.length > 5 ? <span className="text-xs text-muted-foreground">+{service.models.length - 5}</span> : null}</div></div>
    <div>{offer ? <MerchantModelPrice offer={offer} /> : <div className="text-xs leading-5 text-muted-foreground">{t('merchantPool.selectModelToCompare')}<p className="mt-1">{t('merchantPool.modelCount', { count: service.models.length })}</p></div>}</div>
    <MerchantHealth stats={stats} />
    <div className="flex flex-wrap gap-3 text-xs lg:flex-col lg:items-end"><Link className="text-primary hover:underline" to={`/console/merchant-pool/merchants/${service.merchant_id}?pool=${service.pool_id}&service=${service.id}`}>{t('merchantPool.details')}</Link><Link className="text-primary hover:underline" to={`/console/merchant-pool/routes?pool=${service.pool_id}&merchant=${service.merchant_id}`}>{t('merchantPool.addToRoute')}</Link><span className="text-[0.6875rem] text-muted-foreground">{t('merchantPool.updated', { value: poolTimestamp(service.updated_at, i18n.language) })}</span></div>
  </article>
}
