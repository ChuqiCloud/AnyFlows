import { Button, Card, CardBody, CardHeader, Chip, Skeleton } from '@heroui/react'
import { ArrowRight, Coins, FlaskConical, KeyRound, LayoutGrid, RefreshCw, WalletCards } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { useApiKeys } from '@/features/api-keys/api-key-api'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { useUserUsageLogs } from '@/features/usage-logs/usage-log-api'
import { useUserWalletSummary } from '@/features/wallet/wallet-api'
import { cn } from '@/lib/utils'

const RECENT_LOG_LIMIT = 20
const RECENT_LIST_LIMIT = 8

/** 普通用户的首页：余额、Key、最近调用和常用入口，不读取任何管理端接口。 */
export function UserOverviewPage() {
  const { t, i18n } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const walletQuery = useUserWalletSummary()
  const keysQuery = useApiKeys()
  const logsQuery = useUserUsageLogs(undefined, undefined, RECENT_LOG_LIMIT)
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const formatNumber = useMemo(() => new Intl.NumberFormat(locale), [locale])
  const formatTime = useMemo(() => new Intl.DateTimeFormat(locale, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }), [locale])
  const refreshing = walletQuery.isFetching || keysQuery.isFetching || logsQuery.isFetching
  const logs = logsQuery.data?.logs ?? []
  const keys = keysQuery.data?.tokens ?? []
  const activeKeys = keys.filter((key) => key.status === 'enabled').length
  const recentTokens = logs.reduce((sum, log) => sum + log.input_tokens + log.output_tokens, 0)
  const recentQuota = logs.reduce((sum, log) => sum + log.quota, 0)

  const refresh = () => {
    void Promise.all([walletQuery.refetch(), keysQuery.refetch(), logsQuery.refetch()])
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-xl font-semibold tracking-tight">{t('dashboard.user.title')}</h2>
          <p className="mt-1 max-w-2xl text-sm leading-5 text-muted-foreground">{t('dashboard.user.subtitle')}</p>
        </div>
        <Button type="button" size="sm" variant="bordered" isDisabled={refreshing} onClick={refresh}>
          <RefreshCw className={cn('size-3.5', refreshing && 'animate-spin')} aria-hidden="true" />
          {t('dashboard.actions.refresh')}
        </Button>
      </header>

      <section className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_15rem]">
        <Card className="border border-[var(--hairline)]" shadow="none">
          <CardBody className="grid gap-4 p-4 sm:grid-cols-3 sm:p-5 sm:divide-x sm:divide-[var(--hairline)]">
            <Metric
              icon={Coins}
              tone="text-success"
              label={t('dashboard.user.balance')}
              value={walletQuery.data ? formatQuota(walletQuery.data.balance) : undefined}
              hint={t('dashboard.user.balanceHint')}
              pending={walletQuery.isPending}
              href="/console/wallet"
            />
            <Metric
              icon={KeyRound}
              tone="text-brand"
              label={t('dashboard.user.keys')}
              value={keysQuery.data ? formatNumber.format(activeKeys) : undefined}
              hint={keysQuery.data ? t('dashboard.user.keysHint', { total: keys.length, capacity: keysQuery.data.capacity }) : ''}
              pending={keysQuery.isPending}
              href="/console/api-keys"
              className="sm:pl-4"
            />
            <Metric
              icon={LayoutGrid}
              tone="text-info"
              label={t('dashboard.user.recentQuota', { count: logs.length })}
              value={logsQuery.data ? formatQuota(recentQuota) : undefined}
              hint={logsQuery.data ? t('dashboard.user.recentTokens', { count: formatNumber.format(recentTokens) }) : ''}
              pending={logsQuery.isPending}
              href="/console/usage-logs"
              className="sm:pl-4"
            />
          </CardBody>
        </Card>

        <Card className="border border-[var(--hairline)]" shadow="none">
          <CardBody className="flex flex-col gap-1.5 p-3.5">
            <span className="px-2 text-xs font-medium text-muted-foreground">{t('dashboard.user.quickActions')}</span>
            <QuickAction icon={KeyRound} href="/console/api-keys" label={t('dashboard.user.actions.createKey')} />
            <QuickAction icon={FlaskConical} href="/console/playground" label={t('dashboard.user.actions.playground')} />
            <QuickAction icon={LayoutGrid} href="/console/models" label={t('dashboard.user.actions.models')} />
            <QuickAction icon={WalletCards} href="/console/wallet" label={t('dashboard.user.actions.topup')} />
          </CardBody>
        </Card>
      </section>

      <Card className="overflow-hidden border border-[var(--hairline)]" shadow="none">
        <CardHeader className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--hairline)] bg-surface-1/45 p-5">
          <h3 className="text-base font-semibold">{t('dashboard.user.recentTitle')}</h3>
          <Button as="a" href="/console/usage-logs" size="sm" variant="light">
            {t('dashboard.user.viewAll')}
            <ArrowRight className="size-3.5" aria-hidden="true" />
          </Button>
        </CardHeader>
        <CardBody className="p-0">
          {logsQuery.isPending ? (
            <div className="grid gap-3 p-5" role="status">
              {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-9 rounded-lg" />)}
            </div>
          ) : logsQuery.isError ? (
            <div className="flex items-center justify-between gap-3 p-5 text-xs">
              <span className="text-muted-foreground">{t('usageLogs.error.body')}</span>
              <Button type="button" size="sm" variant="bordered" onClick={() => void logsQuery.refetch()}>{t('dashboard.actions.retry')}</Button>
            </div>
          ) : logs.length === 0 ? (
            <div className="grid min-h-40 place-items-center p-6 text-center">
              <div>
                <p className="text-sm font-medium">{t('dashboard.user.emptyTitle')}</p>
                <p className="mt-1 text-xs text-muted-foreground">{t('dashboard.user.emptyBody')}</p>
                <Button as="a" className="mt-4" color="primary" href="/console/playground" size="sm">{t('dashboard.user.actions.playground')}</Button>
              </div>
            </div>
          ) : (
            <div className="divide-y divide-[var(--hairline)]">
              {logs.slice(0, RECENT_LIST_LIMIT).map((log) => (
                <div key={log.id} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 px-5 py-3 text-xs">
                  <div className="min-w-0">
                    <p className="truncate font-medium">{log.model ?? t('usageLogs.values.legacyModel')}</p>
                    <p className="mt-0.5 text-muted-foreground">
                      {formatTime.format(log.created_at * 1_000)}
                      {' · '}
                      {t('usageLogs.values.inputOutputCompact', { input: formatNumber.format(log.input_tokens), output: formatNumber.format(log.output_tokens) })}
                    </p>
                  </div>
                  <Chip className="bg-surface-2 tabular-nums" size="sm" variant="flat">{formatQuota(log.quota)}</Chip>
                </div>
              ))}
            </div>
          )}
        </CardBody>
      </Card>
    </div>
  )
}

