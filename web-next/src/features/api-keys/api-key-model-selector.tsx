import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Button, Chip, Drawer, DrawerBody, DrawerContent, DrawerHeader, DrawerFooter, Input, Skeleton } from '@heroui/react'
import { Boxes, Check, ChevronRight, Cpu, RefreshCw, Search, SearchX } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { findProviderLogo } from '@/components/brand/model-logos'
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
      <Drawer
        backdrop="blur"
        classNames={{ base: 'w-full max-h-none sm:max-w-md' }}
        isOpen={open}
        placement="right"
        scrollBehavior="inside"
        onOpenChange={setOpen}
      >
        {/* HeroUI 没有 SheetTrigger；按钮单独渲染，抽屉由状态控制。 */}
        <DrawerContent>
          {() => (
            <>
              <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
                <h2 className="text-base font-medium text-foreground">{t('apiKeys.modelSelector.title')}</h2>
                <p className="text-sm text-muted-foreground">{t('apiKeys.modelSelector.description')}</p>
              </DrawerHeader>
              <div className="border-b border-[var(--hairline)] p-3">
                <Input
                  autoFocus
                  classNames={{ input: 'pl-8' }}
                  isClearable
                  placeholder={t('apiKeys.modelSelector.placeholder')}
                  size="sm"
                  startContent={<Search className="size-3.5 text-muted-foreground" aria-hidden="true" />}
                  type="search"
                  value={searchInput}
                  onValueChange={setSearchInput}
                />
              </div>
              <DrawerBody className="min-h-0 flex-1 gap-0 overflow-y-auto p-2">
                <div role="listbox" aria-label={t('apiKeys.modelSelector.title')} className="min-h-0">
                  {catalogQuery.isPending ? (
                    <div className="grid gap-1.5 p-1" aria-label={t('apiKeys.modelSelector.loading')}>
                      {[0, 1, 2, 3, 4, 5].map((item) => <Skeleton key={item} className="h-11 rounded-lg" />)}
                    </div>
                  ) : unavailable ? (
                    <SelectorMessage icon={RefreshCw} message={t('apiKeys.modelSelector.error')}>
                      <Button type="button" size="sm" variant="bordered" onClick={() => void catalogQuery.refetch()}>
                        {t('apiKeys.actions.retry')}
                      </Button>
                    </SelectorMessage>
                  ) : models.length === 0 ? (
                    <SelectorMessage icon={SearchX} message={t('apiKeys.modelSelector.empty')} />
                  ) : (
                    <div className="grid gap-0.5">
                      {models.map((item) => {
                        const selected = values.includes(item.model)
                        const logo = findProviderLogo(item.provider)
                        const ProviderIcon = logo?.Icon ?? Cpu
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
                            {/* 只使用模型商品里的权威厂商字段与显式图标，不按模型名猜测归属。 */}
                            <span className="relative grid size-7 shrink-0 place-items-center overflow-hidden rounded-full bg-surface-2 text-muted-foreground">
                              <ProviderIcon className="size-4" aria-hidden="true" />
                              {item.icon_url ? (
                                <img
                                  key={item.icon_url}
                                  src={item.icon_url}
                                  alt=""
                                  className="absolute inset-0 size-full object-cover"
                                  onError={(event) => { event.currentTarget.hidden = true }}
                                />
                              ) : null}
                            </span>
                            <span className="min-w-0 flex-1 truncate font-mono text-xs">{item.model}</span>
                            <Chip className="shrink-0 bg-surface-2 text-muted-foreground" size="sm" variant="flat">
                              {t(`models.billing.${item.billing_mode}`)}
                            </Chip>
                            <Check className={cn('size-3.5 shrink-0 text-info', !selected && 'opacity-0')} aria-hidden="true" />
                          </button>
                        )
                      })}
                      {catalogQuery.hasNextPage ? (
                        <Button
                          type="button"
                          size="sm"
                          variant="light"
                          className="mt-2 justify-center"
                          isDisabled={catalogQuery.isFetchingNextPage}
                          onClick={() => void catalogQuery.fetchNextPage()}
                        >
                          {t(catalogQuery.isFetchingNextPage ? 'apiKeys.modelSelector.loadingMore' : 'apiKeys.actions.loadMore')}
                        </Button>
                      ) : null}
                    </div>
                  )}
                </div>
              </DrawerBody>
              <DrawerFooter className="border-t border-[var(--hairline)] p-3">
                <Button type="button" className="w-full" color="primary" onClick={() => setOpen(false)}>
                  <Check className="size-4" aria-hidden="true" />{t('apiKeys.actions.done')}
                </Button>
              </DrawerFooter>
            </>
          )}
        </DrawerContent>
      </Drawer>
      <Button
        id="api-key-models"
        type="button"
        variant="bordered"
        className="h-auto min-h-9 w-full justify-between px-2.5 py-2 text-left"
        aria-invalid={!!error}
        onClick={() => setOpen(true)}
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
      {values.length > 0 ? (
        <div className="flex min-w-0 gap-1.5 overflow-hidden" aria-label={t('apiKeys.modelSelector.selectedLabel')}>
          {values.map((model) => (
            <Chip
              key={model}
              classNames={{
                base: 'max-w-40 shrink-0 !flex-nowrap bg-surface-2 font-mono text-muted-foreground',
                content: 'min-w-0 truncate whitespace-nowrap',
                closeButton: 'inline-flex size-4 shrink-0 self-center items-center justify-center [&>svg]:block',
              }}
              size="sm"
              variant="flat"
              onClose={() => toggle(model)}
            >
              {model}
            </Chip>
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
