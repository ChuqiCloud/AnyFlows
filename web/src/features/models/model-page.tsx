import { RefreshCw, Search } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { ModelCatalogList } from './model-catalog-list'
import { type ModelBillingFilter, useModelCatalog } from './model-api'

type ModelPageProps = {
  authenticated: boolean
  standalone?: boolean
}

export function ModelPage({ authenticated, standalone = false }: ModelPageProps) {
  const { t } = useTranslation()
  const [searchInput, setSearchInput] = useState('')
  const [search, setSearch] = useState('')
  const [billingMode, setBillingMode] = useState<ModelBillingFilter>('all')

  useEffect(() => {
    // 短延迟合并连续输入，避免每个按键都创建一条目录请求。
    const timer = window.setTimeout(() => setSearch(searchInput.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [searchInput])

  const catalogQuery = useModelCatalog(search, billingMode, authenticated)
  const models = useMemo(
    () => catalogQuery.data?.pages.flatMap((page) => page.models) ?? [],
    [catalogQuery.data],
  )
  const filtered = search.length > 0 || billingMode !== 'all'
  const initialError = catalogQuery.isError && models.length === 0
  const pricingScope = catalogQuery.data?.pages[0]?.pricing_scope
  const Heading = standalone ? 'h1' : 'h2'

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <Heading className="text-lg font-semibold">{t('models.title')}</Heading>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t(pricingScope ? `models.scope.${pricingScope}.subtitle` : 'models.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          className="self-start sm:self-auto"
          disabled={catalogQuery.isFetching}
          onClick={() => void catalogQuery.refetch()}
        >
          <RefreshCw className={catalogQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('models.actions.refresh')}
        </Button>
      </header>

      <div className="grid gap-2 rounded-xl border border-[var(--hairline)] bg-surface-1 p-2.5 sm:grid-cols-[minmax(0,1fr)_11rem]">
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground">
          <span>{t('models.filters.search')}</span>
          <span className="relative block">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
            <Input
              type="search"
              className="pl-8"
              value={searchInput}
              placeholder={t('models.filters.searchPlaceholder')}
              onChange={(event) => setSearchInput(event.target.value)}
            />
          </span>
        </label>
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground">
          <span>{t('models.filters.billingMode')}</span>
          <Select
            value={billingMode}
            onChange={(event) => setBillingMode(event.target.value as ModelBillingFilter)}
          >
            <option value="all">{t('models.filters.all')}</option>
            <option value="per_token">{t('models.billing.per_token')}</option>
            <option value="free">{t('models.billing.free')}</option>
          </Select>
        </label>
      </div>

      <div className="flex min-h-8 items-center justify-between gap-3 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span aria-live="polite">
          {catalogQuery.isPending
            ? t('models.loading')
            : t('models.loadedCount', { count: models.length })}
        </span>
        <Badge className="shrink-0 bg-surface-2 text-right text-muted-foreground">
          {t(pricingScope ? `models.scope.${pricingScope}.label` : 'models.scope.pending')}
        </Badge>
      </div>

      {initialError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('models.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('models.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void catalogQuery.refetch()}>
            {t('models.actions.retry')}
          </Button>
        </div>
      ) : (
        <ModelCatalogList
          models={models}
          loading={catalogQuery.isPending}
          filtered={filtered}
          pricingScope={pricingScope}
        />
      )}

      {catalogQuery.isFetchNextPageError ? (
        <div
          role="alert"
          className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2 text-xs text-muted-foreground"
        >
          <span>{t('models.error.moreBody')}</span>
          <Button type="button" size="xs" variant="ghost" onClick={() => void catalogQuery.fetchNextPage()}>
            {t('models.actions.retry')}
          </Button>
        </div>
      ) : null}

      {catalogQuery.isFetchingNextPage ? (
        <div className="grid gap-2" aria-label={t('models.loadingMore')}>
          {[0, 1].map((item) => <Skeleton key={item} className="h-16 rounded-xl" />)}
        </div>
      ) : null}

      {!initialError && catalogQuery.hasNextPage ? (
        <div className="flex justify-center">
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={catalogQuery.isFetchingNextPage}
            onClick={() => void catalogQuery.fetchNextPage()}
          >
            {t('models.actions.loadMore')}
          </Button>
        </div>
      ) : null}
    </div>
  )
}
