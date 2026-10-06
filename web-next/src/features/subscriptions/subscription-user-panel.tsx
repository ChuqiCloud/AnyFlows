import { Button, Chip, Select, SelectItem, Skeleton } from '@heroui/react'
import { Link2, RefreshCw, UsersRound } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useAdminUsers } from '@/features/users/user-api'
import { cn } from '@/lib/utils'
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
  // HeroUI Select 的动态选项必须走 items + 渲染函数（数组子节点不被类型接受）。
  const userItems = useMemo(
    () => users.map((user) => ({ key: String(user.id), label: user.email ? `${user.username} (${user.email})` : user.username })),
    [users],
  )

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
        <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={() => void usersQuery.refetch()}>
          <RefreshCw className="size-3.5" aria-hidden="true" />
          {t('subscriptions.actions.retry')}
        </Button>
      </div>
    )
  }
  if (!selectedUser) {
    return (
      <div className="grid min-h-64 place-items-center border-t border-[var(--hairline)] py-10 text-center">
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
          {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持原有层级。 */}
          <label className="text-xs font-medium leading-none text-foreground" htmlFor="subscription-user-selector">{t('subscriptions.user.selectorLabel')}</label>
          <Select
            aria-label={t('subscriptions.user.selectorLabel')}
            id="subscription-user-selector"
            items={userItems}
            selectedKeys={[String(selectedUser.id)]}
            size="sm"
            onSelectionChange={(keys) => {
              const next = Number(Array.from(keys)[0])
              if (Number.isFinite(next)) setChosenUserId(next)
              setLifecycleOperation(undefined)
            }}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>
        </div>
        <div className="flex flex-wrap gap-2 lg:justify-end">
          {usersQuery.hasNextPage ? (
            <Button
              type="button"
              size="sm"
              variant="bordered"
              isDisabled={usersQuery.isFetchingNextPage}
              onClick={() => void usersQuery.fetchNextPage()}
            >
              {t(usersQuery.isFetchingNextPage ? 'subscriptions.actions.loadingMoreUsers' : 'subscriptions.actions.loadMoreUsers')}
            </Button>
          ) : null}
          <Button type="button" size="sm" variant="bordered" isDisabled={subscriptionsQuery.isFetching} onClick={() => void subscriptionsQuery.refetch()}>
            <RefreshCw className={cn('size-3.5', subscriptionsQuery.isFetching && 'animate-spin')} aria-hidden="true" />
            {t('subscriptions.actions.refresh')}
          </Button>
          <Button type="button" color="primary" size="sm" onClick={() => setBinding(true)}>
            <Link2 className="size-3.5" aria-hidden="true" />
            {t('subscriptions.actions.bindPlan')}
          </Button>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span className="font-medium text-foreground">{selectedUser.username}</span>
        <span>{selectedUser.email ?? t('subscriptions.user.noEmail')}</span>
        <Chip size="sm" variant="flat">{t(`subscriptions.user.status.${selectedUser.status}`)}</Chip>
        <span>{t('subscriptions.summary.userSubscriptions', { count: subscriptions.length })}</span>
      </div>

      {usersQuery.isFetchNextPageError ? (
        <div role="alert" className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
          <span>{t('subscriptions.errors.moreUsers')}</span>
          <Button type="button" size="sm" variant="light" onClick={() => void usersQuery.fetchNextPage()}>
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
