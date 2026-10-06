import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Boxes, Check, ChevronRight, RefreshCw, Search, SearchX, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet'
import { Skeleton } from '@/components/ui/skeleton'
import { EMPTY_MODEL_CATALOG_FILTERS, useModelCatalog } from '@/features/models/model-api'
import { cn } from '@/lib/utils'

const MAX_SELECTED_MODELS = 512

type ApiKeyModelSelectorProps = {
  values: string[]
  error?: string
  onChange: (values: string[]) => void
}

/** 从当前登录用户的可用模型目录中选择精确白名单。 */
export function ApiKeyModelSelector({ values, error, onChange }: ApiKeyModelSelectorProps) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [searchInput, setSearchInput] = useState('')
  const [search, setSearch] = useState('')
  const catalogQuery = useModelCatalog(search, EMPTY_MODEL_CATALOG_FILTERS, true)
  const models = useMemo(
    () => catalogQuery.data?.pages.flatMap((page) => page.models) ?? [],
    [catalogQuery.data],
  )
  const pricingScope = catalogQuery.data?.pages[0]?.pricing_scope
  const unavailable = (catalogQuery.isError && models.length === 0)
    || (pricingScope !== undefined && pricingScope !== 'group')

  useEffect(() => {
    const timer = window.setTimeout(() => setSearch(searchInput.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [searchInput])

  const toggle = (model: string) => {
    if (values.includes(model)) {
      onChange(values.filter((value) => value !== model))
    } else if (values.length < MAX_SELECTED_MODELS) {
      onChange([...values, model])
    }
  }

  return (
    <div className="grid gap-2">
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetTrigger asChild>
          <Button
            id="api-key-models"
            type="button"
            variant="outline"
            className="h-auto min-h-9 w-full justify-between px-2.5 py-2 text-left"
            aria-invalid={!!error}
          >
            <span className="flex min-w-0 items-center gap-2">
              <Boxes className="size-3.5 shrink-0 text-info" aria-hidden="true" />
              <span className="truncate text-xs">
                {values.length > 0
                  ? t('apiKeys.modelSelector.selectedCount', { count: values.length })
                  : t('apiKeys.modelSelector.unrestricted')}
              </span>
            </span>
            <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
          </Button>
        </SheetTrigger>
        <SheetContent className="gap-0 sm:max-w-md">
          <SheetHeader className="border-b border-[var(--hairline)] pr-12">
            <SheetTitle>{t('apiKeys.modelSelector.title')}</SheetTitle>
            <SheetDescription>{t('apiKeys.modelSelector.description')}</SheetDescription>
          </SheetHeader>
          <div className="border-b border-[var(--hairline)] p-3">
            <label className="relative block">
              <span className="sr-only">{t('apiKeys.modelSelector.search')}</span>
              <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
              <Input
                autoFocus
                type="search"
                className="pl-8"
                value={searchInput}
                placeholder={t('apiKeys.modelSelector.placeholder')}
                onChange={(event) => setSearchInput(event.target.value)}
              />
            </label>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-2" role="listbox" aria-label={t('apiKeys.modelSelector.title')}>
            {catalogQuery.isPending ? (
              <div className="grid gap-1.5 p-1" aria-label={t('apiKeys.modelSelector.loading')}>
                {[0, 1, 2, 3, 4, 5].map((item) => <Skeleton key={item} className="h-11 rounded-lg" />)}
              </div>
            ) : unavailable ? (
              <SelectorMessage icon={RefreshCw} message={t('apiKeys.modelSelector.error')}>
                <Button type="button" size="sm" variant="secondary" onClick={() => void catalogQuery.refetch()}>
                  {t('apiKeys.actions.retry')}
                </Button>
              </SelectorMessage>
            ) : models.length === 0 ? (
              <SelectorMessage icon={SearchX} message={t('apiKeys.modelSelector.empty')} />
            ) : (
              <div className="grid gap-0.5">
                {models.map((item) => {
                  const selected = values.includes(item.model)
                  return (
                    <button
                      key={item.model}
                      type="button"
                      role="option"
                      aria-selected={selected}
                      disabled={!selected && values.length >= MAX_SELECTED_MODELS}
                      className={cn(
                        'flex min-h-11 w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left outline-none transition-colors',
                        'hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-50',
                        selected && 'bg-surface-2',
                      )}
                      onClick={() => toggle(item.model)}
                    >
                      <Check className={cn('size-3.5 shrink-0 text-info', !selected && 'opacity-0')} aria-hidden="true" />
                      <span className="min-w-0 flex-1 truncate font-mono text-xs">{item.model}</span>
                      <Badge className="shrink-0 bg-surface-2 text-muted-foreground">
                        {t(`models.billing.${item.billing_mode}`)}
                      </Badge>
                    </button>
                  )
                })}
                {catalogQuery.hasNextPage ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    className="mt-2 justify-center"
                    disabled={catalogQuery.isFetchingNextPage}
                    onClick={() => void catalogQuery.fetchNextPage()}
                  >
                    {t(catalogQuery.isFetchingNextPage ? 'apiKeys.modelSelector.loadingMore' : 'apiKeys.actions.loadMore')}
                  </Button>
                ) : null}
              </div>
            )}
          </div>
          <div className="border-t border-[var(--hairline)] p-3">
            <Button type="button" className="w-full" onClick={() => setOpen(false)}>
              <Check aria-hidden="true" />{t('apiKeys.actions.done')}
            </Button>
          </div>
        </SheetContent>
      </Sheet>
      {values.length > 0 ? (
        <div className="flex max-h-24 flex-wrap gap-1.5 overflow-y-auto" aria-label={t('apiKeys.modelSelector.selectedLabel')}>
          {values.map((model) => (
            <Badge key={model} className="max-w-full gap-1 bg-surface-2 pr-0.5 font-mono text-muted-foreground">
              <span className="truncate">{model}</span>
              <button
                type="button"
                className="grid size-4 shrink-0 place-items-center rounded-sm hover:bg-background focus-visible:ring-2 focus-visible:ring-ring/60"
                aria-label={t('apiKeys.modelSelector.remove', { model })}
                onClick={() => toggle(model)}
              >
                <X className="size-3" aria-hidden="true" />
              </button>
            </Badge>
          ))}
        </div>
      ) : null}
    </div>
  )
}

function SelectorMessage({ icon: Icon, message, children }: {
  icon: typeof RefreshCw
  message: string
  children?: ReactNode
}) {
  return (
    <div role="status" className="grid min-h-56 place-items-center p-4 text-center">
      <div>
        <Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
        <p className="mt-2 text-sm font-medium">{message}</p>
        {children ? <div className="mt-3">{children}</div> : null}
      </div>
    </div>
  )
}
