import { BarChart3, List, Plus, RefreshCw, Ticket } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { DataTablePagination, DEFAULT_TABLE_PAGE_SIZE } from '@/components/data-table/data-table-pagination'
import { useLoadedCursorPagination } from '@/components/data-table/use-table-pagination'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminRedemptionBatch, IssuedAdminRedemptionBatch } from '@/lib/api/generated/types.gen'
import { useAdminRedemptionBatches } from './redemption-api'
import { RedemptionCreateSheet } from './redemption-create-sheet'
import { RedemptionDisableDialog } from './redemption-disable-dialog'
import { RedemptionIssuedDialog } from './redemption-issued-dialog'
import { RedemptionTable } from './redemption-table'
import { RedemptionAuditPanel } from './redemption-audit-panel'

/** 管理员维护兑换码批次，明文签发与后续状态查询严格分离。 */
export function RedemptionPage() {
  const { t } = useTranslation()
  const [mode, setMode] = useState<'batches' | 'audit'>('batches')
  const [pageSize, setPageSize] = useState(DEFAULT_TABLE_PAGE_SIZE)
  const batchesQuery = useAdminRedemptionBatches(pageSize)
  const pagination = useLoadedCursorPagination({
    availablePageCount: batchesQuery.data?.pages.length ?? 1,
    pageSize,
    onPageSizeChange: setPageSize,
  })
  const [creating, setCreating] = useState(false)
  const [issued, setIssued] = useState<IssuedAdminRedemptionBatch>()
  const [disabling, setDisabling] = useState<AdminRedemptionBatch>()
  const batches = batchesQuery.data?.pages[pagination.pageIndex]?.batches ?? []
  const now = Math.floor(Date.now() / 1_000)
  const activeCount = batches.filter((batch) => (
    batch.status === 'active' && (batch.expires_at === null || batch.expires_at > now)
  )).length
  const redeemedCount = batches.reduce((total, batch) => total + batch.redeemed_count, 0)
  const availableCount = batches.reduce((total, batch) => (
    total + Math.max(0, batch.code_count - batch.redeemed_count)
  ), 0)

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <Ticket className="size-3.5" aria-hidden="true" />
            {t('redemptions.eyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('redemptions.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('redemptions.subtitle')}</p>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <div className="flex items-center rounded-lg border border-[var(--hairline)] bg-surface-2/45 p-0.5" role="tablist" aria-label={t('redemptions.views.label')}>
            <Button type="button" size="sm" variant={mode === 'batches' ? 'secondary' : 'ghost'} role="tab" aria-selected={mode === 'batches'} onClick={() => setMode('batches')}>
              <List aria-hidden="true" />
              {t('redemptions.views.batches')}
            </Button>
            <Button type="button" size="sm" variant={mode === 'audit' ? 'secondary' : 'ghost'} role="tab" aria-selected={mode === 'audit'} onClick={() => setMode('audit')}>
              <BarChart3 aria-hidden="true" />
              {t('redemptions.views.audit')}
            </Button>
          </div>
          {mode === 'batches' ? (
            <>
              <Button type="button" size="sm" variant="secondary" disabled={batchesQuery.isFetching} onClick={() => void batchesQuery.refetch()}>
                <RefreshCw className={batchesQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
                {t('redemptions.actions.refresh')}
              </Button>
              <Button type="button" size="sm" onClick={() => setCreating(true)}>
                <Plus aria-hidden="true" />
                {t('redemptions.actions.create')}
              </Button>
            </>
          ) : null}
        </div>
      </header>

      {mode === 'audit' ? <RedemptionAuditPanel /> : (
        <>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
            <span>{t('redemptions.summary.loaded', { count: batches.length })}</span>
            <span>{t('redemptions.summary.active', { count: activeCount })}</span>
            <span>{t('redemptions.summary.redeemed', { count: redeemedCount })}</span>
            <span>{t('redemptions.summary.available', { count: availableCount })}</span>
          </div>

          {batchesQuery.isPending ? (
            <div className="grid gap-2" aria-label={t('redemptions.loading')}>
              {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-20 rounded-lg" />)}
            </div>
          ) : batchesQuery.isError && batches.length === 0 ? (
            <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
              <h2 className="text-sm font-semibold text-destructive">{t('redemptions.errors.listTitle')}</h2>
              <p className="mt-1 text-xs text-muted-foreground">{t('redemptions.errors.listBody')}</p>
              <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void batchesQuery.refetch()}>
                {t('redemptions.actions.retry')}
              </Button>
            </div>
          ) : (
            <>
              <RedemptionTable batches={batches} onDisable={setDisabling} />
              {batchesQuery.isFetchNextPageError ? (
                <div role="alert" className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
                  <span>{t('redemptions.errors.more')}</span>
                  <Button type="button" size="sm" variant="ghost" onClick={() => void batchesQuery.fetchNextPage()}>
                    {t('redemptions.actions.retry')}
                  </Button>
                </div>
              ) : null}
              <DataTablePagination
                currentPage={pagination.currentPage}
                availablePageCount={pagination.availablePageCount}
                pageSize={pagination.pageSize}
                itemCount={batches.length}
                hasNextPage={pagination.hasLoadedNextPage || Boolean(batchesQuery.hasNextPage)}
                fetching={batchesQuery.isFetching}
                onFirstPage={pagination.goToFirstPage}
                onPreviousPage={pagination.goToPreviousPage}
                onPageSelect={pagination.selectPage}
                onNextPage={() => void pagination.goToNextPage(Boolean(batchesQuery.hasNextPage), async () => (await batchesQuery.fetchNextPage()).isSuccess)}
                onPageSizeChange={pagination.setPageSize}
              />
            </>
          )}
        </>
      )}

      <RedemptionCreateSheet open={creating} onOpenChange={setCreating} onIssued={setIssued} />
      <RedemptionIssuedDialog issued={issued} onClose={() => setIssued(undefined)} />
      <RedemptionDisableDialog batch={disabling} onClose={() => setDisabling(undefined)} />
    </div>
  )
}
