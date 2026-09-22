import { Button, Chip, Drawer, DrawerBody, DrawerContent, DrawerHeader, Select, SelectItem } from '@heroui/react'
import { Link2, LoaderCircle, UsersRound } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

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
  // HeroUI Select 的动态选项必须走 items + 渲染函数（数组子节点不被类型接受）。
  const planItems = useMemo(
    () => activePlans.map((plan) => ({ key: plan.plan_id, label: `${plan.name} (${t(`subscriptions.cycle.${plan.cycle}`)})` })),
    [activePlans, t],
  )

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
    <Drawer
      aria-describedby="subscription-bind-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-xl' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t('subscriptions.bind.title')}</h2>
              <p className="text-sm text-muted-foreground" id="subscription-bind-description">
                {t('subscriptions.bind.description')}
              </p>
            </DrawerHeader>
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-y-auto p-5">
              <div className="grid gap-5">
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
                  {user ? <Chip size="sm" variant="flat">{t(`subscriptions.user.status.${user.status}`)}</Chip> : null}
                </div>
                <p className="mt-1 truncate text-xs text-muted-foreground">
                  {user?.email ?? t('subscriptions.user.noEmail')}
                </p>
              </div>
            </div>
          </div>

          <div className="grid gap-2">
            {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持原有层级。 */}
            <label className="text-xs font-medium leading-none text-foreground" htmlFor="subscription-bind-plan">{t('subscriptions.fields.plan')}</label>
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
                aria-label={t('subscriptions.fields.plan')}
                id="subscription-bind-plan"
                isDisabled={mutation.isPending}
                items={planItems}
                selectedKeys={planId ? [planId] : []}
                size="sm"
                onSelectionChange={(keys) => setPlanId(String(Array.from(keys)[0] ?? ''))}
              >
                {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
              </Select>
            )}
            <p className="text-xs leading-5 text-muted-foreground">{t('subscriptions.fields.planHint')}</p>
          </div>

          {selectedPlan ? (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-[var(--hairline)] py-3 text-xs text-muted-foreground">
              <span>{t('subscriptions.bind.planQuota', { value: formatQuota(selectedPlan.quota_amount) })}</span>
              <span>{t('subscriptions.bind.planCycle', { value: t(`subscriptions.cycle.${selectedPlan.cycle}`) })}</span>
              <span>{t('subscriptions.bind.planVersion', { version: selectedPlan.version })}</span>
            </div>
          ) : null}

          {plansQuery.isFetchNextPageError ? (
            <div role="alert" className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
              <span>{t('subscriptions.errors.morePlans')}</span>
              <Button type="button" size="sm" variant="light" onClick={() => void plansQuery.fetchNextPage()}>
                {t('subscriptions.actions.retry')}
              </Button>
            </div>
          ) : plansQuery.hasNextPage ? (
            <Button
              type="button"
              size="sm"
              variant="bordered"
              className="justify-self-start"
              isDisabled={plansQuery.isFetchingNextPage}
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
            <Button type="button" variant="bordered" isDisabled={mutation.isPending} onClick={() => onOpenChange(false)}>
              {t('subscriptions.actions.cancel')}
            </Button>
            <Button type="button" color="primary" isDisabled={!user || !selectedPlan || mutation.isPending} onClick={() => void bind()}>
              {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Link2 className="size-4" aria-hidden="true" />}
              {t(mutation.isPending ? 'subscriptions.actions.binding' : 'subscriptions.actions.bind')}
            </Button>
          </div>
              </div>
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
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
