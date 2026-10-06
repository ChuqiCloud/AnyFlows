import { Chip } from '@heroui/react'
import { BellRing, CalendarRange, Clock3 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type {
  AdminUserSubscriptionLifecycleAction,
  UserSubscription,
} from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { isSubscriptionAlertDue, subscriptionRemainingPercent } from './subscription-alert-policy'
import { SubscriptionLifecycleActions } from './subscription-lifecycle-actions'

export type SubscriptionAlertPolicy = {
  enabled: boolean
  thresholdPercent: number
}

type SubscriptionListItemProps = {
  subscription: UserSubscription
  formatQuota: (value: number) => string
  formatTime: (value: number) => string
  alertPolicy?: SubscriptionAlertPolicy
  onLifecycleAction?: (
    subscription: UserSubscription,
    action: AdminUserSubscriptionLifecycleAction,
  ) => void
}

/** 桌面表格中的单条订阅事实。 */
export function SubscriptionDesktopRow(props: SubscriptionListItemProps) {
  const { subscription, onLifecycleAction } = props
  return (
    <tr className="hover:bg-surface-2/35">
      <td className="px-3 py-3"><SubscriptionIdentity subscription={subscription} /></td>
      <td className="px-3 py-3"><SubscriptionQuota {...props} /></td>
      <td className="px-3 py-3"><SubscriptionWindow {...props} /></td>
      <td className="px-3 py-3"><SubscriptionBinding {...props} /></td>
      {onLifecycleAction ? (
        <td className="px-3 py-3 align-middle">
          <SubscriptionLifecycleActions
            subscription={subscription}
            onSelect={(action) => onLifecycleAction(subscription, action)}
          />
        </td>
      ) : null}
    </tr>
  )
}

/** 移动端单列中的订阅事实与可用动作。 */
export function SubscriptionMobileItem(props: SubscriptionListItemProps) {
  const { subscription, onLifecycleAction } = props
  return (
    <article className="rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
      <SubscriptionIdentity subscription={subscription} />
      <div className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3">
        <SubscriptionQuota {...props} />
        <SubscriptionBinding {...props} />
      </div>
      <div className="mt-3 border-t border-[var(--hairline)] pt-3">
        <SubscriptionWindow {...props} />
      </div>
      {onLifecycleAction ? (
        <div className="mt-3 border-t border-[var(--hairline)] pt-3">
          <SubscriptionLifecycleActions
            subscription={subscription}
            onSelect={(action) => onLifecycleAction(subscription, action)}
          />
        </div>
      ) : null}
    </article>
  )
}

function SubscriptionIdentity({ subscription }: Pick<SubscriptionListItemProps, 'subscription'>) {
  const { t } = useTranslation()
  return (
    <div className="min-w-0">
      <div className="flex min-w-0 flex-wrap items-center gap-1.5">
        <span className="truncate font-medium">{subscription.plan_name}</span>
        <SubscriptionStatusBadge status={subscription.status} />
      </div>
      <div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground" title={subscription.subscription_id}>
        {subscription.subscription_id.slice(0, 8)} · {t('subscriptions.values.planVersion', { version: subscription.plan_version })}
      </div>
    </div>
  )
}

function SubscriptionStatusBadge({ status }: { status: UserSubscription['status'] }) {
  const { t } = useTranslation()
  return (
    <Chip className={cn(
      status === 'active' && 'bg-success/10 text-success',
      status === 'suspended' && 'bg-warning/10 text-warning',
      status === 'canceled' && 'bg-destructive/10 text-destructive',
      status === 'expired' && 'bg-surface-2 text-muted-foreground',
    )} size="sm" variant="flat">
      {t(`subscriptions.status.${status}`)}
    </Chip>
  )
}

function SubscriptionQuota({
  subscription,
  formatQuota,
  alertPolicy,
}: SubscriptionListItemProps) {
  const { t } = useTranslation()
  const percent = subscription.quota_amount <= 0
    ? 0
    : Math.min(100, Math.round(subscription.quota_used / subscription.quota_amount * 100))
  const remainingPercent = subscriptionRemainingPercent(subscription)
  const alertDue = Boolean(
    alertPolicy?.enabled
      && isSubscriptionAlertDue(subscription, alertPolicy.thresholdPercent),
  )
  return (
    <div className="tabular-nums">
      <div className="font-medium">
        {t('subscriptions.values.quotaUsage', {
          used: formatQuota(subscription.quota_used),
          total: formatQuota(subscription.quota_amount),
        })}
      </div>
      <div className="mt-2 h-1 overflow-hidden rounded-full bg-surface-2" aria-hidden="true">
        <div className={cn('h-full rounded-full transition-[width] duration-150', alertDue ? 'bg-warning' : 'bg-brand')} style={{ width: `${percent}%` }} />
      </div>
      <div className="mt-1.5 flex min-h-5 items-center justify-between gap-2 text-[0.6875rem] text-muted-foreground">
        <span>{t('subscriptions.values.remainingPercent', { percent: remainingPercent })}</span>
        {alertDue ? (
          <Chip className="gap-1 bg-warning/10 px-1.5 py-0 text-warning" size="sm" variant="flat">
            <BellRing className="size-3" aria-hidden="true" />
            {t('subscriptions.alerts.due')}
          </Chip>
        ) : null}
      </div>
    </div>
  )
}

function SubscriptionWindow({ subscription, formatTime }: SubscriptionListItemProps) {
  const { t } = useTranslation()
  return (
    <div>
      <div className="flex items-center gap-1.5 font-medium">
        <CalendarRange className="size-3.5 text-muted-foreground" aria-hidden="true" />
        {t(`subscriptions.cycle.${subscription.cycle}`)}
      </div>
      <div className="mt-1 text-[0.6875rem] leading-5 text-muted-foreground">
        {t('subscriptions.values.window', {
          start: formatTime(subscription.window_started_at),
          end: formatTime(subscription.window_ends_at),
        })}
      </div>
    </div>
  )
}

function SubscriptionBinding({ subscription, formatTime }: SubscriptionListItemProps) {
  const { t } = useTranslation()
  return (
    <div>
      <div className="flex items-center gap-1.5 font-medium">
        <Clock3 className="size-3.5 text-muted-foreground" aria-hidden="true" />
        {formatTime(subscription.bound_at)}
      </div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {t('subscriptions.values.subscriptionVersion', { version: subscription.version })}
      </div>
    </div>
  )
}
