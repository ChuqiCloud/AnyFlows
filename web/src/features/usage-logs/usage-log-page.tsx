import { useMemo, useState } from 'react'
import { ChevronLeft, ChevronRight, Filter, RefreshCw, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { useAdminUsageLogs, useUserUsageLogs } from './usage-log-api'
import { UsageLogTable, type UsageLogRow } from './usage-log-table'

type UsageLogPageProps = {
  admin: boolean
}

export function UsageLogPage({ admin }: UsageLogPageProps) {
  const { t } = useTranslation()
  const [cursorStack, setCursorStack] = useState<Array<number | undefined>>([undefined])
  const [modelFilter, setModelFilter] = useState('')
  const [protocolFilter, setProtocolFilter] = useState('all')
  const cursor = cursorStack.at(-1)
  const adminQuery = useAdminUsageLogs(admin ? cursor : undefined)
  const userQuery = useUserUsageLogs(admin ? undefined : cursor)
  const logsQuery = admin ? adminQuery : userQuery
  const nextCursor = logsQuery.data?.next_cursor
  const filteredLogs = useMemo(() => {
    const logs = (logsQuery.data?.logs ?? []) as UsageLogRow[]
    const normalizedModel = modelFilter.trim().toLocaleLowerCase()
    return logs.filter((log) => {
      const modelMatches = !normalizedModel || log.model?.toLocaleLowerCase().includes(normalizedModel)
      const protocolMatches = protocolFilter === 'all' || log.protocol === protocolFilter
      return modelMatches && protocolMatches
    })
  }, [logsQuery.data?.logs, modelFilter, protocolFilter])
  const logCount = logsQuery.data?.logs.length ?? 0
  const hasFilters = modelFilter.length > 0 || protocolFilter !== 'all'

  const resetFilters = () => {
    setModelFilter('')
    setProtocolFilter('all')
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 lg:flex-row lg:items-end lg:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('usageLogs.title')}</h2>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">
            {t(admin ? 'usageLogs.adminSubtitle' : 'usageLogs.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          className="self-start lg:self-auto"
          disabled={logsQuery.isFetching}
          onClick={() => logsQuery.refetch()}
        >
          <RefreshCw className={logsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('usageLogs.actions.refresh')}
        </Button>
      </header>

      <div className="grid gap-2 border-y border-[var(--hairline)] py-3 md:grid-cols-[minmax(12rem,1fr)_12rem_auto]">
        <label className="relative">
          <span className="sr-only">{t('usageLogs.filters.model')}</span>
          <Filter className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
          <Input
            className="h-8 pl-8 text-xs"
            value={modelFilter}
            placeholder={t('usageLogs.filters.modelPlaceholder')}
            onChange={(event) => setModelFilter(event.target.value)}
          />
        </label>
        <label>
          <span className="sr-only">{t('usageLogs.filters.protocol')}</span>
          <Select className="h-8 text-xs" value={protocolFilter} onChange={(event) => setProtocolFilter(event.target.value)}>
            <option value="all">{t('usageLogs.filters.allProtocols')}</option>
            {['openai_chat', 'openai_responses', 'anthropic', 'gemini', 'openai_embeddings', 'openai_images', 'openai_audio', 'openai_speech', 'jina_rerank', 'cohere_rerank', 'xai_video'].map((protocol) => (
              <option key={protocol} value={protocol}>{t(`usageLogs.protocol.${protocol}`)}</option>
            ))}
          </Select>
        </label>
        <Button type="button" size="sm" variant="ghost" disabled={!hasFilters} onClick={resetFilters}>
          <X aria-hidden="true" />
          {t('usageLogs.actions.reset')}
        </Button>
      </div>

      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>{t('usageLogs.filteredCount', { visible: filteredLogs.length, count: logCount })}</span>
        <span>{t('usageLogs.page', { page: cursorStack.length })}</span>
      </div>

      {logsQuery.isPending ? (
        <div className="grid gap-2" aria-label={t('usageLogs.loading')}>
          {[0, 1, 2, 3, 4].map((item) => <Skeleton key={item} className="h-16 rounded-lg" />)}
        </div>
      ) : logsQuery.isError ? (
        <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('usageLogs.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('usageLogs.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => logsQuery.refetch()}>
            {t('usageLogs.actions.retry')}
          </Button>
        </div>
      ) : <UsageLogTable logs={filteredLogs} admin={admin} filtered={hasFilters} />}

      {!logsQuery.isPending && !logsQuery.isError && (cursorStack.length > 1 || nextCursor != null) ? (
        <nav className="flex items-center justify-center gap-2" aria-label={t('usageLogs.pagination')}>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={cursorStack.length === 1 || logsQuery.isFetching}
            onClick={() => setCursorStack((current) => current.length > 1 ? current.slice(0, -1) : current)}
          >
            <ChevronLeft aria-hidden="true" />
            {t('usageLogs.actions.previous')}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={nextCursor == null || logsQuery.isFetching}
            onClick={() => nextCursor != null && setCursorStack((current) => [...current, nextCursor])}
          >
            {t('usageLogs.actions.next')}
            <ChevronRight aria-hidden="true" />
          </Button>
        </nav>
      ) : null}
    </div>
  )
}
