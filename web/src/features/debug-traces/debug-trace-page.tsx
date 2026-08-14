import { ChevronLeft, ChevronRight, Filter, RefreshCw, Search } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { useAdminDebugTrace, useAdminDebugTraces, type DebugTraceFilters } from './debug-trace-api'
import { DebugTraceDetail } from './debug-trace-detail'
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

/** 组合筛选、稳定游标、主从选择和设置入口的调试追踪工作台。 */
export function DebugTracePage() {
  const { t } = useTranslation()
  const [cursorStack, setCursorStack] = useState<Array<number | undefined>>([undefined])
  const [filters, setFilters] = useState<DebugTraceFilters>(() => {
    const requestId = requestIdFromHash()
    return requestId ? { requestId } : {}
  })
  const [modelDraft, setModelDraft] = useState('')
  const [requestIdDraft, setRequestIdDraft] = useState(() => requestIdFromHash() ?? '')
  const [outcomeDraft, setOutcomeDraft] = useState('all')
  const [selectedId, setSelectedId] = useState<number>()
  const cursor = cursorStack.at(-1)
  const tracesQuery = useAdminDebugTraces(cursor, filters)
  const traces = tracesQuery.data?.traces ?? EMPTY_TRACES
  const detailQuery = useAdminDebugTrace(selectedId)
  const nextCursor = tracesQuery.data?.next_cursor

  useEffect(() => {
    if (traces.length === 0) {
      setSelectedId(undefined)
      return
    }
    if (!traces.some((trace) => trace.id === selectedId)) {
      setSelectedId(traces[0].id)
    }
  }, [selectedId, traces])

  const applyFilters = () => {
    setCursorStack([undefined])
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
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('debugTraces.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={tracesQuery.isFetching} onClick={() => void tracesQuery.refetch()}>
            <RefreshCw className={tracesQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('debugTraces.actions.refresh')}
          </Button>
          <DebugTraceSettingsSheet />
        </div>
      </header>

      <form className="grid gap-2 rounded-lg border border-[var(--hairline)] bg-surface-1/45 p-2.5 lg:grid-cols-[10rem_minmax(12rem,1fr)_minmax(14rem,1fr)_auto]" onSubmit={(event) => { event.preventDefault(); applyFilters() }}>
        <div className="relative">
          <Filter className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
          <Select aria-label={t('debugTraces.filters.outcome')} className="pl-8" value={outcomeDraft} onChange={(event) => setOutcomeDraft(event.target.value)}>
            <option value="all">{t('debugTraces.filters.all')}</option>
            <option value="succeeded">{t('debugTraces.outcome.succeeded')}</option>
            <option value="failed">{t('debugTraces.outcome.failed')}</option>
          </Select>
        </div>
        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
          <Input aria-label={t('debugTraces.filters.model')} className="pl-8" value={modelDraft} placeholder={t('debugTraces.filters.modelPlaceholder')} onChange={(event) => setModelDraft(event.target.value)} />
        </div>
        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
          <Input aria-label={t('debugTraces.filters.requestId')} className="pl-8 font-mono text-xs" value={requestIdDraft} placeholder={t('debugTraces.filters.requestIdPlaceholder')} onChange={(event) => setRequestIdDraft(event.target.value)} />
        </div>
        <Button type="submit" size="sm" variant="secondary">{t('debugTraces.actions.filter')}</Button>
      </form>

      <div className="overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1/35 xl:grid xl:min-h-[38rem] xl:grid-cols-[minmax(20rem,0.82fr)_minmax(28rem,1.18fr)]">
        <section className="border-b border-[var(--hairline)] xl:border-b-0 xl:border-r" aria-label={t('debugTraces.listLabel')}>
          <div className="flex min-h-10 items-center justify-between border-b border-[var(--hairline)] px-3 text-xs text-muted-foreground">
            <span>{t('debugTraces.count', { count: traces.length })}</span>
            <span>{t('debugTraces.page', { page: cursorStack.length })}</span>
          </div>
          {tracesQuery.isPending ? <div className="grid gap-2 p-3" aria-label={t('debugTraces.loading')}>{[0, 1, 2, 3, 4].map((item) => <Skeleton key={item} className="h-24 rounded-lg" />)}</div>
            : tracesQuery.isError ? <div className="m-3 rounded-lg border border-destructive/25 bg-destructive/8 p-4" role="alert"><h3 className="text-sm font-semibold text-destructive">{t('debugTraces.error.title')}</h3><p className="mt-1 text-xs text-muted-foreground">{t('debugTraces.error.body')}</p><Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void tracesQuery.refetch()}>{t('debugTraces.actions.retry')}</Button></div>
              : <DebugTraceList traces={traces} selectedId={selectedId} onSelect={setSelectedId} />}

          {!tracesQuery.isPending && !tracesQuery.isError && (cursorStack.length > 1 || nextCursor != null) ? (
            <nav className="flex items-center justify-center gap-2 border-t border-[var(--hairline)] p-2" aria-label={t('debugTraces.pagination')}>
              <Button type="button" size="icon-sm" variant="ghost" aria-label={t('debugTraces.actions.previous')} disabled={cursorStack.length === 1 || tracesQuery.isFetching} onClick={() => setCursorStack((current) => current.slice(0, -1))}><ChevronLeft aria-hidden="true" /></Button>
              <Button type="button" size="icon-sm" variant="ghost" aria-label={t('debugTraces.actions.next')} disabled={nextCursor == null || tracesQuery.isFetching} onClick={() => nextCursor != null && setCursorStack((current) => [...current, nextCursor])}><ChevronRight aria-hidden="true" /></Button>
            </nav>
          ) : null}
        </section>

        <section aria-label={t('debugTraces.detailLabel')}>
          <DebugTraceDetail detail={detailQuery.data} pending={selectedId !== undefined && detailQuery.isPending} error={detailQuery.isError} onRetry={() => void detailQuery.refetch()} />
        </section>
      </div>
    </div>
  )
}
