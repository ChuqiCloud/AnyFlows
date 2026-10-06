import { Link2, RefreshCw, UsersRound } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { useAdminUsers } from '@/features/users/user-api'
import type { AdminSubscriptionPlan } from '@/lib/api/generated/types.gen'
import { SubscriptionBindSheet } from './subscription-bind-sheet'
import {
  SubscriptionLifecycleDialog,
  type SubscriptionLifecycleOperation,
} from './subscription-lifecycle-dialog'
import { SubscriptionList } from './subscription-list'
import { useAdminSubscriptionPlans, useAdminUserSubscriptions } from './subscription-api'

type SubscriptionPlansQuery = ReturnType<typeof useAdminSubscriptionPlans>

type SubscriptionUserPanelProps = {
  plans: AdminSubscriptionPlan[]
  plansQuery: SubscriptionPlansQuery
}

/** 通过真实用户目录选择主体，并维护其订阅绑定。 */
export function SubscriptionUserPanel({ plans, plansQuery }: SubscriptionUserPanelProps) {
  const { t } = useTranslation()
  const usersQuery = useAdminUsers()
  const users = useMemo(() => usersQuery.data?.pages.flatMap((page) => page.users) ?? [], [usersQuery.data])
  const [chosenUserId, setChosenUserId] = useState<number>()
  const selectedUser = users.find((user) => user.id === chosenUserId) ?? users[0]
  const subscriptionsQuery = useAdminUserSubscriptions(selectedUser?.id)
  const subscriptions = subscriptionsQuery.data?.pages.flatMap((page) => page.subscriptions) ?? []
  const [binding, setBinding] = useState(false)
  const [lifecycleOperation, setLifecycleOperation] = useState<SubscriptionLifecycleOperation>()

  if (usersQuery.isPending) {
    return (
      <div className="grid gap-3" aria-label={t('subscriptions.user.loading')}>
        <Skeleton className="h-20 rounded-lg" />
        {[0, 1, 2].map((item) => <Skeleton key={item} className="h-24 rounded-lg" />)}
      </div>
    )
  }
  if (usersQuery.isError && usersQuery.data === undefined) {
    return (
      <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
        <h3 className="text-sm font-semibold text-destructive">{t('subscriptions.errors.usersTitle')}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('subscriptions.errors.usersBody')}</p>
        <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void usersQuery.refetch()}>
          <RefreshCw aria-hidden="true" />
          {t('subscriptions.actions.retry')}
        </Button>
      </div>
    )
  }
  if (!selectedUser) {
    return (
      <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center">
        <div className="max-w-sm">
          <UsersRound className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
          <h3 className="mt-3 text-sm font-semibold">{t('subscriptions.empty.usersTitle')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('subscriptions.empty.usersBody')}</p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-3 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3 lg:grid-cols-[minmax(16rem,1fr)_auto] lg:items-end">
        <div className="grid gap-2">
          <Label htmlFor="subscription-user-selector">{t('subscriptions.user.selectorLabel')}</Label>
          <Select
            id="subscription-user-selector"
            value={selectedUser.id}
            onChange={(event) => {
              setChosenUserId(Number(event.target.value))
              setLifecycleOperation(undefined)
            }}
          >
            {users.map((user) => (
              <option key={user.id} value={user.id}>
                {user.username}{user.email ? ` (${user.email})` : ''}
              </option>
            ))}
          </Select>
        </div>
        <div className="flex flex-wrap gap-2 lg:justify-end">
          {usersQuery.hasNextPage ? (
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={usersQuery.isFetchingNextPage}
              onClick={() => void usersQuery.fetchNextPage()}
            >
              {t(usersQuery.isFetchingNextPage ? 'subscriptions.actions.loadingMoreUsers' : 'subscriptions.actions.loadMoreUsers')}
            </Button>
          ) : null}
          <Button type="button" size="sm" variant="secondary" disabled={subscriptionsQuery.isFetching} onClick={() => void subscriptionsQuery.refetch()}>
            <RefreshCw className={subscriptionsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('subscriptions.actions.refresh')}
          </Button>
          <Button type="button" size="sm" onClick={() => setBinding(true)}>
            <Link2 aria-hidden="true" />
            {t('subscriptions.actions.bindPlan')}
          </Button>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span className="font-medium text-foreground">{selectedUser.username}</span>
        <span>{selectedUser.email ?? t('subscriptions.user.noEmail')}</span>
        <Badge>{t(`subscriptions.user.status.${selectedUser.status}`)}</Badge>
        <span>{t('subscriptions.summary.userSubscriptions', { count: subscriptions.length })}</span>
      </div>

      {usersQuery.isFetchNextPageError ? (
        <div role="alert" className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
          <span>{t('subscriptions.errors.moreUsers')}</span>
          <Button type="button" size="sm" variant="ghost" onClick={() => void usersQuery.fetchNextPage()}>
            {t('subscriptions.actions.retry')}
          </Button>
        </div>
      ) : null}

      <SubscriptionList
        subscriptions={subscriptions}
        query={subscriptionsQuery}
        scope="admin"
        onLifecycleAction={(subscription, action) => {
          setLifecycleOperation({ subscription, action })
        }}
      />
      <SubscriptionBindSheet
        open={binding}
        onOpenChange={setBinding}
        user={selectedUser}
        plans={plans}
        plansQuery={plansQuery}
      />
      <SubscriptionLifecycleDialog
        userId={selectedUser.id}
        operation={lifecycleOperation}
        onClose={() => setLifecycleOperation(undefined)}
      />
    </div>
  )
}
