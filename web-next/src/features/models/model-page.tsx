import { Button, Input, Skeleton } from '@heroui/react'
import { RefreshCw, Search } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import { ModelCatalogDesktopFilters, ModelCatalogMobileFilters, countModelCatalogFilters } from './model-catalog-filters'
import { ModelCatalogList } from './model-catalog-list'
import { EMPTY_MODEL_CATALOG_FILTERS, useModelCatalog, useModelCatalogProviders } from './model-api'

type ModelPageProps = {
  authenticated: boolean
  admin?: boolean
  standalone?: boolean
}

export function ModelPage({ authenticated, admin = false, standalone = false }: ModelPageProps) {
  const { t } = useTranslation()
  const [searchInput, setSearchInput] = useState('')
  const [search, setSearch] = useState('')
  const [filters, setFilters] = useState(EMPTY_MODEL_CATALOG_FILTERS)

  useEffect(() => {
    // 短延迟合并连续输入，避免每个按键都创建一条目录请求。
    const timer = window.setTimeout(() => setSearch(searchInput.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [searchInput])

  const catalogQuery = useModelCatalog(search, filters, authenticated)
  const providersQuery = useModelCatalogProviders(authenticated)
  const models = useMemo(
    () => catalogQuery.data?.pages.flatMap((page) => page.models) ?? [],
    [catalogQuery.data],
  )
  const activeFilterCount = countModelCatalogFilters(filters)
  const filtered = search.length > 0 || activeFilterCount > 0
  const initialError = catalogQuery.isError && models.length === 0
  const pricingScope = catalogQuery.data?.pages[0]?.pricing_scope
  const providers = providersQuery.data?.providers ?? []
  const filterProps = {
    authenticated,
    filters,
    providers,
    providersLoading: providersQuery.isPending,
    providersError: providersQuery.isError,
    onChange: setFilters,
    onRetryProviders: () => { void providersQuery.refetch() },
  }
  const Heading = standalone ? 'h1' : 'h2'

  return (
    <div className="flex flex-col gap-5">
      <header className="flex flex-col gap-3 border-b border-[var(--hairline)] pb-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="max-w-3xl">
          <Heading className="text-2xl font-semibold leading-tight tracking-tight">{t('models.title')}</Heading>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">
            {t(pricingScope ? `models.scope.${pricingScope}.subtitle` : 'models.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="bordered"
          className="self-start sm:self-auto"
          isDisabled={catalogQuery.isFetching}
          onClick={() => void catalogQuery.refetch()}
        >
          <RefreshCw className={cn('size-3.5', catalogQuery.isFetching && 'animate-spin')} aria-hidden="true" />
          {t('models.actions.refresh')}
        </Button>
      </header>

      <div className="flex items-start gap-0">
        <ModelCatalogDesktopFilters {...filterProps} />

        <section className="min-w-0 flex-1 lg:border-l lg:border-[var(--hairline)] lg:pl-5">
          <div className="flex flex-col gap-3 border-b border-[var(--hairline)] pb-4 sm:flex-row sm:items-center">
            <label className="relative block min-w-0 flex-1">
              <span className="sr-only">{t('models.filters.search')}</span>
              <Input
                classNames={{ input: 'pl-9', inputWrapper: 'h-10' }}
                placeholder={t('models.filters.searchPlaceholder')}
                size="md"
                startContent={<Search className="size-4 text-muted-foreground" aria-hidden="true" />}
                type="search"
                value={searchInput}
                onValueChange={setSearchInput}
              />
            </label>
            <ModelCatalogMobileFilters {...filterProps} />
          </div>

          <div className="flex min-h-11 flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
            <span aria-live="polite">
              {catalogQuery.isPending
                ? t('models.loading')
                : t('models.loadedCount', { count: models.length })}
            </span>
            <span>{t(pricingScope ? `models.scope.${pricingScope}.label` : 'models.scope.pending')}</span>
          </div>

          {initialError ? (
            <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
              <h2 className="text-sm font-semibold text-destructive">{t('models.error.title')}</h2>
              <p className="mt-1 text-xs text-muted-foreground">{t('models.error.body')}</p>
              <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={() => void catalogQuery.refetch()}>
                {t('models.actions.retry')}
              </Button>
            </div>
          ) : (
            <ModelCatalogList
              models={models}
              loading={catalogQuery.isPending}
              filtered={filtered}
              pricingScope={pricingScope}
              admin={admin}
            />
          )}

          {catalogQuery.isFetchNextPageError ? (
            <div
              role="alert"
              className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2 text-xs text-muted-foreground"
            >
              <span>{t('models.error.moreBody')}</span>
              <Button type="button" size="sm" variant="light" onClick={() => void catalogQuery.fetchNextPage()}>
                {t('models.actions.retry')}
              </Button>
            </div>
          ) : null}

          {catalogQuery.isFetchingNextPage ? (
            <div className="grid gap-2" aria-label={t('models.loadingMore')}>
              {[0, 1].map((item) => <Skeleton key={item} className="h-16 rounded-lg" />)}
            </div>
          ) : null}

          {!initialError && catalogQuery.hasNextPage ? (
            <div className="flex justify-center">
              <Button
                type="button"
                size="sm"
                variant="bordered"
                isDisabled={catalogQuery.isFetchingNextPage}
                onClick={() => void catalogQuery.fetchNextPage()}
              >
                {t('models.actions.loadMore')}
              </Button>
            </div>
          ) : null}
        </section>
      </div>
    </div>
  )
}
