import { useMemo, useState } from 'react'
import { CircleAlert, Coins, RefreshCw, Search, SlidersHorizontal, X, Zap } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useCursorPagination } from '@/components/data-table/use-table-pagination'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { FailedCallTable } from '@/features/usage-logs/failed-call-table'
import { UsageLogDetailsSheet } from '@/features/usage-logs/usage-log-details-sheet'
import { formatUsageLatency, formatUsageNumber } from '@/features/usage-logs/usage-log-format'
import {
  filterFailedCallLogs,
  filterUsageLogs,
  summarizeUsageLogs,
  type FailedCallLogRow,
  type UsageLogModeFilter,
  type UsageLogRow,
  usageLogProtocols,
} from '@/features/usage-logs/usage-log-model'
import { UsageLogSummary } from '@/features/usage-logs/usage-log-summary'
import { UsageLogTable } from '@/features/usage-logs/usage-log-table'
import type { OrganizationUsageLogSummaryResponse } from '@/lib/api/generated/types.gen'
import {
  organizationActivityErrorStatus,
  type OrganizationUsageScope,
  useOrganizationUsageLogs,
} from './organization-activity-api'

type OrganizationUsagePanelProps = {
  organizationId: number
  initialScope: OrganizationUsageScope
  scopes: OrganizationUsageScope[]
}

