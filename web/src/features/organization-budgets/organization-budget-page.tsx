import { useQueryClient } from '@tanstack/react-query'
import {
  Building2,
  CircleCheck,
  Plus,
  RefreshCw,
  ShieldX,
  SlidersHorizontal,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'
import {
  clearOrganizationWorkspaceCache,
  organizationBudgetErrorStatus,
  useOrganizationBudgets,
  useOrganizationDirectory,
  useOrganizationMemberCatalog,
  useOrganizationOverview,
  useOrganizationTeamCatalog,
} from './organization-budget-api'
import { OrganizationBudgetOverview } from './organization-budget-overview'
import { OrganizationBudgetPolicySheet } from './organization-budget-policy-sheet'

const BUDGET_PERMISSION = 'organization.budgets.manage'

/** 企业空间预算入口；选择器是唯一组织上下文来源，切换时彻底释放旧组织状态。 */
export function OrganizationBudgetPage() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const organizationsQuery = useOrganizationDirectory()
  const [selectedId, setSelectedId] = useState<number>()
  const [switching, setSwitching] = useState(false)
  const organizations = useMemo(() => organizationsQuery.data ?? [], [organizationsQuery.data])

  useEffect(() => {
    if (organizations.length === 0) {
      setSelectedId(undefined)
      return
    }
    if (selectedId === undefined) {
      setSelectedId(organizations[0].id)
      return
    }
    if (!organizations.some((organization) => organization.id === selectedId)) {
      void clearOrganizationWorkspaceCache(queryClient, selectedId)
      setSelectedId(organizations[0].id)
    }
  }, [organizations, queryClient, selectedId])

  const selectOrganization = async (nextId: number) => {
    if (nextId === selectedId || switching) return
    setSwitching(true)
    if (selectedId !== undefined) {
      await clearOrganizationWorkspaceCache(queryClient, selectedId)
    }
    setSelectedId(nextId)
    setSwitching(false)
  }

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <header className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div className="min-w-0">
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <Building2 className="size-3.5" aria-hidden="true" />
            {t('organizationBudgets.eyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('organizationBudgets.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('organizationBudgets.subtitle')}
          </p>
        </div>

        {organizations.length > 0 ? (
          <div className="w-full min-w-0 sm:w-72">
            <label htmlFor="organization-budget-workspace" className="mb-1.5 block text-[0.6875rem] font-medium text-muted-foreground">
              {t('organizationBudgets.workspace.label')}
            </label>
            <Select
              id="organization-budget-workspace"
              value={selectedId ?? ''}
              disabled={switching}
              onChange={(event) => void selectOrganization(Number(event.target.value))}
            >
              {organizations.map((organization) => (
                <option key={organization.id} value={organization.id}>
                  {organization.name} ({organization.slug})
                </option>
              ))}
            </Select>
          </div>
        ) : null}
      </header>

      {organizationsQuery.isPending ? (
        <OrganizationDirectoryLoading />
      ) : organizationsQuery.isError && organizationsQuery.data === undefined ? (
        <PageState
          title={t('organizationBudgets.errors.directoryTitle')}
          body={t('organizationBudgets.errors.directoryBody')}
          action={(
            <Button type="button" size="sm" variant="secondary" onClick={() => void organizationsQuery.refetch()}>
              <RefreshCw aria-hidden="true" />
              {t('organizationBudgets.actions.retry')}
            </Button>
          )}
        />
      ) : organizations.length === 0 ? (
        <PageState
          icon={Building2}
          title={t('organizationBudgets.workspace.emptyTitle')}
          body={t('organizationBudgets.workspace.emptyBody')}
        />
      ) : selectedId === undefined || switching ? (
        <OrganizationWorkspaceLoading />
      ) : (
        <OrganizationBudgetWorkspace key={selectedId} organizationId={selectedId} />
      )}
    </div>
  )
}

