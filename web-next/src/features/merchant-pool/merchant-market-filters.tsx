import { Input } from '@heroui/react'
import { AudioLines, Braces, Clapperboard, Image, Layers3, MessageSquare, Mic, RotateCcw, Search, SlidersHorizontal, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { MerchantPool } from './merchant-pool-model'
import { merchantProtocols, merchantProtocolLabel } from './merchant-pool-model'
import { PoolField, PoolSelect } from './merchant-pool-ui'
import { cn } from '@/lib/utils'

export type MerchantMarketFilters = {
  model: string
  search: string
  status: string
  sort: string
  protocol: string
  streaming: string
  capability: string
  minRatio: string
  maxRatio: string
}

const protocolIcons = { openai_chat: MessageSquare, openai_responses: Braces, openai_images: Image, openai_audio: Mic, openai_speech: AudioLines, xai_video: Clapperboard }
const control = { inputWrapper: 'border border-[var(--hairline)] bg-background shadow-none data-[hover=true]:bg-surface-2/60 group-data-[focus=true]:border-primary/60', input: 'text-xs' }

export function MerchantMarketFiltersPanel({ pools, poolId, loading, filters, invalidPriceRange, onPoolChange, onChange, onReset }: {
  pools: MerchantPool[]
  poolId?: number
  loading: boolean
  filters: MerchantMarketFilters
  invalidPriceRange: boolean
  onPoolChange: (value: string) => void
  onChange: (patch: Partial<MerchantMarketFilters>) => void
  onReset: () => void
}) {
  const { t } = useTranslation()
  const active = [
    filters.model && { key: 'model', text: filters.model },
    filters.search && { key: 'search', text: `${t('merchantPool.search')}: ${filters.search}` },
    filters.protocol && { key: 'protocol', text: merchantProtocolLabel(filters.protocol) },
    filters.status && { key: 'status', text: t(`merchantPool.status.${filters.status}`) },
    filters.streaming && { key: 'streaming', text: t(filters.streaming === 'true' ? 'merchantPool.streaming' : 'merchantPool.nonStreaming') },
    filters.capability && { key: 'capability', text: t(`merchantPool.${filters.capability}`) },
    filters.minRatio && { key: 'minRatio', text: `${t('merchantPool.minimumRatio')} ${filters.minRatio}×` },
    filters.maxRatio && { key: 'maxRatio', text: `${t('merchantPool.maximumRatio')} ${filters.maxRatio}×` },
  ].filter((item): item is { key: string; text: string } => Boolean(item))
  const advancedCount = [filters.streaming, filters.capability, filters.minRatio, filters.maxRatio].filter(Boolean).length
  return <section aria-label={t('merchantPool.market.filters')} className="min-w-0 rounded-xl border border-[var(--hairline)] bg-surface-1 shadow-[0_1px_2px_rgb(0_0_0/0.025)]">
    <div className="flex min-w-0 flex-col gap-3 p-3 sm:p-4">
      <div className="flex min-w-0 flex-col gap-3 lg:flex-row lg:items-center">
        <div className="flex shrink-0 items-center gap-2 text-xs"><span className="text-muted-foreground">{t('merchantPool.pool')}</span><div className="w-40 max-w-full"><PoolSelect label={t('merchantPool.pool')} value={poolId ? String(poolId) : ''} allowEmpty={false} disabled={loading} onChange={onPoolChange} options={pools.map((item) => ({ value: String(item.id), label: item.name }))} /></div></div>
        <div role="group" aria-label={t('merchantPool.market.protocols')} className="flex min-w-0 flex-1 gap-1.5 overflow-x-auto pb-1 lg:pb-0">
          <button type="button" aria-pressed={!filters.protocol} onClick={() => onChange({ protocol: '' })} className={cn('flex h-8 shrink-0 items-center gap-1.5 rounded-lg border px-2.5 text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary', !filters.protocol ? 'border-primary bg-primary text-primary-foreground shadow-sm' : 'border-[var(--hairline)] bg-background text-muted-foreground hover:bg-surface-2 hover:text-foreground')}><Layers3 className="size-3.5" aria-hidden="true" />{t('merchantPool.market.allSupply')}</button>
          {merchantProtocols.map(({ value }) => { const Icon = protocolIcons[value]; return <button key={value} type="button" aria-pressed={filters.protocol === value} onClick={() => onChange({ protocol: value })} className={cn('flex h-8 shrink-0 items-center gap-1.5 rounded-lg border px-2.5 text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary', filters.protocol === value ? 'border-primary/35 bg-primary/10 text-primary' : 'border-[var(--hairline)] bg-background text-muted-foreground hover:bg-surface-2 hover:text-foreground')}><Icon className="size-3.5" aria-hidden="true" />{t(`merchantPool.market.protocolLabels.${value}`)}</button> })}
        </div>
      </div>
      <div className="grid min-w-0 grid-cols-2 gap-2.5 border-t border-[var(--hairline)] pt-3 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1.7fr)_145px_145px]">
        <PoolField label={t('merchantPool.model')}><Input size="sm" aria-label={t('merchantPool.model')} placeholder={t('merchantPool.modelPlaceholder')} classNames={control} value={filters.model} onValueChange={(model) => onChange({ model })} /></PoolField>
        <PoolField label={t('merchantPool.search')}><Input size="sm" aria-label={t('merchantPool.search')} placeholder={t('merchantPool.market.searchPlaceholder')} classNames={control} startContent={<Search className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />} value={filters.search} onValueChange={(search) => onChange({ search })} /></PoolField>
        <PoolField label={t('merchantPool.statusLabel')}><PoolSelect label={t('merchantPool.statusLabel')} value={filters.status} onChange={(status) => onChange({ status })} options={['published', 'paused', 'maintenance', 'suspended'].map((value) => ({ value, label: t(`merchantPool.status.${value}`) }))} /></PoolField>
        <PoolField label={t('merchantPool.sort')}><PoolSelect label={t('merchantPool.sort')} allowEmpty={false} value={filters.model ? filters.sort : 'name'} onChange={(sort) => onChange({ sort })} options={[{ value: 'name', label: t('merchantPool.sortName') }, ...(filters.model ? [{ value: 'price', label: t('merchantPool.sortPrice') }, { value: 'success_rate', label: t('merchantPool.sortSuccess') }, { value: 'latency', label: t('merchantPool.sortLatency') }] : [])]} /></PoolField>
      </div>
      <details className="group border-t border-[var(--hairline)] pt-2.5">
        <summary className="flex w-fit cursor-pointer list-none items-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden"><SlidersHorizontal className="size-3.5" aria-hidden="true" />{t('merchantPool.moreFilters')}{advancedCount ? <span className="rounded bg-primary/10 px-1.5 text-[10px] font-medium text-primary">{advancedCount}</span> : null}</summary>
        <div className="mt-3 grid grid-cols-2 gap-3 lg:grid-cols-5">
          <PoolField label={t('merchantPool.protocol')}><PoolSelect label={t('merchantPool.protocol')} value={filters.protocol} onChange={(protocol) => onChange({ protocol })} options={[...merchantProtocols]} /></PoolField>
          <PoolField label={t('merchantPool.responseMode')}><PoolSelect label={t('merchantPool.responseMode')} allowEmpty={false} value={filters.streaming} onChange={(streaming) => onChange({ streaming })} options={[{ value: '', label: t('merchantPool.all') }, { value: 'true', label: t('merchantPool.streaming') }, { value: 'false', label: t('merchantPool.nonStreaming') }]} /></PoolField>
          <PoolField label={t('merchantPool.capability')}><PoolSelect label={t('merchantPool.capability')} value={filters.capability} onChange={(capability) => onChange({ capability })} options={['streaming', 'tools', 'vision'].map((value) => ({ value, label: t(`merchantPool.${value}`) }))} /></PoolField>
          <PoolField label={t('merchantPool.minimumRatio')}><Input aria-label={t('merchantPool.minimumRatio')} size="sm" inputMode="decimal" classNames={control} value={filters.minRatio} onValueChange={(minRatio) => onChange({ minRatio })} /></PoolField>
          <PoolField label={t('merchantPool.maximumRatio')}><Input aria-label={t('merchantPool.maximumRatio')} size="sm" inputMode="decimal" classNames={control} value={filters.maxRatio} onValueChange={(maxRatio) => onChange({ maxRatio })} /></PoolField>
        </div>
      </details>
      {invalidPriceRange ? <p role="alert" className="text-xs text-danger">{t('merchantPool.invalidForm')}</p> : null}
    </div>
    <div className="flex min-w-0 flex-wrap items-center gap-2 border-t border-[var(--hairline)] bg-surface-2/25 px-3 py-2 text-[11px] sm:px-4"><span className="shrink-0 text-muted-foreground">{t('merchantPool.market.selected')}</span><div className="flex min-w-0 flex-1 flex-wrap gap-1.5">{active.length ? active.map(({ key, text }) => <button key={key} type="button" aria-label={t('merchantPool.market.removeFilter', { value: text })} title={text} onClick={() => onChange({ [key]: '' })} className="flex min-w-0 max-w-56 items-center gap-1 rounded-md border border-primary/15 bg-primary/7 px-1.5 py-0.5 text-primary"><span className="truncate">{text}</span><X className="size-3 shrink-0" aria-hidden="true" /></button>) : <span className="text-muted-foreground">{t('merchantPool.market.noFilters')}</span>}</div><button type="button" disabled={!active.length && filters.sort === 'name'} onClick={onReset} className="flex shrink-0 items-center gap-1 text-muted-foreground transition-colors hover:text-primary disabled:opacity-40"><RotateCcw className="size-3" aria-hidden="true" />{t('merchantPool.market.resetFilters')}</button></div>
  </section>
}