/** 企业调用日志面板按权限限定可选范围，企业明细不会链接到平台管理入口。 */
export function OrganizationUsagePanel({ organizationId, initialScope, scopes }: OrganizationUsagePanelProps) {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const pagination = useCursorPagination<number>()
  const failedPagination = useCursorPagination<number>()
  const [scope, setScope] = useState(initialScope)
  const [modelFilter, setModelFilter] = useState('')
  const [protocolFilter, setProtocolFilter] = useState('all')
  const [modeFilter, setModeFilter] = useState<UsageLogModeFilter>('all')
  const [selectedLog, setSelectedLog] = useState<UsageLogRow>()
  const query = useOrganizationUsageLogs(
    organizationId,
    scope,
    pagination.cursor,
    failedPagination.cursor,
    pagination.pageSize,
    true,
  )
  const logs = useMemo(() => (query.data?.logs ?? []) as UsageLogRow[], [query.data?.logs])
  const failedLogs = useMemo(() => (query.data?.failed_logs ?? []) as FailedCallLogRow[], [query.data?.failed_logs])
  const summaries = useMemo(() => query.data?.summaries ?? [], [query.data?.summaries])
  const filteredLogs = useMemo(
    () => filterUsageLogs(logs, modelFilter, protocolFilter, modeFilter),
    [logs, modeFilter, modelFilter, protocolFilter],
  )
  const filteredSummaries = useMemo(
    () => filterSummaries(summaries, modelFilter, protocolFilter),
    [modelFilter, protocolFilter, summaries],
  )
  const pageSummary = useMemo(() => summarizeUsageLogs(filteredLogs), [filteredLogs])
  const filteredFailedLogs = useMemo(
    () => filterFailedCallLogs(failedLogs, modelFilter, protocolFilter),
    [failedLogs, modelFilter, protocolFilter],
  )
  const nextCursor = query.data?.next_cursor
  const failedNextCursor = query.data?.failed_next_cursor
  const isSummary = scope === 'summary'
  const visibleCount = isSummary ? filteredSummaries.length : filteredLogs.length
  const itemCount = isSummary ? summaries.length : logs.length
  const hasFilters = modelFilter.length > 0
    || protocolFilter !== 'all'
    || (!isSummary && modeFilter !== 'all')
  const hasFailedFilters = modelFilter.length > 0 || protocolFilter !== 'all'

  const setPageSize = (pageSize: number) => {
    pagination.setPageSize(pageSize)
    failedPagination.setPageSize(pageSize)
  }

  const changeScope = (nextScope: OrganizationUsageScope) => {
    if (nextScope === scope) return
    setScope(nextScope)
    pagination.reset()
    failedPagination.reset()
    setSelectedLog(undefined)
  }

  const resetFilters = () => {
    setModelFilter('')
    setProtocolFilter('all')
    setModeFilter('all')
  }

  return (
    <div className="grid min-w-0 gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div role="group" aria-label={t('organizationActivity.usage.scopeLabel')} className="flex flex-wrap items-center gap-1 rounded-lg border border-[var(--hairline)] bg-surface-2/45 p-1">
          {scopes.map((item) => (
            <Button
              key={item}
              type="button"
              size="sm"
              variant={scope === item ? 'secondary' : 'ghost'}
              aria-pressed={scope === item}
              onClick={() => changeScope(item)}
            >
              {t(`organizationActivity.usage.scopes.${item}`)}
            </Button>
          ))}
        </div>
        <Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('organizationActivity.actions.refresh')}
        </Button>
      </div>

      {!isSummary && !query.isPending && !query.isError ? (
        <UsageLogSummary summary={pageSummary} formatQuota={formatQuota} />
      ) : null}

      <div className="rounded-lg border border-[var(--hairline)] bg-surface-1/45 p-2.5">
        <div className="mb-2 flex items-center justify-between gap-3 px-0.5 text-[0.6875rem] text-muted-foreground">
          <span className="inline-flex items-center gap-1.5"><SlidersHorizontal className="size-3.5" aria-hidden="true" />{t('organizationActivity.filters.label')}</span>
          <span>{t('organizationActivity.filters.visible', { visible: visibleCount, count: itemCount })}</span>
        </div>
        <div className={`grid gap-2 ${isSummary ? 'md:grid-cols-[minmax(12rem,1fr)_12rem_auto]' : 'md:grid-cols-[minmax(12rem,1fr)_12rem_10rem_auto]'}`}>
          <label className="relative">
            <span className="sr-only">{t('usageLogs.filters.model')}</span>
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
            <Input className="h-8 pl-8 text-xs" value={modelFilter} placeholder={t('usageLogs.filters.modelPlaceholder')} onChange={(event) => setModelFilter(event.target.value)} />
          </label>
          <label>
            <span className="sr-only">{t('usageLogs.filters.protocol')}</span>
            <Select className="h-8 text-xs" value={protocolFilter} onChange={(event) => setProtocolFilter(event.target.value)}>
              <option value="all">{t('usageLogs.filters.allProtocols')}</option>
              {usageLogProtocols.map((protocol) => <option key={protocol} value={protocol}>{t(`usageLogs.protocol.${protocol}`)}</option>)}
            </Select>
          </label>
          {!isSummary ? (
            <label>
              <span className="sr-only">{t('usageLogs.filters.mode')}</span>
              <Select className="h-8 text-xs" value={modeFilter} onChange={(event) => setModeFilter(event.target.value as UsageLogModeFilter)}>
                <option value="all">{t('usageLogs.filters.allModes')}</option>
                <option value="stream">{t('usageLogs.values.stream')}</option>
                <option value="sync">{t('usageLogs.values.sync')}</option>
                <option value="legacy">{t('usageLogs.values.legacy')}</option>
              </Select>
            </label>
          ) : null}
          <Button type="button" size="sm" variant="ghost" className="justify-self-start md:justify-self-auto" disabled={!hasFilters} onClick={resetFilters}>
            <X aria-hidden="true" />{t('organizationActivity.actions.reset')}
          </Button>
        </div>
      </div>

      {query.isPending ? (
        <UsageContentLoading />
      ) : query.isError ? (
        <UsageError forbidden={organizationActivityErrorStatus(query.error) === 403} onRetry={() => void query.refetch()} />
      ) : isSummary ? (
        <OrganizationUsageSummaryTable summaries={filteredSummaries} filtered={hasFilters} formatQuota={formatQuota} />
      ) : (
        <>
          <UsageLogTable
            logs={filteredLogs}
            admin={false}
            showPrincipal={scope === 'organization'}
            filtered={hasFilters}
            formatQuota={formatQuota}
            onSelect={setSelectedLog}
          />
          <DataTablePagination
            currentPage={pagination.currentPage}
            availablePageCount={pagination.availablePageCount}
            pageSize={pagination.pageSize}
            itemCount={logs.length}
            hasNextPage={nextCursor != null || pagination.hasLoadedNextPage}
            fetching={query.isFetching}
            onFirstPage={pagination.goToFirstPage}
            onPreviousPage={pagination.goToPreviousPage}
            onPageSelect={pagination.selectPage}
            onNextPage={() => pagination.goToNextPage(nextCursor)}
            onPageSizeChange={setPageSize}
          />
          <section className="grid gap-3 border-t border-[var(--hairline)] pt-4" aria-labelledby="organization-failed-call-log-heading">
            <header className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <CircleAlert className="size-4 text-destructive" aria-hidden="true" />
                <h3 id="organization-failed-call-log-heading" className="text-sm font-semibold">{t('usageLogs.failed.title')}</h3>
                <Badge className="border-transparent bg-destructive/10 text-destructive">{t('usageLogs.failed.badge')}</Badge>
              </div>
              <span className="text-[0.6875rem] text-muted-foreground">{t('organizationActivity.filters.visible', { visible: filteredFailedLogs.length, count: failedLogs.length })}</span>
            </header>
            <FailedCallTable logs={filteredFailedLogs} showInternal filtered={hasFailedFilters} />
            <DataTablePagination
              currentPage={failedPagination.currentPage}
              availablePageCount={failedPagination.availablePageCount}
              pageSize={pagination.pageSize}
              itemCount={failedLogs.length}
              hasNextPage={failedNextCursor != null || failedPagination.hasLoadedNextPage}
              fetching={query.isFetching}
              onFirstPage={failedPagination.goToFirstPage}
              onPreviousPage={failedPagination.goToPreviousPage}
              onPageSelect={failedPagination.selectPage}
              onNextPage={() => failedPagination.goToNextPage(failedNextCursor)}
              onPageSizeChange={setPageSize}
            />
          </section>
        </>
      )}

      {!query.isPending && !query.isError && isSummary ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={itemCount}
          hasNextPage={nextCursor != null || pagination.hasLoadedNextPage}
          fetching={query.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={() => pagination.goToNextPage(nextCursor)}
          onPageSizeChange={setPageSize}
        />
      ) : null}

      <UsageLogDetailsSheet
        log={selectedLog}
        admin={false}
        showPrincipal={scope === 'organization'}
        formatQuota={formatQuota}
        onOpenChange={(open) => !open && setSelectedLog(undefined)}
        onTraceOpen={() => undefined}
      />
    </div>
  )
}

