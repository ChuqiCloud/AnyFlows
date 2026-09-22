import { Button, Skeleton } from '@heroui/react'
import { Building2, ClipboardCheck, RefreshCw } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { useOrganizationOverview } from '@/features/organization-budgets/organization-budget-api'
import type { OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'
import { OrganizationInvitationAcceptCard } from './organization-invitation-accept'
import { useOrganizationWorkspace } from './organization-workspace-context'
import { OrganizationMobileWorkspaceSelect } from './organization-workspace-switcher'

/** 统一企业页面的目录、当前企业和权限投影加载状态。 */
export function useOrganizationPageState() {
  const workspace = useOrganizationWorkspace()
  const organizationId = workspace.selectedId ?? 0
  const overviewQuery = useOrganizationOverview(organizationId, workspace.selectedId !== undefined)
  return { organizationId, overviewQuery, workspace }
}

export function OrganizationPageHeader({
  icon: Icon,
  title,
  subtitle,
  actions,
}: {
  icon: LucideIcon
  title: string
  subtitle: string
  actions?: ReactNode
}) {
  const { t } = useTranslation()
  return (
    <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
      <div>
        <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
          <Icon className="size-3.5" aria-hidden="true" />
          {t('organizationConsole.eyebrow')}
        </div>
        <h2 className="text-lg font-semibold">{title}</h2>
        <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{subtitle}</p>
      </div>
      <div className="flex w-full flex-col gap-2 sm:w-auto sm:items-end">
        <OrganizationMobileWorkspaceSelect />
        {actions}
      </div>
    </header>
  )
}

export function OrganizationPageBoundary({
  state,
  children,
}: {
  state: ReturnType<typeof useOrganizationPageState>
  children: (organizationId: number, overview: OrganizationOverviewResponse) => ReactNode
}) {
  const { t } = useTranslation()
  const { overviewQuery, workspace } = state
  if (workspace.organizationsQuery.isPending) return <OrganizationPageLoading />
  if (workspace.organizationsQuery.isError && workspace.organizationsQuery.data === undefined) {
    return (
      <OrganizationPageState
        title={t('organizationConsole.errors.directoryTitle')}
        body={t('organizationConsole.errors.directoryBody')}
        action={<Button type="button" size="sm" variant="bordered" onClick={() => void workspace.organizationsQuery.refetch()}><RefreshCw className="size-3.5" aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>}
      />
    )
  }
  if (workspace.organizations.length === 0) {
    return (
      <div className="grid gap-4">
        <OrganizationInvitationAcceptCard />
        <OrganizationPageState
          icon={Building2}
          title={t('organizationConsole.empty.title')}
          body={t('organizationConsole.empty.body')}
          action={
            <div className="flex flex-col items-center gap-2">
              <Button as="a" color="primary" href="/console/organization-provisioning" size="sm">
                <ClipboardCheck className="size-3.5" aria-hidden="true" />
                {t('organizationConsole.empty.applyProvisioning')}
              </Button>
              <p className="max-w-xs text-[0.6875rem] leading-4 text-muted-foreground">
                {t('organizationConsole.empty.provisioningHint')}
              </p>
            </div>
          }
        />
      </div>
    )
  }
  if (workspace.selectedId === undefined || workspace.switching) return <OrganizationPageLoading />
  if (overviewQuery.isPending) return <OrganizationPageLoading />
  if (overviewQuery.isError || !overviewQuery.data) {
    return (
      <OrganizationPageState
        title={t('organizationConsole.errors.overviewTitle')}
        body={t('organizationConsole.errors.overviewBody')}
        action={<Button type="button" size="sm" variant="bordered" onClick={() => void overviewQuery.refetch()}><RefreshCw className="size-3.5" aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>}
      />
    )
  }
  return children(workspace.selectedId, overviewQuery.data)
}

export function OrganizationPageLoading() {
  return (
    <div className="grid gap-3" role="status">
      <div className="grid gap-3 sm:grid-cols-3">
        {[0, 1, 2].map((item) => <Skeleton key={item} className="h-28 rounded-xl" />)}
      </div>
      <Skeleton className="h-72 rounded-xl" />
    </div>
  )
}

export function OrganizationPageState({
  icon: Icon,
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
    <div className="grid min-h-64 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1/35 p-6 text-center">
      <div className="max-w-sm">
        {Icon ? <Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /> : null}
        <h3 className="mt-3 text-sm font-semibold">{title}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>
        {action ? <div className="mt-4">{action}</div> : null}
      </div>
    </div>
  )
}
