import { Check, CircleAlert, CreditCard, FileCheck2, RefreshCw, Send, X } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { DataTablePagination, DEFAULT_TABLE_PAGE_SIZE } from '@/components/data-table/data-table-pagination'
import { useLoadedCursorPagination } from '@/components/data-table/use-table-pagination'
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Textarea } from '@/components/ui/textarea'
import type { AdminRefundRequest } from '@/lib/api/generated/types.gen'
import { refundErrorCode, useAdminRefunds, useApproveAdminRefund, useManualCompleteAdminRefund, useRejectAdminRefund, useSubmitAdminRefund } from './refund-api'
import { RefundReconciliationList } from './refund-reconciliation-list'

type Filter = 'all' | 'pending' | 'approved' | 'rejected'
type Decision = { action: 'approve' | 'reject'; request: AdminRefundRequest }
type ManualCompletion = { request: AdminRefundRequest; result: 'completed' | 'failed'; reference: string; completionKey: string }
const EMPTY_REFUND_ENTRIES: AdminRefundRequest[] = []

const statusTone: Record<AdminRefundRequest['status'], string> = {
  requested: 'text-info',
  submitted: 'text-warning',
  succeeded: 'text-success',
  failed: 'text-destructive',
  canceled: 'text-muted-foreground',
  manually_succeeded: 'text-success',
  manually_failed: 'text-destructive',
}

function actionErrorKey(code: string | undefined) {
  return code === 'refund_auto_submit_failed'
    || code === 'refund_outcome_unknown'
    || code === 'refund_unavailable'
    || code === 'refund_conflict'
    ? code
    : 'unknown'
}

function newCompletionKey() {
  const bytes = new Uint8Array(16)
  crypto.getRandomValues(bytes)
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
}

