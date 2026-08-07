import { CalendarRange, RefreshCw } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { SubscriptionList } from './subscription-list'
import { useCurrentUserSubscriptions } from './subscription-api'

/** 展示当前登录用户自己的订阅与 UTC 周期窗口。 */
export function MySubscriptionsPage() {
  const { i18n, t } = useTranslation()
  const query = useCurrentUserSubscriptions()
  const subscriptions = query.data?.pages.flatMap((page) => page.subscriptions) ?? []
  const active = subscriptions.filter((subscription) => subscription.status === 'active')
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
        <Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('subscriptions.actions.refresh')}
        </Button>
      </header>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('subscriptions.summary.loaded', { count: subscriptions.length })}</span>
        <span>{t('subscriptions.summary.active', { count: active.length })}</span>
        <span>
          {nextWindowEnd === undefined
            ? t('subscriptions.summary.noUpcomingWindow')
            : t('subscriptions.summary.nextWindow', { value: timeFormat.format(nextWindowEnd * 1_000) })}
        </span>
      </div>

      <SubscriptionList subscriptions={subscriptions} query={query} scope="current" />
    </div>
  )
}
