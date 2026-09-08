import {
  ArrowDownRight,
  ArrowUpRight,
  ChartNoAxesColumnIncreasing,
  Clock3,
  Coins,
  History,
  RefreshCw,
  WalletCards,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useMemo, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import type { UserWalletEntry, UserWalletSummary } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { RefundReconciliationList } from '@/features/refunds/refund-reconciliation-list'
import { useUserWalletEntries, useUserWalletSummary } from './wallet-api'
import { WalletRedemptionPanel } from './wallet-redemption-panel'
import { WalletTopupPanel } from './wallet-topup-panel'

const entryTone = {
  opening_balance: 'text-info',
  admin_adjustment: 'text-warning',
  topup: 'text-success',
  redemption: 'text-success',
  invite_rebate: 'text-success',
} as const satisfies Record<UserWalletEntry['entry_type'], string>

/** 展示当前登录用户自己的余额状态与不可变账本。 */
export function WalletPage() {
  const { i18n, t } = useTranslation()
  const summaryQuery = useUserWalletSummary()
  const entriesQuery = useUserWalletEntries()
  const { formatQuota } = useBalanceDisplay()
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const timeFormat = useMemo(() => new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }), [locale])
  const entries = entriesQuery.data?.pages.flatMap((page) => page.entries) ?? []
  const refreshing = summaryQuery.isFetching || entriesQuery.isFetching

  const refresh = () => {
    void Promise.all([summaryQuery.refetch(), entriesQuery.refetch()])
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <WalletCards className="size-3.5" aria-hidden="true" />
            {t('wallet.eyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('wallet.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('wallet.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={refreshing}
          onClick={refresh}
        >
          <RefreshCw className={refreshing ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('wallet.actions.refresh')}
        </Button>
      </header>

      {summaryQuery.isPending ? (
        <WalletSummaryLoading />
      ) : summaryQuery.isError || !summaryQuery.data ? (
        <WalletError
          title={t('wallet.errors.summaryTitle')}
          body={t('wallet.errors.summaryBody')}
          onRetry={() => void summaryQuery.refetch()}
        />
      ) : (
        <WalletSummaryCards summary={summaryQuery.data} formatQuota={formatQuota} />
      )}

      <WalletTopupPanel />

      <WalletRedemptionPanel />

      <Card className="overflow-hidden">
        <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <CardTitle className="text-base">{t('wallet.ledger.title')}</CardTitle>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('wallet.ledger.subtitle')}</p>
            </div>
            <Badge className="bg-surface-2 text-muted-foreground">
              {t('wallet.ledger.count', { count: entries.length })}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="p-0">
          <WalletLedger
            entries={entries}
            query={entriesQuery}
            formatQuota={formatQuota}
            formatTime={(value) => timeFormat.format(value * 1_000)}
          />
        </CardContent>
      </Card>

      <RefundReconciliationList scope="account" translationPrefix="wallet.reconciliation" />
    </div>
  )
}

function WalletSummaryCards({
  summary,
  formatQuota,
}: {
  summary: UserWalletSummary
  formatQuota: (value: number) => string
}) {
  const { t } = useTranslation()
  const metrics: readonly {
    key: 'balance' | 'used' | 'frozen'
    icon: LucideIcon
    value: number
    tone: string
  }[] = [
    { key: 'balance', icon: Coins, value: summary.balance, tone: 'text-success' },
    { key: 'used', icon: ChartNoAxesColumnIncreasing, value: summary.used_quota, tone: 'text-info' },
    { key: 'frozen', icon: Clock3, value: summary.frozen_quota, tone: 'text-warning' },
  ]
  return (
    <section className="grid gap-3 sm:grid-cols-3" aria-label={t('wallet.summary.label')}>
      {metrics.map((metric) => {
        const Icon = metric.icon
        return (
          <Card key={metric.key}>
            <CardContent className="p-4">
              <div className="flex items-center justify-between gap-3">
                <span className="text-xs text-muted-foreground">{t(`wallet.summary.${metric.key}`)}</span>
                <span className={cn('grid size-7 place-items-center rounded-lg bg-surface-2', metric.tone)}>
                  <Icon className="size-3.5" aria-hidden="true" />
                </span>
              </div>
              <p className="mt-3 truncate text-xl font-semibold tabular-nums" title={formatQuota(metric.value)}>
                {formatQuota(metric.value)}
              </p>
              <p className="mt-1 text-[0.6875rem] text-muted-foreground">
                {t(`wallet.summary.${metric.key}Hint`)}
              </p>
            </CardContent>
          </Card>
        )
      })}
    </section>
  )
}

function WalletSummaryLoading() {
  return (
    <div className="grid gap-3 sm:grid-cols-3" role="status">
      {[0, 1, 2].map((item) => <Skeleton key={item} className="h-32 rounded-xl" />)}
    </div>
  )
}

type WalletLedgerQuery = ReturnType<typeof useUserWalletEntries>

function WalletLedger({
  entries,
  query,
  formatQuota,
  formatTime,
}: {
  entries: UserWalletEntry[]
  query: WalletLedgerQuery
  formatQuota: (value: number) => string
  formatTime: (value: number) => string
}) {
  const { t } = useTranslation()
  if (query.isPending) return <WalletLedgerLoading />
  if (query.isError && query.data === undefined) {
    return (
      <WalletLedgerState
        icon={History}
        title={t('wallet.errors.ledgerTitle')}
        body={t('wallet.errors.ledgerBody')}
        action={(
          <Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}>
            <RefreshCw aria-hidden="true" />
            {t('wallet.actions.retry')}
          </Button>
        )}
      />
    )
  }
  if (entries.length === 0) {
    return (
      <WalletLedgerState
        icon={WalletCards}
        title={t('wallet.ledger.emptyTitle')}
        body={t('wallet.ledger.emptyBody')}
      />
    )
  }
  return (
    <>
      <div className="divide-y divide-[var(--hairline)]">
        {entries.map((entry) => (
          <WalletEntryRow
            key={entry.id}
            entry={entry}
            formatQuota={formatQuota}
            formatTime={formatTime}
          />
        ))}
      </div>
      {query.isFetchNextPageError ? (
        <div role="alert" className="m-3 flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
          <span>{t('wallet.errors.more')}</span>
          <Button type="button" size="sm" variant="ghost" onClick={() => void query.fetchNextPage()}>
            {t('wallet.actions.retry')}
          </Button>
        </div>
      ) : null}
      {query.hasNextPage && !query.isFetchNextPageError ? (
        <div className="flex justify-center border-t border-[var(--hairline)] p-4">
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={query.isFetchingNextPage}
            onClick={() => void query.fetchNextPage()}
          >
            {t(query.isFetchingNextPage ? 'wallet.actions.loadingMore' : 'wallet.actions.loadMore')}
          </Button>
        </div>
      ) : null}
    </>
  )
}

