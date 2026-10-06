import { Button, Chip } from '@heroui/react'
import { Ban, CalendarRange, Layers3 } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { AdminSubscriptionPlan } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { SiteTooltip } from '@/shared/components/site-tooltip'

type SubscriptionPlanTableProps = {
  plans: AdminSubscriptionPlan[]
  onDisable: (plan: AdminSubscriptionPlan) => void
}

/** 展示不可变计划快照，并仅对有效计划开放停用操作。 */
export function SubscriptionPlanTable({ plans, onDisable }: SubscriptionPlanTableProps) {
  const { i18n, t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const timeFormat = useMemo(() => new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }), [locale])
  const formatTime = (value: number) => timeFormat.format(value * 1_000)

  if (plans.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-t border-[var(--hairline)] py-10 text-center">
        <div className="max-w-sm">
          <span className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground">
            <Layers3 className="size-4" aria-hidden="true" />
          </span>
          <h3 className="mt-3 text-sm font-semibold">{t('subscriptions.empty.plansTitle')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('subscriptions.empty.plansBody')}</p>
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
              <th className="w-[32%] px-3 py-2 font-medium">{t('subscriptions.columns.plan')}</th>
              <th className="w-[22%] px-3 py-2 font-medium">{t('subscriptions.columns.quota')}</th>
              <th className="w-[24%] px-3 py-2 font-medium">{t('subscriptions.columns.cycle')}</th>
              <th className="w-[14%] px-3 py-2 font-medium">{t('subscriptions.columns.updated')}</th>
              <th className="w-[8%] px-3 py-2"><span className="sr-only">{t('subscriptions.columns.actions')}</span></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {plans.map((plan) => (
              <tr key={plan.plan_id} className="hover:bg-surface-2/35">
                <td className="px-3 py-3"><PlanIdentity plan={plan} /></td>
                <td className="px-3 py-3 font-medium tabular-nums">{formatQuota(plan.quota_amount)}</td>
                <td className="px-3 py-3"><PlanCycle plan={plan} /></td>
                <td className="px-3 py-3"><PlanTiming plan={plan} formatTime={formatTime} /></td>
                <td className="px-2 py-3"><PlanActions plan={plan} onDisable={onDisable} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {plans.map((plan) => (
          <article key={plan.plan_id} className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3">
              <PlanIdentity plan={plan} />
              <PlanActions plan={plan} onDisable={onDisable} />
            </div>
            <div className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3">
              <div>
                <div className="text-[0.6875rem] text-muted-foreground">{t('subscriptions.columns.quota')}</div>
                <div className="mt-1 font-medium tabular-nums">{formatQuota(plan.quota_amount)}</div>
              </div>
              <PlanCycle plan={plan} />
            </div>
            <div className="mt-3 border-t border-[var(--hairline)] pt-3">
              <PlanTiming plan={plan} formatTime={formatTime} />
            </div>
          </article>
        ))}
      </div>
    </>
  )
}

function PlanIdentity({ plan }: { plan: AdminSubscriptionPlan }) {
  const { t } = useTranslation()
  return (
    <div className="min-w-0">
      <div className="flex min-w-0 flex-wrap items-center gap-1.5">
        <span className="truncate font-medium">{plan.name}</span>
        <Chip className={cn(
          plan.status === 'active'
            ? 'bg-success/10 text-success'
            : 'bg-surface-2 text-muted-foreground',
        )} size="sm" variant="flat">
          {t(`subscriptions.planStatus.${plan.status}`)}
        </Chip>
      </div>
      <div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground" title={plan.plan_id}>
        {plan.plan_id.slice(0, 8)} · {t('subscriptions.values.planVersion', { version: plan.version })}
      </div>
    </div>
  )
}

function PlanCycle({ plan }: { plan: AdminSubscriptionPlan }) {
  const { t } = useTranslation()
  return (
    <div>
      <div className="flex items-center gap-1.5 font-medium">
        <CalendarRange className="size-3.5 text-muted-foreground" aria-hidden="true" />
        {t(`subscriptions.cycle.${plan.cycle}`)}
      </div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {t('subscriptions.values.creator', { id: plan.created_by_user_id })}
      </div>
    </div>
  )
}

function PlanTiming({
  plan,
  formatTime,
}: {
  plan: AdminSubscriptionPlan
  formatTime: (value: number) => string
}) {
  const { t } = useTranslation()
  return (
    <div>
      <div className="font-medium">{formatTime(plan.updated_at)}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {plan.disabled_at === null
          ? t('subscriptions.values.createdAt', { value: formatTime(plan.created_at) })
          : t('subscriptions.values.disabledAt', { value: formatTime(plan.disabled_at) })}
      </div>
    </div>
  )
}

function PlanActions({
  plan,
  onDisable,
}: {
  plan: AdminSubscriptionPlan
  onDisable: (plan: AdminSubscriptionPlan) => void
}) {
  const { t } = useTranslation()
  if (plan.status === 'disabled') return null
  return (
    <div className="flex justify-end">
      <SiteTooltip content={t('subscriptions.actions.disablePlan')}>
        <Button
          isIconOnly
          aria-label={t('subscriptions.actions.disablePlan')}
          className="size-10 text-muted-foreground hover:text-destructive md:size-8"
          size="sm"
          type="button"
          variant="light"
          onClick={() => onDisable(plan)}
        >
          <Ban className="size-3.5" aria-hidden="true" />
        </Button>
      </SiteTooltip>
    </div>
  )
}
