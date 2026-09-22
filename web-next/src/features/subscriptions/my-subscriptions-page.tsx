import { Button, Chip } from '@heroui/react'
import { BellRing, CalendarRange, RefreshCw } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { useUserProfile } from '@/features/profile/profile-api'
import { cn } from '@/lib/utils'
import { isSubscriptionAlertDue } from './subscription-alert-policy'
import { SubscriptionCatalog } from './subscription-catalog'
import { SubscriptionList } from './subscription-list'
import { useCurrentUserSubscriptions } from './subscription-api'

/** 展示当前登录用户自己的订阅与 UTC 周期窗口。 */
export function MySubscriptionsPage() {
  const { i18n, t } = useTranslation()
  const query = useCurrentUserSubscriptions()
  const profileQuery = useUserProfile()
  const subscriptions = query.data?.pages.flatMap((page) => page.subscriptions) ?? []
  const active = subscriptions.filter((subscription) => subscription.status === 'active')
  const activePlanIds = useMemo(
    () => new Set(active.map((subscription) => subscription.plan_id)),
    [active],
  )
  const notifications = profileQuery.data?.notifications
  const alertThresholdPercent = notifications?.subscription_remaining_percent ?? 20
  const alertPolicyEnabled = Boolean(
    notifications?.subscription_alert_enabled && notifications.email_usage_alerts,
  )
  const warningCount = alertPolicyEnabled
    ? active.filter((subscription) => isSubscriptionAlertDue(subscription, alertThresholdPercent)).length
    : 0
  const nextWindowEnd = active.reduce<number | undefined>((current, subscription) => (
    current === undefined || subscription.window_ends_at < current
      ? subscription.window_ends_at
      : current
  ), undefined)
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const timeFormat = useMemo(() => new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }), [locale])

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <CalendarRange className="size-3.5" aria-hidden="true" />
            {t('subscriptions.eyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('subscriptions.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('subscriptions.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="bordered"
          isDisabled={query.isFetching || profileQuery.isFetching}
          onClick={() => void Promise.all([query.refetch(), profileQuery.refetch()])}
        >
          <RefreshCw className={cn('size-3.5', (query.isFetching || profileQuery.isFetching) && 'animate-spin')} aria-hidden="true" />
          {t('subscriptions.actions.refresh')}
        </Button>
      </header>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('subscriptions.summary.loaded', { count: subscriptions.length })}</span>
        <span>{t('subscriptions.summary.active', { count: active.length })}</span>
        <span>
          {nextWindowEnd === undefined
            ? t('subscriptions.summary.noUpcomingWindow')
            : t('subscriptions.summary.nextWindow', { value: timeFormat.format(nextWindowEnd * 1_000) })}
        </span>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--hairline)] pb-3 text-xs">
        <div className="flex min-w-0 items-start gap-2 text-muted-foreground">
          <BellRing className="mt-0.5 size-3.5 shrink-0 text-info" aria-hidden="true" />
          <span>
            {profileQuery.isError
              ? t('subscriptions.alerts.unavailable')
              : t('subscriptions.alerts.description', { percent: alertThresholdPercent })}
          </span>
        </div>
        <Chip className={alertPolicyEnabled ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground'} size="sm" variant="flat">
          {profileQuery.isPending
            ? t('subscriptions.alerts.loading')
            : alertPolicyEnabled
              ? t('subscriptions.alerts.active', { count: warningCount })
              : t('subscriptions.alerts.paused')}
        </Chip>
      </div>

      <SubscriptionList
        subscriptions={subscriptions}
        query={query}
        scope="current"
        alertPolicy={{ enabled: alertPolicyEnabled, thresholdPercent: alertThresholdPercent }}
      />

      <SubscriptionCatalog activePlanIds={activePlanIds} subscriptionsFetching={query.isFetching} />
    </div>
  )
}
