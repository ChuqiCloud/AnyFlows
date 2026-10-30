import { Button, Calendar, Input, Popover, PopoverContent, PopoverTrigger, Select, SelectItem, Skeleton, type DateValue } from '@heroui/react'
import { useMemo, useState, type ReactNode } from 'react'
import { Activity, CalendarDays, ChevronDown, Eye, EyeOff, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useCursorPagination } from '@/components/data-table/use-table-pagination'
import { cn } from '@/lib/utils'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { navigateTo } from '@/lib/router-navigation'
import { useAdminUsageLogs, useUserUsageLogs } from './usage-log-api'
import { UsageLogDetailsSheet } from './usage-log-details-sheet'
import { FailedCallLogDetailsSheet } from './failed-call-log-details-sheet'
import {
  filterUsageLogs,
  filterFailedCallLogs,
  type FailedCallLogRow,
  type UsageLogFilters,
  type UsageLogRange,
  type RequestLogRow,
  summarizeUsageLogs,
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

type RangePreset = { label: string; hours: number }

/** 按本地时区把日期值换成秒级时间戳；只给日期时取当天起止。 */
function toSeconds(value: DateValue | null, endOfDay = false) {
  if (!value) return null
  const hour = 'hour' in value ? value.hour : endOfDay ? 23 : 0
  const minute = 'minute' in value ? value.minute : endOfDay ? 59 : 0
  const second = 'second' in value ? value.second : endOfDay ? 59 : 0
  return new Date(value.year, value.month - 1, value.day, hour, minute, second).getTime() / 1000
}

const emptyFilters: UsageLogFilters = { model: '', protocol: 'all', mode: 'all', username: '', token: '', channel: '', group: '', requestId: '', upstreamRequestId: '' }

const rangePresets: RangePreset[] = [
  { label: '24h', hours: 24 },
  { label: '7d', hours: 24 * 7 },
  { label: '30d', hours: 24 * 30 },
]

function Field({ label, children }: { label: string; children: ReactNode }) {
  return <label className="grid min-w-0 gap-1 text-[0.6875rem] text-muted-foreground"><span className="truncate">{label}</span>{children}</label>
}

/** HeroUI 日历按天选起止，外加常用预设区间；触发按钮显示当前范围。 */
function TimeRangePicker({ range, onChange }: { range: UsageLogRange; onChange: (range: UsageLogRange) => void }) {
  const { t, i18n } = useTranslation()
  const [open, setOpen] = useState(false)
  const format = (value?: number) => (value == null ? '' : new Date(value * 1000).toLocaleString(i18n.language, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }))
  const applyPreset = (hours: number | null) => {
    const now = new Date().getTime() / 1000
    onChange(hours === null ? {} : { start: now - hours * 3_600 })
    setOpen(false)
  }
  return (
    <Popover isOpen={open} placement="bottom-start" onOpenChange={setOpen}>
      <PopoverTrigger>
        <Button type="button" size="sm" variant="flat" className="w-full justify-start gap-2 font-normal tabular-nums" startContent={<CalendarDays className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />}>
          <span className="truncate">{range.start == null && range.end == null ? t('usageLogs.filters.anyTime') : `${format(range.start) || t('usageLogs.filters.start')} ~ ${format(range.end) || t('usageLogs.filters.end')}`}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto max-w-[calc(100vw-2rem)] p-0">
        {() => <div className="flex w-fit flex-col">
          <p className="px-3 pt-2 text-xs font-semibold">{t('usageLogs.filters.pickRange')}</p>
          <div className="flex flex-nowrap gap-0.5 border-b border-[var(--hairline)] px-1.5 py-1">
            {[{ label: t('usageLogs.filters.anyTime'), hours: null }, ...rangePresets].map((preset) => (
              <Button key={preset.label} type="button" size="sm" variant="light" className="h-7 whitespace-nowrap px-2 text-xs font-normal" onClick={() => applyPreset(preset.hours)}>{preset.label}</Button>
            ))}
          </div>
          <Calendar aria-label={t('usageLogs.summary.range')} classNames={{ base: 'w-[17rem] shadow-none', content: 'w-full' }} showMonthAndYearPickers visibleMonths={1} value={undefined} onChange={(value: DateValue) => { const picked = toSeconds(value); if (picked == null) return; if (range.start != null && range.end == null && picked > range.start) { onChange({ start: range.start, end: toSeconds(value, true) ?? undefined }); setOpen(false) } else onChange({ start: picked }) }} />
        </div>}
      </PopoverContent>
    </Popover>
  )
}

