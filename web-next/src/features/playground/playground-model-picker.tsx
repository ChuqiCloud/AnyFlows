import { Button, Checkbox, Chip, Input, Popover, PopoverContent, PopoverTrigger, Skeleton } from '@heroui/react'
import { providerDisplayName } from '@/components/brand/provider-catalog'

import {
  Boxes,
  Check,
  ChevronDown,
  LockKeyhole,
  RefreshCw,
  Search,
  SearchX,
} from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { findProviderLogo } from '@/components/brand/model-logos'
import type {
  ModelCatalogItem,
  ModelCatalogProtocol,
} from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import {
  ALL_PLAYGROUND_MODEL_FILTER,
  DEFAULT_PLAYGROUND_MODEL_FILTER,
  filterPlaygroundModels,
  playgroundModelProviderCategories,
  type PlaygroundModelFilter,
} from './playground-model-filter'
import {
  modelsForPlaygroundSelectionMode,
  selectPlaygroundModel,
} from './playground-model-selection'
import { playgroundProtocolForModel, type PlaygroundProtocol } from './playground-protocol'
import { MAX_PLAYGROUND_MODELS } from './playground-types'

type PlaygroundModelPickerProps = {
  compact?: boolean
  values: string[]
  comparisonEnabled: boolean
  search: string
  models: ModelCatalogItem[]
  protocolByModel: Readonly<Record<string, PlaygroundProtocol>>
  loading: boolean
  error: boolean
  loadingMore: boolean
  hasMore: boolean
  locked: boolean
  onChange: (models: string[], comparisonEnabled: boolean) => void
  onLoadMore: () => void
  onRefresh: () => void
  onSearch: (value: string) => void
}

const playgroundProtocols: PlaygroundProtocol[] = ['openai_chat', 'openai_responses', 'anthropic']

