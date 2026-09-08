import { Filter, RefreshCw, Search, SlidersHorizontal } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useCursorPagination } from '@/components/data-table/use-table-pagination'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { useAdminDebugTraces, type DebugTraceFilters } from './debug-trace-api'
import { DebugTraceDetailSheet } from './debug-trace-detail-sheet'
import { DebugTraceList } from './debug-trace-list'
import { DebugTraceSettingsSheet } from './debug-trace-settings-sheet'

const EMPTY_TRACES = [] as const

function requestIdFromHash() {
  const queryStart = window.location.hash.indexOf('?')
  if (queryStart < 0) return undefined
  const values = new URLSearchParams(window.location.hash.slice(queryStart + 1)).getAll('request_id')
  if (values.length !== 1 || values[0].length === 0 || values[0].length > 64 || values[0].trim() !== values[0]) return undefined
  return values[0]
}

/** 组合服务端筛选、稳定游标分页和按需详情侧栏的调试追踪工作台。 */
export function DebugTracePage() {
  const { t } = useTranslation()
  const pagination = useCursorPagination<number>()
  const [filters, setFilters] = useState<DebugTraceFilters>(() => {
    const requestId = requestIdFromHash()
    return requestId ? { requestId } : {}
  })
  const [modelDraft, setModelDraft] = useState('')
  const [requestIdDraft, setRequestIdDraft] = useState(() => requestIdFromHash() ?? '')
  const [outcomeDraft, setOutcomeDraft] = useState('all')
  const [selectedId, setSelectedId] = useState<number>()
  const tracesQuery = useAdminDebugTraces(pagination.cursor, filters, pagination.pageSize)
  const traces = tracesQuery.data?.traces ?? EMPTY_TRACES
  const nextCursor = tracesQuery.data?.next_cursor

  const applyFilters = () => {
    pagination.reset()
    setSelectedId(undefined)
    setFilters({
      outcome: outcomeDraft === 'all' ? undefined : outcomeDraft as 'succeeded' | 'failed',
      model: modelDraft.trim() || undefined,
      requestId: requestIdDraft.trim() || undefined,
    })
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('debugTraces.title')}</h2>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">{t('debugTraces.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2 self-start sm:self-auto">
          <Button type="button" size="sm" variant="secondary" disabled={tracesQuery.isFetching} onClick={() => void tracesQuery.refetch()}>
            <RefreshCw className={tracesQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('debugTraces.actions.refresh')}
          </Button>
          <DebugTraceSettingsSheet />
        </div>
      </header>

      <form className="rounded-lg border border-[var(--hairline)] bg-surface-1/45 p-2.5" onSubmit={(event) => { event.preventDefault(); applyFilters() }}>
        <div className="mb-2 flex items-center justify-between gap-3 px-0.5 text-[0.6875rem] text-muted-foreground">
          <span className="inline-flex items-center gap-1.5"><SlidersHorizontal className="size-3.5" aria-hidden="true" />{t('debugTraces.filters.label')}</span>
          <span>{t('debugTraces.pageSummary', { page: pagination.currentPage, count: traces.length })}</span>
        </div>
        <div className="grid gap-2 lg:grid-cols-[10rem_minmax(12rem,1fr)_minmax(14rem,1fr)_auto]">
          <label className="relative">
            <span className="sr-only">{t('debugTraces.filters.outcome')}</span>
            <Filter className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
            <Select className="h-8 pl-8 text-xs" value={outcomeDraft} onChange={(event) => setOutcomeDraft(event.target.value)}>
              <option value="all">{t('debugTraces.filters.all')}</option>
              <option value="succeeded">{t('debugTraces.outcome.succeeded')}</option>
              <option value="failed">{t('debugTraces.outcome.failed')}</option>
            </Select>
          </label>
          <label className="relative">
            <span className="sr-only">{t('debugTraces.filters.model')}</span>
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
            <Input className="h-8 pl-8 text-xs" value={modelDraft} placeholder={t('debugTraces.filters.modelPlaceholder')} onChange={(event) => setModelDraft(event.target.value)} />
          </label>
          <label className="relative">
            <span className="sr-only">{t('debugTraces.filters.requestId')}</span>
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
            <Input className="h-8 pl-8 font-mono text-xs" value={requestIdDraft} placeholder={t('debugTraces.filters.requestIdPlaceholder')} onChange={(event) => setRequestIdDraft(event.target.value)} />
          </label>
          <Button type="submit" size="sm" variant="secondary">{t('debugTraces.actions.filter')}</Button>
        </div>
      </form>

      {tracesQuery.isPending ? (
        <div className="overflow-hidden rounded-lg border border-[var(--hairline)]" aria-label={t('debugTraces.loading')}>
          <Skeleton className="h-9 rounded-none" />
          {[0, 1, 2, 3, 4, 5].map((item) => <Skeleton key={item} className="h-[4.25rem] rounded-none border-t border-[var(--hairline)]" />)}
        </div>
      ) : tracesQuery.isError ? (
        <div className="rounded-lg border border-destructive/25 bg-destructive/8 p-4" role="alert">
          <h3 className="text-sm font-semibold text-destructive">{t('debugTraces.error.title')}</h3>
          <p className="mt-1 text-xs text-muted-foreground">{t('debugTraces.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void tracesQuery.refetch()}>{t('debugTraces.actions.retry')}</Button>
        </div>
      ) : <DebugTraceList traces={traces} selectedId={selectedId} onSelect={setSelectedId} />}

      {!tracesQuery.isPending && !tracesQuery.isError ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={traces.length}
          hasNextPage={nextCursor != null || pagination.hasLoadedNextPage}
          fetching={tracesQuery.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={() => pagination.goToNextPage(nextCursor)}
          onPageSizeChange={(value) => { pagination.setPageSize(value); setSelectedId(undefined) }}
        />
      ) : null}

      <DebugTraceDetailSheet traceId={selectedId} onOpenChange={(open) => !open && setSelectedId(undefined)} />
    </div>
  )
}
