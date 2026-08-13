import { BrainCircuit, ChevronDown, Gauge, Wrench } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { ModelCatalogItem, ModelCatalogPricingScope } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { ModelCatalogExpandedDetails } from './model-catalog-expanded-details'
import { ModelCatalogIdentity } from './model-catalog-identity'
import { formatContextWindow, formatExactContextWindow } from './model-context-window'
import { ModelPriceValue } from './model-price-value'

type EntryProps = {
  item: ModelCatalogItem
  pricingScope: ModelCatalogPricingScope
  expanded: boolean
  onToggle: () => void
}

type SummaryProps = {
  className?: string
  item: ModelCatalogItem
}

const lifecycleStyles = {
  active: 'border-transparent bg-success/10 text-success',
  deprecated: 'border-transparent bg-warning/10 text-warning',
} as const

const runtimeStyles = {
  available: 'border-transparent bg-info/10 text-info',
  not_evaluated: 'bg-surface-2 text-muted-foreground',
} as const

function ModelStateBadges({ className, item }: SummaryProps) {
  const { t } = useTranslation()
  const availableProtocols = item.available_protocols ?? []
  return (
    <div className={cn('flex min-w-0 flex-wrap items-center gap-1', className)}>
      <Badge className={cn(
        'whitespace-nowrap border-transparent',
        item.billing_mode === 'free' ? 'bg-success/10 text-success' : 'bg-info/10 text-info',
      )}>
        {t(`models.billing.${item.billing_mode}`)}
      </Badge>
      <Badge className={cn('whitespace-nowrap', lifecycleStyles[item.lifecycle])}>{t(`models.lifecycle.${item.lifecycle}`)}</Badge>
      <Badge className={cn('whitespace-nowrap', runtimeStyles[item.runtime_status])}>{t(`models.runtime.${item.runtime_status}`)}</Badge>
      {availableProtocols.map((protocol) => (
        <Badge key={protocol} className="whitespace-nowrap border-transparent bg-surface-2 font-mono text-[0.625rem] text-muted-foreground">
          {t(`models.protocols.${protocol}`)}
        </Badge>
      ))}
    </div>
  )
}

function ModelCapabilitySummary({ className, item }: SummaryProps) {
  const { t, i18n } = useTranslation()
  const exactContext = item.context_window === null
    ? undefined
    : t('models.values.context', {
        value: formatExactContextWindow(item.context_window, i18n.resolvedLanguage),
      })
  return (
    <div className={cn('flex min-w-0 flex-wrap gap-x-3 gap-y-1 text-[0.6875rem] text-muted-foreground', className)}>
      <span className="inline-flex items-center gap-1" title={exactContext}>
        <Gauge className="size-3" aria-hidden="true" />
        {item.context_window === null
          ? t('models.values.contextUnknownShort')
          : t('models.values.contextShort', { value: formatContextWindow(item.context_window) })}
      </span>
      {item.supports_reasoning ? (
        <span className="inline-flex items-center gap-1"><BrainCircuit className="size-3" aria-hidden="true" />{t('models.capabilities.reasoning')}</span>
      ) : null}
      {item.supports_tool_calls ? (
        <span className="inline-flex items-center gap-1"><Wrench className="size-3" aria-hidden="true" />{t('models.capabilities.tool_calls')}</span>
      ) : null}
    </div>
  )
}

function ModelDescription({ className, item }: SummaryProps) {
  if (!item.description) return null
  return (
    <p
      className={cn('line-clamp-2 break-words text-xs leading-5 text-muted-foreground', className)}
      title={item.description}
    >
      {item.description}
    </p>
  )
}

export function ModelCatalogDesktopEntry({ item, pricingScope, expanded, onToggle }: EntryProps) {
  const { t } = useTranslation()
  const detailsId = `model-details-${encodeURIComponent(item.model)}`
  return (
    <>
      <tr className="transition-colors duration-150 hover:bg-surface-2/35">
        <td className="px-3 py-3">
          <ModelCatalogIdentity item={item} />
          <ModelDescription item={item} className="mt-2" />
          <ModelCapabilitySummary item={item} className="mt-2" />
        </td>
        <td className="px-3 py-2.5"><ModelPriceValue label={t('models.columns.input')} value={item.prices?.input} /></td>
        <td className="px-3 py-2.5"><ModelPriceValue label={t('models.columns.output')} value={item.prices?.output} /></td>
        <td className="px-3 py-2.5"><ModelPriceValue label={t('models.columns.cacheRead')} value={item.prices?.cache_read} muted /></td>
        <td className="px-3 py-2.5">
          <div className="flex flex-wrap items-center justify-end gap-1.5">
            <ModelStateBadges item={item} />
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
            <ModelCatalogExpandedDetails item={item} pricingScope={pricingScope} />
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
    <article className="@container rounded-lg border border-[var(--hairline)] bg-surface-1 p-3 transition-colors duration-150 hover:border-info/25">
      <div className="flex min-w-0 items-start gap-3">
        <div className="min-w-0 flex-1"><ModelCatalogIdentity item={item} /></div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="shrink-0"
          aria-expanded={expanded}
          aria-controls={detailsId}
          onClick={onToggle}
        >
          {t('models.actions.details')}
          <ChevronDown className={cn('size-3.5 transition-transform', expanded && 'rotate-180')} aria-hidden="true" />
        </Button>
      </div>
      <ModelDescription item={item} className="mt-2" />
      <div className="mt-2.5 grid gap-2.5 @min-[48rem]:grid-cols-[minmax(0,1fr)_auto] @min-[48rem]:items-end @min-[48rem]:gap-5">
        <div className="grid min-w-0 gap-2 @min-[48rem]:flex @min-[48rem]:flex-wrap @min-[48rem]:items-center @min-[48rem]:gap-x-3">
          <ModelCapabilitySummary item={item} />
          <ModelStateBadges item={item} />
        </div>
        <div className="grid w-full grid-cols-2 gap-4 @min-[48rem]:w-48">
          <ModelPriceValue label={t('models.columns.input')} value={item.prices?.input} showLabel />
          <ModelPriceValue label={t('models.columns.output')} value={item.prices?.output} showLabel />
        </div>
      </div>
      {expanded ? (
        <div id={detailsId} className="mt-3 border-t border-[var(--hairline)] pt-3">
          <ModelCatalogExpandedDetails item={item} pricingScope={pricingScope} />
        </div>
      ) : null}
    </article>
  )
}
