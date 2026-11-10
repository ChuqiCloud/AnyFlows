import { Skeleton } from '@heroui/react'
import { Boxes, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { resolveProviderLogo } from '@/components/brand/model-logos'
import { findProviderOption, type ProviderOption } from '@/components/brand/provider-catalog'
import { useModelProviderOptions } from '@/components/brand/provider-catalog-api'
import { cn } from '@/lib/utils'
import type { MerchantModelProvider } from './merchant-pool-model'

export type MerchantProviderSelection = Array<string | null>

export function serializeMerchantProviders(providers: MerchantProviderSelection) {
  const sorted = [...new Set(providers)].sort((a, b) => a === b ? 0 : a === null ? 1 : b === null ? -1 : a < b ? -1 : 1)
  return sorted.length ? JSON.stringify(sorted) : ''
}

export function parseMerchantProviders(value: string | null): MerchantProviderSelection {
  if (!value || new TextEncoder().encode(value).length > 16_384) return []
  try {
    const parsed: unknown = JSON.parse(value)
    if (!Array.isArray(parsed) || !parsed.every(validProviderSelection)) return []
    const providers = [...new Set(parsed)]
    return providers.length <= 32 ? providers : []
  } catch { return [] }
}

function validProviderSelection(value: unknown): value is string | null {
  return value === null || (typeof value === 'string'
    && value.length > 0
    && !/^\p{White_Space}|\p{White_Space}$/u.test(value)
    && !/[\p{Cc}\p{Cs}]/u.test(value)
    && new TextEncoder().encode(value).length <= 64)
}

export function merchantProviderIdentity(provider: string | null | undefined, options: readonly ProviderOption[], unclassified: string) {
  const option = provider == null ? undefined : findProviderOption(provider, options)
  const logo = provider == null ? undefined : resolveProviderLogo(option?.logo, option?.id, option?.name, provider)
  return { name: provider == null ? unclassified : option?.name ?? provider, Icon: logo?.Icon ?? Boxes }
}

export function MerchantModelProviderBadge({ provider, options }: { provider: string | null; options: readonly ProviderOption[] }) {
  const { t } = useTranslation()
  const { name, Icon } = merchantProviderIdentity(provider, options, t('merchantPool.market.unclassified'))
  return <span title={name} className="inline-flex min-w-0 max-w-full items-center gap-1 rounded border border-[var(--hairline)] bg-surface-2/50 px-1.5 py-0.5 text-[10px] text-muted-foreground"><Icon className="size-3 shrink-0 text-foreground" aria-hidden="true" /><span className="truncate">{name}</span></span>
}

export function MerchantModelProviderFilter({ providers, selected, loading, error, onChange, onRetry }: {
  providers: MerchantModelProvider[]
  selected: MerchantProviderSelection
  loading: boolean
  error: boolean
  onChange: (providers: MerchantProviderSelection) => void
  onRetry: () => void
}) {
  const { t } = useTranslation()
  const { options } = useModelProviderOptions()
  const total = providers.reduce((sum, provider) => sum + provider.model_count, 0)
  const button = 'inline-flex h-8 max-w-full shrink-0 items-center gap-1.5 rounded-lg border px-2.5 text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary disabled:opacity-40'
  return <div className="min-w-0 border-t border-[var(--hairline)] pt-3">
    <div className="mb-2 flex flex-wrap items-baseline gap-x-3 gap-y-1"><span className="text-xs font-medium">{t('merchantPool.market.providers')}</span><span className="text-[10px] leading-4 text-muted-foreground">{t('merchantPool.market.providersHint')}</span></div>
    <div role="group" aria-label={t('merchantPool.market.providers')} className="flex min-w-0 flex-wrap gap-1.5">
      <button type="button" aria-label={t('merchantPool.market.allProviders')} aria-pressed={selected.length === 0} onClick={() => onChange([])} className={cn(button, selected.length === 0 ? 'border-primary/35 bg-primary/10 text-primary' : 'border-[var(--hairline)] bg-background text-muted-foreground hover:bg-surface-2 hover:text-foreground')}><Boxes className="size-3.5 shrink-0" aria-hidden="true" />{t('merchantPool.market.allProviders')}{!loading && !error ? <span className="text-[10px] tabular-nums opacity-70" aria-hidden="true">{total}</span> : null}</button>
      {loading ? <div aria-label={t('merchantPool.market.providersLoading')} className="flex gap-1.5">{[0, 1, 2].map((item) => <Skeleton key={item} className="h-8 w-24 rounded-lg" />)}</div> : providers.map((provider) => {
        const { name, Icon } = merchantProviderIdentity(provider.name, options, t('merchantPool.market.unclassified'))
        const checked = selected.includes(provider.name)
        return <button key={JSON.stringify(provider.name)} type="button" aria-label={name} aria-pressed={checked} disabled={!checked && selected.length >= 32} title={provider.name ?? name} onClick={() => onChange(checked ? selected.filter((value) => value !== provider.name) : [...selected, provider.name])} className={cn(button, checked ? 'border-primary/35 bg-primary/10 text-primary' : 'border-[var(--hairline)] bg-background text-muted-foreground hover:bg-surface-2 hover:text-foreground')}><Icon className="size-3.5 shrink-0 text-foreground" aria-hidden="true" /><span className="truncate">{name}</span><span className="text-[10px] tabular-nums opacity-70" aria-hidden="true">{provider.model_count}</span></button>
      })}
    </div>
    {error ? <div role="alert" className="mt-2 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground"><span>{t('merchantPool.market.providersError')}</span><button type="button" onClick={onRetry} className="inline-flex items-center gap-1 text-primary hover:underline"><RefreshCw className="size-3" aria-hidden="true" />{t('merchantPool.market.retryProviders')}</button></div> : null}
  </div>
}