function Metric({
  icon: Icon,
  tone,
  label,
  value,
  hint,
  pending,
  href,
  className,
}: {
  icon: LucideIcon
  tone: string
  label: string
  value?: string
  hint: string
  pending: boolean
  href: string
  className?: string
}) {
  return (
    <a href={href} className={cn('group flex min-w-0 items-start gap-3', className)}>
      <span className={cn('grid size-8 shrink-0 place-items-center rounded-full bg-surface-2', tone)}>
        <Icon className="size-4" aria-hidden="true" />
      </span>
      <span className="min-w-0">
        <span className="block text-xs text-muted-foreground">{label}</span>
        {pending
          ? <Skeleton className="mt-1.5 h-7 w-24 rounded-md" />
          : <span className="mt-1 block truncate text-2xl font-semibold tabular-nums group-hover:underline" title={value}>{value ?? '—'}</span>}
        <span className="mt-0.5 block text-[0.6875rem] text-muted-foreground">{hint}</span>
      </span>
    </a>
  )
}

function QuickAction({ icon: Icon, href, label }: { icon: LucideIcon; href: string; label: string }) {
  return (
    <a href={href} className="group flex items-center gap-3 rounded-lg px-2.5 py-2 text-sm transition-colors hover:bg-surface-2/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
      <Icon className="size-4 text-brand" aria-hidden="true" />
      <span className="flex-1">{label}</span>
      <ArrowRight className="size-3.5 text-muted-foreground transition-transform group-hover:translate-x-0.5" aria-hidden="true" />
    </a>
  )
}
