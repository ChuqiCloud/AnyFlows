import { useMemo, useState } from 'react'
import { Activity, RefreshCw, Search, SlidersHorizontal, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useCursorPagination } from '@/components/data-table/use-table-pagination'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { useAdminUsageLogs, useUserUsageLogs } from './usage-log-api'
import { UsageLogDetailsSheet } from './usage-log-details-sheet'
import {
  filterUsageLogs,
  filterFailedCallLogs,
  summarizeUsageLogs,
  type RequestLogRow,
  type UsageLogModeFilter,
  type UsageLogRow,
  usageLogProtocols,
} from './usage-log-model'
import { UsageLogSummary } from './usage-log-summary'
import { UsageLogTable } from './usage-log-table'
import { UsageLogUserSheet } from './usage-log-user-sheet'

type UsageLogPageProps = {
  admin: boolean
}

export function UsageLogPage({ admin }: UsageLogPageProps) {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const pagination = useCursorPagination<number>()
  const failedPagination = useCursorPagination<number>()
  const [modelFilter, setModelFilter] = useState('')
  const [protocolFilter, setProtocolFilter] = useState('all')
  const [modeFilter, setModeFilter] = useState<UsageLogModeFilter>('all')
  const [selectedLog, setSelectedLog] = useState<UsageLogRow>()
  const [selectedUserId, setSelectedUserId] = useState<number>()
  const adminQuery = useAdminUsageLogs(pagination.cursor, failedPagination.cursor, pagination.pageSize, admin)
  const userQuery = useUserUsageLogs(pagination.cursor, failedPagination.cursor, pagination.pageSize, !admin)
  const logsQuery = admin ? adminQuery : userQuery
  const logs = useMemo(() => (logsQuery.data?.logs ?? []) as UsageLogRow[], [logsQuery.data?.logs])
  const failedLogs = useMemo(() => logsQuery.data?.failed_logs ?? [], [logsQuery.data?.failed_logs])
  const nextCursor = logsQuery.data?.next_cursor
  const failedNextCursor = logsQuery.data?.failed_next_cursor
  const filteredLogs = useMemo(
    () => filterUsageLogs(logs, modelFilter, protocolFilter, modeFilter),
    [logs, modeFilter, modelFilter, protocolFilter],
  )
  const filteredFailedLogs = useMemo(
    () => filterFailedCallLogs(failedLogs, modelFilter, protocolFilter),
    [failedLogs, modelFilter, protocolFilter],
  )
  const summary = useMemo(() => summarizeUsageLogs(filteredLogs), [filteredLogs])
  const hasFilters = modelFilter.length > 0 || protocolFilter !== 'all' || modeFilter !== 'all'
  const requestLogs = useMemo(() => [...filteredLogs, ...filteredFailedLogs].sort((left, right) => right.created_at - left.created_at) as RequestLogRow[], [filteredFailedLogs, filteredLogs])

  const setPageSize = (pageSize: number) => {
    pagination.setPageSize(pageSize)
    failedPagination.setPageSize(pageSize)
  }

  const resetFilters = () => {
    setModelFilter('')
    setProtocolFilter('all')
    setModeFilter('all')
  }

  const openUser = (userId: number) => {
    setSelectedLog(undefined)
    setSelectedUserId(userId)
  }

  const openTrace = (requestId: string) => {
    setSelectedLog(undefined)
    window.location.hash = `#/console/debug-traces?request_id=${encodeURIComponent(requestId)}`
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 lg:flex-row lg:items-end lg:justify-between">
        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-semibold">{t('usageLogs.title')}</h2>
            <Badge className="border-transparent bg-success/10 text-success">{t('usageLogs.status.settledAndFailed')}</Badge>
          </div>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">
            {t(admin ? 'usageLogs.adminSubtitle' : 'usageLogs.subtitle')}
          </p>
        </div>
        <div className="flex items-center gap-2 self-start lg:self-auto">
          {admin ? (
            <Button asChild size="sm" variant="ghost">
              <a href="#/console/debug-traces"><Activity aria-hidden="true" />{t('usageLogs.actions.debugTraces')}</a>
            </Button>
          ) : null}
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={logsQuery.isFetching}
            onClick={() => void logsQuery.refetch()}
          >
            <RefreshCw className={logsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('usageLogs.actions.refresh')}
          </Button>
        </div>
      </header>

      {!logsQuery.isPending && !logsQuery.isError ? <UsageLogSummary summary={summary} formatQuota={formatQuota} /> : null}

      <div className="rounded-lg border border-[var(--hairline)] bg-surface-1/45 p-2.5">
        <div className="mb-2 flex items-center justify-between gap-3 px-0.5 text-[0.6875rem] text-muted-foreground">
          <span className="inline-flex items-center gap-1.5"><SlidersHorizontal className="size-3.5" aria-hidden="true" />{t('usageLogs.filters.label')}</span>
          <span>{t('usageLogs.filteredCount', { visible: requestLogs.length, count: logs.length + failedLogs.length })}</span>
        </div>
        <div className="grid gap-2 md:grid-cols-[minmax(12rem,1fr)_12rem_10rem_auto]">
          <label className="relative">
            <span className="sr-only">{t('usageLogs.filters.model')}</span>
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
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
              {usageLogProtocols.map((protocol) => (
                <option key={protocol} value={protocol}>{t(`usageLogs.protocol.${protocol}`)}</option>
              ))}
            </Select>
          </label>
          <label>
            <span className="sr-only">{t('usageLogs.filters.mode')}</span>
            <Select className="h-8 text-xs" value={modeFilter} onChange={(event) => setModeFilter(event.target.value as UsageLogModeFilter)}>
              <option value="all">{t('usageLogs.filters.allModes')}</option>
              <option value="stream">{t('usageLogs.values.stream')}</option>
              <option value="sync">{t('usageLogs.values.sync')}</option>
              <option value="legacy">{t('usageLogs.values.legacy')}</option>
            </Select>
          </label>
          <Button type="button" size="sm" variant="ghost" className="justify-self-start md:justify-self-auto" disabled={!hasFilters} onClick={resetFilters}>
            <X aria-hidden="true" />
            {t('usageLogs.actions.reset')}
          </Button>
        </div>
      </div>

      {logsQuery.isPending ? (
        <div className="overflow-hidden rounded-lg border border-[var(--hairline)]" aria-label={t('usageLogs.loading')}>
          <Skeleton className="h-9 rounded-none" />
          {[0, 1, 2, 3, 4, 5].map((item) => <Skeleton key={item} className="h-[4.5rem] rounded-none border-t border-[var(--hairline)]" />)}
        </div>
      ) : logsQuery.isError ? (
        <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('usageLogs.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('usageLogs.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void logsQuery.refetch()}>
            {t('usageLogs.actions.retry')}
          </Button>
        </div>
      ) : (
        <>
          <UsageLogTable logs={requestLogs} admin={admin} filtered={hasFilters} formatQuota={formatQuota} onSelect={setSelectedLog} onUserSelect={openUser} />

          <DataTablePagination
            currentPage={pagination.currentPage}
            availablePageCount={pagination.availablePageCount}
            pageSize={pagination.pageSize}
            itemCount={requestLogs.length}
            hasNextPage={nextCursor != null || failedNextCursor != null || pagination.hasLoadedNextPage || failedPagination.hasLoadedNextPage}
            fetching={logsQuery.isFetching}
            onFirstPage={() => { pagination.goToFirstPage(); failedPagination.goToFirstPage() }}
            onPreviousPage={() => { pagination.goToPreviousPage(); failedPagination.goToPreviousPage() }}
            onPageSelect={(page) => { pagination.selectPage(page); failedPagination.selectPage(page) }}
            onNextPage={() => { pagination.goToNextPage(nextCursor ?? logs.at(-1)?.id); failedPagination.goToNextPage(failedNextCursor ?? failedLogs.at(-1)?.id) }}
            onPageSizeChange={setPageSize}
          />

        </>
      )}

      <UsageLogDetailsSheet log={selectedLog} admin={admin} formatQuota={formatQuota} onOpenChange={(open) => !open && setSelectedLog(undefined)} onTraceOpen={openTrace} onUserSelect={openUser} />
      {admin ? <UsageLogUserSheet userId={selectedUserId} formatQuota={formatQuota} onOpenChange={(open) => !open && setSelectedUserId(undefined)} /> : null}
    </div>
  )
}
