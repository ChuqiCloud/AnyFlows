import { Activity, Building2, RefreshCw, ScrollText, ShieldCheck, ShieldX } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import {
  organizationBudgetErrorStatus,
  useOrganizationOverview,
} from '@/features/organization-budgets/organization-budget-api'
import { useOrganizationWorkspace } from '@/features/organization-console/organization-workspace-context'
import { OrganizationMobileWorkspaceSelect } from '@/features/organization-console/organization-workspace-switcher'
import type { OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'
import type { OrganizationUsageScope } from './organization-activity-api'
import { OrganizationAuditPanel } from './organization-audit-panel'
import { OrganizationUsagePanel } from './organization-usage-panel'

const USAGE_READ_ALL = 'organization.usage_logs.read_all'
const USAGE_READ_SELF = 'organization.usage_logs.read_self'
const AUDIT_READ = 'organization.audit.read'

type ActivityTab = 'audit' | 'usage'

/** 企业活动入口；企业切换会取消旧请求并通过 keyed workspace 清空页签、筛选和游标。 */
export function OrganizationActivityPage() {
  const { t } = useTranslation()
  const workspace = useOrganizationWorkspace()

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <header className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div className="min-w-0">
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <Activity className="size-3.5" aria-hidden="true" />
            {t('organizationActivity.eyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('organizationActivity.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('organizationActivity.subtitle')}</p>
        </div>

        <OrganizationMobileWorkspaceSelect />
      </header>

      {workspace.organizationsQuery.isPending ? (
        <OrganizationDirectoryLoading />
      ) : workspace.organizationsQuery.isError && workspace.organizationsQuery.data === undefined ? (
        <PageState
          title={t('organizationActivity.errors.directoryTitle')}
          body={t('organizationActivity.errors.directoryBody')}
          action={<Button type="button" size="sm" variant="secondary" onClick={() => void workspace.organizationsQuery.refetch()}><RefreshCw aria-hidden="true" />{t('organizationActivity.actions.retry')}</Button>}
        />
      ) : workspace.organizations.length === 0 ? (
        <PageState icon={Building2} title={t('organizationActivity.workspace.emptyTitle')} body={t('organizationActivity.workspace.emptyBody')} />
      ) : workspace.selectedId === undefined || workspace.switching ? (
        <OrganizationWorkspaceLoading />
      ) : (
        <OrganizationActivityWorkspace key={workspace.selectedId} organizationId={workspace.selectedId} />
      )}
    </div>
  )
}

function OrganizationActivityWorkspace({ organizationId }: { organizationId: number }) {
  const { t } = useTranslation()
  const overviewQuery = useOrganizationOverview(organizationId)
  const [tab, setTab] = useState<ActivityTab>('usage')

  if (overviewQuery.isPending) return <OrganizationWorkspaceLoading />
  if (overviewQuery.isError || !overviewQuery.data) {
    const forbidden = organizationBudgetErrorStatus(overviewQuery.error) === 403
    return (
      <PageState
        icon={forbidden ? ShieldX : undefined}
        title={t(forbidden ? 'organizationActivity.errors.forbiddenTitle' : 'organizationActivity.errors.overviewTitle')}
        body={t(forbidden ? 'organizationActivity.errors.forbiddenBody' : 'organizationActivity.errors.overviewBody')}
        action={forbidden ? undefined : <Button type="button" size="sm" variant="secondary" onClick={() => void overviewQuery.refetch()}><RefreshCw aria-hidden="true" />{t('organizationActivity.actions.retry')}</Button>}
      />
    )
  }

  const overview = overviewQuery.data
  const scopes = usageScopes(overview)
  const canAudit = overview.permissions.includes(AUDIT_READ)
  const availableTabs: ActivityTab[] = [
    ...(scopes.length > 0 ? ['usage' as const] : []),
    ...(canAudit ? ['audit' as const] : []),
  ]
  const activeTab = availableTabs.includes(tab) ? tab : availableTabs[0]

  if (!activeTab) {
    return (
      <>
        <OrganizationSummary overview={overview} />
        <PageState icon={ShieldX} title={t('organizationActivity.permission.title')} body={t('organizationActivity.permission.body')} />
      </>
    )
  }

  return (
    <div className="grid min-w-0 gap-4">
      <OrganizationSummary overview={overview} />

      <div role="tablist" aria-label={t('organizationActivity.tabs.label')} className="flex w-fit max-w-full items-center gap-1 rounded-lg border border-[var(--hairline)] bg-surface-2/45 p-1">
        {availableTabs.map((item) => (
          <Button
            key={item}
            id={`organization-activity-tab-${item}`}
            type="button"
            role="tab"
            size="sm"
            variant={activeTab === item ? 'secondary' : 'ghost'}
            aria-selected={activeTab === item}
            aria-controls={`organization-activity-panel-${item}`}
            onClick={() => setTab(item)}
          >
            {item === 'usage' ? <ScrollText aria-hidden="true" /> : <ShieldCheck aria-hidden="true" />}
            {t(`organizationActivity.tabs.${item}`)}
          </Button>
        ))}
      </div>

      {scopes.length > 0 ? (
        <section
          id="organization-activity-panel-usage"
          role="tabpanel"
          aria-labelledby="organization-activity-tab-usage"
          hidden={activeTab !== 'usage'}
        >
          <OrganizationUsagePanel
            organizationId={organizationId}
            initialScope={scopes[0]}
            scopes={scopes}
          />
        </section>
      ) : null}
      {canAudit ? (
        <section
          id="organization-activity-panel-audit"
          role="tabpanel"
          aria-labelledby="organization-activity-tab-audit"
          hidden={activeTab !== 'audit'}
        >
          <OrganizationAuditPanel organizationId={organizationId} />
        </section>
      ) : null}
    </div>
  )
}

function usageScopes(overview: OrganizationOverviewResponse): OrganizationUsageScope[] {
  const canReadAll = overview.permissions.includes(USAGE_READ_ALL)
  const canReadSelf = overview.permissions.includes(USAGE_READ_SELF)
  if (canReadAll) return canReadSelf ? ['organization', 'self'] : ['organization']
  if (overview.role === 'viewer') return canReadSelf ? ['summary', 'self'] : ['summary']
  return canReadSelf ? ['self'] : []
}

function OrganizationSummary({ overview }: { overview: OrganizationOverviewResponse }) {
  const { t } = useTranslation()
  return (
    <section className="grid gap-3 border-y border-[var(--hairline)] py-3 sm:grid-cols-[minmax(0,1.5fr)_repeat(3,minmax(0,1fr))] sm:items-center">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h3 className="truncate text-sm font-semibold">{overview.name}</h3>
          <Badge>{t(`organizationBudgets.roles.${overview.role}`, { defaultValue: overview.role })}</Badge>
          <Badge className="text-success">{t(`organizationBudgets.entitlement.${overview.entitlement.status}`, { defaultValue: overview.entitlement.status })}</Badge>
        </div>
        <p className="mt-1 truncate text-[0.6875rem] text-muted-foreground">{overview.slug}</p>
      </div>
      <SummaryMetric label={t('organizationActivity.summary.membership')} value={`#${overview.membership_id}`} />
      <SummaryMetric label={t('organizationActivity.summary.role')} value={t(`organizationBudgets.roles.${overview.role}`, { defaultValue: overview.role })} />
      <SummaryMetric label={t('organizationActivity.summary.retention')} value={t('organizationActivity.summary.days', { count: overview.entitlement.audit_retention_days })} />
    </section>
  )
}

function SummaryMetric({ label, value }: { label: string; value: string }) {
  return <div className="min-w-0 sm:text-right"><p className="text-[0.6875rem] text-muted-foreground">{label}</p><p className="mt-1 truncate text-xs font-semibold tabular-nums" title={value}>{value}</p></div>
}

function OrganizationDirectoryLoading() {
  return <div className="grid gap-3" role="status"><Skeleton className="h-10 w-full rounded-lg" /><OrganizationWorkspaceLoading /></div>
}

function OrganizationWorkspaceLoading() {
  return <div className="grid gap-4" role="status"><Skeleton className="h-20 w-full rounded-lg" /><Skeleton className="h-9 w-56 rounded-lg" /><Skeleton className="h-80 w-full rounded-lg" /></div>
}

function PageState({ icon: Icon = Activity, title, body, action }: {
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
