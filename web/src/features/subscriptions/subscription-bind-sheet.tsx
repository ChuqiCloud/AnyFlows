import { Link2, LoaderCircle, UsersRound } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { AdminSubscriptionPlan, AdminUser } from '@/lib/api/generated/types.gen'
import {
  subscriptionErrorCode,
  useAdminSubscriptionPlans,
  useBindAdminUserSubscription,
} from './subscription-api'

type SubscriptionPlansQuery = ReturnType<typeof useAdminSubscriptionPlans>

type SubscriptionBindSheetProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  user?: AdminUser
  plans: AdminSubscriptionPlan[]
  plansQuery: SubscriptionPlansQuery
}

/** 给已选用户绑定一个当前有效的不可变计划快照。 */
export function SubscriptionBindSheet({
  open,
  onOpenChange,
  user,
  plans,
  plansQuery,
}: SubscriptionBindSheetProps) {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const mutation = useBindAdminUserSubscription()
  const resetMutation = mutation.reset
  const activePlans = useMemo(() => plans.filter((plan) => plan.status === 'active'), [plans])
  const [planId, setPlanId] = useState('')
  const selectedPlan = activePlans.find((plan) => plan.plan_id === planId)

  useEffect(() => {
    if (!open) return
    setPlanId((current) => (
      activePlans.some((plan) => plan.plan_id === current)
        ? current
        : activePlans[0]?.plan_id ?? ''
    ))
    resetMutation()
  }, [activePlans, open, resetMutation])

  const bind = async () => {
    if (!user || !selectedPlan) return
    await mutation.mutateAsync({
      userId: user.id,
      body: { plan_id: selectedPlan.plan_id },
    })
    onOpenChange(false)
  }
  const errorKey = bindErrorKey(subscriptionErrorCode(mutation.error))

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 overflow-y-auto data-[side=right]:w-full data-[side=right]:sm:max-w-xl" aria-describedby="subscription-bind-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('subscriptions.bind.title')}</SheetTitle>
          <SheetDescription id="subscription-bind-description">
            {t('subscriptions.bind.description')}
          </SheetDescription>
        </SheetHeader>
        <div className="grid gap-5 p-5">
          <span className="grid size-10 place-items-center rounded-lg bg-brand/10 text-brand">
            <Link2 className="size-4" aria-hidden="true" />
          </span>

          <div className="rounded-lg border border-[var(--hairline)] bg-surface-2/45 p-3">
            <div className="flex items-center gap-3">
              <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-surface-2 text-muted-foreground">
                <UsersRound className="size-4" aria-hidden="true" />
              </span>
              <div className="min-w-0">
                <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                  <span className="truncate text-sm font-medium">{user?.username ?? t('subscriptions.user.none')}</span>
                  {user ? <Badge>{t(`subscriptions.user.status.${user.status}`)}</Badge> : null}
                </div>
                <p className="mt-1 truncate text-xs text-muted-foreground">
                  {user?.email ?? t('subscriptions.user.noEmail')}
                </p>
              </div>
            </div>
          </div>

          <div className="grid gap-2">
            <Label htmlFor="subscription-bind-plan">{t('subscriptions.fields.plan')}</Label>
            {plansQuery.isPending ? (
              <p role="status" className="rounded-lg bg-surface-2 px-3 py-2 text-xs text-muted-foreground">
                {t('subscriptions.bind.loadingPlans')}
              </p>
            ) : plansQuery.isError && plansQuery.data === undefined ? (
              <div role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
                {t('subscriptions.errors.bindPlans')}
              </div>
            ) : activePlans.length === 0 ? (
              <p className="rounded-lg bg-surface-2 px-3 py-2 text-xs text-muted-foreground">
                {t('subscriptions.bind.noPlans')}
              </p>
            ) : (
              <Select
                id="subscription-bind-plan"
                value={planId}
                disabled={mutation.isPending}
                onChange={(event) => setPlanId(event.target.value)}
              >
                {activePlans.map((plan) => (
                  <option key={plan.plan_id} value={plan.plan_id}>
                    {plan.name} ({t(`subscriptions.cycle.${plan.cycle}`)})
                  </option>
                ))}
              </Select>
            )}
            <p className="text-xs leading-5 text-muted-foreground">{t('subscriptions.fields.planHint')}</p>
          </div>

          {selectedPlan ? (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-y border-[var(--hairline)] py-3 text-xs text-muted-foreground">
              <span>{t('subscriptions.bind.planQuota', { value: formatQuota(selectedPlan.quota_amount) })}</span>
              <span>{t('subscriptions.bind.planCycle', { value: t(`subscriptions.cycle.${selectedPlan.cycle}`) })}</span>
              <span>{t('subscriptions.bind.planVersion', { version: selectedPlan.version })}</span>
            </div>
          ) : null}

          {plansQuery.isFetchNextPageError ? (
            <div role="alert" className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
              <span>{t('subscriptions.errors.morePlans')}</span>
              <Button type="button" size="sm" variant="ghost" onClick={() => void plansQuery.fetchNextPage()}>
                {t('subscriptions.actions.retry')}
              </Button>
            </div>
          ) : plansQuery.hasNextPage ? (
            <Button
              type="button"
              size="sm"
              variant="secondary"
              className="justify-self-start"
              disabled={plansQuery.isFetchingNextPage}
              onClick={() => void plansQuery.fetchNextPage()}
            >
              {t(plansQuery.isFetchingNextPage ? 'subscriptions.actions.loadingMorePlans' : 'subscriptions.actions.loadMorePlans')}
            </Button>
          ) : null}

          {mutation.isError ? (
            <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
              {t(`subscriptions.errors.${errorKey}`)}
            </p>
          ) : null}

          <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
            <Button type="button" variant="secondary" disabled={mutation.isPending} onClick={() => onOpenChange(false)}>
              {t('subscriptions.actions.cancel')}
            </Button>
            <Button type="button" disabled={!user || !selectedPlan || mutation.isPending} onClick={() => void bind()}>
              {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Link2 aria-hidden="true" />}
              {t(mutation.isPending ? 'subscriptions.actions.binding' : 'subscriptions.actions.bind')}
            </Button>
          </div>
        </div>
      </SheetContent>
    </Sheet>
  )
}

function bindErrorKey(code: string | undefined) {
  if (code === 'subscription_plan_not_found') return 'planNotFound'
  if (code === 'subscription_plan_disabled') return 'planDisabled'
  if (code === 'user_not_found') return 'userNotFound'
  if (code === 'subscription_conflict') return 'bindConflict'
  if (code === 'subscription_outcome_unknown') return 'outcomeUnknown'
  return 'bind'
}
