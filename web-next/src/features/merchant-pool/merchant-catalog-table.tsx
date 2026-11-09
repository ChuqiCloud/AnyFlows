import { Checkbox } from '@heroui/react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import type { MerchantCatalogService, MerchantStatistics, MerchantTimeline } from './merchant-pool-model'
import { poolRatio, poolTimestamp } from './merchant-pool-model'
import { MerchantTimelineStrip } from './merchant-catalog-card'
import { PoolStatus } from './merchant-pool-ui'

const cell = 'border-b border-[var(--hairline)] px-2 py-2 align-middle whitespace-nowrap'

export function MerchantCatalogTable({ services, model, statistics, timelines, compared, onCompare }: { services: MerchantCatalogService[]; model: string; statistics: Map<number, MerchantStatistics>; timelines: Map<number, MerchantTimeline>; compared: number[]; onCompare: (id: number) => void }) {
  const { t, i18n } = useTranslation()
  const milliseconds = (value: number | null | undefined) => value == null ? '—' : `${Math.round(value).toLocaleString()} ms`
  return <div className="min-w-0 overflow-hidden rounded-lg border border-[var(--hairline)]">
    <div className="overflow-x-auto" tabIndex={0} role="region" aria-label={t('merchantPool.catalogTable')}>
      <table className="w-full min-w-[1330px] text-left text-xs tabular-nums">
        <thead><tr className="border-b border-[var(--hairline)] bg-surface-2/60 text-muted-foreground">
          {['merchantService', 'status', 'model', 'ratio', 'price', 'success', 'timing', 'trend', 'updated', 'actions'].map((name, index) => <th key={name} scope="col" className={`h-10 px-2 font-medium whitespace-nowrap ${index === 0 ? 'sticky left-0 z-20 w-48 bg-surface-1 pl-3.5' : index === 9 ? 'lg:sticky lg:right-0 lg:z-20 lg:border-l lg:border-[var(--hairline)] lg:bg-surface-1' : ''}`}>{t(`merchantPool.tableColumns.${name}`)}</th>)}
        </tr></thead>
        <tbody>{services.map((service) => {
          const offer = service.models.find((item) => item.model === model)
          const stats = statistics.get(service.id)
          return <tr key={service.id} className="group hover:bg-surface-2/50">
            <td className={`${cell} sticky left-0 z-10 w-48 max-w-48 bg-surface-1 pl-3.5 group-hover:bg-surface-2`}><div className="flex items-center gap-2"><Checkbox size="sm" aria-label={t('merchantPool.compareService', { name: service.name })} isSelected={compared.includes(service.id)} isDisabled={compared.length >= 4 && !compared.includes(service.id)} onValueChange={() => onCompare(service.id)} /><div className="min-w-0"><Link className="block max-w-36 truncate font-medium text-foreground hover:underline" title={service.merchant_name} to={`/console/merchant-pool/merchants/${service.merchant_id}?pool=${service.pool_id}&service=${service.id}`}>{service.merchant_name}</Link><span className="mt-0.5 block max-w-36 truncate text-[11px] text-muted-foreground" title={service.description}>{service.name}</span></div></div></td>
            <td className={cell}><PoolStatus status={service.status} />{stats?.status && !['normal', 'unknown'].includes(stats.status) ? <div className="mt-0.5"><PoolStatus status={stats.status} /></div> : null}</td>
            <td className={cell}><div className="max-w-48"><span className="block truncate font-mono text-xs" title={model || service.models.map((item) => item.model).join(', ')}>{model || service.models[0]?.model || '—'}</span><span className="mt-0.5 block text-[11px] text-muted-foreground">{model ? service.protocol === 'openai_responses' ? 'Responses' : 'Chat Completions' : t('merchantPool.modelCount', { count: service.models.length })}</span></div></td>
            <td className={`${cell} font-mono`}>{offer ? `${poolRatio(offer.sales_ratio_micros)}×` : '—'}</td>
            <td className={cell}>{offer?.prices ? <div className="flex flex-col gap-0.5"><span>{t('merchantPool.priceInput', { value: offer.prices.input })}</span><span>{t('merchantPool.priceOutput', { value: offer.prices.output })}</span><details className="text-[11px] text-muted-foreground"><summary className="w-fit cursor-pointer">{t('merchantPool.cachePrices')}</summary><span className="block">{t('merchantPool.cacheRead', { value: offer.prices.cache_read })}</span><span className="block">{t('merchantPool.cacheWrite5m', { value: offer.prices.cache_write_5m })}</span><span className="block">{t('merchantPool.cacheWrite1h', { value: offer.prices.cache_write_1h })}</span></details></div> : <span className="text-muted-foreground">{t(model ? 'merchantPool.noPrice' : 'merchantPool.selectModelShort')}</span>}</td>
            <td className={cell}><span className={stats?.success_rate != null && stats.success_rate < 95 ? 'font-medium text-warning' : 'font-medium'}>{stats?.sample_count && stats.success_rate !== null ? `${stats.success_rate.toFixed(1)}%` : t('merchantPool.noData')}</span><span className="mt-0.5 block text-[11px] text-muted-foreground">{stats?.sample_count ? t('merchantPool.samples', { count: stats.sample_count }) : '—'}</span>{stats?.sample_count && stats.sample_count < 100 ? <span className="block text-[11px] text-warning">{t('merchantPool.lowSamples')}</span> : null}</td>
            <td className={cell}><span className="block">{t('merchantPool.ttftCompact', { p50: milliseconds(stats?.ttft_p50_ms), p95: milliseconds(stats?.ttft_p95_ms) })}</span><span className="mt-0.5 block text-[11px] text-muted-foreground">{t('merchantPool.latency', { value: milliseconds(stats?.latency_p95_ms) })}</span></td>
            <td className={cell}><div className="w-28"><MerchantTimelineStrip timeline={timelines.get(service.id)} compact />{!timelines.get(service.id) ? <span className="text-muted-foreground">—</span> : null}</div></td>
            <td className={`${cell} text-[11px] text-muted-foreground`}>{poolTimestamp(service.updated_at, i18n.language)}</td>
            <td className={`${cell} lg:sticky lg:right-0 lg:z-10 lg:border-l lg:border-[var(--hairline)] lg:bg-surface-1 lg:group-hover:bg-surface-2`}><div className="flex flex-col gap-1"><Link className="text-primary hover:underline" to={`/console/merchant-pool/merchants/${service.merchant_id}?pool=${service.pool_id}&service=${service.id}`}>{t('merchantPool.details')}</Link><Link className="text-primary hover:underline" to={`/console/merchant-pool/routes?pool=${service.pool_id}&merchant=${service.merchant_id}`}>{t('merchantPool.addToRoute')}</Link></div></td>
          </tr>
        })}</tbody>
      </table>
    </div>
  </div>
}
