import {
  Ban,
  BarChart3,
  CalendarDays,
  Clock3,
  Filter,
  RefreshCw,
  RotateCcw,
  Search,
  Ticket,
} from 'lucide-react'
import { type FormEvent, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { DataTablePagination, DEFAULT_TABLE_PAGE_SIZE } from '@/components/data-table/data-table-pagination'
import { useLoadedCursorPagination } from '@/components/data-table/use-table-pagination'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { AdminRedemptionAuditBatch } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { useAdminRedemptionAudit } from './redemption-api'
import {
  type RedemptionAuditFilterDraft,
  type RedemptionAuditFilters,
  type RedemptionAuditStatusFilter,
  parseRedemptionAuditFilters,
} from './redemption-audit-model'

const initialDraft: RedemptionAuditFilterDraft = {
  status: 'all',
  batchId: '',
  redeemedAfter: '',
  redeemedBefore: '',
}

/** 管理员只读审计视图，展示批次统计而不暴露兑换码明文或摘要。 */
export function RedemptionAuditPanel() {
  const { i18n, t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const [draft, setDraft] = useState<RedemptionAuditFilterDraft>(initialDraft)
  const [filters, setFilters] = useState<RedemptionAuditFilters>({})
  const [pageSize, setPageSize] = useState(DEFAULT_TABLE_PAGE_SIZE)
  const validation = useMemo(() => parseRedemptionAuditFilters(draft), [draft])
  const query = useAdminRedemptionAudit(filters, pageSize)
  const pagination = useLoadedCursorPagination({
    availablePageCount: query.data?.pages.length ?? 1,
    pageSize,
    onPageSizeChange: setPageSize,
  })
  const currentPage = query.data?.pages[pagination.pageIndex]
  const batches = currentPage?.batches ?? []
  const summary = currentPage?.summary ?? {
    issued_count: 0,
    redeemed_count: 0,
    remaining_count: 0,
    expired_count: 0,
    disabled_count: 0,
  }
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const numberFormat = useMemo(() => new Intl.NumberFormat(locale), [locale])
  const timeFormat = useMemo(() => new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }), [locale])

  function applyFilters(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (validation.ok) {
      pagination.reset()
      setFilters(validation.filters)
    }
  }

  function resetFilters() {
    setDraft({ ...initialDraft })
    pagination.reset()
    setFilters({})
  }

  return (
    <section className="grid gap-4" aria-labelledby="redemption-audit-heading">
      <div className="flex flex-col gap-3 rounded-lg border border-[var(--hairline)] bg-surface-1 p-4">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex items-start gap-3">
            <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-info/10 text-info">
              <BarChart3 className="size-4" aria-hidden="true" />
            </div>
            <div>
              <h3 id="redemption-audit-heading" className="text-sm font-semibold">{t('redemptions.audit.title')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('redemptions.audit.description')}</p>
            </div>
          </div>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={query.isFetching}
            onClick={() => void query.refetch()}
          >
            <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('redemptions.actions.refresh')}
          </Button>
        </div>

        <form className="grid gap-3 border-t border-[var(--hairline)] pt-3" onSubmit={applyFilters}>
          <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)]">
            <label className="grid gap-1.5 text-xs">
              <span className="flex items-center gap-1.5 font-medium"><Filter className="size-3.5 text-muted-foreground" aria-hidden="true" />{t('redemptions.audit.filters.status')}</span>
              <Select
                value={draft.status}
                onChange={(event) => setDraft((current) => ({
                  ...current,
                  status: event.target.value as RedemptionAuditStatusFilter,
                }))}
              >
                <option value="all">{t('redemptions.audit.status.all')}</option>
                <option value="active">{t('redemptions.status.active')}</option>
                <option value="expired">{t('redemptions.status.expired')}</option>
                <option value="disabled">{t('redemptions.status.disabled')}</option>
                <option value="redeemed">{t('redemptions.audit.status.redeemed')}</option>
              </Select>
            </label>
            <label className="grid gap-1.5 text-xs">
              <span className="font-medium">{t('redemptions.audit.filters.batchId')}</span>
              <Input
                aria-invalid={!validation.ok && validation.error === 'invalidBatchId'}
                value={draft.batchId}
                maxLength={32}
                placeholder={t('redemptions.audit.filters.batchIdPlaceholder')}
                onChange={(event) => setDraft((current) => ({
                  ...current,
                  batchId: event.target.value,
                }))}
              />
            </label>
            <div className="grid gap-1.5 text-xs">
              <span className="font-medium">{t('redemptions.audit.filters.redeemedWindow')}</span>
              <div className="grid gap-2 sm:grid-cols-2">
                <label className="relative">
                  <CalendarDays className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
                  <Input
                    aria-invalid={!validation.ok && validation.error !== 'invalidBatchId'}
                    aria-label={t('redemptions.audit.filters.redeemedAfter')}
                    type="datetime-local"
                    className="pl-8"
                    value={draft.redeemedAfter}
                    onChange={(event) => setDraft((current) => ({
                      ...current,
                      redeemedAfter: event.target.value,
                    }))}
                  />
                </label>
                <label className="relative">
                  <Clock3 className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
                  <Input
                    aria-invalid={!validation.ok && validation.error !== 'invalidBatchId'}
                    aria-label={t('redemptions.audit.filters.redeemedBefore')}
                    type="datetime-local"
                    className="pl-8"
                    value={draft.redeemedBefore}
                    onChange={(event) => setDraft((current) => ({
                      ...current,
                      redeemedBefore: event.target.value,
                    }))}
                  />
                </label>
              </div>
            </div>
          </div>
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <p
              role={validation.ok ? undefined : 'alert'}
              className={cn(
                'text-[0.6875rem] leading-5',
                validation.ok ? 'text-muted-foreground' : 'text-destructive',
              )}
            >
              {t(validation.ok
                ? 'redemptions.audit.filters.hint'
                : `redemptions.audit.filters.${validation.error}`)}
            </p>
            <div className="flex shrink-0 justify-end gap-2">
              <Button type="button" size="sm" variant="ghost" onClick={resetFilters}>
                <RotateCcw aria-hidden="true" />
                {t('redemptions.audit.filters.reset')}
              </Button>
              <Button type="submit" size="sm" disabled={!validation.ok}>
                <Search aria-hidden="true" />
                {t('redemptions.audit.filters.apply')}
              </Button>
            </div>
          </div>
        </form>
      </div>

      <div className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-[var(--hairline)] bg-[var(--hairline)] sm:grid-cols-5">
        <AuditMetric label={t('redemptions.audit.metrics.issued')} value={numberFormat.format(summary.issued_count)} />
        <AuditMetric label={t('redemptions.audit.metrics.redeemed')} value={numberFormat.format(summary.redeemed_count)} tone="success" />
        <AuditMetric label={t('redemptions.audit.metrics.remaining')} value={numberFormat.format(summary.remaining_count)} />
        <AuditMetric label={t('redemptions.audit.metrics.expired')} value={numberFormat.format(summary.expired_count)} tone="warning" />
        <AuditMetric label={t('redemptions.audit.metrics.disabled')} value={numberFormat.format(summary.disabled_count)} tone="muted" />
      </div>

      {query.isPending ? (
        <div className="grid gap-2" aria-label={t('redemptions.audit.loading')}>
          {[0, 1, 2].map((item) => <Skeleton key={item} className="h-24 rounded-lg" />)}
        </div>
      ) : query.isError && batches.length === 0 ? (
        <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
          <h3 className="text-sm font-semibold text-destructive">{t('redemptions.audit.errors.title')}</h3>
          <p className="mt-1 text-xs text-muted-foreground">{t('redemptions.audit.errors.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void query.refetch()}>
            {t('redemptions.actions.retry')}
          </Button>
        </div>
      ) : (
        <>
          <AuditTable batches={batches} numberFormat={numberFormat} timeFormat={timeFormat} formatQuota={formatQuota} />
          {query.isFetchNextPageError ? (
            <div role="alert" className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
              <span>{t('redemptions.audit.errors.more')}</span>
              <Button type="button" size="sm" variant="ghost" onClick={() => void query.fetchNextPage()}>
                {t('redemptions.actions.retry')}
              </Button>
            </div>
          ) : null}
          <DataTablePagination
            currentPage={pagination.currentPage}
            availablePageCount={pagination.availablePageCount}
            pageSize={pagination.pageSize}
            itemCount={batches.length}
            hasNextPage={pagination.hasLoadedNextPage || Boolean(query.hasNextPage)}
            fetching={query.isFetching}
            onFirstPage={pagination.goToFirstPage}
            onPreviousPage={pagination.goToPreviousPage}
            onPageSelect={pagination.selectPage}
            onNextPage={() => void pagination.goToNextPage(Boolean(query.hasNextPage), async () => (await query.fetchNextPage()).isSuccess)}
            onPageSizeChange={pagination.setPageSize}
          />
        </>
      )}
      <p className="text-[0.6875rem] leading-5 text-muted-foreground">{t('redemptions.audit.privacy')}</p>
    </section>
  )
}