function filterSummaries(
  summaries: readonly OrganizationUsageLogSummaryResponse[],
  modelFilter: string,
  protocolFilter: string,
) {
  const normalizedModel = modelFilter.trim().toLocaleLowerCase()
  return summaries.filter((summary) => (
    (!normalizedModel || summary.model?.toLocaleLowerCase().includes(normalizedModel))
    && (protocolFilter === 'all' || summary.protocol === protocolFilter)
  ))
}

function OrganizationUsageSummaryTable({ summaries, filtered, formatQuota }: {
  summaries: OrganizationUsageLogSummaryResponse[]
  filtered: boolean
  formatQuota: (value: number) => string
}) {
  const { i18n, t } = useTranslation()
  const locale = i18n.language
  if (summaries.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center rounded-lg border border-[var(--hairline)] text-center">
        <div className="max-w-sm px-5">
          <Zap className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
          <h3 className="mt-3 text-sm font-semibold">{t(filtered ? 'organizationActivity.usage.emptyFilteredTitle' : 'organizationActivity.usage.emptyTitle')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t(filtered ? 'organizationActivity.usage.emptyFilteredBody' : 'organizationActivity.usage.emptyBody')}</p>
        </div>
      </div>
    )
  }
  return (
    <section className="overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1" aria-label={t('organizationActivity.usage.summaryLabel')}>
      <div className="hidden grid-cols-[minmax(12rem,1.5fr)_8rem_8rem_8rem_8rem] gap-3 border-b border-[var(--hairline)] bg-surface-2/60 px-4 py-2 text-[0.6875rem] font-medium text-muted-foreground lg:grid">
        <span>{t('organizationActivity.usage.columns.call')}</span>
        <span>{t('organizationActivity.usage.columns.requests')}</span>
        <span>{t('organizationActivity.usage.columns.tokens')}</span>
        <span>{t('organizationActivity.usage.columns.duration')}</span>
        <span className="text-right">{t('organizationActivity.usage.columns.quota')}</span>
      </div>
      {summaries.map((summary, index) => {
        const tokenCount = summary.input_tokens + summary.output_tokens
        const averageDuration = summary.request_count > 0 ? Math.round(summary.duration_ms / summary.request_count) : null
        return (
          <article key={`${summary.model ?? 'legacy'}-${summary.protocol ?? 'legacy'}-${summary.operation ?? 'legacy'}-${index}`} className="grid gap-3 border-b border-[var(--hairline)] px-3 py-3 last:border-b-0 lg:grid-cols-[minmax(12rem,1.5fr)_8rem_8rem_8rem_8rem] lg:items-center lg:px-4 lg:py-2.5">
            <div className="min-w-0">
              <div className="flex min-w-0 items-center gap-2">
                <span className="truncate text-sm font-semibold">{summary.model ?? t('usageLogs.values.legacyModel')}</span>
                <Badge className="shrink-0 border-transparent bg-info/10 text-info">{summary.protocol ? t(`usageLogs.protocol.${summary.protocol}`) : t('usageLogs.values.legacy')}</Badge>
              </div>
              <p className="mt-1 truncate text-[0.6875rem] text-muted-foreground">{summary.operation ? t(`usageLogs.operation.${summary.operation}`) : t('usageLogs.values.notSpecified')}</p>
            </div>
            <SummaryMetric icon={Zap} label={t('organizationActivity.usage.columns.requests')} value={formatUsageNumber(summary.request_count, locale)} />
            <SummaryMetric icon={Zap} label={t('organizationActivity.usage.columns.tokens')} value={formatUsageNumber(tokenCount, locale)} />
            <SummaryMetric icon={Zap} label={t('organizationActivity.usage.columns.duration')} value={formatUsageLatency(averageDuration, locale)} />
            <SummaryMetric icon={Coins} label={t('organizationActivity.usage.columns.quota')} value={formatQuota(summary.quota)} alignRight />
          </article>
        )
      })}
    </section>
  )
}

