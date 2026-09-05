import {
  Activity,
  Bot,
  Building2,
  ChartNoAxesColumnIncreasing,
  KeyRound,
  RefreshCw,
  ShieldCheck,
  UsersRound,
  WalletCards,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { useOrganizationBudgets } from '@/features/organization-budgets/organization-budget-api'
import { useOrganizationUsageLogs } from '@/features/organization-activity/organization-activity-api'
import { useOrganizationWalletSummary } from '@/features/organization-wallet/organization-wallet-api'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import {
  OrganizationPageBoundary,
  OrganizationPageHeader,
  useOrganizationPageState,
} from './organization-page-shell'
import { OrganizationPlanCatalog } from './organization-plan-catalog'

const WALLET_READ = 'organization.wallet.read'
const WALLET_MANAGE = 'organization.wallet.manage'
const BUDGETS_MANAGE = 'organization.budgets.manage'
const USAGE_READ_ALL = 'organization.usage_logs.read_all'
const USAGE_READ_SELF = 'organization.usage_logs.read_self'

/** 企业控制台首页只组合服务端授权投影、钱包、预算和日志事实。 */
export function OrganizationOverviewPage() {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <OrganizationPageHeader
        icon={Building2}
        title={t('organizationConsole.overview.title')}
        subtitle={t('organizationConsole.overview.subtitle')}
      />
      <OrganizationPageBoundary state={state}>
        {(organizationId, overview) => (
          <OrganizationOverviewWorkspace
            key={organizationId}
            organizationId={organizationId}
            overview={overview}
          />
        )}
      </OrganizationPageBoundary>
    </div>
  )
}

