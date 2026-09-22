import { Button, Checkbox, Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { ChevronDown, Cpu, RotateCcw, SlidersHorizontal, Waypoints } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import type {
  ModelCatalogCapability,
  ModelCatalogModality,
  ModelCatalogProtocol,
  ModelCatalogProvider,
} from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import {
  EMPTY_MODEL_CATALOG_FILTERS,
  type ModelBillingFilter,
  type ModelCatalogFilters,
  countModelCatalogFilters,
  toggleModelCatalogFilter,
} from './model-catalog-filter-model'
import { ModelCatalogProviderFilter } from './model-catalog-provider-filter'

const modalities: ModelCatalogModality[] = ['text', 'image', 'audio', 'video']
const capabilities: ModelCatalogCapability[] = ['reasoning', 'tool_calls', 'responses_compact']
const protocols: ModelCatalogProtocol[] = [
  'openai_chat',
  'openai_responses',
  'anthropic',
  'gemini',
  'openai_embeddings',
  'openai_images',
  'openai_audio',
  'openai_speech',
  'jina_rerank',
  'cohere_rerank',
  'xai_video',
]

type FilterContentProps = {
  authenticated: boolean
  filters: ModelCatalogFilters
  providers: ModelCatalogProvider[]
  providersLoading: boolean
  providersError: boolean
  onChange: (filters: ModelCatalogFilters) => void
  onRetryProviders: () => void
  showHeading?: boolean
}

export { countModelCatalogFilters } from './model-catalog-filter-model'

function FilterContent({
  authenticated,
  filters,
  providers,
  providersLoading,
  providersError,
  onChange,
  onRetryProviders,
  showHeading = true,
}: FilterContentProps) {
  const { t } = useTranslation()
  const activeCount = countModelCatalogFilters(filters)
  const featureCount = filters.inputModalities.length
    + filters.outputModalities.length
    + filters.capabilities.length

  return (
    <div>
      <div className="flex min-h-9 items-center justify-between gap-3 pb-3">
        {showHeading ? (
          <div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold">{t('models.filters.title')}</span>
              {activeCount > 0 ? <FilterCount value={activeCount} /> : null}
            </div>
            <p className="mt-0.5 text-[0.6875rem] text-muted-foreground">
              {t('models.filters.sidebarHint')}
            </p>
          </div>
        ) : <span />}
        <Button
          isIconOnly
          aria-label={t('models.filters.reset')}
          className="size-6 min-w-6"
          isDisabled={activeCount === 0}
          size="sm"
          title={t('models.filters.reset')}
          type="button"
          variant="light"
          onClick={() => onChange(EMPTY_MODEL_CATALOG_FILTERS)}
        >
          <RotateCcw className="size-3" aria-hidden="true" />
        </Button>
      </div>

      <BillingModeFilter
        value={filters.billingMode}
        onChange={(billingMode) => onChange({ ...filters, billingMode })}
      />

      <ModelCatalogProviderFilter
        providers={providers}
        selected={filters.providers}
        loading={providersLoading}
        error={providersError}
        onChange={(selected) => onChange({ ...filters, providers: selected })}
        onRetry={onRetryProviders}
      />

      <AdvancedFilterSection
        icon={Cpu}
        title={t('models.filters.features')}
        count={featureCount}
      >
        <OptionGroup title={t('models.filters.inputModalities')}>
          {modalities.map((modality) => (
            <FilterOption
              key={modality}
              checked={filters.inputModalities.includes(modality)}
              label={t(`models.modalities.${modality}`)}
              onCheckedChange={(checked) => onChange({
                ...filters,
                inputModalities: toggleModelCatalogFilter(filters.inputModalities, modality, checked),
              })}
            />
          ))}
        </OptionGroup>
        <OptionGroup title={t('models.filters.outputModalities')}>
          {modalities.map((modality) => (
            <FilterOption
              key={modality}
              checked={filters.outputModalities.includes(modality)}
              label={t(`models.modalities.${modality}`)}
              onCheckedChange={(checked) => onChange({
                ...filters,
                outputModalities: toggleModelCatalogFilter(filters.outputModalities, modality, checked),
              })}
            />
          ))}
        </OptionGroup>
        <OptionGroup title={t('models.filters.capabilities')}>
          {capabilities.map((capability) => (
            <FilterOption
              key={capability}
              checked={filters.capabilities.includes(capability)}
              label={t(`models.capabilities.${capability}`)}
              onCheckedChange={(checked) => onChange({
                ...filters,
                capabilities: toggleModelCatalogFilter(filters.capabilities, capability, checked),
              })}
            />
          ))}
        </OptionGroup>
      </AdvancedFilterSection>

      <AdvancedFilterSection
        icon={Waypoints}
        title={t('models.filters.protocols')}
        count={filters.protocols.length}
      >
        {authenticated ? (
          <div className="grid gap-1">
            {protocols.map((protocol) => (
              <FilterOption
                key={protocol}
                checked={filters.protocols.includes(protocol)}
                label={t(`models.protocols.${protocol}`)}
                onCheckedChange={(checked) => onChange({
                  ...filters,
                  protocols: toggleModelCatalogFilter(filters.protocols, protocol, checked),
                })}
              />
            ))}
          </div>
        ) : (
          <p className="text-xs leading-5 text-muted-foreground">
            {t('models.filters.protocolsLoginHint')}
          </p>
        )}
      </AdvancedFilterSection>
    </div>
  )
}

function BillingModeFilter(props: {
  value: ModelBillingFilter
  onChange: (value: ModelBillingFilter) => void
}) {
  const { t } = useTranslation()
  const options: { value: ModelBillingFilter; label: string }[] = [
    { value: 'all', label: t('models.filters.allShort') },
    { value: 'per_token', label: t('models.billing.per_token_short') },
    { value: 'free', label: t('models.billing.free') },
  ]
  return (
    <fieldset className="border-t border-[var(--hairline)] pt-4">
      <legend className="mb-2.5 text-xs font-semibold">{t('models.filters.billingMode')}</legend>
      <div className="grid grid-cols-3 gap-1 rounded-lg bg-surface-2/70 p-1">
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            className={cn(
              'h-7 rounded-md px-1.5 text-[0.6875rem] font-medium transition-colors',
              props.value === option.value
                ? 'bg-surface-1 text-foreground'
                : 'text-muted-foreground hover:text-foreground',
            )}
            aria-pressed={props.value === option.value}
            onClick={() => props.onChange(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </fieldset>
  )
}

function AdvancedFilterSection(props: {
  icon: typeof Cpu
  title: string
  count: number
  children: React.ReactNode
}) {
  const Icon = props.icon
  return (
    <details className="group border-t border-[var(--hairline)] py-1">
      <summary className="flex min-h-11 cursor-pointer list-none items-center gap-2 text-xs font-semibold outline-none focus-visible:ring-2 focus-visible:ring-ring/60 [&::-webkit-details-marker]:hidden">
        <Icon className="size-3.5 text-muted-foreground" aria-hidden="true" />
        <span className="min-w-0 flex-1">{props.title}</span>
        {props.count > 0 ? <FilterCount value={props.count} /> : null}
        <ChevronDown className="size-3.5 text-muted-foreground transition-transform group-open:rotate-180" aria-hidden="true" />
      </summary>
      <div className="grid gap-4 pb-4 pl-5">{props.children}</div>
    </details>
  )
}

function OptionGroup({ children, title }: { children: React.ReactNode; title: string }) {
  return (
    <fieldset className="grid gap-1">
      <legend className="mb-1.5 text-[0.6875rem] font-medium text-muted-foreground">{title}</legend>
      {children}
    </fieldset>
  )
}

function FilterOption({ checked, label, onCheckedChange }: {
  checked: boolean
  label: string
  onCheckedChange: (checked: boolean) => void
}) {
  return (
    <label className="flex min-h-8 cursor-pointer items-center gap-2 rounded-md px-1 text-xs text-muted-foreground transition-colors hover:bg-surface-2/60 hover:text-foreground">
      <Checkbox
        aria-label={label}
        isSelected={checked}
        size="sm"
        onValueChange={(value) => onCheckedChange(value === true)}
      />
      <span>{label}</span>
    </label>
  )
}

function FilterCount({ value }: { value: number }) {
  return (
    <span className="grid min-w-5 place-items-center rounded-md bg-primary/12 px-1 text-[0.625rem] font-medium text-info tabular-nums">
      {value}
    </span>
  )
}

export function ModelCatalogDesktopFilters(props: FilterContentProps) {
  return (
    <aside className="hidden w-64 shrink-0 lg:block">
      <div className="sticky top-20 max-h-[calc(100dvh-6rem)] overflow-y-auto pr-5">
        <FilterContent {...props} />
      </div>
    </aside>
  )
}

export function ModelCatalogMobileFilters(props: FilterContentProps) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const activeCount = countModelCatalogFilters(props.filters)

  return (
    <>
      <Button type="button" size="sm" variant="bordered" className="lg:hidden" onClick={() => setOpen(true)}>
        <SlidersHorizontal className="size-3.5" aria-hidden="true" />
        {t('models.filters.action')}
        {activeCount > 0 ? <FilterCount value={activeCount} /> : null}
      </Button>
      <Drawer
        backdrop="blur"
        classNames={{ base: 'w-full max-h-none sm:max-w-[22rem]' }}
        isOpen={open}
        placement="left"
        scrollBehavior="inside"
        onOpenChange={setOpen}
      >
        <DrawerContent>
          {() => (
            <>
              <DrawerHeader className="block border-b border-[var(--hairline)] p-5 pr-12">
                <h2 className="text-base font-medium text-foreground">{t('models.filters.title')}</h2>
                <p className="text-sm text-muted-foreground">{t('models.filters.description')}</p>
              </DrawerHeader>
              <DrawerBody className="min-h-0 flex-1 gap-0 overflow-y-auto p-5">
                <FilterContent {...props} showHeading={false} />
              </DrawerBody>
            </>
          )}
        </DrawerContent>
      </Drawer>
    </>
  )
}