/** 管理员退款审批工作台，所有资金动作都通过服务端原请求 CAS 完成。 */
export function RefundPage() {
  const { t } = useTranslation()
  const [mode, setMode] = useState<'approvals' | 'reconciliation'>('approvals')
  const [filter, setFilter] = useState<Filter>('pending')
  const [pageSize, setPageSize] = useState(DEFAULT_TABLE_PAGE_SIZE)
  const [decision, setDecision] = useState<Decision>()
  const [manual, setManual] = useState<ManualCompletion>()
  const [reason, setReason] = useState('')
  const query = useAdminRefunds(filter === 'all' ? undefined : filter, pageSize)
  const pagination = useLoadedCursorPagination({
    availablePageCount: query.data?.pages.length ?? 1,
    pageSize,
    onPageSizeChange: setPageSize,
  })
  const entries = query.data?.pages[pagination.pageIndex]?.entries ?? EMPTY_REFUND_ENTRIES
  const approve = useApproveAdminRefund()
  const reject = useRejectAdminRefund()
  const submit = useSubmitAdminRefund()
  const manualComplete = useManualCompleteAdminRefund()
  const actionError = approve.error ?? reject.error ?? submit.error ?? manualComplete.error
  const errorCode = refundErrorCode(actionError)
  const errorKey = actionErrorKey(errorCode)
  const busy = approve.isPending || reject.isPending || submit.isPending || manualComplete.isPending
  const summary = useMemo(() => ({
    pending: entries.filter((entry) => entry.approval_status === 'pending').length,
    approved: entries.filter((entry) => entry.approval_status === 'approved').length,
    succeeded: entries.filter((entry) => entry.status === 'succeeded' || entry.status === 'manually_succeeded').length,
  }), [entries])

  const openDecision = (action: Decision['action'], request: AdminRefundRequest) => {
    setReason('')
    setDecision({ action, request })
  }

  const confirmDecision = () => {
    if (!decision) return
    const mutation = decision.action === 'approve' ? approve : reject
    mutation.mutate({ requestId: decision.request.request_id, reason }, {
      onSuccess: () => setDecision(undefined),
    })
  }

  const confirmManualCompletion = () => {
    if (!manual || !manual.reference.trim()) return
    manualComplete.mutate({
      requestId: manual.request.request_id,
      body: {
        completion_key: manual.completionKey,
        expected_version: manual.request.version,
        result: manual.result,
        reference: manual.reference.trim(),
      },
    }, { onSuccess: () => setManual(undefined) })
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <CreditCard className="size-3.5" aria-hidden="true" />
            {t('refunds.eyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('refunds.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('refunds.subtitle')}</p>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <div className="flex items-center rounded-lg border border-[var(--hairline)] bg-surface-2/45 p-0.5" role="tablist" aria-label={t('refunds.views.label')}>
            <Button type="button" size="sm" variant={mode === 'approvals' ? 'secondary' : 'ghost'} role="tab" aria-selected={mode === 'approvals'} onClick={() => setMode('approvals')}><Check aria-hidden="true" />{t('refunds.views.approvals')}</Button>
            <Button type="button" size="sm" variant={mode === 'reconciliation' ? 'secondary' : 'ghost'} role="tab" aria-selected={mode === 'reconciliation'} onClick={() => setMode('reconciliation')}><FileCheck2 aria-hidden="true" />{t('refunds.views.reconciliation')}</Button>
          </div>
          {mode === 'approvals' ? <Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}><RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />{t('refunds.actions.refresh')}</Button> : null}
        </div>
      </header>

      {mode === 'reconciliation' ? <RefundReconciliationList scope="admin" translationPrefix="refunds.reconciliation" /> : (
      <>

      <div className="flex flex-wrap items-center gap-3 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <label className="flex items-center gap-2">
          <span>{t('refunds.filters.label')}</span>
          <Select value={filter} onChange={(event) => { setFilter(event.target.value as Filter); pagination.goToFirstPage() }} className="h-8 w-32 text-xs">
            <option value="pending">{t('refunds.filters.pending')}</option>
            <option value="all">{t('refunds.filters.all')}</option>
            <option value="approved">{t('refunds.filters.approved')}</option>
            <option value="rejected">{t('refunds.filters.rejected')}</option>
          </Select>
        </label>
        <span>{t('refunds.summary.pending', { count: summary.pending })}</span>
        <span>{t('refunds.summary.approved', { count: summary.approved })}</span>
        <span>{t('refunds.summary.succeeded', { count: summary.succeeded })}</span>
      </div>

      {actionError ? (
        <div role="alert" className="flex items-start gap-2 rounded-lg border border-destructive/25 bg-destructive/8 p-3 text-xs">
          <CircleAlert className="mt-0.5 size-4 shrink-0 text-destructive" aria-hidden="true" />
          <div>
            <p className="font-medium text-destructive">{t('refunds.errors.actionTitle')}</p>
            <p className="mt-1 text-muted-foreground">{t(`refunds.errors.${errorKey}`)}</p>
          </div>
        </div>
      ) : null}

      {query.isPending ? (
        <div className="grid gap-2" aria-label={t('refunds.loading')}>
          {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-20 rounded-lg" />)}
        </div>
      ) : query.isError ? (
        <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
          <h3 className="text-sm font-semibold text-destructive">{t('refunds.errors.listTitle')}</h3>
          <p className="mt-1 text-xs text-muted-foreground">{t('refunds.errors.listBody')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void query.refetch()}>{t('refunds.actions.retry')}</Button>
        </div>
      ) : entries.length === 0 ? (
        <div className="rounded-lg border border-dashed border-[var(--hairline)] p-8 text-center text-sm text-muted-foreground">{t('refunds.empty')}</div>
      ) : (
        <>
          <div className="overflow-x-auto rounded-lg border border-[var(--hairline)]">
            <table className="w-full min-w-[860px] text-left text-xs">
              <thead className="bg-surface-2/50 text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 font-medium">{t('refunds.table.request')}</th>
                  <th className="px-3 py-2 font-medium">{t('refunds.table.order')}</th>
                  <th className="px-3 py-2 font-medium">{t('refunds.table.amount')}</th>
                  <th className="px-3 py-2 font-medium">{t('refunds.table.status')}</th>
                  <th className="px-3 py-2 font-medium">{t('refunds.table.approval')}</th>
                  <th className="px-3 py-2 text-right font-medium">{t('refunds.table.actions')}</th>
                </tr>
              </thead>
              <tbody>
                {entries.map((entry) => (
                  <tr key={entry.request_id} className="border-t border-[var(--hairline)] align-top">
                    <td className="px-3 py-3"><code className="font-mono text-[0.6875rem]">{entry.request_id.slice(0, 8)}…</code><div className="mt-1 text-muted-foreground">{t('refunds.user', { id: entry.user_id })}</div></td>
                    <td className="px-3 py-3"><div className="font-medium">{t(`refunds.orderKinds.${entry.order_kind}`)}</div><div className="mt-1 max-w-[180px] truncate font-mono text-muted-foreground">{entry.order_key}</div><div className="mt-1 text-muted-foreground">{entry.provider} · {entry.currency}</div></td>
                    <td className="px-3 py-3 font-mono">{entry.refund_amount_minor.toLocaleString()} <span className="text-muted-foreground">/ {entry.original_amount_minor.toLocaleString()}</span></td>
                    <td className="px-3 py-3"><span className={statusTone[entry.status]}>{t(`refunds.statuses.${entry.status}`)}</span></td>
                    <td className="px-3 py-3"><Badge className={entry.approval_status === 'approved' ? 'text-success' : entry.approval_status === 'rejected' ? 'text-destructive' : 'text-warning'}>{t(`refunds.approval.${entry.approval_status}`)}</Badge>{entry.approval_reason ? <div className="mt-1 max-w-[170px] truncate text-muted-foreground" title={entry.approval_reason}>{entry.approval_reason}</div> : null}</td>
                    <td className="px-3 py-3"><div className="flex justify-end gap-1.5">
                      {entry.approval_status === 'pending' ? <><Button type="button" size="sm" variant="secondary" disabled={busy} onClick={() => openDecision('approve', entry)}><Check aria-hidden="true" />{t('refunds.actions.approve')}</Button><Button type="button" size="icon-sm" variant="ghost" aria-label={t('refunds.actions.reject')} disabled={busy} onClick={() => openDecision('reject', entry)}><X aria-hidden="true" /></Button></> : null}
                      {entry.approval_status === 'approved' && (entry.status === 'requested' || entry.status === 'failed') ? <>
                        {entry.provider === 'epay' ? <Button type="button" size="sm" variant="secondary" disabled={busy} onClick={() => setManual({ request: entry, result: 'completed', reference: '', completionKey: newCompletionKey() })}><Check aria-hidden="true" />{t('refunds.actions.manualComplete')}</Button> : <Button type="button" size="sm" variant="secondary" disabled={busy} onClick={() => submit.mutate(entry.request_id)}><Send aria-hidden="true" />{t('refunds.actions.submit')}</Button>}
                      </> : null}
                    </div></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <DataTablePagination
            currentPage={pagination.currentPage}
            availablePageCount={pagination.availablePageCount}
            pageSize={pagination.pageSize}
            itemCount={entries.length}
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

      <AlertDialog open={decision !== undefined} onOpenChange={(open) => { if (!open && !busy) setDecision(undefined) }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{decision?.action === 'approve' ? t('refunds.confirm.approveTitle') : t('refunds.confirm.rejectTitle')}</AlertDialogTitle>
            <AlertDialogDescription>{decision?.action === 'approve' ? t('refunds.confirm.approveBody') : t('refunds.confirm.rejectBody')}</AlertDialogDescription>
          </AlertDialogHeader>
          <Textarea value={reason} maxLength={512} placeholder={t('refunds.confirm.reasonPlaceholder')} onChange={(event) => setReason(event.target.value)} aria-label={t('refunds.confirm.reasonLabel')} />
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>{t('refunds.actions.cancel')}</AlertDialogCancel>
            <AlertDialogAction disabled={busy} onClick={(event) => { event.preventDefault(); confirmDecision() }}>{decision?.action === 'approve' ? t('refunds.actions.approve') : t('refunds.actions.reject')}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <AlertDialog open={manual !== undefined} onOpenChange={(open) => { if (!open && !manualComplete.isPending) setManual(undefined) }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('refunds.manual.title')}</AlertDialogTitle>
            <AlertDialogDescription>{t('refunds.manual.body')}</AlertDialogDescription>
          </AlertDialogHeader>
          <div className="grid gap-3">
            <label className="grid gap-1.5 text-xs font-medium" htmlFor="refund-manual-result">
              {t('refunds.manual.resultLabel')}
              <Select id="refund-manual-result" value={manual?.result ?? 'completed'} disabled={manualComplete.isPending} onChange={(event) => setManual((current) => current ? { ...current, result: event.target.value as ManualCompletion['result'] } : current)}>
                <option value="completed">{t('refunds.manual.completed')}</option>
                <option value="failed">{t('refunds.manual.failed')}</option>
              </Select>
            </label>
            <label className="grid gap-1.5 text-xs font-medium" htmlFor="refund-manual-reference">
              {t('refunds.manual.referenceLabel')}
              <input id="refund-manual-reference" value={manual?.reference ?? ''} maxLength={256} disabled={manualComplete.isPending} onChange={(event) => setManual((current) => current ? { ...current, reference: event.target.value } : current)} className="h-9 rounded-md border border-[var(--hairline)] bg-background px-3 text-sm font-normal outline-none focus:ring-2 focus:ring-ring" placeholder={t('refunds.manual.referencePlaceholder')} />
            </label>
          </div>
          {manualComplete.isError ? (
            <div role="alert" className="rounded-md border border-destructive/25 bg-destructive/8 p-3 text-xs text-destructive">
              {t(`refunds.errors.${errorKey}`)}
            </div>
          ) : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={manualComplete.isPending}>{t('refunds.actions.cancel')}</AlertDialogCancel>
            <AlertDialogAction disabled={manualComplete.isPending || !manual?.reference.trim()} onClick={(event) => { event.preventDefault(); confirmManualCompletion() }}>{manualComplete.isPending ? t('refunds.manual.submitting') : t('refunds.actions.manualConfirm')}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      </>
      )}
    </div>
  )
}
