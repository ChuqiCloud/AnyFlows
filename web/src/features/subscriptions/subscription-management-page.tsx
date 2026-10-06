import { Layers3, Plus, RefreshCw, UsersRound } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { DataTablePagination, DEFAULT_TABLE_PAGE_SIZE } from '@/components/data-table/data-table-pagination'
import { useLoadedCursorPagination } from '@/components/data-table/use-table-pagination'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminSubscriptionPlan } from '@/lib/api/generated/types.gen'
import { useAdminSubscriptionPlans } from './subscription-api'
import { SubscriptionPlanCreateSheet } from './subscription-plan-create-sheet'
import { SubscriptionPlanDisableDialog } from './subscription-plan-disable-dialog'
import { SubscriptionPlanTable } from './subscription-plan-table'
import { SubscriptionUserPanel } from './subscription-user-panel'

/** 管理员维护计划目录，并按真实用户主体创建订阅绑定。 */
export function SubscriptionManagementPage() {
  const { t } = useTranslation()
  const [mode, setMode] = useState<'plans' | 'users'>('plans')
  const [pageSize, setPageSize] = useState(DEFAULT_TABLE_PAGE_SIZE)
  const plansQuery = useAdminSubscriptionPlans(pageSize)
  const pagination = useLoadedCursorPagination({
    availablePageCount: plansQuery.data?.pages.length ?? 1,
    pageSize,
    onPageSizeChange: setPageSize,
  })
  const loadedPlans = plansQuery.data?.pages.flatMap((page) => page.plans) ?? []
  const plans = plansQuery.data?.pages[pagination.pageIndex]?.plans ?? []
  const activePlans = plans.filter((plan) => plan.status === 'active')
  const [creating, setCreating] = useState(false)
  const [disabling, setDisabling] = useState<AdminSubscriptionPlan>()

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <Layers3 className="size-3.5" aria-hidden="true" />
            {t('subscriptions.managementEyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('subscriptions.managementTitle')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('subscriptions.managementSubtitle')}
          </p>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <div className="flex items-center rounded-lg border border-[var(--hairline)] bg-surface-2/45 p-0.5" role="tablist" aria-label={t('subscriptions.views.label')}>
            <Button type="button" size="sm" variant={mode === 'plans' ? 'secondary' : 'ghost'} role="tab" aria-selected={mode === 'plans'} onClick={() => setMode('plans')}>
              <Layers3 aria-hidden="true" />
              {t('subscriptions.views.plans')}
            </Button>
            <Button type="button" size="sm" variant={mode === 'users' ? 'secondary' : 'ghost'} role="tab" aria-selected={mode === 'users'} onClick={() => setMode('users')}>
              <UsersRound aria-hidden="true" />
              {t('subscriptions.views.users')}
            </Button>
          </div>
          {mode === 'plans' ? (
            <>
              <Button type="button" size="sm" variant="secondary" disabled={plansQuery.isFetching} onClick={() => void plansQuery.refetch()}>
                <RefreshCw className={plansQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
                {t('subscriptions.actions.refresh')}
              </Button>
              <Button type="button" size="sm" onClick={() => setCreating(true)}>
                <Plus aria-hidden="true" />
                {t('subscriptions.actions.createPlan')}
              </Button>
            </>
          ) : null}
        </div>
      </header>

      {mode === 'users' ? (
        <SubscriptionUserPanel plans={loadedPlans} plansQuery={plansQuery} />
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
            <span>{t('subscriptions.summary.plansLoaded', { count: plans.length })}</span>
            <span>{t('subscriptions.summary.plansActive', { count: activePlans.length })}</span>
            <span>{t('subscriptions.summary.planPages', { count: plansQuery.data?.pages.length ?? 1 })}</span>
          </div>

          {plansQuery.isPending ? (
            <div className="grid gap-2" aria-label={t('subscriptions.loadingPlans')}>
              {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-20 rounded-lg" />)}
            </div>
          ) : plansQuery.isError && plansQuery.data === undefined ? (
            <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
              <h3 className="text-sm font-semibold text-destructive">{t('subscriptions.errors.plansTitle')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('subscriptions.errors.plansBody')}</p>
              <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void plansQuery.refetch()}>
                <RefreshCw aria-hidden="true" />
                {t('subscriptions.actions.retry')}
              </Button>
            </div>
          ) : (
            <SubscriptionPlanTable plans={plans} onDisable={setDisabling} />
          )}

          {plansQuery.isFetchNextPageError ? (
            <div role="alert" className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
              <span>{t('subscriptions.errors.morePlans')}</span>
              <Button type="button" size="sm" variant="ghost" onClick={() => void plansQuery.fetchNextPage()}>
                {t('subscriptions.actions.retry')}
              </Button>
            </div>
          ) : null}
          {!plansQuery.isPending && !(plansQuery.isError && plansQuery.data === undefined) ? (
            <DataTablePagination
              currentPage={pagination.currentPage}
              availablePageCount={pagination.availablePageCount}
              pageSize={pagination.pageSize}
              itemCount={plans.length}
              hasNextPage={pagination.hasLoadedNextPage || Boolean(plansQuery.hasNextPage)}
              fetching={plansQuery.isFetching}
              onFirstPage={pagination.goToFirstPage}
              onPreviousPage={pagination.goToPreviousPage}
              onPageSelect={pagination.selectPage}
              onNextPage={() => void pagination.goToNextPage(Boolean(plansQuery.hasNextPage), async () => (await plansQuery.fetchNextPage()).isSuccess)}
              onPageSizeChange={pagination.setPageSize}
            />
          ) : null}
        </>
      )}

      <SubscriptionPlanCreateSheet open={creating} onOpenChange={setCreating} />
      <SubscriptionPlanDisableDialog plan={disabling} onClose={() => setDisabling(undefined)} />
    </div>
  )
}
