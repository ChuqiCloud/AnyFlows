import { PlatformEntitlementEditor } from './platform-entitlement-editor'
import { Building2, LoaderCircle, RefreshCw, Settings2, UsersRound } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useOrganizationWorkspace } from '@/features/organization-console/organization-workspace-context'
import type { PlatformOrganizationSummaryResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

import { usePlatformOrganizations } from './platform-organization-api'
import { PlatformOrganizationMembersSheet } from './platform-organization-members-sheet'

const statusStyles: Readonly<Record<string, string>> = {
  active: 'border-transparent bg-success/10 text-success',
  grace: 'border-transparent bg-warning/10 text-warning',
  read_only: 'border-transparent bg-warning/10 text-warning',
  suspended: 'border-transparent bg-destructive/10 text-destructive',
  closed: 'bg-surface-2 text-muted-foreground',
}

/** 平台管理员进入页面后直接读取全站企业安全摘要。 */
export function PlatformOrganizationPage() {
  const { i18n, t } = useTranslation()
  const [entitlementOrganization, setEntitlementOrganization] = useState<PlatformOrganizationSummaryResponse>()
  const [selectedOrganization, setSelectedOrganization] = useState<PlatformOrganizationSummaryResponse>()
  const query = usePlatformOrganizations()
  const workspace = useOrganizationWorkspace()
  const organizations = query.data?.pages.flatMap((page) => page.organizations) ?? []
  const timeFormat = useMemo(
    () => new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', {
      dateStyle: 'medium',
    }),
    [i18n.resolvedLanguage],
  )
  const canEnterOrganization = (organizationId: number) => workspace.organizations.some((organization) => organization.id === organizationId)
  const enterOrganizationSettings = async (organizationId: number) => {
    if (!canEnterOrganization(organizationId)) return
    await workspace.selectOrganization(organizationId)
    window.location.hash = '#/console/organization-settings'
  }

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <Building2 className="size-3.5" aria-hidden="true" />
            {t('platformOrganizations.eyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('platformOrganizations.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('platformOrganizations.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={query.isFetching}
          onClick={() => void query.refetch()}
        >
          <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('platformOrganizations.actions.refresh')}
        </Button>
      </header>

      {query.isPending ? (
        <div className="grid gap-2" aria-label={t('platformOrganizations.loading')}>
          {[0, 1, 2].map((item) => <Skeleton key={item} className="h-20 rounded-lg" />)}
        </div>
      ) : query.isError && query.data === undefined ? (
        <DirectoryState
          error
          title={t('platformOrganizations.error.title')}
          body={t('platformOrganizations.error.body')}
          action={(
            <Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}>
              {t('platformOrganizations.actions.retry')}
            </Button>
          )}
        />
      ) : (
        <>
          <div className="flex flex-wrap gap-x-4 gap-y-1 border-b border-[var(--hairline)] pb-2 text-xs text-muted-foreground">
            <span>{t('platformOrganizations.summary.loaded', { count: organizations.length })}</span>
            <span>{t('platformOrganizations.summary.pages', { count: query.data?.pages.length ?? 0 })}</span>
          </div>
          {organizations.length === 0 ? (
            <DirectoryState
              title={t('platformOrganizations.empty.title')}
              body={t('platformOrganizations.empty.body')}
            />
          ) : <OrganizationDirectory organizations={organizations} timeFormat={timeFormat} canEnterOrganization={canEnterOrganization} onManage={setSelectedOrganization} onEntitlement={setEntitlementOrganization} onEnterSettings={(organization) => void enterOrganizationSettings(organization.id)} />}
          {query.isFetchNextPageError ? (
            <div role="alert" className="text-xs text-destructive">{t('platformOrganizations.error.more')}</div>
          ) : null}
          {query.hasNextPage ? (
            <Button
              type="button"
              size="sm"
              variant="secondary"
              className="self-center"
              disabled={query.isFetchingNextPage}
              onClick={() => void query.fetchNextPage()}
            >
              {query.isFetchingNextPage ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
              {t('platformOrganizations.actions.loadMore')}
            </Button>
          ) : null}
        </>
      )}
      {entitlementOrganization ? <PlatformEntitlementEditor key={entitlementOrganization.id} organizationId={entitlementOrganization.id} name={entitlementOrganization.name} onClose={() => setEntitlementOrganization(undefined)} /> : null}
      <PlatformOrganizationMembersSheet
        open={selectedOrganization !== undefined}
        organization={selectedOrganization}
        onOpenChange={(open) => { if (!open) setSelectedOrganization(undefined) }}
      />
    </div>
  )
}

