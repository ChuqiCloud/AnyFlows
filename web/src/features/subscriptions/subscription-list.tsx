import { BellRing, CalendarRange, Clock3, Layers3, RefreshCw } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import type { UserSubscription } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { isSubscriptionAlertDue, subscriptionRemainingPercent } from './subscription-alert-policy'
import { useCurrentUserSubscriptions } from './subscription-api'

type SubscriptionListQuery = Pick<
  ReturnType<typeof useCurrentUserSubscriptions>,
  | 'data'
  | 'fetchNextPage'
  | 'hasNextPage'
  | 'isError'
  | 'isFetchNextPageError'
  | 'isFetchingNextPage'
  | 'isPending'
  | 'refetch'
>

type SubscriptionListProps = {
  subscriptions: UserSubscription[]
  query: SubscriptionListQuery
  scope: 'current' | 'admin'
  alertPolicy?: {
    enabled: boolean
    thresholdPercent: number
  }
}

/** 以桌面表格和移动条目展示订阅窗口及额度事实。 */
export function SubscriptionList({ subscriptions, query, scope, alertPolicy }: SubscriptionListProps) {
  const { i18n, t } = useTranslation()
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const numberFormat = useMemo(() => new Intl.NumberFormat(locale), [locale])
  const timeFormat = useMemo(() => new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }), [locale])
  const formatTime = (value: number) => timeFormat.format(value * 1_000)

  if (query.isPending) {
    return (
      <div className="grid gap-2" aria-label={t('subscriptions.loading')}>
        {[0, 1, 2].map((item) => <Skeleton key={item} className="h-24 rounded-lg" />)}
      </div>
    )
  }
  if (query.isError && query.data === undefined) {
    return (
      <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
        <h3 className="text-sm font-semibold text-destructive">
          {t(`subscriptions.errors.${scope}Title`)}
        </h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          {t(`subscriptions.errors.${scope}Body`)}
        </p>
        <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void query.refetch()}>
          <RefreshCw aria-hidden="true" />
          {t('subscriptions.actions.retry')}
        </Button>
      </div>
    )
  }
  if (subscriptions.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center">
        <div className="max-w-sm">
          <span className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground">
            <Layers3 className="size-4" aria-hidden="true" />
          </span>
          <h3 className="mt-3 text-sm font-semibold">{t(`subscriptions.empty.${scope}Title`)}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t(`subscriptions.empty.${scope}Body`)}
          </p>
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
              <th className="w-[28%] px-3 py-2 font-medium">{t('subscriptions.columns.plan')}</th>
              <th className="w-[22%] px-3 py-2 font-medium">{t('subscriptions.columns.quota')}</th>
              <th className="w-[30%] px-3 py-2 font-medium">{t('subscriptions.columns.window')}</th>
              <th className="w-[20%] px-3 py-2 font-medium">{t('subscriptions.columns.bound')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {subscriptions.map((subscription) => (
              <tr key={subscription.subscription_id} className="hover:bg-surface-2/35">
                <td className="px-3 py-3"><SubscriptionIdentity subscription={subscription} /></td>
                <td className="px-3 py-3"><SubscriptionQuota subscription={subscription} formatQuota={numberFormat.format} alertPolicy={alertPolicy} /></td>
                <td className="px-3 py-3"><SubscriptionWindow subscription={subscription} formatTime={formatTime} /></td>
                <td className="px-3 py-3"><SubscriptionBinding subscription={subscription} formatTime={formatTime} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {subscriptions.map((subscription) => (
          <article key={subscription.subscription_id} className="rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
            <SubscriptionIdentity subscription={subscription} />
            <div className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3">
              <SubscriptionQuota subscription={subscription} formatQuota={numberFormat.format} alertPolicy={alertPolicy} />
              <SubscriptionBinding subscription={subscription} formatTime={formatTime} />
            </div>
            <div className="mt-3 border-t border-[var(--hairline)] pt-3">
              <SubscriptionWindow subscription={subscription} formatTime={formatTime} />
            </div>
          </article>
        ))}
      </div>

      {query.isFetchNextPageError ? (
        <div role="alert" className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
          <span>{t('subscriptions.errors.more')}</span>
          <Button type="button" size="sm" variant="ghost" onClick={() => void query.fetchNextPage()}>
            {t('subscriptions.actions.retry')}
          </Button>
        </div>
      ) : null}
      {query.hasNextPage && !query.isFetchNextPageError ? (
        <div className="flex justify-center border-t border-[var(--hairline)] pt-4">
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={query.isFetchingNextPage}
            onClick={() => void query.fetchNextPage()}
          >
            {t(query.isFetchingNextPage ? 'subscriptions.actions.loadingMore' : 'subscriptions.actions.loadMore')}
          </Button>
        </div>
      ) : null}
    </>
  )
}

function SubscriptionIdentity({ subscription }: { subscription: UserSubscription }) {
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
    <Badge className={cn(
      'border-transparent',
      status === 'active' && 'bg-success/10 text-success',
      status === 'suspended' && 'bg-warning/10 text-warning',
      status === 'canceled' && 'bg-destructive/10 text-destructive',
      status === 'expired' && 'bg-surface-2 text-muted-foreground',
    )}>
      {t(`subscriptions.status.${status}`)}
    </Badge>
  )
}

function SubscriptionQuota({
  subscription,
  formatQuota,
  alertPolicy,
}: {
  subscription: UserSubscription
  formatQuota: (value: number) => string
  alertPolicy?: SubscriptionListProps['alertPolicy']
}) {
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
          <Badge className="gap-1 bg-warning/10 px-1.5 py-0 text-warning">
            <BellRing className="size-3" aria-hidden="true" />
            {t('subscriptions.alerts.due')}
          </Badge>
        ) : null}
      </div>
    </div>
  )
}

function SubscriptionWindow({
  subscription,
  formatTime,
}: {
  subscription: UserSubscription
  formatTime: (value: number) => string
}) {
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

function SubscriptionBinding({
  subscription,
  formatTime,
}: {
  subscription: UserSubscription
  formatTime: (value: number) => string
}) {
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