function OrganizationOverviewWorkspace({
  organizationId,
  overview,
}: {
  organizationId: number
  overview: OrganizationOverviewResponse
}) {
  const { t, i18n } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const formatTime = useMemo(
    () => new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }),
    [i18n.resolvedLanguage],
  )
  const canReadWallet = overview.permissions.includes(WALLET_READ)
  const canManageBudgets = overview.permissions.includes(BUDGETS_MANAGE)
  const canReadUsage = overview.permissions.includes(USAGE_READ_ALL)
    || overview.permissions.includes(USAGE_READ_SELF)
  const walletQuery = useOrganizationWalletSummary(organizationId, canReadWallet)
  const budgetsQuery = useOrganizationBudgets(organizationId, canManageBudgets)
  const usageQuery = useOrganizationUsageLogs(organizationId, 'summary', undefined, 50, canReadUsage)
  const requestCount = useMemo(
    () => usageQuery.data?.summaries.reduce((total, item) => total + item.request_count, 0),
    [usageQuery.data?.summaries],
  )
  const organizationWindow = budgetsQuery.data?.windows.find((window) => window.scope.kind === 'organization')
  const refreshing = walletQuery.isFetching || budgetsQuery.isFetching || usageQuery.isFetching
  const refresh = () => {
    const requests: Promise<unknown>[] = []
    if (canReadWallet) requests.push(walletQuery.refetch())
    if (canManageBudgets) requests.push(budgetsQuery.refetch())
    if (canReadUsage) requests.push(usageQuery.refetch())
    void Promise.all(requests)
  }

  const metrics: Array<{
    key: 'balance' | 'budget' | 'requests' | 'seats'
    icon: LucideIcon
    tone: string
    value: string
    hint: string
  }> = [
    {
      key: 'balance',
      icon: WalletCards,
      tone: 'text-success',
      value: canReadWallet && walletQuery.data ? formatQuota(walletQuery.data.balance) : t('organizationConsole.common.restricted'),
      hint: canReadWallet ? t('organizationConsole.overview.metrics.balanceHint') : t('organizationConsole.overview.metrics.permissionHint'),
    },
    {
      key: 'budget',
      icon: ChartNoAxesColumnIncreasing,
      tone: 'text-info',
      value: canManageBudgets && organizationWindow
        ? formatQuota(organizationWindow.remaining_quota)
        : canManageBudgets
          ? t('organizationConsole.common.unlimited')
          : t('organizationConsole.common.restricted'),
      hint: canManageBudgets ? t('organizationConsole.overview.metrics.budgetHint') : t('organizationConsole.overview.metrics.permissionHint'),
    },
    {
      key: 'requests',
      icon: Activity,
      tone: 'text-brand',
      value: canReadUsage && requestCount !== undefined ? requestCount.toLocaleString() : t('organizationConsole.common.restricted'),
      hint: canReadUsage ? t('organizationConsole.overview.metrics.requestsHint') : t('organizationConsole.overview.metrics.permissionHint'),
    },
    {
      key: 'seats',
      icon: UsersRound,
      tone: 'text-warning',
      value: `${overview.entitlement.active_seats} / ${overview.entitlement.seat_limit}`,
      hint: t('organizationConsole.overview.metrics.seatsHint', { count: overview.entitlement.remaining_seats }),
    },
  ]

  return (
    <>
      <section className="flex flex-col gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1/45 p-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="truncate text-base font-semibold">{overview.name}</h3>
            <Badge className={entitlementTone(overview.entitlement.status)}>{t(`organizationConsole.entitlement.${overview.entitlement.status}`)}</Badge>
            <Badge className="bg-surface-2 text-muted-foreground">{t(`organizationConsole.roles.${overview.role}`)}</Badge>
          </div>
          <p className="mt-1 truncate text-xs text-muted-foreground">{overview.slug} · #{overview.id}</p>
          <p className="mt-2 text-xs leading-5 text-muted-foreground">{t(`organizationConsole.entitlementHint.${overview.entitlement.status}`)}</p>
        </div>
        <Button type="button" size="sm" variant="secondary" disabled={refreshing} onClick={refresh}>
          <RefreshCw className={refreshing ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('organizationConsole.actions.refresh')}
        </Button>
      </section>

      <section className="grid gap-3 lg:grid-cols-[minmax(0,1.2fr)_minmax(18rem,0.8fr)]">
        <Card>
          <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
            <CardTitle className="text-base">{t('organizationConsole.overview.entitlement.title')}</CardTitle>
            <p className="text-xs leading-5 text-muted-foreground">{t('organizationConsole.overview.entitlement.subtitle')}</p>
          </CardHeader>
          <CardContent className="grid gap-3 p-4 sm:grid-cols-2">
            <EntitlementDate
              label={t('organizationConsole.overview.entitlement.validUntil')}
              timestamp={overview.entitlement.valid_until}
              formatTime={formatTime}
            />
            <EntitlementDate
              label={t('organizationConsole.overview.entitlement.graceUntil')}
              timestamp={overview.entitlement.grace_until}
              formatTime={formatTime}
              optional
            />
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
            <CardTitle className="text-base">{t('organizationConsole.overview.funding.title')}</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-3 p-4 text-xs leading-5 text-muted-foreground">
            <div className="flex items-start gap-2">
              <WalletCards className="mt-0.5 size-4 shrink-0 text-brand" aria-hidden="true" />
              <p>{t('organizationConsole.overview.funding.wallet')}</p>
            </div>
            <p className="border-t border-[var(--hairline)] pt-3">{t('organizationConsole.overview.funding.upgrade')}</p>
          </CardContent>
        </Card>
      </section>

      <OrganizationPlanCatalog
        organizationId={organizationId}
        canPurchase={overview.permissions.includes(WALLET_MANAGE)}
      />

      <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-label={t('organizationConsole.overview.metrics.label')}>
        {metrics.map((metric) => {
          const Icon = metric.icon
          return (
            <Card key={metric.key}>
              <CardContent className="p-4">
                <div className="flex items-center justify-between gap-3">
                  <span className="text-xs text-muted-foreground">{t(`organizationConsole.overview.metrics.${metric.key}`)}</span>
                  <span className={cn('grid size-7 place-items-center rounded-lg bg-surface-2', metric.tone)}><Icon className="size-3.5" aria-hidden="true" /></span>
                </div>
                <p className="mt-3 truncate text-xl font-semibold tabular-nums" title={metric.value}>{metric.value}</p>
                <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{metric.hint}</p>
              </CardContent>
            </Card>
          )
        })}
      </section>

      <Card className="overflow-hidden">
        <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
          <CardTitle className="text-base">{t('organizationConsole.overview.capacity.title')}</CardTitle>
          <p className="text-xs leading-5 text-muted-foreground">{t('organizationConsole.overview.capacity.subtitle')}</p>
        </CardHeader>
        <CardContent className="grid gap-0 p-0 sm:grid-cols-3 sm:divide-x sm:divide-[var(--hairline)]">
          <CapacityItem icon={UsersRound} label={t('organizationConsole.overview.capacity.seats')} used={overview.entitlement.active_seats} limit={overview.entitlement.seat_limit} />
          <CapacityItem icon={Building2} label={t('organizationConsole.overview.capacity.teams')} used={overview.entitlement.active_teams} limit={overview.entitlement.team_limit} />
          <CapacityItem icon={KeyRound} label={t('organizationConsole.overview.capacity.keys')} used={0} limit={overview.entitlement.organization_key_limit} hideUsed />
        </CardContent>
      </Card>

      <section className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <QuickLink icon={UsersRound} href="#/console/organization-members" label={t('organizationConsole.overview.links.members')} />
        <QuickLink icon={Building2} href="#/console/organization-departments" label={t('organizationConsole.overview.links.departments')} />
        <QuickLink icon={KeyRound} href="#/console/organization-api-keys" label={t('organizationConsole.overview.links.keys')} />
        <QuickLink icon={Bot} href="#/console/organization-service-accounts" label={t('organizationConsole.overview.links.serviceAccounts')} />
        <QuickLink icon={WalletCards} href="#/console/organization-wallet" label={t('organizationConsole.overview.links.wallet')} />
        <QuickLink icon={ChartNoAxesColumnIncreasing} href="#/console/organization-budgets" label={t('organizationConsole.overview.links.budgets')} />
        <QuickLink icon={Activity} href="#/console/organization-activity" label={t('organizationConsole.overview.links.activity')} />
        <QuickLink icon={ShieldCheck} href="#/console/organization-settings" label={t('organizationConsole.overview.links.settings')} />
      </section>
    </>
  )
}

