import { Layers3, RefreshCw } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type {
  AdminUserSubscriptionLifecycleAction,
  UserSubscription,
} from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { useCurrentUserSubscriptions } from './subscription-api'
import {
  SubscriptionDesktopRow,
  SubscriptionMobileItem,
  type SubscriptionAlertPolicy,
} from './subscription-list-item'

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
  alertPolicy?: SubscriptionAlertPolicy
  onLifecycleAction?: (
    subscription: UserSubscription,
    action: AdminUserSubscriptionLifecycleAction,
  ) => void
}

/** 以桌面表格和移动条目展示订阅窗口及额度事实。 */
export function SubscriptionList({
  subscriptions,
  query,
  scope,
  alertPolicy,
  onLifecycleAction,
}: SubscriptionListProps) {
  const { i18n, t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
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
              <th className={cn(onLifecycleAction ? 'w-[22%]' : 'w-[28%]', 'px-3 py-2 font-medium')}>{t('subscriptions.columns.plan')}</th>
              <th className={cn(onLifecycleAction ? 'w-[20%]' : 'w-[22%]', 'px-3 py-2 font-medium')}>{t('subscriptions.columns.quota')}</th>
              <th className={cn(onLifecycleAction ? 'w-[24%]' : 'w-[30%]', 'px-3 py-2 font-medium')}>{t('subscriptions.columns.window')}</th>
              <th className={cn(onLifecycleAction ? 'w-[16%]' : 'w-[20%]', 'px-3 py-2 font-medium')}>{t('subscriptions.columns.bound')}</th>
              {onLifecycleAction ? <th className="w-[18%] px-3 py-2 font-medium">{t('subscriptions.columns.actions')}</th> : null}
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {subscriptions.map((subscription) => (
              <SubscriptionDesktopRow
                key={subscription.subscription_id}
                subscription={subscription}
                formatQuota={formatQuota}
                formatTime={formatTime}
                alertPolicy={alertPolicy}
                onLifecycleAction={onLifecycleAction}
              />
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {subscriptions.map((subscription) => (
          <SubscriptionMobileItem
            key={subscription.subscription_id}
            subscription={subscription}
            formatQuota={formatQuota}
            formatTime={formatTime}
            alertPolicy={alertPolicy}
            onLifecycleAction={onLifecycleAction}
          />
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