function AuditMetric({ label, value, tone = 'default' }: { label: string; value: string; tone?: 'default' | 'success' | 'warning' | 'muted' }) {
  return (
    <div className="bg-surface-1 px-3 py-3">
      <div className="text-[0.6875rem] text-muted-foreground">{label}</div>
      <div className={cn(
        'mt-1 text-base font-semibold tabular-nums',
        tone === 'success' && 'text-success',
        tone === 'warning' && 'text-warning',
        tone === 'muted' && 'text-muted-foreground',
      )}>{value}</div>
    </div>
  )
}

function AuditTable({
  batches,
  numberFormat,
  timeFormat,
  formatQuota,
}: {
  batches: AdminRedemptionAuditBatch[]
  numberFormat: Intl.NumberFormat
  timeFormat: Intl.DateTimeFormat
  formatQuota: (value: number) => string
}) {
  const { t } = useTranslation()
  const formatTime = (value: number | null) => value === null ? t('redemptions.values.never') : timeFormat.format(value * 1_000)
  if (batches.length === 0) {
    return (
      <div className="grid min-h-56 place-items-center border-y border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><Ticket className="size-4" aria-hidden="true" /></div>
          <h3 className="mt-3 text-sm font-semibold">{t('redemptions.audit.empty.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('redemptions.audit.empty.body')}</p>
        </div>
      </div>
    )
  }
  return (
    <>
      <div className="hidden overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr>
              <th className="w-[26%] px-3 py-2 font-medium">{t('redemptions.audit.columns.batch')}</th>
              <th className="w-[16%] px-3 py-2 font-medium">{t('redemptions.audit.columns.issued')}</th>
              <th className="w-[25%] px-3 py-2 font-medium">{t('redemptions.audit.columns.states')}</th>
              <th className="w-[23%] px-3 py-2 font-medium">{t('redemptions.audit.columns.redeemedAt')}</th>
              <th className="w-[10%] px-3 py-2 font-medium">{t('redemptions.audit.columns.status')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {batches.map((batch) => <AuditRow key={batch.batch_id} batch={batch} numberFormat={numberFormat} formatQuota={formatQuota} formatTime={formatTime} />)}
          </tbody>
        </table>
      </div>
      <div className="grid gap-2 md:hidden">
        {batches.map((batch) => (
          <article key={batch.batch_id} className="rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3"><AuditIdentity batch={batch} /><AuditState batch={batch} /></div>
            <div className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3 text-xs">
              <AuditStateCounts batch={batch} numberFormat={numberFormat} />
              <div><div className="text-muted-foreground">{t('redemptions.audit.columns.redeemedAt')}</div><div className="mt-1 font-medium">{formatTime(batch.last_redeemed_at)}</div></div>
            </div>
          </article>
        ))}
      </div>
    </>
  )
}

function AuditRow({ batch, numberFormat, formatQuota, formatTime }: { batch: AdminRedemptionAuditBatch; numberFormat: Intl.NumberFormat; formatQuota: (value: number) => string; formatTime: (value: number | null) => string }) {
  return (
    <tr className="hover:bg-surface-2/35">
      <td className="px-3 py-3"><AuditIdentity batch={batch} /></td>
      <td className="px-3 py-3"><div className="font-medium tabular-nums">{numberFormat.format(batch.issued_count)}</div><div className="mt-1 text-[0.6875rem] text-muted-foreground">{formatQuota(batch.quota_amount)}</div></td>
      <td className="px-3 py-3"><AuditStateCounts batch={batch} numberFormat={numberFormat} /></td>
      <td className="px-3 py-3"><div className="flex items-center gap-1.5 font-medium"><Clock3 className="size-3.5 text-muted-foreground" aria-hidden="true" />{formatTime(batch.last_redeemed_at)}</div><div className="mt-1 text-[0.6875rem] text-muted-foreground">{formatTime(batch.created_at)}</div></td>
      <td className="px-3 py-3"><AuditState batch={batch} /></td>
    </tr>
  )
}

function AuditIdentity({ batch }: { batch: AdminRedemptionAuditBatch }) {
  const { t } = useTranslation()
  return <div className="min-w-0"><div className="truncate font-medium">{batch.name}</div><div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground" title={batch.batch_id}>{batch.batch_id.slice(0, 8)} · {t('redemptions.values.creator', { id: batch.created_by_user_id })}</div></div>
}

function AuditState({ batch }: { batch: AdminRedemptionAuditBatch }) {
  const { t } = useTranslation()
  const state = batch.status === 'disabled' ? 'disabled' : batch.expires_at !== null && batch.expires_at <= Math.floor(Date.now() / 1_000) ? 'expired' : 'active'
  return <Badge className={cn('border-transparent', state === 'active' && 'bg-success/10 text-success', state === 'expired' && 'bg-warning/10 text-warning', state === 'disabled' && 'bg-surface-2 text-muted-foreground')}><span className="inline-flex items-center gap-1">{state === 'disabled' ? <Ban className="size-3" aria-hidden="true" /> : null}{t(`redemptions.status.${state}`)}</span></Badge>
}

function AuditStateCounts({ batch, numberFormat }: { batch: AdminRedemptionAuditBatch; numberFormat: Intl.NumberFormat }) {
  const { t } = useTranslation()
  return <div className="grid gap-1 tabular-nums"><span className="text-success">{t('redemptions.audit.values.redeemed', { count: numberFormat.format(batch.redeemed_count) })}</span><span>{t('redemptions.audit.values.remaining', { count: numberFormat.format(batch.remaining_count) })}</span><span className="text-warning">{t('redemptions.audit.values.invalid', { count: numberFormat.format(batch.expired_count + batch.disabled_count) })}</span></div>
}
