import { Button, Chip, Input, Select, SelectItem, Skeleton } from '@heroui/react'
import { useMemo, useState, type FormEvent } from 'react'
import { Fingerprint, RefreshCw, Search, ShieldCheck, SlidersHorizontal, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useCursorPagination } from '@/components/data-table/use-table-pagination'
import { cn } from '@/lib/utils'
import { formatUsageDateLong } from '@/features/usage-logs/usage-log-format'
import type { OrganizationAuditLogResponse } from '@/lib/api/generated/types.gen'
import {
  organizationActivityErrorStatus,
  type OrganizationAuditFilters,
  useOrganizationAuditLogs,
} from './organization-activity-api'

type AuditDraft = {
  actor: string
  operation: string
  targetType: string
  outcome: '' | 'succeeded' | 'denied' | 'failed'
}

const emptyDraft: AuditDraft = {
  actor: '',
  operation: '',
  targetType: '',
  outcome: '',
}

// HeroUI Select 不接受空字符串 key，用哨兵 key 表达“全部结果”。
const ALL_OUTCOMES_KEY = '__all__'

const OUTCOME_FILTER_ITEMS: Array<{ value: AuditDraft['outcome']; label: string }> = [
  { value: 'succeeded', label: 'organizationActivity.audit.outcomes.succeeded' },
  { value: 'denied', label: 'organizationActivity.audit.outcomes.denied' },
  { value: 'failed', label: 'organizationActivity.audit.outcomes.failed' },
]

const OUTCOME_FILTER_LABEL = 'organizationActivity.audit.filters.allOutcomes'

