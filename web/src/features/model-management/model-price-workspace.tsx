import { useMemo, useState } from 'react'
import { CheckCircle2, CloudDownload, RefreshCw, ShieldCheck, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { DataTablePagination, DEFAULT_TABLE_PAGE_SIZE } from '@/components/data-table/data-table-pagination'
import { useLoadedCursorPagination } from '@/components/data-table/use-table-pagination'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminModelPriceSourcePreview } from '@/lib/api/generated/types.gen'
import { useAdminModelMetadata } from './model-management-api'
import {
  modelPriceSources,
  modelPriceErrorCode,
  useAdminModelPrices,
  useApplyAdminModelPrices,
  usePreviewAdminModelPrices,
  type ModelPriceSource,
} from './model-price-api'
import { ModelPriceApplyDialog, type ModelPriceApplyItem } from './model-price-apply-dialog'
import { ModelPriceEditorSheet, type ModelPriceEditorTarget } from './model-price-editor-sheet'
import {
  modelPriceDraftIsComplete,
  toModelPriceWriteItem,
  type ModelPriceDraft,
  type StagedModelPriceDraft,
} from './model-price-form-model'
import { ModelPriceTable, type ModelPriceRow } from './model-price-table'

/** 组合权威模型、正式价格与公开来源证据，形成可批量提交的管理工作区。 */
export function ModelPriceWorkspace() {
  const { t } = useTranslation()
  const [pageSize, setPageSize] = useState(DEFAULT_TABLE_PAGE_SIZE)
  const modelsQuery = useAdminModelMetadata(pageSize)
  const pagination = useLoadedCursorPagination({
    availablePageCount: modelsQuery.data?.pages.length ?? 1,
    pageSize,
    onPageSizeChange: setPageSize,
  })
  const pricesQuery = useAdminModelPrices()
  const previewMutation = usePreviewAdminModelPrices()
  const applyMutation = useApplyAdminModelPrices()
  const [priceSource, setPriceSource] = useState<ModelPriceSource>('models_dev')
  const [previews, setPreviews] = useState<Partial<Record<ModelPriceSource, AdminModelPriceSourcePreview>>>({})
  const [staged, setStaged] = useState<Record<string, StagedModelPriceDraft>>({})
  const [selected, setSelected] = useState<Set<string>>(() => new Set())
  const [editingModel, setEditingModel] = useState<string>()
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [lastAppliedCount, setLastAppliedCount] = useState<number>()

  const preview = previews[priceSource]
  const models = useMemo(() => modelsQuery.data?.pages.flatMap((page) => page.models) ?? [], [modelsQuery.data])
  const currentModels = useMemo(
    () => modelsQuery.data?.pages[pagination.pageIndex]?.models ?? [],
    [modelsQuery.data, pagination.pageIndex],
  )
  const prices = useMemo(() => pricesQuery.data?.pages.flatMap((page) => page.prices) ?? [], [pricesQuery.data])
  const priceByModel = useMemo(() => new Map(prices.map((price) => [price.model, price])), [prices])
  const candidateByModel = useMemo(() => new Map(preview?.candidates.map((candidate) => [candidate.model, candidate]) ?? []), [preview])
  const rows = useMemo<ModelPriceRow[]>(() => models.map((model) => ({
    model,
    price: priceByModel.get(model.model),
    candidate: candidateByModel.get(model.model),
    source: candidateByModel.has(model.model) ? priceSource : undefined,
    staged: staged[model.model],
  })), [candidateByModel, models, priceByModel, priceSource, staged])
  const rowByModel = useMemo(() => new Map(rows.map((row) => [row.model.model, row])), [rows])
  const visibleRows = useMemo(() => currentModels.flatMap((model) => {
    const row = rowByModel.get(model.model)
    return row ? [row] : []
  }), [currentModels, rowByModel])
  const priceIndexComplete = !pricesQuery.hasNextPage
  const pricedCount = rows.filter((row) => row.price !== undefined).length
  const freeCount = rows.filter((row) => row.price?.billing_mode === 'free').length
  const applyItems = useMemo<ModelPriceApplyItem[]>(() => [...selected]
    .sort()
    .flatMap((model) => {
      const draft = staged[model]
      const currentPrice = rowByModel.get(model)?.price
      const versionReady = currentPrice === undefined || draft?.expectedVersion === currentPrice.version
      return draft && versionReady && modelPriceDraftIsComplete(draft.draft) ? [{ model, staged: draft }] : []
    }), [rowByModel, selected, staged])
  const editorTarget = editingModel ? rowToEditorTarget(rowByModel.get(editingModel)) : undefined
  const initialError = (modelsQuery.isError && modelsQuery.data === undefined)
    || (pricesQuery.isError && pricesQuery.data === undefined)

  const createPreview = async () => {
    const requestedSource = priceSource
    try {
      const next = await previewMutation.mutateAsync(requestedSource)
      setPreviews((current) => ({ ...current, [requestedSource]: next }))
    } catch {
      // 已编辑草稿和上一份成功预览保持不变，管理员可以按稳定错误分类重试。
    }
  }
  const changePriceSource = (source: ModelPriceSource) => {
    previewMutation.reset()
    setPriceSource(source)
  }
  const stageDraft = (model: string, draft: ModelPriceDraft, expectedVersion: number | null, contextWindow: number | null) => {
    setStaged((current) => ({
      ...current,
      [model]: { draft, expectedVersion, contextWindow },
    }))
    setSelected((current) => new Set(current).add(model))
    setEditingModel(undefined)
  }
  const apply = async () => {
    if (applyItems.length === 0) return
    try {
      const result = await applyMutation.mutateAsync({
        items: applyItems.map(({ model, staged: item }) => toModelPriceWriteItem(model, item)),
      })
      const applied = new Set(result.prices.map((price) => price.model))
      setStaged((current) => Object.fromEntries(Object.entries(current).filter(([model]) => !applied.has(model))))
      setSelected((current) => new Set([...current].filter((model) => !applied.has(model))))
      setLastAppliedCount(result.prices.length)
      setConfirmOpen(false)
    } catch {
      // 乐观版本或刷新失败时保留整批草稿与选择，供管理员检查后重试。
    }
  }
  const refresh = () => {
    void Promise.all([modelsQuery.refetch(), pricesQuery.refetch()])
  }
  const openConfirm = () => {
    applyMutation.reset()
    setConfirmOpen(true)
  }

  return (
    <div className="grid gap-4">
      <div className="flex flex-col gap-3 border-y border-[var(--hairline)] py-2 lg:flex-row lg:items-center lg:justify-between">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
          <span>{t('modelManagement.prices.summary.loaded', { count: rows.length })}</span>
          <span>{t('modelManagement.prices.summary.priced', { count: pricedCount })}</span>
          <span>{t('modelManagement.prices.summary.free', { count: freeCount })}</span>
          <span>{t('modelManagement.prices.summary.staged', { count: Object.keys(staged).length })}</span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" size="sm" variant="ghost" disabled={modelsQuery.isFetching || pricesQuery.isFetching} onClick={refresh}>
            <RefreshCw className={modelsQuery.isFetching || pricesQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('modelManagement.actions.refresh')}
          </Button>
          <label htmlFor="model-price-source" className="sr-only">{t('modelManagement.prices.sources.label')}</label>
          <Select
            id="model-price-source"
            className="w-40 text-xs"
            value={priceSource}
            disabled={previewMutation.isPending}
            onChange={(event) => changePriceSource(event.target.value as ModelPriceSource)}
          >
            {modelPriceSources.map((source) => (
              <option key={source} value={source}>{t(`modelManagement.prices.sources.${source}`)}</option>
            ))}
          </Select>
          <Button type="button" size="sm" variant="secondary" disabled={previewMutation.isPending} onClick={() => void createPreview()}>
            <CloudDownload className={previewMutation.isPending ? 'animate-pulse' : undefined} aria-hidden="true" />
            {t(previewMutation.isPending ? 'modelManagement.prices.actions.previewing' : 'modelManagement.prices.actions.preview')}
          </Button>
          <Button type="button" size="sm" disabled={applyItems.length === 0} onClick={openConfirm}>
            <ShieldCheck aria-hidden="true" />{t('modelManagement.prices.actions.reviewApply', { count: applyItems.length })}
          </Button>
        </div>
      </div>

      {preview ? (
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground" role="status">
          <Badge>{t(`modelManagement.prices.sources.${priceSource}`)}</Badge>
          <span>{t('modelManagement.prices.preview.loaded', { count: preview.candidates.length })}</span>
          <span>{t('modelManagement.prices.preview.fetchedAt', { value: new Date(preview.fetched_at * 1000).toLocaleString() })}</span>
          {preview.revision ? <span className="max-w-64 truncate font-mono">{preview.revision}</span> : null}
        </div>
      ) : null}
      {previewMutation.error && previewMutation.variables === priceSource ? (
        <ModelPriceError
          code={modelPriceErrorCode(previewMutation.error) ?? 'unknown'}
          source={t(`modelManagement.prices.sources.${priceSource}`)}
        />
      ) : null}
      {lastAppliedCount !== undefined ? (
        <div role="status" className="flex items-start gap-2 rounded-xl border border-success/20 bg-success/8 p-3 text-xs text-success">
          <CheckCircle2 className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <span>{t('modelManagement.prices.apply.success', { count: lastAppliedCount })}</span>
        </div>
      ) : null}

      {!priceIndexComplete ? (
        <div className="flex flex-col gap-2 rounded-xl border border-info/20 bg-info/8 p-3 text-xs sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-start gap-2 text-info">
            <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
            <span>{t('modelManagement.prices.pagination.priceIndexIncomplete', { count: prices.length })}</span>
          </div>
          <Button type="button" size="sm" variant="secondary" disabled={pricesQuery.isFetchingNextPage} onClick={() => void pricesQuery.fetchNextPage()}>
            {t(pricesQuery.isFetchingNextPage ? 'modelManagement.loadingMore' : 'modelManagement.prices.pagination.loadMorePrices')}
          </Button>
        </div>
      ) : null}

      {modelsQuery.isPending || pricesQuery.isPending ? (
        <div className="grid gap-2" aria-label={t('modelManagement.prices.loading')}>{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-24 rounded-xl" />)}</div>
      ) : initialError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h3 className="text-sm font-semibold text-destructive">{t('modelManagement.prices.error.title')}</h3>
          <p className="mt-1 text-xs text-muted-foreground">{t('modelManagement.prices.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={refresh}>{t('modelManagement.actions.retry')}</Button>
        </div>
      ) : (
        <ModelPriceTable
          priceIndexComplete={priceIndexComplete}
          rows={visibleRows}
          selected={selected}
          onEdit={(row) => setEditingModel(row.model.model)}
          onSelect={(model, checked) => setSelected((current) => {
            const next = new Set(current)
            if (checked) next.add(model)
            else next.delete(model)
            return next
          })}
        />
      )}

      {(modelsQuery.isFetchNextPageError || pricesQuery.isFetchNextPageError) ? (
        <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('modelManagement.prices.pagination.moreError')}</p>
      ) : null}
      {!modelsQuery.isPending && !initialError ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={visibleRows.length}
          hasNextPage={pagination.hasLoadedNextPage || Boolean(modelsQuery.hasNextPage)}
          fetching={modelsQuery.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={() => void pagination.goToNextPage(Boolean(modelsQuery.hasNextPage), async () => (await modelsQuery.fetchNextPage()).isSuccess)}
          onPageSizeChange={pagination.setPageSize}
        />
      ) : null}

      <ModelPriceEditorSheet
        target={editorTarget}
        onOpenChange={(open) => !open && setEditingModel(undefined)}
        onStage={stageDraft}
      />
      <ModelPriceApplyDialog
        errorCode={applyMutation.error ? modelPriceErrorCode(applyMutation.error) ?? 'unknown' : undefined}
        items={applyItems}
        open={confirmOpen}
        pending={applyMutation.isPending}
        onApply={() => void apply()}
        onOpenChange={setConfirmOpen}
      />
    </div>
  )
}

function rowToEditorTarget(row?: ModelPriceRow): ModelPriceEditorTarget | undefined {
  return row ? {
    model: row.model,
    price: row.price,
    candidate: row.candidate,
    source: row.source,
    staged: row.staged,
  } : undefined
}

function ModelPriceError({ code, source }: { code: string; source: string }) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="flex items-start gap-2 rounded-xl border border-destructive/20 bg-destructive/8 p-3 text-xs text-destructive">
      <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
      <span>{t(`modelManagement.prices.errors.${code}`, { defaultValue: t('modelManagement.prices.errors.unknown'), source })}</span>
    </div>
  )
}
