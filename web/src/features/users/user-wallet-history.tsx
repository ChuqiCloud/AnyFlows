import { ArrowDownRight, ArrowUpRight, History, RefreshCw, WalletCards } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { AdminWalletEntry } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { useAdminWalletEntries } from './user-api'

type UserWalletHistoryProps = {
  open: boolean
  userId: number
}

const walletEntryCopy = {
  opening_balance: {
    reason: 'users.wallet.history.openingReason',
    actor: 'users.wallet.history.systemActor',
  },
  admin_adjustment: {
    reason: 'users.wallet.history.openingReason',
    actor: 'users.wallet.history.systemActor',
  },
  topup: {
    reason: 'users.wallet.history.topupReason',
    actor: 'users.wallet.history.paymentActor',
  },
  redemption: {
    reason: 'users.wallet.history.redemptionReason',
    actor: 'users.wallet.history.redemptionActor',
  },
  invite_rebate: {
    reason: 'users.wallet.history.inviteRebateReason',
    actor: 'users.wallet.history.inviteRebateActor',
  },
} as const satisfies Record<AdminWalletEntry['entry_type'], { reason: string; actor: string }>

export function UserWalletHistory({ open, userId }: UserWalletHistoryProps) {
  const { i18n, t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const query = useAdminWalletEntries(userId, open)
  const entries = query.data?.pages.flatMap((page) => page.entries) ?? []
  const formatTime = (value: number) => new Intl.DateTimeFormat(i18n.language, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(value * 1000)

  return (
    <section className="min-h-0 flex-1 overflow-y-auto" aria-labelledby="wallet-history-title">
      <div className="sticky top-0 z-10 flex items-center justify-between gap-3 border-b border-[var(--hairline)] bg-popover/95 px-4 py-3 backdrop-blur-sm">
        <div>
          <h3 id="wallet-history-title" className="text-xs font-semibold">{t('users.wallet.history.title')}</h3>
          <p className="mt-0.5 text-[0.6875rem] text-muted-foreground">{t('users.wallet.history.count', { count: entries.length })}</p>
        </div>
        <Button type="button" size="icon-sm" variant="ghost" title={t('users.wallet.actions.refresh')} disabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          <span className="sr-only">{t('users.wallet.actions.refresh')}</span>
        </Button>
      </div>

      {query.isPending ? (
        <div className="grid gap-3 p-4" aria-label={t('users.wallet.history.loading')}>
          {Array.from({ length: 4 }, (_, index) => (
            <div key={index} className="grid grid-cols-[2rem_1fr] gap-3">
              <Skeleton className="size-8 rounded-lg" />
              <div className="grid gap-2"><Skeleton className="h-4 w-2/3" /><Skeleton className="h-3 w-5/6" /></div>
            </div>
          ))}
        </div>
      ) : query.isError && query.data === undefined ? (
        <HistoryState
          icon={History}
          title={t('users.wallet.history.errorTitle')}
          body={t('users.wallet.history.errorBody')}
          action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{t('users.wallet.actions.retry')}</Button>}
        />
      ) : entries.length === 0 ? (
        <HistoryState icon={WalletCards} title={t('users.wallet.history.emptyTitle')} body={t('users.wallet.history.emptyBody')} />
      ) : (
        <div className="divide-y divide-[var(--hairline)]">
          {entries.map((entry) => (
            <WalletEntryRow key={entry.id} entry={entry} formatQuota={formatQuota} formatTime={formatTime} />
          ))}
        </div>
      )}

      {query.isFetchNextPageError ? (
        <div role="alert" className="m-3 flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
          <span>{t('users.wallet.history.moreError')}</span>
          <Button type="button" size="sm" variant="ghost" onClick={() => void query.fetchNextPage()}>{t('users.wallet.actions.retry')}</Button>
        </div>
      ) : null}
      {query.hasNextPage && !query.isFetchNextPageError ? (
        <div className="flex justify-center p-4">
          <Button type="button" size="sm" variant="secondary" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>
            {t(query.isFetchingNextPage ? 'users.wallet.history.loadingMore' : 'users.wallet.actions.loadMore')}
          </Button>
        </div>
      ) : null}
    </section>
  )
}

function WalletEntryRow(props: {
  entry: AdminWalletEntry
  formatQuota: (value: number) => string
  formatTime: (value: number) => string
}) {
  const { t } = useTranslation()
  const { entry } = props
  const positive = entry.quota_delta > 0
  const opening = entry.entry_type === 'opening_balance'
  const automaticCredit = entry.entry_type === 'topup'
    || entry.entry_type === 'redemption'
    || entry.entry_type === 'invite_rebate'
  const copy = walletEntryCopy[entry.entry_type]
  const reason = entry.reason ?? t(copy.reason)
  const actor = entry.actor_user_id === null
    ? t(copy.actor)
    : t('users.wallet.history.adminActor', { id: entry.actor_user_id })
  const delta = `${positive ? '+' : ''}${props.formatQuota(entry.quota_delta)}`
  const Icon = positive ? ArrowUpRight : ArrowDownRight
  return (
    <article className="grid grid-cols-[2rem_minmax(0,1fr)] gap-3 px-4 py-3.5 sm:grid-cols-[2rem_minmax(0,1fr)_auto]">
      <span className={cn(
        'grid size-8 place-items-center rounded-lg bg-surface-2 [&_svg]:size-4',
        positive ? 'text-success' : 'text-destructive',
      )}>
        <Icon aria-hidden="true" />
      </span>
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          <span className="truncate text-xs font-medium">{reason}</span>
          <Badge className={cn(opening && 'text-info', automaticCredit && 'text-success')}>{t(`users.wallet.entryType.${entry.entry_type}`)}</Badge>
        </div>
        <p className="mt-1 truncate text-[0.6875rem] text-muted-foreground">
          {props.formatTime(entry.created_at)} · {actor}
        </p>
        <p title={entry.event_id} className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground/80">
          {entry.event_id.slice(0, 8)}…{entry.event_id.slice(-8)} · #{entry.id}
        </p>
      </div>
      <div className="col-start-2 text-left sm:col-start-auto sm:text-right">
        <div className={cn('text-sm font-semibold tabular-nums', positive ? 'text-success' : 'text-destructive')}>{delta}</div>
        <div className="mt-1 text-[0.6875rem] text-muted-foreground tabular-nums">
          {t('users.wallet.history.balanceAfter', { value: props.formatQuota(entry.balance_after) })}
        </div>
      </div>
    </article>
  )
}

function HistoryState(props: {
  icon: typeof History
  title: string
  body: string
  action?: ReactNode
}) {
  const Icon = props.icon
  return (
    <div className="grid min-h-56 place-items-center px-5 text-center">
      <div className="max-w-xs">
        <Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
        <h3 className="mt-3 text-sm font-semibold">{props.title}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{props.body}</p>
        {props.action ? <div className="mt-4">{props.action}</div> : null}
      </div>
    </div>
  )
}