function OrganizationBudgetWorkspace({ organizationId }: { organizationId: number }) {
  const { i18n, t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const overviewQuery = useOrganizationOverview(organizationId)
  const canManage = overviewQuery.data?.permissions.includes(BUDGET_PERMISSION) === true
  const budgetsQuery = useOrganizationBudgets(organizationId, canManage)
  const teamsQuery = useOrganizationTeamCatalog(organizationId, canManage)
  const membersQuery = useOrganizationMemberCatalog(organizationId, canManage)
  const [creating, setCreating] = useState(false)
  const [createdVersion, setCreatedVersion] = useState<number>()
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const timeFormat = useMemo(() => new Intl.DateTimeFormat(locale, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    timeZoneName: 'short',
  }), [locale])
  const refreshing = overviewQuery.isFetching
    || budgetsQuery.isFetching
    || teamsQuery.isFetching
    || membersQuery.isFetching

  const refresh = async () => {
    const requests: Promise<unknown>[] = [overviewQuery.refetch()]
    if (canManage) {
      requests.push(budgetsQuery.refetch(), teamsQuery.refetch(), membersQuery.refetch())
    }
    await Promise.all(requests)
  }

  if (overviewQuery.isPending) return <OrganizationWorkspaceLoading />
  if (overviewQuery.isError || !overviewQuery.data) {
    const forbidden = organizationBudgetErrorStatus(overviewQuery.error) === 403
    return (
      <PageState
        icon={forbidden ? ShieldX : undefined}
        title={t(forbidden ? 'organizationBudgets.errors.forbiddenTitle' : 'organizationBudgets.errors.overviewTitle')}
        body={t(forbidden ? 'organizationBudgets.errors.forbiddenBody' : 'organizationBudgets.errors.overviewBody')}
        action={forbidden ? undefined : (
          <Button type="button" size="sm" variant="secondary" onClick={() => void overviewQuery.refetch()}>
            <RefreshCw aria-hidden="true" />
            {t('organizationBudgets.actions.retry')}
          </Button>
        )}
      />
    )
  }

  if (!canManage) {
    return (
      <>
        <OrganizationSummary overview={overviewQuery.data} />
        <PageState
          icon={ShieldX}
          title={t('organizationBudgets.permission.title')}
          body={t('organizationBudgets.permission.body')}
        />
      </>
    )
  }

  const budgetsForbidden = budgetsQuery.isError && organizationBudgetErrorStatus(budgetsQuery.error) === 403
  const teams = teamsQuery.data ?? []
  const members = membersQuery.data ?? []

  return (
    <div className="grid min-w-0 gap-4">
      <OrganizationSummary overview={overviewQuery.data} />

      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <SlidersHorizontal className="size-3.5 shrink-0" aria-hidden="true" />
          <span>{t('organizationBudgets.summary.loaded', {
            policies: budgetsQuery.data?.policies.length ?? 0,
            windows: budgetsQuery.data?.windows.length ?? 0,
          })}</span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={refreshing} onClick={() => void refresh()}>
            <RefreshCw className={refreshing ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('organizationBudgets.actions.refresh')}
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={budgetsQuery.isPending || budgetsForbidden}
            onClick={() => setCreating(true)}
          >
            <Plus aria-hidden="true" />
            {t('organizationBudgets.actions.newPolicy')}
          </Button>
        </div>
      </div>

      {createdVersion !== undefined ? (
        <div role="status" className="flex items-center gap-2 rounded-lg border border-success/25 bg-success/8 px-3 py-2.5 text-xs text-success">
          <CircleCheck className="size-4 shrink-0" aria-hidden="true" />
          <span>{t('organizationBudgets.success.created', { version: createdVersion })}</span>
        </div>
      ) : null}

      {budgetsQuery.isPending ? (
        <BudgetContentLoading />
      ) : budgetsQuery.isError || !budgetsQuery.data ? (
        <PageState
          icon={budgetsForbidden ? ShieldX : undefined}
          title={t(budgetsForbidden ? 'organizationBudgets.errors.forbiddenTitle' : 'organizationBudgets.errors.budgetsTitle')}
          body={t(budgetsForbidden ? 'organizationBudgets.errors.forbiddenBody' : 'organizationBudgets.errors.budgetsBody')}
          action={budgetsForbidden ? undefined : (
            <Button type="button" size="sm" variant="secondary" onClick={() => void budgetsQuery.refetch()}>
              <RefreshCw aria-hidden="true" />
              {t('organizationBudgets.actions.retry')}
            </Button>
          )}
        />
      ) : (
        <OrganizationBudgetOverview
          overview={budgetsQuery.data}
          teams={teams}
          members={members}
          formatQuota={formatQuota}
          formatTime={(value) => timeFormat.format(value * 1_000)}
        />
      )}

      {budgetsQuery.data ? (
        <OrganizationBudgetPolicySheet
          organizationId={organizationId}
          open={creating}
          onOpenChange={setCreating}
          policies={budgetsQuery.data.policies}
          teams={teams}
          members={members}
          directoriesPending={teamsQuery.isPending || membersQuery.isPending}
          directoriesError={teamsQuery.isError || membersQuery.isError}
          onRefresh={refresh}
          onCreated={(policy) => {
            setCreatedVersion(policy.version)
            setCreating(false)
          }}
        />
      ) : null}
    </div>
  )
}