function OrganizationDirectory({
  organizations,
  timeFormat,
  canEnterOrganization,
  onManage,
  onEntitlement,
  onEnterSettings,
}: {
  organizations: PlatformOrganizationSummaryResponse[]
  timeFormat: Intl.DateTimeFormat
  canEnterOrganization: (organizationId: number) => boolean
  onEntitlement: (organization: PlatformOrganizationSummaryResponse) => void
  onManage: (organization: PlatformOrganizationSummaryResponse) => void
  onEnterSettings: (organization: PlatformOrganizationSummaryResponse) => void
}) {
  const { t } = useTranslation()
  return (
    <>
      <div className="hidden overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr>
              <th className="w-[23%] px-3 py-2 font-medium">{t('platformOrganizations.columns.organization')}</th>
              <th className="w-[19%] px-3 py-2 font-medium">{t('platformOrganizations.columns.owner')}</th>
              <th className="w-[14%] px-3 py-2 font-medium">{t('platformOrganizations.columns.members')}</th>
              <th className="w-[17%] px-3 py-2 font-medium">{t('platformOrganizations.columns.status')}</th>
              <th className="w-[16%] px-3 py-2 font-medium">{t('platformOrganizations.columns.validUntil')}</th>
              <th className="w-[11%] px-3 py-2 text-right font-medium">{t('platformOrganizations.columns.actions')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {organizations.map((organization) => (
              <tr key={organization.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-3"><OrganizationIdentity organization={organization} /></td>
                <td className="px-3 py-3"><Owner organization={organization} /></td>
                <td className="px-3 py-3 tabular-nums">{t('platformOrganizations.values.seats', { active: organization.active_member_count, limit: organization.seat_limit })}</td>
                <td className="px-3 py-3"><StatusPair organization={organization} /></td>
                <td className="px-3 py-3">{timeFormat.format(organization.valid_until * 1_000)}</td>
                <td className="px-3 py-3 text-right"><div className="flex justify-end gap-1"><Button type="button" size="sm" variant="ghost" onClick={() => onManage(organization)}><UsersRound aria-hidden="true" />{t('platformOrganizations.actions.manageMembers')}</Button><Button type="button" size="sm" variant="ghost" onClick={() => onEntitlement(organization)}><Settings2 className="size-4" />{t('enterpriseEntitlement.action')}</Button>{canEnterOrganization(organization.id) ? <Button type="button" size="icon-sm" variant="ghost" title={t('platformOrganizations.actions.openSettings')} onClick={() => onEnterSettings(organization)}><Settings2 aria-hidden="true" /><span className="sr-only">{t('platformOrganizations.actions.openSettings')}</span></Button> : null}</div></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="grid gap-2 md:hidden">
        {organizations.map((organization) => (
          <article key={organization.id} className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex min-w-0 items-start justify-between gap-3">
              <OrganizationIdentity organization={organization} />
              <StatusPair organization={organization} />
            </div>
            <dl className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3 text-xs">
              <div><dt className="text-muted-foreground">{t('platformOrganizations.columns.owner')}</dt><dd className="mt-1"><Owner organization={organization} /></dd></div>
              <div><dt className="text-muted-foreground">{t('platformOrganizations.columns.members')}</dt><dd className="mt-1 tabular-nums">{t('platformOrganizations.values.seats', { active: organization.active_member_count, limit: organization.seat_limit })}</dd></div>
              <div className="col-span-2"><dt className="text-muted-foreground">{t('platformOrganizations.columns.validUntil')}</dt><dd className="mt-1">{timeFormat.format(organization.valid_until * 1_000)}</dd></div>
            </dl>
            <div className="mt-3 flex gap-2"><Button type="button" size="sm" variant="secondary" className="min-w-0 flex-1" onClick={() => onManage(organization)}><UsersRound aria-hidden="true" />{t('platformOrganizations.actions.manageMembers')}</Button><Button type="button" size="sm" variant="ghost" onClick={() => onEntitlement(organization)}><Settings2 className="size-4" />{t('enterpriseEntitlement.action')}</Button>{canEnterOrganization(organization.id) ? <Button type="button" size="icon-sm" variant="secondary" title={t('platformOrganizations.actions.openSettings')} onClick={() => onEnterSettings(organization)}><Settings2 aria-hidden="true" /><span className="sr-only">{t('platformOrganizations.actions.openSettings')}</span></Button> : null}</div>
          </article>
        ))}
      </div>
    </>
  )
}

function OrganizationIdentity({ organization }: { organization: PlatformOrganizationSummaryResponse }) {
  return (
    <div className="min-w-0">
      <div className="truncate font-medium">{organization.name}</div>
      <div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">
        {organization.slug} · #{organization.id}
      </div>
    </div>
  )
}

function Owner({ organization }: { organization: PlatformOrganizationSummaryResponse }) {
  return (
    <div className="min-w-0">
      <div className="truncate font-medium">{organization.owner_username}</div>
      <div className="mt-1 truncate text-[0.6875rem] text-muted-foreground">
        #{organization.owner_user_id} · M#{organization.owner_membership_id}
      </div>
    </div>
  )
}

function StatusPair({ organization }: { organization: PlatformOrganizationSummaryResponse }) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-wrap gap-1">
      <Badge className={cn('shrink-0', statusStyles[organization.organization_status])}>
        {t(`platformOrganizations.organizationStatus.${organization.organization_status}`, { defaultValue: organization.organization_status })}
      </Badge>
      <Badge className={cn('shrink-0', statusStyles[organization.entitlement_status])}>
        {t(`platformOrganizations.entitlementStatus.${organization.entitlement_status}`, { defaultValue: organization.entitlement_status })}
      </Badge>
    </div>
  )
}

function DirectoryState({
  title,
  body,
  action,
  error = false,
}: {
  title: string
  body: string
  action?: React.ReactNode
  error?: boolean
}) {
  return (
    <div className={cn('grid min-h-52 place-items-center border-y border-[var(--hairline)] py-8 text-center', error && 'border-destructive/25')}>
      <div className="max-w-sm">
        <Building2 className={cn('mx-auto size-5 text-muted-foreground', error && 'text-destructive')} aria-hidden="true" />
        <h3 className={cn('mt-3 text-sm font-semibold', error && 'text-destructive')}>{title}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>
        {action ? <div className="mt-4">{action}</div> : null}
      </div>
    </div>
  )
}
