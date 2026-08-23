import { ArrowDownRight, ArrowUpRight, ChartNoAxesColumnIncreasing, Clock3, Coins, History, RefreshCw, WalletCards } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useMemo, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { OrganizationPageBoundary, OrganizationPageHeader, OrganizationPageState, useOrganizationPageState } from '@/features/organization-console/organization-page-shell'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { WalletTopupPanel } from '@/features/wallet/wallet-topup-panel'
import type { OrganizationOverviewResponse, OrganizationWalletEntryResponse, OrganizationWalletSummaryResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { OrganizationWalletAdjustmentForm } from './organization-wallet-adjustment-form'
import { useOrganizationWalletEntries, useOrganizationWalletSummary } from './organization-wallet-api'

const WALLET_READ = 'organization.wallet.read'
const WALLET_MANAGE = 'organization.wallet.manage'

/** 企业钱包页面严格绑定当前企业，余额、账本、调账和充值不会回退个人钱包。 */
export function OrganizationWalletPage() {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <OrganizationPageHeader icon={WalletCards} title={t('organizationConsole.wallet.title')} subtitle={t('organizationConsole.wallet.subtitle')} />
      <OrganizationPageBoundary state={state}>{(organizationId, overview) => <OrganizationWalletWorkspace key={organizationId} organizationId={organizationId} overview={overview} />}</OrganizationPageBoundary>
    </div>
  )
}

