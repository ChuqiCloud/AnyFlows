import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import type { ModelCatalogItem, ModelCatalogPricingScope } from '@/lib/api/generated/types.gen'
import { formatContextWindow, formatExactContextWindow } from './model-context-window'
import { ModelPriceValue } from './model-price-value'

type ExpandedDetailsProps = {
  item: ModelCatalogItem
  pricingScope: ModelCatalogPricingScope
}

/** 展开后集中呈现权威能力与定价快照，避免把高密度主行撑高。 */
export function ModelCatalogExpandedDetails({ item, pricingScope }: ExpandedDetailsProps) {
  const { t, i18n } = useTranslation()
  const prices = item.prices ?? undefined
  const availableProtocols = item.available_protocols ?? []
  const exactContext = item.context_window === null
    ? undefined
    : t('models.values.context', {
        value: formatExactContextWindow(item.context_window, i18n.resolvedLanguage),
      })
  const ratios = [
    ['group', item.ratios.group_micros],
    ['groupModel', item.ratios.group_model_micros],
    ['peak', item.ratios.peak_micros],
  ] as const
  const capabilities = [
    item.supports_reasoning ? 'reasoning' : undefined,
    item.supports_tool_calls ? 'tool_calls' : undefined,
    item.supports_responses_compact ? 'responses_compact' : undefined,
  ].filter((value): value is 'reasoning' | 'tool_calls' | 'responses_compact' => value !== undefined)

  return (
    <div className="grid gap-5 lg:grid-cols-[minmax(0,1.25fr)_minmax(18rem,0.75fr)]">
      <section aria-label={t('models.details.capabilities')}>
        <h3 className="text-xs font-semibold">{t('models.details.capabilities')}</h3>
        <p className="mt-1 line-clamp-3 max-w-3xl break-words text-xs leading-5 text-muted-foreground">
          {item.description ?? t('models.values.descriptionUnknown')}
        </p>
        <dl className="mt-3 grid gap-x-5 gap-y-3 sm:grid-cols-2">
          <DetailGroup label={t('models.details.context')}>
            {item.context_window === null
              ? t('models.values.contextUnknown')
              : (
                  <span title={exactContext}>
                    {t('models.values.context', { value: formatContextWindow(item.context_window) })}
                  </span>
                )}
          </DetailGroup>
          <DetailGroup label={t('models.details.inputModalities')}>
            <BadgeList values={item.input_modalities.map((value) => t(`models.modalities.${value}`))} />
          </DetailGroup>
          <DetailGroup label={t('models.details.outputModalities')}>
            <BadgeList values={item.output_modalities.map((value) => t(`models.modalities.${value}`))} />
          </DetailGroup>
          <DetailGroup label={t('models.details.additionalCapabilities')}>
            {capabilities.length > 0
              ? <BadgeList values={capabilities.map((value) => t(`models.capabilities.${value}`))} />
              : t('models.values.noAdditionalCapabilities')}
          </DetailGroup>
          <DetailGroup label={t('models.details.protocols')}>
            {availableProtocols.length > 0
              ? <BadgeList values={availableProtocols.map((value) => t(`models.protocols.${value}`))} />
              : t(pricingScope === 'group'
                ? 'models.values.protocolsUnavailable'
                : 'models.values.protocolsPublicUnknown')}
          </DetailGroup>
        </dl>
      </section>

      <section className="border-t border-[var(--hairline)] pt-4 lg:border-t-0 lg:border-l lg:pt-0 lg:pl-5" aria-label={t('models.details.pricing')}>
        <h3 className="text-xs font-semibold">{t('models.details.pricing')}</h3>
        <div className="mt-3 grid grid-cols-2 gap-4">
          <ModelPriceValue label={t('models.columns.cacheCreation5m')} value={prices?.cache_creation_5m} />
          <ModelPriceValue label={t('models.columns.cacheCreation1h')} value={prices?.cache_creation_1h} />
        </div>
        <div className="mt-4 text-[0.6875rem] text-muted-foreground">
          {t(pricingScope === 'group' ? 'models.values.appliedRatios' : 'models.values.baseRatios')}
        </div>
        <div className="mt-1.5 flex flex-wrap gap-1.5">
          {ratios.map(([key, value]) => (
            <Badge key={key} className="bg-surface-1 font-mono text-muted-foreground">
              {t(`models.ratios.${key}`)} {formatRatioMicros(value)}
            </Badge>
          ))}
          <Badge className="bg-surface-1 font-mono text-muted-foreground">
            {t('models.values.priceVersion', { value: item.price_version })}
          </Badge>
        </div>
      </section>
    </div>
  )
}

function DetailGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <dt className="text-[0.6875rem] text-muted-foreground">{label}</dt>
      <dd className="mt-1 text-xs text-foreground">{children}</dd>
    </div>
  )
}

function BadgeList({ values }: { values: string[] }) {
  return <span className="flex flex-wrap gap-1">{values.map((value) => <Badge key={value}>{value}</Badge>)}</span>
}

function formatRatioMicros(value: string) {
  const micros = BigInt(value)
  const whole = micros / 1_000_000n
  const fraction = (micros % 1_000_000n).toString().padStart(6, '0').replace(/0+$/, '')
  return `${whole}${fraction ? `.${fraction}` : ''}x`
}