/** 审计筛选在提交后才请求服务端，输入过程不会制造无意义的查询风暴。 */
export function OrganizationAuditPanel({ organizationId }: { organizationId: number }) {
  const { t } = useTranslation()
  const pagination = useCursorPagination<number>()
  const [draft, setDraft] = useState<AuditDraft>(emptyDraft)
  const [filters, setFilters] = useState<OrganizationAuditFilters>({})
  const query = useOrganizationAuditLogs(
    organizationId,
    pagination.cursor,
    pagination.pageSize,
    filters,
    true,
  )
  const logs = useMemo(() => query.data?.logs ?? [], [query.data?.logs])
  const nextCursor = query.data?.next_cursor
  const actorValid = draft.actor.trim().length === 0 || validPositiveInteger(draft.actor.trim())
  const hasFilters = Object.values(filters).some((value) => value !== undefined)

  const applyFilters = (event: FormEvent) => {
    event.preventDefault()
    if (!actorValid) return
    setFilters({
      actorUserId: draft.actor.trim() ? Number(draft.actor.trim()) : undefined,
      operation: draft.operation.trim() || undefined,
      outcome: draft.outcome || undefined,
      targetType: draft.targetType.trim() || undefined,
    })
    pagination.reset()
  }

  const resetFilters = () => {
    setDraft(emptyDraft)
    setFilters({})
    pagination.reset()
  }

  return (
    <div className="grid min-w-0 gap-4">
      <form className="rounded-lg border border-[var(--hairline)] bg-surface-1/45 p-2.5" onSubmit={applyFilters}>
        <div className="mb-2 flex items-center justify-between gap-3 px-0.5 text-[0.6875rem] text-muted-foreground">
          <span className="inline-flex items-center gap-1.5"><SlidersHorizontal className="size-3.5" aria-hidden="true" />{t('organizationActivity.filters.auditLabel')}</span>
          <span>{t('organizationActivity.audit.loaded', { count: logs.length })}</span>
        </div>
        <div className="grid gap-2 md:grid-cols-[minmax(10rem,0.8fr)_minmax(12rem,1.3fr)_minmax(10rem,0.9fr)_10rem_auto]">
          <label>
            <span className="sr-only">{t('organizationActivity.audit.filters.actor')}</span>
            <Input
              className="h-8 text-xs"
              inputMode="numeric"
              isInvalid={!actorValid}
              placeholder={t('organizationActivity.audit.filters.actorPlaceholder')}
              size="sm"
              value={draft.actor}
              onChange={(event) => setDraft((current) => ({ ...current, actor: event.target.value }))}
            />
          </label>
          <label>
            <span className="sr-only">{t('organizationActivity.audit.filters.operation')}</span>
            <Input
              className="h-8"
              classNames={{ input: 'font-mono text-xs' }}
              maxLength={96}
              placeholder={t('organizationActivity.audit.filters.operationPlaceholder')}
              size="sm"
              startContent={<Search className="pointer-events-none size-3.5 text-muted-foreground" aria-hidden="true" />}
              value={draft.operation}
              onChange={(event) => setDraft((current) => ({ ...current, operation: event.target.value }))}
            />
          </label>
          <label>
            <span className="sr-only">{t('organizationActivity.audit.filters.target')}</span>
            <Input
              className="h-8"
              classNames={{ input: 'font-mono text-xs' }}
              maxLength={32}
              placeholder={t('organizationActivity.audit.filters.targetPlaceholder')}
              size="sm"
              value={draft.targetType}
              onChange={(event) => setDraft((current) => ({ ...current, targetType: event.target.value }))}
            />
          </label>
          <label>
            <span className="sr-only">{t('organizationActivity.audit.filters.outcome')}</span>
            <Select
              aria-label={t('organizationActivity.audit.filters.outcome')}
              className="h-8 text-xs"
              items={[{ key: ALL_OUTCOMES_KEY, label: t(OUTCOME_FILTER_LABEL) }, ...OUTCOME_FILTER_ITEMS.map((item) => ({ key: item.value, label: t(item.label) }))]}
              selectedKeys={[draft.outcome === '' ? ALL_OUTCOMES_KEY : draft.outcome]}
              size="sm"
              onSelectionChange={(keys) => {
                const next = String(Array.from(keys)[0] ?? ALL_OUTCOMES_KEY)
                setDraft((current) => ({ ...current, outcome: (next === ALL_OUTCOMES_KEY ? '' : next) as AuditDraft['outcome'] }))
              }}
            >
              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
            </Select>
          </label>
          <div className="flex items-center gap-1">
            <Button type="submit" color="primary" size="sm" isDisabled={!actorValid}><Search className="size-3.5" aria-hidden="true" />{t('organizationActivity.actions.apply')}</Button>
            <Button isIconOnly type="button" size="sm" variant="light" isDisabled={!hasFilters && draftIsEmpty(draft)} aria-label={t('organizationActivity.actions.reset')} onClick={resetFilters}><X className="size-3.5" aria-hidden="true" /></Button>
          </div>
        </div>
        {!actorValid ? <p className="mt-2 text-[0.6875rem] text-destructive">{t('organizationActivity.audit.filters.actorInvalid')}</p> : null}
      </form>

      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
        <span className="inline-flex items-center gap-1.5"><ShieldCheck className="size-3.5" aria-hidden="true" />{t('organizationActivity.audit.privacy')}</span>
        <Button type="button" size="sm" variant="bordered" isDisabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw className={cn('size-3.5', query.isFetching && 'animate-spin')} aria-hidden="true" />
          {t('organizationActivity.actions.refresh')}
        </Button>
      </div>

      {query.isPending ? (
        <AuditLoading />
      ) : query.isError ? (
        <AuditError forbidden={organizationActivityErrorStatus(query.error) === 403} onRetry={() => void query.refetch()} />
      ) : (
        <OrganizationAuditTable logs={logs} filtered={hasFilters} />
      )}

      {!query.isPending && !query.isError ? (
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
          onPageSizeChange={pagination.setPageSize}
        />
      ) : null}
    </div>
  )
}

function OrganizationAuditTable({ logs, filtered }: { logs: OrganizationAuditLogResponse[]; filtered: boolean }) {
  const { t } = useTranslation()
  if (logs.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center rounded-lg border border-[var(--hairline)] text-center">
        <div className="max-w-sm px-5">
          <ShieldCheck className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
          <h3 className="mt-3 text-sm font-semibold">{t(filtered ? 'organizationActivity.audit.emptyFilteredTitle' : 'organizationActivity.audit.emptyTitle')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t(filtered ? 'organizationActivity.audit.emptyFilteredBody' : 'organizationActivity.audit.emptyBody')}</p>
        </div>
      </div>
    )
  }
  return (
    <section className="overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1" aria-label={t('organizationActivity.audit.listLabel')}>
      <div className="hidden grid-cols-[8rem_minmax(11rem,1fr)_minmax(13rem,1.3fr)_minmax(10rem,1fr)_7rem] gap-3 border-b border-[var(--hairline)] bg-surface-2/60 px-4 py-2 text-[0.6875rem] font-medium text-muted-foreground lg:grid">
        <span>{t('organizationActivity.audit.columns.time')}</span>
        <span>{t('organizationActivity.audit.columns.actor')}</span>
        <span>{t('organizationActivity.audit.columns.action')}</span>
        <span>{t('organizationActivity.audit.columns.target')}</span>
        <span>{t('organizationActivity.audit.columns.outcome')}</span>
      </div>
      {logs.map((log) => <OrganizationAuditItem key={log.id} log={log} />)}
    </section>
  )
}