function EntitlementDate({
  label,
  timestamp,
  formatTime,
  optional = false,
}: {
  label: string
  timestamp?: number | null
  formatTime: Intl.DateTimeFormat
  optional?: boolean
}) {
  const { t } = useTranslation()
  const value = timestamp && timestamp > 0
    ? formatTime.format(timestamp * 1_000)
    : optional
      ? t('organizationConsole.overview.entitlement.notApplicable')
      : t('organizationConsole.overview.entitlement.unavailable')
  return (
    <div className="rounded-lg border border-[var(--hairline)] bg-surface-1/35 p-3">
      <p className="text-[0.6875rem] text-muted-foreground">{label}</p>
      <p className="mt-1 text-sm font-semibold tabular-nums">{value}</p>
    </div>
  )
}

function CapacityItem({
  icon: Icon,
  label,
  used,
  limit,
  hideUsed = false,
}: {
  icon: LucideIcon
  label: string
  used: number
  limit: number
  hideUsed?: boolean
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-3 border-b border-[var(--hairline)] p-4 last:border-b-0 sm:border-b-0">
      <span className="grid size-8 place-items-center rounded-lg bg-surface-2 text-brand"><Icon className="size-4" aria-hidden="true" /></span>
      <div className="min-w-0">
        <p className="text-xs text-muted-foreground">{label}</p>
        <p className="mt-1 text-sm font-semibold tabular-nums">
          {hideUsed ? t('organizationConsole.overview.capacity.limitOnly', { limit }) : t('organizationConsole.overview.capacity.value', { limit, used })}
        </p>
      </div>
    </div>
  )
}

function QuickLink({ icon: Icon, href, label }: { icon: LucideIcon; href: string; label: string }) {
  return (
    <a href={href} className="flex min-h-12 items-center gap-3 rounded-lg border border-[var(--hairline)] bg-surface-1/35 px-3 py-2 text-sm font-medium transition-colors hover:bg-surface-2/70">
      <Icon className="size-4 shrink-0 text-brand" aria-hidden="true" />
      <span>{label}</span>
    </a>
  )
}

function entitlementTone(status: string) {
  if (status === 'active') return 'border-success/25 bg-success/10 text-success'
  if (status === 'grace') return 'border-warning/25 bg-warning/10 text-warning'
  if (status === 'read_only') return 'border-info/25 bg-info/10 text-info'
  return 'border-destructive/25 bg-destructive/10 text-destructive'
}