export function PlaygroundModelPicker(props: PlaygroundModelPickerProps) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [draftValues, setDraftValues] = useState(props.values)
  const [draftComparisonEnabled, setDraftComparisonEnabled] = useState(
    props.comparisonEnabled,
  )
  const [filter, setFilter] = useState<PlaygroundModelFilter>(DEFAULT_PLAYGROUND_MODEL_FILTER)
  const summary = props.values.length === 1
    ? props.values[0]
    : t('playground.model.selectedCount', { count: props.values.length })
  const providerCategories = useMemo(
    () => playgroundModelProviderCategories(props.models),
    [props.models],
  )
  const availableProtocols = useMemo(() => (
    playgroundProtocols.filter((protocol) => props.models.some((item) => (
      item.available_protocols.includes(protocol as ModelCatalogProtocol)
    )))
  ), [props.models])
  const visibleModels = useMemo(
    () => filterPlaygroundModels(props.models, filter),
    [filter, props.models],
  )

  const setOpenState = (nextOpen: boolean) => {
    if (nextOpen) {
      setDraftValues(props.values)
      setDraftComparisonEnabled(props.comparisonEnabled)
      setFilter(DEFAULT_PLAYGROUND_MODEL_FILTER)
    }
    setOpen(nextOpen)
  }

  return (
    <Popover isOpen={open} placement="top" onOpenChange={setOpenState}>
      <PopoverTrigger>
        <Button
          type="button"
          variant="bordered"
          aria-expanded={open}
          className="h-auto min-h-9 w-full justify-between px-2.5 py-2 text-left"
          isDisabled={props.locked}
        >
          <span className="flex min-w-0 items-center gap-2">
            <Boxes className="size-3.5 shrink-0 text-info" aria-hidden="true" />
            <span className={cn('truncate font-mono text-xs', props.values.length === 0 && 'text-muted-foreground')}>
              {summary || t('playground.model.pending')}
            </span>
          </span>
          {props.locked
            ? <LockKeyhole className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
            : <ChevronDown className={cn('size-3.5 shrink-0 text-muted-foreground transition-transform', open && 'rotate-180')} aria-hidden="true" />}
        </Button>
      </PopoverTrigger>
      {!props.compact && props.values.length > 0 ? (
        <div className="flex flex-wrap gap-1.5" aria-label={t('playground.model.selectedLabel')}>
          {props.values.map((model) => (
            <Chip key={model} className="max-w-full bg-surface-2 font-mono text-muted-foreground" size="sm" variant="flat">
              <span className="truncate">{model}</span>
              {props.protocolByModel[model] ? (
                <span className="ml-1 shrink-0 font-sans text-[0.625rem]">
                  {t(`playground.model.protocols.${props.protocolByModel[model]}`)}
                </span>
              ) : null}
            </Chip>
          ))}
        </div>
      ) : null}
      {!props.compact && props.locked ? (
        <p className="flex items-center gap-1.5 text-[0.6875rem] text-muted-foreground">
          <LockKeyhole className="size-3" aria-hidden="true" />
          {t('playground.model.locked')}
        </p>
      ) : null}
      <PopoverContent
        className="w-[calc(100vw-1rem)] max-w-[34rem] items-stretch overflow-hidden p-0 sm:w-[34rem]"
      >
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--hairline)] px-3 py-2.5">
          <div className="min-w-0">
            <h2 className="truncate text-xs font-semibold">{t('playground.model.title')}</h2>
            <p className="mt-0.5 text-[0.6875rem] text-muted-foreground">
              {draftComparisonEnabled
                ? t('playground.model.selectionCount', { count: draftValues.length, max: MAX_PLAYGROUND_MODELS })
                : t('playground.model.singleMode')}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <label className="flex cursor-pointer items-center gap-1.5 text-[0.6875rem] font-medium">
              <Checkbox
                aria-label={t('playground.model.comparisonMode')}
                isSelected={draftComparisonEnabled}
                size="sm"
                onValueChange={(checked) => {
                  const enabled = checked === true
                  setDraftComparisonEnabled(enabled)
                  setDraftValues((current) => modelsForPlaygroundSelectionMode(current, enabled))
                }}
              />
              <span>{t('playground.model.comparisonMode')}</span>
            </label>
            <Chip className="bg-info/10 text-info" size="sm" variant="flat">
              {t('playground.model.catalogCount', { count: props.models.length })}
            </Chip>
          </div>
        </div>
        <div className="grid max-h-[min(70vh,34rem)] min-h-0 grid-cols-1 md:grid-cols-[9rem_minmax(0,1fr)]">
          <aside className="min-w-0 border-b border-[var(--hairline)] bg-surface-2/35 p-2 md:border-b-0 md:border-r">
            <div className="mb-1 px-1 text-[0.6875rem] font-semibold text-muted-foreground">
              {t('playground.model.providers')}
            </div>
            <div className="flex gap-1 overflow-x-auto md:grid md:max-h-[min(50vh,28rem)] md:overflow-y-auto">
              {providerCategories.map((category) => {
                const logo = category.value === ALL_PLAYGROUND_MODEL_FILTER
                  ? undefined
                  : findProviderLogo(category.value)
                const ProviderIcon = logo?.Icon ?? Boxes
                const selected = filter.provider === category.value
                return (
                  <button
                    key={category.value}
                    type="button"
                    className={cn(
                      'flex min-w-max items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-[0.6875rem] transition-colors',
                      'hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
                      selected ? 'bg-primary/10 font-medium text-foreground' : 'text-muted-foreground',
                    )}
                    onClick={() => setFilter((current) => ({ ...current, provider: category.value }))}
                  >
                    <ProviderIcon className="size-3.5 shrink-0" aria-hidden="true" />
                    <span className="max-w-24 truncate">{category.value === ALL_PLAYGROUND_MODEL_FILTER
                      ? t('playground.model.allModels')
                      : providerDisplayName(category.value)}</span>
                    <span className="ml-auto tabular-nums text-[0.625rem] text-muted-foreground">{category.count}</span>
                  </button>
                )
              })}
            </div>
          </aside>
          <div className="flex min-h-0 min-w-0 flex-col">
            <div className="space-y-2 border-b border-[var(--hairline)] p-2">
              <label className="relative block">
                <span className="sr-only">{t('playground.model.search')}</span>
                <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
                <Input
                  autoFocus
                  classNames={{ input: 'text-xs', inputWrapper: 'h-8 min-h-8 pl-8' }}
                  isDisabled={false}
                  placeholder={t('playground.model.searchPlaceholder')}
                  type="search"
                  value={props.search}
                  startContent={<Search className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />}
                  onChange={(event) => props.onSearch(event.target.value)}
                />
              </label>
              <div className="flex gap-1 overflow-x-auto" role="tablist" aria-label={t('playground.model.protocolsLabel')}>
                {[ALL_PLAYGROUND_MODEL_FILTER, ...availableProtocols].map((protocol) => {
                  const selected = filter.protocol === protocol
                  return (
                    <button
                      key={protocol}
                      type="button"
                      role="tab"
                      aria-selected={selected}
                      className={cn(
                        'shrink-0 rounded-md px-2 py-1 text-[0.6875rem] transition-colors',
                        selected ? 'bg-primary text-primary-foreground' : 'bg-surface-2 text-muted-foreground hover:text-foreground',
                      )}
                      onClick={() => setFilter((current) => ({ ...current, protocol }))}
                    >
                      {protocol === ALL_PLAYGROUND_MODEL_FILTER
                        ? t('playground.model.allProtocols')
                        : t(`playground.model.protocols.${protocol}`)}
                    </button>
                  )
                })}
              </div>
            </div>
            <div
              className="min-h-0 flex-1 overflow-y-auto p-1.5"
              role="listbox"
              aria-label={t('playground.model.title')}
              aria-multiselectable={draftComparisonEnabled}
            >
              {props.loading ? (
                <div className="grid gap-1 p-1" aria-label={t('playground.model.loading')}>
                  {[0, 1, 2, 3, 4].map((item) => <Skeleton key={item} className="h-9 rounded-md" />)}
                </div>
              ) : props.error ? (
                <div role="alert" className="grid min-h-40 place-items-center p-4 text-center">
                  <div>
                    <RefreshCw className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
                    <p className="mt-2 text-xs font-medium">{t('playground.model.error')}</p>
                    <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={props.onRefresh}>
                      {t('playground.actions.retry')}
                    </Button>
                  </div>
                </div>
              ) : visibleModels.length === 0 ? (
                <div className="grid min-h-40 place-items-center p-4 text-center">
                  <div>
                    <SearchX className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
                    <p className="mt-2 text-xs font-medium">{t('playground.model.empty')}</p>
                  </div>
                </div>
              ) : (
                <div className="grid gap-0.5">
                  {visibleModels.map((item) => {
                    const selected = draftValues.includes(item.model)
                    const protocol = playgroundProtocolForModel(item)
                    const disabled = (draftComparisonEnabled && selected && draftValues.length === 1)
                      || (draftComparisonEnabled && !selected && draftValues.length >= MAX_PLAYGROUND_MODELS)
                      || protocol === undefined
                    return (
                      <button
                        key={item.model}
                        type="button"
                        role="option"
                        aria-selected={selected}
                        disabled={disabled}
                        className={cn(
                          'flex min-h-9 w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left outline-none transition-colors',
                          'hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-50',
                          selected && 'bg-primary/8',
                        )}
                        onClick={() => setDraftValues(selectPlaygroundModel(
                          draftValues,
                          item.model,
                          draftComparisonEnabled,
                        ))}
                      >
                        <Check className={cn('size-3.5 shrink-0 text-info', !selected && 'opacity-0')} aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate font-mono text-[0.6875rem]">{item.model}</span>
                        <span className="flex shrink-0 items-center gap-1">
                          {protocol ? (
                            <Chip className="bg-info/10 px-1.5 text-[0.625rem] text-info" size="sm" variant="flat">
                              {t(`playground.model.protocols.${protocol}`)}
                            </Chip>
                          ) : (
                            <Chip className="bg-warning/10 text-[0.625rem] text-warning" size="sm" variant="flat">
                              {t('playground.model.protocols.unavailable')}
                            </Chip>
                          )}
                        </span>
                      </button>
                    )
                  })}
                  {props.hasMore ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="light"
                      className="mt-1 justify-center"
                      isDisabled={props.loadingMore}
                      onClick={props.onLoadMore}
                    >
                      {props.loadingMore ? t('playground.model.loadingMore') : t('playground.actions.loadMore')}
                    </Button>
                  ) : null}
                </div>
              )}
            </div>
            <div className="border-t border-[var(--hairline)] p-2">
              <Button
                color="primary"
                type="button"
                size="sm"
                className="w-full"
                onClick={() => {
                  props.onChange(draftValues, draftComparisonEnabled)
                  setOpen(false)
                }}
              >
                <Check className="size-3.5" aria-hidden="true" />{t('playground.model.done')}
              </Button>
            </div>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  )
}
