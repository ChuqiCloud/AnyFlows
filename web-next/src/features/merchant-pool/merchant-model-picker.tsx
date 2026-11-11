import { useEffect, useMemo, useState } from 'react'
import { Input, Skeleton } from '@heroui/react'
import { Check, ChevronDown, Cpu, Search } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { findProviderLogo } from '@/components/brand/model-logos'
import { EMPTY_MODEL_CATALOG_FILTERS, useModelCatalog } from '@/features/models/model-api'
import { cn } from '@/lib/utils'
import { PoolButton as Button } from './merchant-pool-ui'

type MerchantModelPickerProps = {
  value: string
  excludedModels: string[]
  disabled: boolean
  onSelect: (model: string) => void
}

/** Keep catalog selection in the service draft; opening another tab loses its tab-scoped session. */
export function MerchantModelPicker(props: MerchantModelPickerProps) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return <div className="min-w-0">
    <Button type="button" size="sm" variant="light" className="h-7 min-h-7 px-1 text-xs text-primary" aria-expanded={open} isDisabled={props.disabled} onPress={() => setOpen((current) => !current)}>
      <Search className="size-3.5" aria-hidden="true" />{t('merchantPool.platformModelCatalog')}<ChevronDown className={cn('size-3 transition-transform', open && 'rotate-180')} aria-hidden="true" />
    </Button>
    {open ? <MerchantModelCatalog {...props} onSelect={(model) => { props.onSelect(model); setOpen(false) }} /> : null}
  </div>
}

function MerchantModelCatalog({ value, excludedModels, disabled, onSelect }: MerchantModelPickerProps) {
  const { t } = useTranslation()
  const [searchInput, setSearchInput] = useState(value)
  const [search, setSearch] = useState(value.trim())
  const catalog = useModelCatalog(search, EMPTY_MODEL_CATALOG_FILTERS, true)
  const models = useMemo(() => catalog.data?.pages.flatMap((page) => page.models) ?? [], [catalog.data])
  const typedModel = searchInput.trim()
  const searchPending = search !== typedModel
  const canUseTypedModel = typedModel.length > 0 && new TextEncoder().encode(typedModel).length <= 255 && !/\p{Cc}/u.test(typedModel) && !excludedModels.includes(typedModel)
  const matchesCatalog = models.some((model) => model.model === typedModel)

  useEffect(() => {
    const timer = window.setTimeout(() => setSearch(searchInput.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [searchInput])

  return <section role="region" aria-label={t('merchantPool.modelPicker.title')} className="mt-1 grid min-w-0 gap-2 rounded-lg border border-[var(--hairline)] bg-surface-2/35 p-3">
    <Input autoFocus type="search" size="sm" aria-label={t('merchantPool.modelPicker.search')} placeholder={t('merchantPool.modelPicker.searchPlaceholder')} value={searchInput} onValueChange={setSearchInput} startContent={<Search className="size-3.5 text-muted-foreground" aria-hidden="true" />} />
    <p className="text-[11px] leading-5 text-muted-foreground">{t('merchantPool.modelPicker.hint')}</p>
    {catalog.isPending || searchPending ? <div role="status" aria-label={t('merchantPool.modelPicker.loading')} className="grid gap-1.5">{[0, 1, 2].map((item) => <Skeleton key={item} className="h-10 rounded-md" />)}</div> : <>
      {catalog.isError ? <div role="alert" className="flex flex-wrap items-center justify-between gap-2 text-xs text-warning"><p>{t('merchantPool.modelPicker.error')}</p><Button type="button" size="sm" variant="flat" isDisabled={catalog.isFetching} onPress={() => void (catalog.isFetchNextPageError ? catalog.fetchNextPage() : catalog.refetch())}>{t('merchantPool.retry')}</Button></div> : null}
      {models.length > 0 ? <div role="listbox" aria-label={t('merchantPool.modelPicker.title')} className="max-h-60 overflow-y-auto rounded-md bg-background p-1">
        {models.map((model) => {
          const selected = value.trim() === model.model
          const unavailable = excludedModels.includes(model.model)
          const ProviderIcon = findProviderLogo(model.provider)?.Icon ?? Cpu
          return <button key={model.model} type="button" role="option" aria-label={model.model} aria-selected={selected} disabled={disabled || unavailable} onClick={() => onSelect(model.model)} className={cn('flex min-h-10 w-full items-center gap-2 rounded-md px-2 py-1.5 text-left outline-none hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-50', selected && 'bg-primary/8')}>
            <ProviderIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
            <span className="min-w-0 flex-1"><span className="block truncate font-mono text-xs">{model.model}</span><span className="block truncate text-[11px] text-muted-foreground">{model.display_name} · {model.provider}</span></span>
            {unavailable ? <span className="shrink-0 text-[11px] text-muted-foreground">{t('merchantPool.modelPicker.alreadyAdded')}</span> : <Check className={cn('size-3.5 shrink-0 text-primary', !selected && 'opacity-0')} aria-hidden="true" />}
          </button>
        })}
      </div> : !catalog.isError ? <p role="status" className="rounded-md border border-dashed border-[var(--hairline)] p-3 text-xs leading-5 text-muted-foreground">{t('merchantPool.modelPicker.empty')}</p> : null}
      {catalog.hasNextPage ? <Button type="button" size="sm" variant="light" isDisabled={catalog.isFetchingNextPage} onPress={() => void catalog.fetchNextPage()}>{t(catalog.isFetchingNextPage ? 'merchantPool.modelPicker.loading' : 'merchantPool.modelPicker.more')}</Button> : null}
    </>}
    {canUseTypedModel && !matchesCatalog ? <Button type="button" size="sm" variant="bordered" className="h-auto min-h-8 justify-start whitespace-normal py-1.5 text-left" isDisabled={disabled} onPress={() => onSelect(typedModel)}>{t('merchantPool.modelPicker.useTypedName', { model: typedModel })}</Button> : null}
  </section>
}