function OrganizationWalletWorkspace({ organizationId, overview }: { organizationId: number; overview: OrganizationOverviewResponse }) {
  const { i18n, t } = useTranslation()
  const canRead = overview.permissions.includes(WALLET_READ)
  const canManage = overview.permissions.includes(WALLET_MANAGE)
  const summaryQuery = useOrganizationWalletSummary(organizationId, canRead)
  const entriesQuery = useOrganizationWalletEntries(organizationId, canRead)
  const { formatQuota } = useBalanceDisplay()
  const entries = entriesQuery.data?.pages.flatMap((page) => page.entries) ?? []
  const timeFormat = useMemo(() => new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }), [i18n.resolvedLanguage])
  if (!canRead) return <OrganizationPageState icon={WalletCards} title={t('organizationConsole.wallet.restricted.title')} body={t('organizationConsole.wallet.restricted.body')} />
  if (summaryQuery.isPending) return <WalletLoading />
  if (summaryQuery.isError || !summaryQuery.data) return <OrganizationPageState icon={WalletCards} title={t('organizationConsole.wallet.errors.summaryTitle')} body={t('organizationConsole.wallet.errors.summaryBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void summaryQuery.refetch()}><RefreshCw aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>} />
  return (
    <>
      <WalletSummary summary={summaryQuery.data} formatQuota={formatQuota} />
      {canManage ? <WalletTopupPanel key={organizationId} organizationId={organizationId} title={t('organizationConsole.wallet.topup.title')} description={t('organizationConsole.wallet.topup.description', { organization: overview.name })} /> : null}
      {canManage ? <OrganizationWalletAdjustmentForm organizationId={organizationId} balance={summaryQuery.data.balance} /> : null}
      <Card className="overflow-hidden">
        <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45"><div className="flex flex-wrap items-center justify-between gap-3"><div><CardTitle className="text-base">{t('organizationConsole.wallet.ledger.title')}</CardTitle><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.wallet.ledger.subtitle')}</p></div><div className="flex items-center gap-2"><Badge className="bg-surface-2 text-muted-foreground">{entries.length}</Badge><Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.actions.refresh')} disabled={entriesQuery.isFetching} onClick={() => void entriesQuery.refetch()}><RefreshCw className={entriesQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" /><span className="sr-only">{t('organizationConsole.actions.refresh')}</span></Button></div></div></CardHeader>
        <CardContent className="p-0"><WalletLedger entries={entries} query={entriesQuery} formatQuota={formatQuota} formatTime={(value) => timeFormat.format(value * 1_000)} /></CardContent>
      </Card>
    </>
  )
}

function WalletSummary({ summary, formatQuota }: { summary: OrganizationWalletSummaryResponse; formatQuota: (value: number) => string }) {
  const { t } = useTranslation()
  const metrics: Array<{ key: 'balance' | 'used' | 'frozen'; icon: LucideIcon; value: number; tone: string }> = [
    { key: 'balance', icon: Coins, value: summary.balance, tone: 'text-success' },
    { key: 'used', icon: ChartNoAxesColumnIncreasing, value: summary.used_quota, tone: 'text-info' },
    { key: 'frozen', icon: Clock3, value: summary.frozen_quota, tone: 'text-warning' },
  ]
  return <section className="grid gap-3 sm:grid-cols-3" aria-label={t('organizationConsole.wallet.summary.label')}>{metrics.map((metric) => { const Icon = metric.icon; const value = formatQuota(metric.value); return <Card key={metric.key}><CardContent className="p-4"><div className="flex items-center justify-between gap-3"><span className="text-xs text-muted-foreground">{t(`organizationConsole.wallet.summary.${metric.key}`)}</span><span className={cn('grid size-7 place-items-center rounded-lg bg-surface-2', metric.tone)}><Icon className="size-3.5" aria-hidden="true" /></span></div><p className="mt-3 truncate text-xl font-semibold tabular-nums" title={value}>{value}</p><p className="mt-1 text-[0.6875rem] text-muted-foreground">{t(`organizationConsole.wallet.summary.${metric.key}Hint`)}</p></CardContent></Card> })}</section>
}

type WalletQuery = ReturnType<typeof useOrganizationWalletEntries>
function WalletLedger({ entries, query, formatQuota, formatTime }: { entries: OrganizationWalletEntryResponse[]; query: WalletQuery; formatQuota: (value: number) => string; formatTime: (value: number) => string }) {
  const { t } = useTranslation()
  if (query.isPending) return <LedgerLoading />
  if (query.isError && query.data === undefined) return <LedgerState icon={History} title={t('organizationConsole.wallet.errors.ledgerTitle')} body={t('organizationConsole.wallet.errors.ledgerBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>} />
  if (entries.length === 0) return <LedgerState icon={WalletCards} title={t('organizationConsole.wallet.ledger.emptyTitle')} body={t('organizationConsole.wallet.ledger.emptyBody')} />
  return <><div className="divide-y divide-[var(--hairline)]">{entries.map((entry) => <WalletEntry key={entry.id} entry={entry} formatQuota={formatQuota} formatTime={formatTime} />)}</div>{query.isFetchNextPageError ? <div role="alert" className="m-3 flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive"><span>{t('organizationConsole.wallet.errors.more')}</span><Button type="button" size="sm" variant="ghost" onClick={() => void query.fetchNextPage()}>{t('organizationConsole.actions.retry')}</Button></div> : null}{query.hasNextPage && !query.isFetchNextPageError ? <div className="flex justify-center border-t border-[var(--hairline)] p-4"><Button type="button" size="sm" variant="secondary" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>{t(query.isFetchingNextPage ? 'organizationConsole.actions.loadingMore' : 'organizationConsole.actions.loadMore')}</Button></div> : null}</>
}

function WalletEntry({ entry, formatQuota, formatTime }: { entry: OrganizationWalletEntryResponse; formatQuota: (value: number) => string; formatTime: (value: number) => string }) {
  const { t } = useTranslation()
  const positive = entry.quota_delta > 0
  const Icon = positive ? ArrowUpRight : ArrowDownRight
  return <article className="grid grid-cols-[2rem_minmax(0,1fr)] gap-3 px-4 py-3.5 sm:grid-cols-[2rem_minmax(0,1fr)_auto]"><span className={cn('grid size-8 place-items-center rounded-lg bg-surface-2', positive ? 'text-success' : 'text-destructive')}><Icon className="size-4" aria-hidden="true" /></span><div className="min-w-0"><div className="flex min-w-0 flex-wrap items-center gap-2"><span className="truncate text-xs font-medium">{entry.reason ?? t(`organizationConsole.wallet.entryReason.${entry.entry_type}`)}</span><Badge>{t(`organizationConsole.wallet.entryType.${entry.entry_type}`)}</Badge></div><p className="mt-1 text-[0.6875rem] text-muted-foreground">{formatTime(entry.created_at)} · {entry.actor_user_id === null ? t('organizationConsole.wallet.ledger.systemActor') : t('organizationConsole.wallet.ledger.actor', { id: entry.actor_user_id })} · #{entry.id}</p><p className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground/80" title={entry.event_id}>{entry.event_id.slice(0, 8)}…{entry.event_id.slice(-8)}</p></div><div className="col-start-2 text-left sm:col-start-auto sm:text-right"><div className={cn('text-sm font-semibold tabular-nums', positive ? 'text-success' : 'text-destructive')}>{positive ? '+' : ''}{formatQuota(entry.quota_delta)}</div><div className="mt-1 text-[0.6875rem] text-muted-foreground tabular-nums">{t('organizationConsole.wallet.ledger.balanceAfter', { value: formatQuota(entry.balance_after) })}</div></div></article>
}

function WalletLoading() { return <div className="grid gap-3 sm:grid-cols-3" role="status">{[0, 1, 2].map((item) => <Skeleton key={item} className="h-32 rounded-xl" />)}</div> }
function LedgerLoading() { return <div className="grid gap-4 p-5" role="status">{[0, 1, 2, 3].map((item) => <div key={item} className="grid grid-cols-[2rem_1fr_auto] gap-3"><Skeleton className="size-8 rounded-lg" /><div className="grid gap-2"><Skeleton className="h-4 w-2/3" /><Skeleton className="h-3 w-1/2" /></div><Skeleton className="h-5 w-16" /></div>)}</div> }
function LedgerState({ icon: Icon, title, body, action }: { icon: LucideIcon; title: string; body: string; action?: ReactNode }) { return <div className="grid min-h-52 place-items-center p-6 text-center"><div className="max-w-sm"><Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div> }