function WalletEntryRow({
  entry,
  formatQuota,
  formatTime,
}: {
  entry: UserWalletEntry
  formatQuota: (value: number) => string
  formatTime: (value: number) => string
}) {
  const { t } = useTranslation()
  const positive = entry.quota_delta > 0
  const Icon = positive ? ArrowUpRight : ArrowDownRight
  const reason = entry.reason ?? t(`wallet.entryReason.${entry.entry_type}`)
  const delta = `${positive ? '+' : ''}${formatQuota(entry.quota_delta)}`
  return (
    <article className="grid grid-cols-[2rem_minmax(0,1fr)] gap-3 px-4 py-3.5 sm:grid-cols-[2rem_minmax(0,1fr)_auto] sm:px-5">
      <span className={cn(
        'grid size-8 place-items-center rounded-lg bg-surface-2 [&_svg]:size-4',
        positive ? 'text-success' : 'text-destructive',
      )}>
        <Icon aria-hidden="true" />
      </span>
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          <span className="truncate text-xs font-medium">{reason}</span>
          <Badge className={entryTone[entry.entry_type]}>
            {t(`wallet.entryType.${entry.entry_type}`)}
          </Badge>
        </div>
        <p className="mt-1 text-[0.6875rem] text-muted-foreground">
          {formatTime(entry.created_at)} · #{entry.id}
        </p>
      </div>
      <div className="col-start-2 text-left sm:col-start-auto sm:text-right">
        <div className={cn(
          'text-sm font-semibold tabular-nums',
          positive ? 'text-success' : 'text-destructive',
        )}>
          {delta}
        </div>
        <div className="mt-1 text-[0.6875rem] text-muted-foreground tabular-nums">
          {t('wallet.ledger.balanceAfter', { value: formatQuota(entry.balance_after) })}
        </div>
      </div>
    </article>
  )
}

function WalletLedgerLoading() {
  return (
    <div className="grid gap-4 p-5" role="status">
      {[0, 1, 2, 3].map((item) => (
        <div key={item} className="grid grid-cols-[2rem_1fr_auto] gap-3">
          <Skeleton className="size-8 rounded-lg" />
          <div className="grid gap-2">
            <Skeleton className="h-4 w-2/3" />
            <Skeleton className="h-3 w-1/2" />
          </div>
          <Skeleton className="h-5 w-16" />
        </div>
      ))}
    </div>
  )
}

function WalletLedgerState({
  icon: Icon,
  title,
  body,
  action,
}: {
  icon: LucideIcon
  title: string
  body: string
  action?: ReactNode
}) {
  return (
    <div className="grid min-h-52 place-items-center p-6 text-center">
      <div className="max-w-sm">
        <Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
        <h3 className="mt-3 text-sm font-semibold">{title}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>
        {action ? <div className="mt-4">{action}</div> : null}
      </div>
    </div>
  )
}

function WalletError({
  title,
  body,
  onRetry,
}: {
  title: string
  body: string
  onRetry: () => void
}) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-5">
      <h3 className="text-sm font-semibold text-destructive">{title}</h3>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>
      <Button type="button" size="sm" variant="secondary" className="mt-4" onClick={onRetry}>
        {t('wallet.actions.retry')}
      </Button>
    </div>
  )
}