function SummaryMetric({ icon: Icon, label, value, alignRight = false }: {
  icon: typeof Zap
  label: string
  value: string
  alignRight?: boolean
}) {
  return (
    <div className={alignRight ? 'min-w-0 lg:text-right' : 'min-w-0'}>
      <span className="inline-flex items-center gap-1 text-[0.6875rem] text-muted-foreground lg:hidden"><Icon className="size-3" aria-hidden="true" />{label}</span>
      <p className="mt-1 truncate text-xs font-semibold tabular-nums lg:mt-0" title={value}>{value}</p>
    </div>
  )
}

function UsageContentLoading() {
  return <div className="overflow-hidden rounded-lg border border-[var(--hairline)]" role="status"><Skeleton className="h-9 rounded-none" />{[0, 1, 2, 3, 4].map((item) => <Skeleton key={item} className="h-[4.5rem] rounded-none border-t border-[var(--hairline)]" />)}</div>
}

function UsageError({ forbidden, onRetry }: { forbidden: boolean; onRetry: () => void }) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
      <h3 className="text-sm font-semibold text-destructive">{t(forbidden ? 'organizationActivity.errors.forbiddenTitle' : 'organizationActivity.errors.usageTitle')}</h3>
      <p className="mt-1 text-xs text-muted-foreground">{t(forbidden ? 'organizationActivity.errors.forbiddenBody' : 'organizationActivity.errors.usageBody')}</p>
      {!forbidden ? <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={onRetry}>{t('organizationActivity.actions.retry')}</Button> : null}
    </div>
  )
}
