import { useTranslation } from 'react-i18next'
import type { MerchantModelOffer, MerchantPrices, MerchantStatistics, MerchantTimeline } from './merchant-pool-model'
import { poolRatio, poolTimestamp } from './merchant-pool-model'
import { PoolStatus } from './merchant-pool-ui'

export function MerchantModelPrice({ offer }: { offer: MerchantModelOffer }) {
  const { t } = useTranslation()
  return <div className="grid gap-1 text-xs tabular-nums"><span className="font-medium">{t('merchantPool.ratioValue', { value: poolRatio(offer.sales_ratio_micros) })}</span>{offer.prices ? <MerchantPriceBreakdown prices={offer.prices} /> : <span className="text-warning">{t('merchantPool.noPrice')}</span>}</div>
}

export function MerchantHealth({ stats, ttftApplicable = true }: { stats?: MerchantStatistics; ttftApplicable?: boolean }) {
  const { t } = useTranslation()
  const millis = (value: number | null | undefined) => value == null ? '—' : `${Math.round(value).toLocaleString()} ms`
  return <div className="grid gap-1 text-xs tabular-nums"><div className="flex flex-wrap items-center gap-2"><span className="font-medium">{stats?.sample_count && stats.success_rate !== null ? `${stats.success_rate.toFixed(1)}%` : t('merchantPool.noData')}</span>{stats?.sample_count ? <span className="text-muted-foreground">{t('merchantPool.samples', { count: stats.sample_count })}</span> : null}</div>{stats?.sample_count && stats.sample_count < 100 ? <span className="text-warning">{t('merchantPool.lowSamples')}</span> : null}<span className="text-muted-foreground">{ttftApplicable ? t('merchantPool.ttft', { p50: millis(stats?.ttft_p50_ms), p95: millis(stats?.ttft_p95_ms) }) : t('merchantPool.ttftNotApplicable')}</span><span className="text-muted-foreground">{t('merchantPool.latency', { value: millis(stats?.latency_p95_ms) })}</span>{stats ? <PoolStatus status={stats.status} /> : null}</div>
}

export function MerchantTimelineStrip({ timeline, compact = false }: { timeline?: MerchantTimeline; compact?: boolean }) {
  const { t, i18n } = useTranslation()
  if (!timeline) return null
  return <div className={compact ? '' : 'mt-2'}><span className="sr-only">{t('merchantPool.recentAvailability')}</span><div className="flex gap-0.5" role="img" aria-label={t('merchantPool.availabilitySummary', { known: timeline.buckets.filter((bucket) => bucket.sample_count > 0).length, total: timeline.buckets.length })}>{timeline.buckets.map((bucket) => <div key={bucket.started_at} className={`${compact ? 'h-3' : 'h-5'} min-w-0 flex-1 rounded-sm ${bucket.status === 'normal' ? 'bg-success/70' : bucket.status === 'degraded' ? 'bg-warning/80' : 'bg-surface-2'}`} title={`${poolTimestamp(bucket.started_at, i18n.language)} — ${poolTimestamp(bucket.ended_at, i18n.language)} · ${t(`merchantPool.status.${bucket.status}`)} · ${t('merchantPool.samples', { count: bucket.sample_count })}`} />)}</div></div>
}

export function MerchantPriceBreakdown({ prices }: { prices: MerchantPrices }) {
  const { t } = useTranslation()
  if (prices.billing_mode === 'free') return <span>{t('merchantPool.freePrice')}</span>
  const videoPrices = prices.video_per_second
  if (prices.billing_mode === 'video' && videoPrices) return <div className="flex flex-col gap-0.5">{(['p480', 'p720', 'p1080'] as const).map((resolution) => <span key={resolution}>{resolution.slice(1)}p: {videoPrices[resolution] == null ? t('merchantPool.unsupported') : t('merchantPool.pricePerSecond', { value: videoPrices[resolution] })}</span>)}</div>
  return <div className="flex flex-col gap-0.5"><span>{t('merchantPool.priceInput', { value: prices.input })}</span><span>{t('merchantPool.priceOutput', { value: prices.output })}</span><details className="text-[11px] text-muted-foreground"><summary className="w-fit cursor-pointer">{t('merchantPool.cachePrices')}</summary><span className="block">{t('merchantPool.cacheRead', { value: prices.cache_read })}</span><span className="block">{t('merchantPool.cacheWrite5m', { value: prices.cache_write_5m })}</span><span className="block">{t('merchantPool.cacheWrite1h', { value: prices.cache_write_1h })}</span></details></div>
}
