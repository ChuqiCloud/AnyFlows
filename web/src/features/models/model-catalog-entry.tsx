import { ChevronDown, Cpu } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { findModelLogo } from '@/components/brand/model-logos'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { ModelCatalogItem, ModelCatalogPricingScope } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { ModelCopyButton } from './model-copy-button'
import { ModelPriceValue } from './model-price-value'

type EntryProps = {
  item: ModelCatalogItem
  pricingScope: ModelCatalogPricingScope
  expanded: boolean
  onToggle: () => void
}

function ModelIdentity({ model }: { model: string }) {
  const { t } = useTranslation()
  const logo = findModelLogo(model)
  const Icon = logo?.Icon ?? Cpu
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <div className="grid size-8 shrink-0 place-items-center rounded-lg border border-[var(--hairline)] bg-surface-2 text-foreground">
        <Icon className="size-4" aria-hidden="true" />
      </div>
      <div className="min-w-0">
        <div className="truncate font-mono text-xs font-medium text-foreground" title={model}>
          {model}
        </div>
        <div className="mt-1 text-[0.625rem] text-muted-foreground">
          {logo?.name ?? t('models.values.compatible')}
        </div>
      </div>
    </div>
  )
}

function BillingBadge({ item }: { item: ModelCatalogItem }) {
  const { t } = useTranslation()
  return (
    <Badge className={cn(
      'border-transparent',
      item.billing_mode === 'free'
        ? 'bg-success/10 text-success'
        : 'bg-info/10 text-info',
    )}>
      {t(`models.billing.${item.billing_mode}`)}
    </Badge>
  )
}

function formatRatioMicros(value: string) {
  const micros = BigInt(value)
  const whole = micros / 1_000_000n
  const fraction = (micros % 1_000_000n).toString().padStart(6, '0').replace(/0+$/, '')
  return `${whole}${fraction ? `.${fraction}` : ''}×`
}

function ExpandedPriceDetails({
  item,
  pricingScope,
}: Pick<EntryProps, 'item' | 'pricingScope'>) {
  const { t } = useTranslation()
  const prices = item.prices ?? undefined
  const ratios = [
    ['group', item.ratios.group_micros],
    ['groupModel', item.ratios.group_model_micros],
    ['peak', item.ratios.peak_micros],
  ] as const
  return (
    <div className="grid gap-4 sm:grid-cols-[1fr_1fr_minmax(15rem,1.4fr)]">
      <ModelPriceValue
        label={t('models.columns.cacheCreation5m')}
        value={prices?.cache_creation_5m}
      />
      <ModelPriceValue
        label={t('models.columns.cacheCreation1h')}
        value={prices?.cache_creation_1h}
      />
      <div>
        <div className="text-[0.6875rem] text-muted-foreground">
          {t(pricingScope === 'group' ? 'models.values.appliedRatios' : 'models.values.baseRatios')}
        </div>
        <div className="mt-1.5 flex flex-wrap gap-1.5">
          {ratios.map(([key, value]) => (
            <Badge key={key} className="bg-surface-1 font-mono text-muted-foreground">
              {t(`models.ratios.${key}`)} {formatRatioMicros(value)}
            </Badge>
          ))}
        </div>
      </div>
    </div>
  )
}

export function ModelCatalogDesktopEntry({ item, pricingScope, expanded, onToggle }: EntryProps) {
  const { t } = useTranslation()
  const detailsId = `model-details-${encodeURIComponent(item.model)}`
  return (
    <>
      <tr className="hover:bg-surface-2/35">
        <td className="px-3 py-2.5"><ModelIdentity model={item.model} /></td>
        <td className="px-3 py-2.5">
          <ModelPriceValue label={t('models.columns.input')} value={item.prices?.input} />
        </td>
        <td className="px-3 py-2.5">
          <ModelPriceValue label={t('models.columns.output')} value={item.prices?.output} />
        </td>
        <td className="px-3 py-2.5">
          <ModelPriceValue label={t('models.columns.cacheRead')} value={item.prices?.cache_read} muted />
        </td>
        <td className="px-3 py-2.5">
          <div className="flex items-center justify-end gap-1">
            <BillingBadge item={item} />
            <ModelCopyButton model={item.model} />
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={t('models.actions.details')}
              aria-expanded={expanded}
              aria-controls={detailsId}
              onClick={onToggle}
            >
              <ChevronDown className={cn('size-3.5 transition-transform', expanded && 'rotate-180')} aria-hidden="true" />
            </Button>
          </div>
        </td>
      </tr>
      {expanded ? (
        <tr id={detailsId} className="bg-surface-2/20">
          <td colSpan={5} className="px-3 py-3">
            <ExpandedPriceDetails item={item} pricingScope={pricingScope} />
          </td>
        </tr>
      ) : null}
    </>
  )
}

export function ModelCatalogMobileEntry({ item, pricingScope, expanded, onToggle }: EntryProps) {
  const { t } = useTranslation()
  const detailsId = `model-details-mobile-${encodeURIComponent(item.model)}`
  return (
    <article className="rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
      <div className="flex items-start justify-between gap-2">
        <ModelIdentity model={item.model} />
        <div className="flex items-center gap-1"><BillingBadge item={item} /><ModelCopyButton model={item.model} /></div>
      </div>
      <div className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3">
        <ModelPriceValue label={t('models.columns.input')} value={item.prices?.input} />
        <ModelPriceValue label={t('models.columns.output')} value={item.prices?.output} />
        <ModelPriceValue label={t('models.columns.cacheRead')} value={item.prices?.cache_read} muted />
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="self-center justify-self-end"
          aria-expanded={expanded}
          aria-controls={detailsId}
          onClick={onToggle}
        >
          {t('models.actions.details')}
          <ChevronDown className={cn('size-3.5 transition-transform', expanded && 'rotate-180')} aria-hidden="true" />
        </Button>
      </div>
      {expanded ? (
        <div id={detailsId} className="mt-3 border-t border-[var(--hairline)] pt-3">
          <ExpandedPriceDetails item={item} pricingScope={pricingScope} />
        </div>
      ) : null}
    </article>
  )
}
