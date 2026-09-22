import { Button, Chip, Input, Select, SelectItem, Skeleton } from '@heroui/react'
import { useMemo, useState, type FormEvent } from 'react'
import { Fingerprint, RefreshCw, Search, ShieldCheck, SlidersHorizontal, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useCursorPagination } from '@/components/data-table/use-table-pagination'
import { cn } from '@/lib/utils'
import { formatUsageDateLong } from '@/features/usage-logs/usage-log-format'
import type { OrganizationScimAuditLogResponse } from '@/lib/api/generated/types.gen'
import {
  organizationActivityErrorStatus,
  type OrganizationScimAuditFilters,
  useOrganizationScimAuditLogs,
} from './organization-activity-api'

type ScimAuditDraft = {
  eventType: string
  result: string
  reason: string
  resourceKind: string
  tokenKey: string
}

const emptyDraft: ScimAuditDraft = {
  eventType: '',
  result: '',
  reason: '',
  resourceKind: '',
  tokenKey: '',
}

const eventTypes = [
  'token_issued',
  'token_revoked',
  'request_accepted',
  'request_rejected',
  'resource_synchronized',
  'lifecycle_transition',
  'rate_limit_rejected',
] as const

const results = ['applied', 'noop', 'rejected', 'failed'] as const

const resourceKinds = ['users', 'groups'] as const

// HeroUI Select 不接受空字符串 key，用哨兵 key 表达“全部”。
const ALL_KEY = '__all__'

const toSelectItems = (values: readonly string[], label: (value: string) => string) => [
  { key: ALL_KEY, label: label(ALL_KEY) },
  ...values.map((value) => ({ key: value, label: label(value) })),
]

const reasons = [
  'invalid_token',
  'token_revoked',
  'token_expired',
  'cross_organization',
  'stale_resource',
  'entitlement_unavailable',
  'seat_capacity_exceeded',
  'rate_limit_exceeded',
  'storage_unavailable',
  'invalid_request',
  'internal_failure',
] as const

