import { Boxes, Check, ChevronRight, LockKeyhole, RefreshCw, Search, SearchX } from 'lucide-react'
import { useState } from 'react'
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
import type { ModelCatalogItem } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { togglePlaygroundModel } from './playground-model-selection'
import { MAX_PLAYGROUND_MODELS } from './playground-types'

type PlaygroundModelPickerProps = {
  values: string[]
  search: string
  models: ModelCatalogItem[]
  loading: boolean
  error: boolean
  loadingMore: boolean
  hasMore: boolean
  locked: boolean
  onChange: (models: string[]) => void
  onLoadMore: () => void
  onRefresh: () => void
  onSearch: (value: string) => void
}

export function PlaygroundModelPicker(props: PlaygroundModelPickerProps) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const summary = props.values.length === 1
    ? props.values[0]
    : t('playground.model.selectedCount', { count: props.values.length })

  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger asChild>
        <Button
          type="button"
          variant="outline"
          className="h-auto min-h-9 w-full justify-between px-2.5 py-2 text-left"
          disabled={props.locked}
        >
          <span className="flex min-w-0 items-center gap-2">
            <Boxes className="size-3.5 shrink-0 text-info" aria-hidden="true" />
            <span className={cn('truncate font-mono text-xs', props.values.length === 0 && 'text-muted-foreground')}>
              {summary || t('playground.model.pending')}
            </span>
          </span>
          {props.locked
            ? <LockKeyhole className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
            : <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />}
        </Button>
      </SheetTrigger>
      {props.values.length > 0 ? (
        <div className="flex flex-wrap gap-1.5" aria-label={t('playground.model.selectedLabel')}>
          {props.values.map((model) => (
            <Badge key={model} className="max-w-full bg-surface-2 font-mono text-muted-foreground">
              <span className="truncate">{model}</span>
            </Badge>
          ))}
        </div>
      ) : null}
      {props.locked ? (
        <p className="flex items-center gap-1.5 text-[0.6875rem] text-muted-foreground">
          <LockKeyhole className="size-3" aria-hidden="true" />
          {t('playground.model.locked')}
        </p>
      ) : null}
      <SheetContent className="gap-0 sm:max-w-md">
        <SheetHeader className="border-b border-[var(--hairline)]">
          <SheetTitle>{t('playground.model.title')}</SheetTitle>
          <SheetDescription>
            {t('playground.model.selectionCount', {
              count: props.values.length,
              max: MAX_PLAYGROUND_MODELS,
            })}
          </SheetDescription>
        </SheetHeader>
        <div className="border-b border-[var(--hairline)] p-3">
          <label className="relative block">
            <span className="sr-only">{t('playground.model.search')}</span>
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
            <Input
              autoFocus
              type="search"
              className="pl-8"
              value={props.search}
              placeholder={t('playground.model.searchPlaceholder')}
              onChange={(event) => props.onSearch(event.target.value)}
            />
          </label>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-2" role="listbox" aria-label={t('playground.model.title')}>
          {props.loading ? (
            <div className="grid gap-1.5 p-1" aria-label={t('playground.model.loading')}>
              {[0, 1, 2, 3, 4, 5].map((item) => <Skeleton key={item} className="h-11 rounded-lg" />)}
            </div>
          ) : props.error ? (
            <div role="alert" className="grid min-h-56 place-items-center p-4 text-center">
              <div>
                <RefreshCw className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
                <p className="mt-2 text-sm font-medium">{t('playground.model.error')}</p>
                <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={props.onRefresh}>
                  {t('playground.actions.retry')}
                </Button>
              </div>
            </div>
          ) : props.models.length === 0 ? (
            <div className="grid min-h-56 place-items-center p-4 text-center">
              <div>
                <SearchX className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
                <p className="mt-2 text-sm font-medium">{t('playground.model.empty')}</p>
              </div>
            </div>
          ) : (
            <div className="grid gap-0.5">
              {props.models.map((item) => {
                const selected = props.values.includes(item.model)
                const disabled = (selected && props.values.length === 1)
                  || (!selected && props.values.length >= MAX_PLAYGROUND_MODELS)
                return (
                  <button
                    key={item.model}
                    type="button"
                    role="option"
                    aria-selected={selected}
                    disabled={disabled}
                    className={cn(
                      'flex min-h-11 w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left outline-none transition-colors',
                      'hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-50',
                      selected && 'bg-surface-2',
                    )}
                    onClick={() => props.onChange(togglePlaygroundModel(props.values, item.model))}
                  >
                    <Check className={cn('size-3.5 shrink-0 text-info', !selected && 'opacity-0')} aria-hidden="true" />
                    <span className="min-w-0 flex-1 truncate font-mono text-xs">{item.model}</span>
                    <Badge className="shrink-0 bg-surface-2 text-muted-foreground">
                      {t(`models.billing.${item.billing_mode}`)}
                    </Badge>
                  </button>
                )
              })}
              {props.hasMore ? (
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="mt-2 justify-center"
                  disabled={props.loadingMore}
                  onClick={props.onLoadMore}
                >
                  {props.loadingMore ? t('playground.model.loadingMore') : t('playground.actions.loadMore')}
                </Button>
              ) : null}
            </div>
          )}
        </div>
        <div className="border-t border-[var(--hairline)] p-3">
          <Button type="button" className="w-full" onClick={() => setOpen(false)}>
            <Check aria-hidden="true" />{t('playground.model.done')}
          </Button>
        </div>
      </SheetContent>
    </Sheet>
  )
}
