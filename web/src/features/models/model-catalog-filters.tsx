import { RotateCcw, SlidersHorizontal } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Select } from '@/components/ui/select'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet'
import type {
  ModelCatalogCapability,
  ModelCatalogModality,
  ModelCatalogProtocol,
} from '@/lib/api/generated/types.gen'
import {
  EMPTY_MODEL_CATALOG_FILTERS,
  type ModelBillingFilter,
  type ModelCatalogFilters,
} from './model-api'

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
  onChange: (filters: ModelCatalogFilters) => void
  showHeading?: boolean
}

/** 统计当前已启用的筛选条件，供移动端入口和清空操作复用。 */
export function countModelCatalogFilters(filters: ModelCatalogFilters) {
  return (filters.billingMode === 'all' ? 0 : 1)
    + filters.inputModalities.length
    + filters.outputModalities.length
    + filters.capabilities.length
    + filters.protocols.length
}

function toggleValue<T extends string>(values: T[], value: T, checked: boolean) {
  return checked
    ? [...values, value].sort()
    : values.filter((item) => item !== value)
}

function FilterContent({ authenticated, filters, onChange, showHeading = true }: FilterContentProps) {
  const { t } = useTranslation()
  const activeCount = countModelCatalogFilters(filters)

  return (
    <div className="grid gap-5">
      <div className="flex min-h-8 items-center justify-between gap-3">
        {showHeading ? <span className="text-sm font-semibold">{t('models.filters.title')}</span> : <span />}
        <Button
          type="button"
          size="xs"
          variant="ghost"
          disabled={activeCount === 0}
          onClick={() => onChange(EMPTY_MODEL_CATALOG_FILTERS)}
        >
          <RotateCcw aria-hidden="true" />
          {t('models.filters.reset')}
        </Button>
      </div>

      <FilterGroup title={t('models.filters.billingMode')}>
        <Select
          value={filters.billingMode}
          onChange={(event) => onChange({
            ...filters,
            billingMode: event.target.value as ModelBillingFilter,
          })}
        >
          <option value="all">{t('models.filters.all')}</option>
          <option value="per_token">{t('models.billing.per_token')}</option>
          <option value="free">{t('models.billing.free')}</option>
        </Select>
      </FilterGroup>

      <FilterGroup title={t('models.filters.inputModalities')}>
        {modalities.map((modality) => (
          <FilterOption
            key={modality}
            checked={filters.inputModalities.includes(modality)}
            label={t(`models.modalities.${modality}`)}
            onCheckedChange={(checked) => onChange({
              ...filters,
              inputModalities: toggleValue(filters.inputModalities, modality, checked),
            })}
          />
        ))}
      </FilterGroup>

      <FilterGroup title={t('models.filters.outputModalities')}>
        {modalities.map((modality) => (
          <FilterOption
            key={modality}
            checked={filters.outputModalities.includes(modality)}
            label={t(`models.modalities.${modality}`)}
            onCheckedChange={(checked) => onChange({
              ...filters,
              outputModalities: toggleValue(filters.outputModalities, modality, checked),
            })}
          />
        ))}
      </FilterGroup>

      <FilterGroup title={t('models.filters.capabilities')}>
        {capabilities.map((capability) => (
          <FilterOption
            key={capability}
            checked={filters.capabilities.includes(capability)}
            label={t(`models.capabilities.${capability}`)}
            onCheckedChange={(checked) => onChange({
              ...filters,
              capabilities: toggleValue(filters.capabilities, capability, checked),
            })}
          />
        ))}
      </FilterGroup>

      <FilterGroup title={t('models.filters.protocols')}>
        {authenticated ? protocols.map((protocol) => (
          <FilterOption
            key={protocol}
            checked={filters.protocols.includes(protocol)}
            label={t(`models.protocols.${protocol}`)}
            onCheckedChange={(checked) => onChange({
              ...filters,
              protocols: toggleValue(filters.protocols, protocol, checked),
            })}
          />
        )) : (
          <p className="text-xs leading-5 text-muted-foreground">
            {t('models.filters.protocolsLoginHint')}
          </p>
        )}
      </FilterGroup>
    </div>
  )
}

function FilterGroup({ children, title }: { children: React.ReactNode; title: string }) {
  return (
    <fieldset className="grid gap-2.5 border-t border-[var(--hairline)] pt-4">
      <legend className="mb-2 text-xs font-semibold text-foreground">{title}</legend>
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
    <label className="flex min-h-7 cursor-pointer items-center gap-2 text-xs text-muted-foreground transition-colors hover:text-foreground">
      <Checkbox
        checked={checked}
        onCheckedChange={(value) => onCheckedChange(value === true)}
      />
      <span>{label}</span>
    </label>
  )
}

export function ModelCatalogDesktopFilters(props: FilterContentProps) {
  return (
    <aside className="hidden w-52 shrink-0 lg:block">
      <div className="sticky top-20 max-h-[calc(100dvh-6rem)] overflow-y-auto pr-4">
        <FilterContent {...props} />
      </div>
    </aside>
  )
}

export function ModelCatalogMobileFilters(props: FilterContentProps) {
  const { t } = useTranslation()
  const activeCount = countModelCatalogFilters(props.filters)

  return (
    <Sheet>
      <SheetTrigger asChild>
        <Button type="button" size="sm" variant="secondary" className="lg:hidden">
          <SlidersHorizontal aria-hidden="true" />
          {t('models.filters.action')}
          {activeCount > 0 ? (
            <span className="grid size-5 place-items-center rounded-md bg-primary text-[0.6875rem] text-primary-foreground tabular-nums">
              {activeCount}
            </span>
          ) : null}
        </Button>
      </SheetTrigger>
      <SheetContent side="left" className="w-[min(22rem,90vw)] gap-0 overflow-y-auto p-0">
        <SheetHeader className="border-b border-[var(--hairline)] p-5 pr-12">
          <SheetTitle>{t('models.filters.title')}</SheetTitle>
          <SheetDescription>{t('models.filters.description')}</SheetDescription>
        </SheetHeader>
        <div className="p-5">
          <FilterContent {...props} showHeading={false} />
        </div>
      </SheetContent>
    </Sheet>
  )
}
