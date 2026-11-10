import { Checkbox } from '@heroui/react'
import { ArrowUpRight, AudioLines, Bot, Boxes, Braces, Film, Image, Mic, Plus, Store } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useModelProviderOptions } from '@/components/brand/provider-catalog-api'
import type { MerchantCatalogService, MerchantStatistics, MerchantTimeline } from './merchant-pool-model'
import { poolRatio, poolTimestamp, merchantProtocolLabel, merchantTextProtocol } from './merchant-pool-model'
import { MerchantTimelineStrip, MerchantPriceBreakdown } from './merchant-catalog-card'
import { MerchantModelProviderBadge, merchantProviderIdentity } from './merchant-model-provider'
import { PoolStatus } from './merchant-pool-ui'

const cell = 'border-b border-[var(--hairline)] px-3 py-3 align-middle'
const action = 'inline-flex h-8 items-center justify-center gap-1 rounded-lg px-2.5 text-[11px] font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary'
const protocolIcons = { openai_chat: Bot, openai_responses: Braces, openai_images: Image, openai_audio: Mic, openai_speech: AudioLines, xai_video: Film }

export function MerchantCatalogTable({ services, model, statistics, timelines, compared, onCompare }: { services: MerchantCatalogService[]; model: string; statistics: Map<number, MerchantStatistics>; timelines: Map<number, MerchantTimeline>; compared: number[]; onCompare: (id: number) => void }) {
  const { t, i18n } = useTranslation()
  const { options } = useModelProviderOptions()
  const milliseconds = (value: number | null | undefined) => value == null ? '—' : `${Math.round(value).toLocaleString(i18n.language)} ms`
  return <div className="min-w-0 overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1">
    <div className="overflow-x-auto overscroll-x-contain focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary" tabIndex={0} role="region" aria-label={t('merchantPool.catalogTable')}>
      <table className="w-full min-w-[1096px] table-fixed text-left text-xs tabular-nums sm:min-w-[1160px]">
        <colgroup><col className="w-[186px] sm:w-[250px]" /><col className="w-[190px]" /><col className="w-[164px]" /><col className="w-[106px]" /><col className="w-[150px]" /><col className="w-[134px]" /><col className="w-[166px]" /></colgroup>
        <thead><tr className="border-b border-[var(--hairline)] bg-surface-2/45 text-[11px] text-muted-foreground">
          <th scope="col" className="sticky left-0 z-20 bg-surface-1 px-4 py-3 font-medium"><span className="block text-foreground">{t('merchantPool.market.source')}</span><span className="mt-0.5 block font-normal">{t('merchantPool.market.sourceHint')}</span></th>
          <th scope="col" className="px-3 py-3 font-medium"><span className="block text-foreground">{t('merchantPool.market.quote')}</span><span className="mt-0.5 block font-normal">{t('merchantPool.market.quoteHint')}</span></th>
          <th scope="col" className="px-3 py-3 font-medium">{t('merchantPool.market.success')}</th>
          <th scope="col" className="px-3 py-3 font-medium">{t('merchantPool.tableColumns.status')}</th>
          <th scope="col" className="px-3 py-3 font-medium">{t('merchantPool.market.latency')}</th>
          <th scope="col" className="px-3 py-3 font-medium">{t('merchantPool.market.updated')}</th>
          <th scope="col" className="px-3 py-3 font-medium lg:sticky lg:right-0 lg:z-20 lg:border-l lg:border-[var(--hairline)] lg:bg-surface-1">{t('merchantPool.tableColumns.actions')}</th>
        </tr></thead>
        <tbody>{services.map((service) => {
          const offer = service.models.find((item) => item.model === model)
          const stats = statistics.get(service.id)
          const successRate = stats && stats.sample_count > 0 ? stats.success_rate : null
          const timeline = timelines.get(service.id)
          const ProtocolIcon = protocolIcons[service.protocol] ?? Bot
          const details = `/console/merchant-pool/merchants/${service.merchant_id}?pool=${service.pool_id}&service=${service.id}`
          const identityOffer = offer ?? (service.models.length === 1 ? service.models[0] : undefined)
          const identity = merchantProviderIdentity(identityOffer?.provider, options, t('merchantPool.market.unclassified'))
          const IdentityIcon = identityOffer ? identity.Icon : Boxes
          const providers = [...new Set((identityOffer ? [identityOffer] : service.models).map((item) => item.provider ?? null))]
          const rowTitle = model || identityOffer?.model || service.name
          const ttftApplicable = merchantTextProtocol(service.protocol) && Boolean(offer?.supports_streaming)
          const description = service.description.trim()
          return <tr key={service.id} className="group transition-colors hover:bg-surface-2/35 last:[&>td]:border-b-0">
            <td className={`${cell} sticky left-0 z-10 bg-surface-1 px-4 group-hover:bg-surface-2`}>
              <div className="flex min-w-0 items-start gap-2.5">
                <Checkbox className="mt-1 shrink-0" size="sm" aria-label={t('merchantPool.compareService', { name: service.name })} isSelected={compared.includes(service.id)} isDisabled={compared.length >= 4 && !compared.includes(service.id)} onValueChange={() => onCompare(service.id)} />
                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 items-start gap-1.5"><span className="mt-px inline-flex size-5 shrink-0 items-center justify-center rounded-md border border-[var(--hairline)] bg-surface-2 text-foreground" title={identityOffer ? identity.name : t('merchantPool.modelCount', { count: service.models.length })}><IdentityIcon className="size-3.5" aria-hidden="true" /></span><Link className="min-w-0 truncate font-semibold leading-5 text-foreground hover:text-primary" title={model || service.models.map((item) => item.model).join(', ')} to={details}>{rowTitle}</Link></div>
                  <div className="mt-1 flex min-w-0 items-center gap-1 text-[10px] text-muted-foreground"><Store className="size-3 shrink-0" aria-hidden="true" /><Link className="truncate hover:text-primary" title={`${service.merchant_name} · ${service.name}`} to={details}>{service.merchant_name}<span className="mx-1 opacity-50">/</span>{service.name}</Link><span title={merchantProtocolLabel(service.protocol)} aria-label={merchantProtocolLabel(service.protocol)}><ProtocolIcon className="size-3 shrink-0" aria-hidden="true" /></span></div>
                  {description ? <p className="mt-1.5 line-clamp-2 break-words text-[11px] leading-4 text-muted-foreground" title={description}>{description}</p> : null}
                  <div className="mt-1.5 flex min-w-0 flex-wrap gap-1">{!model ? <span className="inline-block rounded bg-surface-2 px-1.5 py-0.5 text-[10px] text-muted-foreground">{t('merchantPool.modelCount', { count: service.models.length })}</span> : null}{providers.map((provider) => <MerchantModelProviderBadge key={JSON.stringify(provider)} provider={provider} options={options} />)}</div>
                </div>
              </div>
            </td>
            <td className={cell}>
              {offer ? <><span className="mb-1.5 inline-flex items-center rounded-md border border-primary/20 bg-primary/7 px-1.5 py-0.5 font-mono text-[11px] font-semibold text-primary" title={t('merchantPool.ratioValue', { value: poolRatio(offer.sales_ratio_micros) })}>×{poolRatio(offer.sales_ratio_micros)}</span>{offer.prices ? <MerchantPriceBreakdown prices={offer.prices} compact /> : <span className="block text-muted-foreground">{t('merchantPool.noPrice')}</span>}</> : <span className="block max-w-36 text-[11px] leading-5 text-muted-foreground">{t('merchantPool.market.chooseModel')}</span>}
            </td>
            <td className={cell} title={t(model ? 'merchantPool.market.modelMetrics' : 'merchantPool.market.serviceMetrics')}>
              <div className="flex items-baseline gap-1.5"><span className={`text-sm font-semibold tracking-tight ${successRate === null ? 'text-muted-foreground' : successRate < 95 ? 'text-warning' : 'text-success'}`}>{successRate === null ? t('merchantPool.noData') : `${successRate.toFixed(1)}%`}</span></div>
              <span className="mt-0.5 block text-[10px] text-muted-foreground">{stats?.sample_count ? t('merchantPool.samples', { count: stats.sample_count }) : '—'}</span>
              <div className="mt-2 w-full max-w-32">{timeline ? <MerchantTimelineStrip timeline={timeline} compact /> : <div className="h-2 rounded-sm bg-surface-2" title={t('merchantPool.noData')} />}</div>
              {stats?.sample_count && stats.sample_count < 100 ? <span className="mt-1 block text-[10px] text-warning">{t('merchantPool.lowSamples')}</span> : null}
            </td>
            <td className={cell}><PoolStatus status={service.status} />{service.maintenance?.length ? <div className="mt-1.5 line-clamp-2 break-words text-[10px] leading-4 text-warning" title={service.maintenance.map((notice) => notice.message).join(' · ')}>{service.maintenance.map((notice) => notice.message).join(' · ')}</div> : null}{stats?.status && !['normal', 'unknown'].includes(stats.status) ? <div className="mt-1.5"><PoolStatus status={stats.status} /></div> : null}</td>
            <td className={cell}><div className="flex flex-col gap-1.5 text-[10px] text-muted-foreground">{ttftApplicable ? <><span className="flex items-baseline justify-between gap-2"><span>{t('merchantPool.tableColumns.ttftP50')}</span><span className="font-mono text-xs font-medium text-foreground">{milliseconds(stats?.ttft_p50_ms)}</span></span><span className="flex items-baseline justify-between gap-2"><span>{t('merchantPool.tableColumns.ttftP95')}</span><span className="font-mono">{milliseconds(stats?.ttft_p95_ms)}</span></span></> : <span className="leading-4">{t('merchantPool.ttftNotApplicable')}</span>}<span className="flex items-baseline justify-between gap-2 border-t border-[var(--hairline)] pt-1.5"><span>{t('merchantPool.tableColumns.durationP95')}</span><span className="font-mono">{milliseconds(stats?.latency_p95_ms)}</span></span></div></td>
            <td className={`${cell} text-[10px] text-muted-foreground`} title={poolTimestamp(service.updated_at, i18n.language)}><span className="block text-[11px] text-foreground">{service.updated_at ? new Intl.DateTimeFormat(i18n.language, { timeStyle: 'short' }).format(service.updated_at * 1000) : '—'}</span><span className="mt-1 block">{service.updated_at ? new Intl.DateTimeFormat(i18n.language, { dateStyle: 'short' }).format(service.updated_at * 1000) : '—'}</span></td>
            <td className={`${cell} lg:sticky lg:right-0 lg:z-10 lg:border-l lg:border-[var(--hairline)] lg:bg-surface-1 lg:group-hover:bg-surface-2`}><div className="flex flex-col items-stretch gap-1.5"><Link className={`${action} border border-primary/15 bg-primary/10 text-primary hover:bg-primary/20`} to={`/console/merchant-pool/routes?pool=${service.pool_id}&merchant=${service.merchant_id}`}><Plus className="size-3 shrink-0" aria-hidden="true" />{t('merchantPool.addToRoute')}</Link><Link className={`${action} border border-[var(--hairline)] text-muted-foreground hover:bg-surface-2 hover:text-foreground`} to={details}>{t('merchantPool.details')}<ArrowUpRight className="size-3 shrink-0" aria-hidden="true" /></Link></div></td>
          </tr>
        })}</tbody>
      </table>
    </div>
  </div>
}