export function UsageLogPage({ admin }: UsageLogPageProps) {
  const { t, i18n } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const pagination = useCursorPagination<number>()
  const failedPagination = useCursorPagination<number>()
  const [draft, setDraft] = useState(emptyFilters)
  const [filters, setFilters] = useState<UsageLogFilters>(emptyFilters)
  const [range, setRange] = useState<UsageLogRange>({})
  const [advanced, setAdvanced] = useState(false)
  const [summaryHidden, setSummaryHidden] = useState(false)
  const [selectedLog, setSelectedLog] = useState<UsageLogRow>()
  const [selectedFailedLog, setSelectedFailedLog] = useState<FailedCallLogRow>()
  const [selectedUserId, setSelectedUserId] = useState<number>()
    const adminQuery = useAdminUsageLogs(pagination.cursor, failedPagination.cursor, pagination.pageSize, admin)
  const userQuery = useUserUsageLogs(pagination.cursor, failedPagination.cursor, pagination.pageSize, !admin)
  const logsQuery = admin ? adminQuery : userQuery
  const logs = useMemo(() => (logsQuery.data?.logs ?? []) as UsageLogRow[], [logsQuery.data?.logs])
  const failedLogs = useMemo(() => logsQuery.data?.failed_logs ?? [], [logsQuery.data?.failed_logs])
  const nextCursor = logsQuery.data?.next_cursor
  const failedNextCursor = logsQuery.data?.failed_next_cursor
  // 时间范围每次按当前时刻重新换算，其余条件来自已应用的表单值。
  const viewFilters = useMemo(() => ({ ...filters, range }), [filters, range])
  const filteredLogs = useMemo(() => filterUsageLogs(logs, viewFilters), [logs, viewFilters])
  const filteredFailedLogs = useMemo(() => filterFailedCallLogs(failedLogs, filters.model, filters.protocol, viewFilters.range), [failedLogs, filters.model, filters.protocol, viewFilters.range])
  const summary = useMemo(() => summarizeUsageLogs(filteredLogs), [filteredLogs])
  const requestLogs = useMemo(() => [...filteredLogs, ...filteredFailedLogs].sort((left, right) => right.created_at - left.created_at) as RequestLogRow[], [filteredFailedLogs, filteredLogs])
  const hasFilters = Object.keys(draft).some((key) => draft[key as keyof UsageLogFilters]) || range.start != null || range.end != null
  const setField = (key: Exclude<keyof UsageLogFilters, 'mode' | 'range'>, value: string) => setDraft((current) => ({ ...current, [key]: value }))

  const setPageSize = (pageSize: number) => {
    pagination.setPageSize(pageSize)
    failedPagination.setPageSize(pageSize)
  }

  const applyFilters = () => setFilters({ ...draft, mode: draft.mode })
  const resetFilters = () => {
    setDraft(emptyFilters)
    setFilters(emptyFilters)
    setRange({})
  }

  // 抽屉与列表共用同一组回调；用户、追踪详情打开前先关掉当前详情，避免抽屉叠层。
  const openLog = (log: UsageLogRow) => { setSelectedFailedLog(undefined); setSelectedLog(log) }
  const openFailedLog = (log: FailedCallLogRow) => { setSelectedLog(undefined); setSelectedFailedLog(log) }
  const openUser = (userId: number) => { setSelectedLog(undefined); setSelectedFailedLog(undefined); setSelectedUserId(userId) }
  const openTrace = (requestId: string) => {
    setSelectedLog(undefined)
    setSelectedFailedLog(undefined)
    navigateTo(`/console/debug-traces?request_id=${encodeURIComponent(requestId)}`)
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <header className="flex flex-col gap-3 lg:flex-row lg:items-end lg:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('usageLogs.title')}</h2>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">
            {t(admin ? 'usageLogs.adminSubtitle' : 'usageLogs.subtitle')}
          </p>
        </div>
        <div className="flex items-center gap-2 self-start lg:self-auto">
          {admin ? (
            <Button as="a" href="/console/debug-traces" size="sm" variant="light">
              <Activity className="size-3.5" aria-hidden="true" />{t('usageLogs.actions.debugTraces')}
            </Button>
          ) : null}
          <Button type="button" size="sm" variant="bordered" isDisabled={logsQuery.isFetching} onClick={() => void logsQuery.refetch()}>
            <RefreshCw className={cn('size-3.5', logsQuery.isFetching && 'animate-spin')} aria-hidden="true" />
            {t('usageLogs.actions.refresh')}
          </Button>
        </div>
      </header>

      {logsQuery.isPending ? (
        <div className="overflow-hidden rounded-lg border border-[var(--hairline)]" aria-label={t('usageLogs.loading')}>
          <Skeleton className="h-28 rounded-none" />
          {[0, 1, 2, 3, 4, 5].map((item) => <Skeleton key={item} className="h-15 rounded-none border-t border-[var(--hairline)]" />)}
        </div>
      ) : logsQuery.isError ? (
        <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('usageLogs.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('usageLogs.error.body')}</p>
          <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={() => void logsQuery.refetch()}>
            {t('usageLogs.actions.retry')}
          </Button>
        </div>
      ) : (
        <div className="min-h-0 flex-1">
          <div className="flex h-full min-h-0 flex-col gap-2.5 sm:gap-3">
            <form className="rounded-lg border border-[var(--hairline)] bg-surface-1/45 p-2.5 sm:p-3" onSubmit={(event) => { event.preventDefault(); if (advanced) applyFilters() }}>
              <div className="flex flex-wrap items-start gap-2">
                <div className="grid min-w-0 flex-1 grid-cols-1 gap-2 sm:grid-cols-[repeat(auto-fit,minmax(10rem,1fr))]">
                  <Field label={t('usageLogs.summary.range')}><TimeRangePicker range={range} onChange={setRange} /></Field>
                  <Field label={t('usageLogs.filters.model')}><Input classNames={{ input: 'text-sm' }} placeholder={t('usageLogs.filters.modelPlaceholder')} size="sm" value={draft.model} onChange={(event) => setField('model', event.target.value)} /></Field>
                  <Field label={t('usageLogs.filters.group')}><Input classNames={{ input: 'text-sm' }} placeholder={t('usageLogs.filters.group')} size="sm" isDisabled={!admin} value={draft.group} onChange={(event) => setField('group', event.target.value)} /></Field>
                  <Field label={t('usageLogs.filters.protocol')}><Select aria-label={t('usageLogs.filters.protocol')} className="text-sm" placeholder={t('usageLogs.filters.allProtocols')} selectedKeys={[draft.protocol]} size="sm" onSelectionChange={(keys) => setDraft((current) => ({ ...current, protocol: String(Array.from(keys)[0] ?? 'all') }))} items={[{ key: 'all', label: t('usageLogs.filters.allProtocols') }, ...usageLogProtocols.map((value) => ({ key: value, label: t(`usageLogs.protocol.${value}`) }))]}>
                      {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                    </Select></Field>
                </div>
                <div className="flex shrink-0 items-center justify-end">
                  <Button type="button" size="sm" variant="light" className="gap-1 px-2 text-muted-foreground" aria-expanded={advanced} onClick={() => setAdvanced((value) => !value)}>
                    {t(`usageLogs.filters.${advanced ? 'less' : 'more'}`)}
                    <ChevronDown className={cn('size-3.5 transition-transform', advanced && 'rotate-180')} aria-hidden="true" />
                  </Button>
                </div>
              </div>
              {advanced ? <div className="mt-2 grid gap-2 sm:grid-cols-[repeat(auto-fit,minmax(10rem,1fr))]">
                <Field label={t('usageLogs.filters.mode')}><Select aria-label={t('usageLogs.filters.mode')} className="text-sm" placeholder={t('usageLogs.filters.allModes')} selectedKeys={[draft.mode]} size="sm" onSelectionChange={(keys) => setDraft((current) => ({ ...current, mode: String(Array.from(keys)[0] ?? 'all') as UsageLogModeFilter }))} items={[{ key: 'all', label: t('usageLogs.filters.allModes') }, { key: 'stream', label: t('usageLogs.values.stream') }, { key: 'sync', label: t('usageLogs.values.sync') }, { key: 'legacy', label: t('usageLogs.values.legacy') }]}>
                  {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                </Select></Field>
                <Field label={t('usageLogs.filters.tokenName')}><Input classNames={{ input: 'text-sm' }} placeholder={t('usageLogs.filters.tokenName')} size="sm" isDisabled={!admin} value={draft.token} onChange={(event) => setField('token', event.target.value)} /></Field>
                <Field label={t('usageLogs.filters.username')}><Input classNames={{ input: 'text-sm' }} placeholder={t('usageLogs.filters.username')} size="sm" isDisabled={!admin} value={draft.username} onChange={(event) => setField('username', event.target.value)} /></Field>
                <Field label={t('usageLogs.filters.channelId')}><Input classNames={{ input: 'font-mono text-sm' }} inputMode="numeric" placeholder={t('usageLogs.filters.channelId')} size="sm" isDisabled={!admin} value={draft.channel} onChange={(event) => setField('channel', event.target.value)} /></Field>
                <Field label={t('usageLogs.filters.requestId')}><Input classNames={{ input: 'font-mono text-sm' }} placeholder={t('usageLogs.filters.requestId')} size="sm" value={draft.requestId} onChange={(event) => setField('requestId', event.target.value)} /></Field>
                <Field label={t('usageLogs.filters.upstreamRequestId')}><Input classNames={{ input: 'font-mono text-sm' }} placeholder={t('usageLogs.filters.upstreamRequestId')} size="sm" value={draft.upstreamRequestId} onChange={(event) => setField('upstreamRequestId', event.target.value)} /></Field>
              </div> : null}
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <Button type="button" size="sm" variant="bordered" isDisabled={!hasFilters} onClick={resetFilters}>{t('usageLogs.actions.reset')}</Button>
                <Button type="submit" size="sm" color="primary" isDisabled={!advanced}>{t('usageLogs.actions.search')}</Button>
                <Button type="button" size="sm" variant="light" isIconOnly className="text-muted-foreground" aria-label={t(summaryHidden ? 'usageLogs.actions.showSummary' : 'usageLogs.actions.hideSummary')} onClick={() => setSummaryHidden((value) => !value)}>
                  {summaryHidden ? <EyeOff className="size-4" aria-hidden="true" /> : <Eye className="size-4" aria-hidden="true" />}
                </Button>
                <span className="ml-auto text-xs tabular-nums text-muted-foreground">{t('usageLogs.filteredCount', { visible: requestLogs.length, count: logs.length + failedLogs.length })}</span>
              </div>
            </form>

            {summaryHidden ? null : <UsageLogSummary summary={summary} formatQuota={formatQuota} range={range.start == null && range.end == null ? undefined : { label: t('usageLogs.summary.range'), value: `${range.start == null ? '…' : new Date(range.start * 1000).toLocaleString(i18n.language, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })} ~ ${range.end == null ? '…' : new Date(range.end * 1000).toLocaleString(i18n.language, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })}` }} />}

            <UsageLogTable logs={requestLogs} showPrincipal={admin} filtered={hasFilters} formatQuota={formatQuota} onSelect={openLog} onFailedSelect={openFailedLog} onUserSelect={openUser} />
          </div>
        </div>
      )}

      {!logsQuery.isPending && !logsQuery.isError ? (
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
      ) : null}

      <UsageLogDetailsSheet log={selectedLog} admin={admin} formatQuota={formatQuota} onOpenChange={(open) => !open && setSelectedLog(undefined)} onTraceOpen={openTrace} onUserSelect={openUser} />
      <FailedCallLogDetailsSheet log={selectedFailedLog} admin={admin} onOpenChange={(open) => !open && setSelectedFailedLog(undefined)} onTraceOpen={openTrace} onUserSelect={openUser} />
      {admin ? <UsageLogUserSheet userId={selectedUserId} formatQuota={formatQuota} onOpenChange={(open) => !open && setSelectedUserId(undefined)} /> : null}
    </div>
  )
}