/** 读取企业范围 SCIM 脱敏审计，筛选在提交后才触发请求。 */
export function OrganizationScimAuditPanel({ organizationId }: { organizationId: number }) {
  const { t } = useTranslation()
  const pagination = useCursorPagination<number>()
  const [draft, setDraft] = useState<ScimAuditDraft>(emptyDraft)
  const [filters, setFilters] = useState<OrganizationScimAuditFilters>({})
  const query = useOrganizationScimAuditLogs(
    organizationId,
    pagination.cursor,
    pagination.pageSize,
    filters,
    true,
  )
  const logs = useMemo(() => query.data?.logs ?? [], [query.data?.logs])
  const nextCursor = query.data?.nextCursor
  const tokenKeyValid = draft.tokenKey.trim().length === 0 || /^[0-9a-f]{32}$/.test(draft.tokenKey.trim())
  const hasFilters = Object.values(filters).some((value) => value !== undefined)

  const applyFilters = (event: FormEvent) => {
    event.preventDefault()
    if (!tokenKeyValid) return
    setFilters({
      eventType: draft.eventType || undefined,
      result: draft.result || undefined,
      reason: draft.reason || undefined,
      resourceKind: draft.resourceKind || undefined,
      tokenKey: draft.tokenKey.trim() || undefined,
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
          <span className="inline-flex items-center gap-1.5"><SlidersHorizontal className="size-3.5" aria-hidden="true" />{t('organizationActivity.scim.filters.label')}</span>
          <span>{t('organizationActivity.scim.loaded', { count: logs.length })}</span>
        </div>
        <div className="grid gap-2 md:grid-cols-[repeat(4,minmax(9rem,1fr))_minmax(12rem,1.2fr)_auto]">
          <label>
            <span className="sr-only">{t('organizationActivity.scim.filters.event')}</span>
            <Select
              aria-label={t('organizationActivity.scim.filters.event')}
              className="h-8 text-xs"
              items={toSelectItems(eventTypes, (value) => (value === ALL_KEY ? t('organizationActivity.scim.filters.allEvents') : t(`organizationActivity.scim.events.${value}`)))}
              selectedKeys={[draft.eventType === '' ? ALL_KEY : draft.eventType]}
              size="sm"
              onSelectionChange={(keys) => {
                const next = String(Array.from(keys)[0] ?? ALL_KEY)
                setDraft((current) => ({ ...current, eventType: next === ALL_KEY ? '' : next }))
              }}
            >
              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
            </Select>
          </label>
          <label>
            <span className="sr-only">{t('organizationActivity.scim.filters.result')}</span>
            <Select
              aria-label={t('organizationActivity.scim.filters.result')}
              className="h-8 text-xs"
              items={toSelectItems(results, (value) => (value === ALL_KEY ? t('organizationActivity.scim.filters.allResults') : t(`organizationActivity.scim.results.${value}`)))}
              selectedKeys={[draft.result === '' ? ALL_KEY : draft.result]}
              size="sm"
              onSelectionChange={(keys) => {
                const next = String(Array.from(keys)[0] ?? ALL_KEY)
                setDraft((current) => ({ ...current, result: next === ALL_KEY ? '' : next }))
              }}
            >
              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
            </Select>
          </label>
          <label>
            <span className="sr-only">{t('organizationActivity.scim.filters.reason')}</span>
            <Select
              aria-label={t('organizationActivity.scim.filters.reason')}
              className="h-8 text-xs"
              items={toSelectItems(reasons, (value) => (value === ALL_KEY ? t('organizationActivity.scim.filters.allReasons') : t(`organizationActivity.scim.reasons.${value}`)))}
              selectedKeys={[draft.reason === '' ? ALL_KEY : draft.reason]}
              size="sm"
              onSelectionChange={(keys) => {
                const next = String(Array.from(keys)[0] ?? ALL_KEY)
                setDraft((current) => ({ ...current, reason: next === ALL_KEY ? '' : next }))
              }}
            >
              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
            </Select>
          </label>
          <label>
            <span className="sr-only">{t('organizationActivity.scim.filters.resource')}</span>
            <Select
              aria-label={t('organizationActivity.scim.filters.resource')}
              className="h-8 text-xs"
              items={toSelectItems(resourceKinds, (value) => (value === ALL_KEY ? t('organizationActivity.scim.filters.allResources') : t(`organizationActivity.scim.resources.${value}`)))}
              selectedKeys={[draft.resourceKind === '' ? ALL_KEY : draft.resourceKind]}
              size="sm"
              onSelectionChange={(keys) => {
                const next = String(Array.from(keys)[0] ?? ALL_KEY)
                setDraft((current) => ({ ...current, resourceKind: next === ALL_KEY ? '' : next }))
              }}
            >
              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
            </Select>
          </label>
          <label>
            <span className="sr-only">{t('organizationActivity.scim.filters.token')}</span>
            <Input
              className="h-8"
              classNames={{ input: 'font-mono text-xs' }}
              inputMode="text"
              isInvalid={!tokenKeyValid}
              maxLength={32}
              placeholder={t('organizationActivity.scim.filters.tokenPlaceholder')}
              size="sm"
              startContent={<Search className="pointer-events-none size-3.5 text-muted-foreground" aria-hidden="true" />}
              value={draft.tokenKey}
              onChange={(event) => setDraft((current) => ({ ...current, tokenKey: event.target.value }))}
            />
          </label>
          <div className="flex items-center gap-1">
            <Button type="submit" color="primary" size="sm" isDisabled={!tokenKeyValid}><Search className="size-3.5" aria-hidden="true" />{t('organizationActivity.actions.apply')}</Button>
            <Button isIconOnly type="button" size="sm" variant="light" isDisabled={!hasFilters && draftIsEmpty(draft)} aria-label={t('organizationActivity.actions.reset')} onClick={resetFilters}><X className="size-3.5" aria-hidden="true" /></Button>
          </div>
        </div>
        {!tokenKeyValid ? <p className="mt-2 text-[0.6875rem] text-destructive">{t('organizationActivity.scim.filters.tokenInvalid')}</p> : null}
      </form>

      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
        <span className="inline-flex items-center gap-1.5"><ShieldCheck className="size-3.5" aria-hidden="true" />{t('organizationActivity.scim.privacy')}</span>
        <Button type="button" size="sm" variant="bordered" isDisabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw className={cn('size-3.5', query.isFetching && 'animate-spin')} aria-hidden="true" />
          {t('organizationActivity.actions.refresh')}
        </Button>
      </div>

      {query.isPending ? (
        <ScimAuditLoading />
      ) : query.isError ? (
        <ScimAuditError forbidden={organizationActivityErrorStatus(query.error) === 403} onRetry={() => void query.refetch()} />
      ) : (
        <OrganizationScimAuditTable logs={logs} filtered={hasFilters} />
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

function OrganizationScimAuditTable({ logs, filtered }: { logs: OrganizationScimAuditLogResponse[]; filtered: boolean }) {
  const { t } = useTranslation()
  if (logs.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center rounded-lg border border-[var(--hairline)] text-center">
        <div className="max-w-sm px-5">
          <ShieldCheck className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
          <h3 className="mt-3 text-sm font-semibold">{t(filtered ? 'organizationActivity.scim.emptyFilteredTitle' : 'organizationActivity.scim.emptyTitle')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t(filtered ? 'organizationActivity.scim.emptyFilteredBody' : 'organizationActivity.scim.emptyBody')}</p>
        </div>
      </div>
    )
  }
  return (
    <section className="overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1" aria-label={t('organizationActivity.scim.listLabel')}>
      <div className="hidden grid-cols-[8rem_minmax(11rem,1fr)_minmax(11rem,1fr)_minmax(12rem,1fr)_minmax(10rem,1fr)] gap-3 border-b border-[var(--hairline)] bg-surface-2/60 px-4 py-2 text-[0.6875rem] font-medium text-muted-foreground lg:grid">
        <span>{t('organizationActivity.scim.columns.time')}</span>
        <span>{t('organizationActivity.scim.columns.event')}</span>
        <span>{t('organizationActivity.scim.columns.result')}</span>
        <span>{t('organizationActivity.scim.columns.resource')}</span>
        <span>{t('organizationActivity.scim.columns.trace')}</span>
      </div>
      {logs.map((log) => <OrganizationScimAuditItem key={log.id} log={log} />)}
    </section>
  )
}

function OrganizationScimAuditItem({ log }: { log: OrganizationScimAuditLogResponse }) {
  const { i18n, t } = useTranslation()
  return (
    <article className="grid gap-3 border-b border-[var(--hairline)] px-3 py-3 last:border-b-0 lg:grid-cols-[8rem_minmax(11rem,1fr)_minmax(11rem,1fr)_minmax(12rem,1fr)_minmax(10rem,1fr)] lg:items-center lg:px-4 lg:py-2.5">
      <div className="min-w-0 text-xs tabular-nums">
        <p>{formatUsageDateLong(log.createdAt, i18n.language)}</p>
        <p className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground" title={String(log.id)}>#{log.id}</p>
      </div>
      <div className="min-w-0">
        <p className="truncate font-mono text-xs font-semibold" title={log.eventType}>{t(`organizationActivity.scim.events.${log.eventType}`, { defaultValue: log.eventType })}</p>
        <p className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground">{log.eventType}</p>
      </div>
      <div className="min-w-0">
        <ScimResultBadge result={log.result} />
        {log.reason ? <p className="mt-1 truncate text-[0.6875rem] text-muted-foreground" title={log.reason}>{t(`organizationActivity.scim.reasons.${log.reason}`, { defaultValue: log.reason })}</p> : null}
      </div>
      <div className="min-w-0">
        <p className="truncate text-xs">{log.resourceKind ? t(`organizationActivity.scim.resources.${log.resourceKind}`, { defaultValue: log.resourceKind }) : t('organizationActivity.scim.redacted')}</p>
        {log.resourceDigest ? <p className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground" title={log.resourceDigest}>{t('organizationActivity.scim.version', { version: log.resourceVersion ?? '-' })} · {compactIdentifier(log.resourceDigest)}</p> : null}
      </div>
      <div className="min-w-0">
        <p className="flex min-w-0 items-center gap-1 truncate font-mono text-[0.625rem] text-muted-foreground" title={log.requestFingerprint}><Fingerprint className="size-3 shrink-0" aria-hidden="true" />{compactIdentifier(log.requestFingerprint)}</p>
        {log.tokenKey ? <p className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground" title={log.tokenKey}>{t('organizationActivity.scim.token')} · {compactIdentifier(log.tokenKey)}</p> : null}
        {log.retryAfterSeconds != null ? <p className="mt-1 truncate text-[0.625rem] text-muted-foreground">{t('organizationActivity.scim.retryAfter', { seconds: log.retryAfterSeconds })}</p> : null}
      </div>
    </article>
  )
}

function ScimResultBadge({ result }: { result: OrganizationScimAuditLogResponse['result'] }) {
  const { t } = useTranslation()
  const tone = result === 'applied'
    ? 'border-transparent bg-success/10 text-success'
    : result === 'noop'
      ? 'border-transparent bg-info/10 text-info'
      : result === 'rejected'
        ? 'border-transparent bg-warning/10 text-warning'
        : 'border-transparent bg-destructive/10 text-destructive'
  return <Chip className={cn('w-fit', tone)} size="sm" variant="flat">{t(`organizationActivity.scim.results.${result}`, { defaultValue: result })}</Chip>
}

function draftIsEmpty(draft: ScimAuditDraft) {
  return !draft.eventType && !draft.result && !draft.reason && !draft.resourceKind && !draft.tokenKey
}

function compactIdentifier(value: string) {
  return value.length <= 18 ? value : `${value.slice(0, 8)}…${value.slice(-6)}`
}

function ScimAuditLoading() {
  return <div className="overflow-hidden rounded-lg border border-[var(--hairline)]" role="status"><Skeleton className="h-9 rounded-none" />{[0, 1, 2, 3, 4].map((item) => <Skeleton key={item} className="h-[5rem] rounded-none border-t border-[var(--hairline)]" />)}</div>
}

function ScimAuditError({ forbidden, onRetry }: { forbidden: boolean; onRetry: () => void }) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
      <h3 className="text-sm font-semibold text-destructive">{t(forbidden ? 'organizationActivity.errors.forbiddenTitle' : 'organizationActivity.errors.scimTitle')}</h3>
      <p className="mt-1 text-xs text-muted-foreground">{t(forbidden ? 'organizationActivity.errors.forbiddenBody' : 'organizationActivity.errors.scimBody')}</p>
      {!forbidden ? <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={onRetry}>{t('organizationActivity.actions.retry')}</Button> : null}
    </div>
  )
}