function OrganizationAuditItem({ log }: { log: OrganizationAuditLogResponse }) {
  const { i18n, t } = useTranslation()
  const summary = formatChangeSummary(log.change_summary)
  return (
    <article className="grid gap-3 border-b border-[var(--hairline)] px-3 py-3 last:border-b-0 lg:grid-cols-[8rem_minmax(11rem,1fr)_minmax(13rem,1.3fr)_minmax(10rem,1fr)_7rem] lg:items-center lg:px-4 lg:py-2.5">
      <div className="min-w-0 text-xs tabular-nums">
        <p>{formatUsageDateLong(log.created_at, i18n.language)}</p>
        <p className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground" title={log.request_id}>#{log.id} · {compactIdentifier(log.request_id)}</p>
      </div>
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-semibold">{log.actor_username}</span>
          {log.platform_proxy ? <Chip className="shrink-0 bg-warning/10 text-warning" size="sm" variant="flat">{t('organizationActivity.audit.platform')}</Chip> : null}
        </div>
        <p className="mt-1 truncate text-[0.6875rem] text-muted-foreground">
          {log.actor_role ? t(`organizationBudgets.roles.${log.actor_role}`, { defaultValue: log.actor_role }) : t('organizationActivity.audit.platformRole')} · #{log.actor_user_id}
        </p>
      </div>
      <div className="min-w-0">
        <p className="truncate font-mono text-xs font-semibold" title={log.operation}>{log.operation}</p>
        <p className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground" title={log.permission_code}>{log.permission_code}</p>
        {summary ? <p className="mt-1 truncate text-[0.6875rem] text-muted-foreground" title={summary}>{summary}</p> : null}
      </div>
      <div className="min-w-0">
        <p className="truncate font-mono text-xs" title={log.target_type}>{log.target_type}</p>
        <p className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground" title={log.target_identifier ?? undefined}>{log.target_identifier ?? t('organizationActivity.audit.redacted')}</p>
        {log.client_fingerprint ? <p className="mt-1 flex min-w-0 items-center gap-1 truncate font-mono text-[0.625rem] text-muted-foreground"><Fingerprint className="size-3 shrink-0" aria-hidden="true" />{compactIdentifier(log.client_fingerprint)}</p> : null}
      </div>
      <OutcomeBadge outcome={log.outcome} />
    </article>
  )
}

function OutcomeBadge({ outcome }: { outcome: OrganizationAuditLogResponse['outcome'] }) {
  const { t } = useTranslation()
  const tone = outcome === 'succeeded'
    ? 'border-transparent bg-success/10 text-success'
    : outcome === 'denied'
      ? 'border-transparent bg-warning/10 text-warning'
      : 'border-transparent bg-destructive/10 text-destructive'
  return <Chip className={cn('w-fit', tone)} size="sm" variant="flat">{t(`organizationActivity.audit.outcomes.${outcome}`)}</Chip>
}

function validPositiveInteger(value: string) {
  if (!/^[1-9]\d*$/.test(value)) return false
  return Number.isSafeInteger(Number(value))
}

function draftIsEmpty(draft: AuditDraft) {
  return !draft.actor && !draft.operation && !draft.targetType && !draft.outcome
}

function compactIdentifier(value: string) {
  return value.length <= 18 ? value : `${value.slice(0, 8)}…${value.slice(-6)}`
}

function formatChangeSummary(value: string | null) {
  if (!value) return undefined
  try {
    const parsed: unknown = JSON.parse(value)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return value
    return Object.entries(parsed)
      .map(([key, item]) => `${key}=${typeof item === 'string' ? item : JSON.stringify(item)}`)
      .join(' · ')
  } catch {
    return value
  }
}

function AuditLoading() {
  return <div className="overflow-hidden rounded-lg border border-[var(--hairline)]" role="status"><Skeleton className="h-9 rounded-none" />{[0, 1, 2, 3, 4].map((item) => <Skeleton key={item} className="h-[5rem] rounded-none border-t border-[var(--hairline)]" />)}</div>
}

function AuditError({ forbidden, onRetry }: { forbidden: boolean; onRetry: () => void }) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
      <h3 className="text-sm font-semibold text-destructive">{t(forbidden ? 'organizationActivity.errors.forbiddenTitle' : 'organizationActivity.errors.auditTitle')}</h3>
      <p className="mt-1 text-xs text-muted-foreground">{t(forbidden ? 'organizationActivity.errors.forbiddenBody' : 'organizationActivity.errors.auditBody')}</p>
      {!forbidden ? <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={onRetry}>{t('organizationActivity.actions.retry')}</Button> : null}
    </div>
  )
}