function OrganizationSummary({ overview }: { overview: OrganizationOverviewResponse }) {
  const { t } = useTranslation()
  const entitlement = overview.entitlement
  return (
    <section className="grid gap-3 border-y border-[var(--hairline)] py-3 sm:grid-cols-[minmax(0,1.5fr)_repeat(3,minmax(0,1fr))] sm:items-center">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h3 className="truncate text-sm font-semibold">{overview.name}</h3>
          <Badge>{t(`organizationBudgets.roles.${overview.role}`, { defaultValue: overview.role })}</Badge>
          <Badge className="text-success">{t(`organizationBudgets.entitlement.${entitlement.status}`, { defaultValue: entitlement.status })}</Badge>
        </div>
        <p className="mt-1 truncate text-[0.6875rem] text-muted-foreground">{overview.slug}</p>
      </div>
      <SummaryMetric
        label={t('organizationBudgets.organization.seats')}
        value={t('organizationBudgets.organization.capacity', { used: entitlement.active_seats, limit: entitlement.seat_limit })}
      />
      <SummaryMetric
        label={t('organizationBudgets.organization.teams')}
        value={t('organizationBudgets.organization.capacity', { used: entitlement.active_teams, limit: entitlement.team_limit })}
      />
      <SummaryMetric
        label={t('organizationBudgets.organization.retention')}
        value={t('organizationBudgets.organization.days', { count: entitlement.audit_retention_days })}
      />
    </section>
  )
}

function SummaryMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 sm:text-right">
      <p className="text-[0.6875rem] text-muted-foreground">{label}</p>
      <p className="mt-1 truncate text-xs font-semibold tabular-nums" title={value}>{value}</p>
    </div>
  )
}

function OrganizationDirectoryLoading() {
  return (
    <div className="grid gap-3" role="status">
      <Skeleton className="h-10 w-full rounded-lg" />
      <OrganizationWorkspaceLoading />
    </div>
  )
}

function OrganizationWorkspaceLoading() {
  return (
    <div className="grid gap-4" role="status">
      <Skeleton className="h-20 w-full rounded-lg" />
      <BudgetContentLoading />
    </div>
  )
}

function BudgetContentLoading() {
  return (
    <div className="grid gap-3" role="status">
      {[0, 1, 2].map((item) => <Skeleton key={item} className="h-40 w-full rounded-lg" />)}
    </div>
  )
}

function PageState({
  icon: Icon = SlidersHorizontal,
  title,
  body,
  action,
}: {
  icon?: LucideIcon
  title: string
  body: string
  action?: ReactNode
}) {
  return (
    <div className="grid min-h-64 place-items-center rounded-lg border border-dashed border-[var(--hairline)] p-6 text-center">
      <div className="max-w-md">
        <Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
        <h3 className="mt-3 text-sm font-semibold">{title}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>
        {action ? <div className="mt-4">{action}</div> : null}
      </div>
    </div>
  )
}
