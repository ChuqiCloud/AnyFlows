import { Building2, Crown, ShieldCheck, UsersRound } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Select } from '@/components/ui/select'
import { useOrganizationMemberCatalog } from '@/features/organization-budgets/organization-budget-api'
import { organizationMemberErrorCode, useTransferOrganizationOwnership } from '@/features/organization-members/organization-member-api'
import type { OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'
import {
  OrganizationPageBoundary,
  OrganizationPageHeader,
  useOrganizationPageState,
} from './organization-page-shell'

const OWNERSHIP_TRANSFER = 'organization.ownership.transfer'

const permissionCodes = [
  'organization.profile.write',
  'organization.ownership.transfer',
  'organization.members.manage',
  'organization.teams.manage',
  'organization.wallet.read',
  'organization.wallet.manage',
  'organization.budgets.manage',
  'organization.keys.read_all',
  'organization.keys.manage_self',
  'organization.usage_logs.read_all',
  'organization.usage_logs.read_self',
  'organization.audit.read',
] as const

/** 企业设置只呈现真实资料、授权与权限；当前唯一写接口为所有权转移。 */
export function OrganizationSettingsPage() {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <OrganizationPageHeader
        icon={ShieldCheck}
        title={t('organizationConsole.settings.title')}
        subtitle={t('organizationConsole.settings.subtitle')}
      />
      <OrganizationPageBoundary state={state}>
        {(organizationId, overview) => (
          <OrganizationSettingsWorkspace key={organizationId} organizationId={organizationId} overview={overview} />
        )}
      </OrganizationPageBoundary>
    </div>
  )
}

function OrganizationSettingsWorkspace({ organizationId, overview }: { organizationId: number; overview: OrganizationOverviewResponse }) {
  const { t } = useTranslation()
  const canTransfer = overview.permissions.includes(OWNERSHIP_TRANSFER)
  const membersQuery = useOrganizationMemberCatalog(organizationId, canTransfer)
  const mutation = useTransferOrganizationOwnership(organizationId)
  const [targetId, setTargetId] = useState<number>()
  const [confirmOpen, setConfirmOpen] = useState(false)
  const activeMembers = useMemo(
    () => (membersQuery.data ?? []).filter((member) => member.status === 'active' && member.id !== overview.membership_id),
    [membersQuery.data, overview.membership_id],
  )
  const currentOwner = membersQuery.data?.find((member) => member.id === overview.membership_id)
  const target = activeMembers.find((member) => member.id === targetId)
  const transfer = async () => {
    if (!currentOwner || !target) return
    try {
      await mutation.mutateAsync({
        current_owner_version: currentOwner.version,
        target_expected_version: target.version,
        target_membership_id: target.id,
      })
      setConfirmOpen(false)
      setTargetId(undefined)
    } catch {
      // CAS 冲突时保留目标选择，用户刷新成员事实后可重新确认。
    }
  }

  return (
    <>
      <Card className="overflow-hidden">
        <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
          <CardTitle className="text-base">{t('organizationConsole.settings.identity.title')}</CardTitle>
          <p className="text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.identity.subtitle')}</p>
        </CardHeader>
        <CardContent className="grid gap-0 p-0 sm:grid-cols-2">
          <ReadOnlyField label={t('organizationConsole.settings.identity.name')} value={overview.name} />
          <ReadOnlyField label={t('organizationConsole.settings.identity.slug')} value={overview.slug} mono />
          <ReadOnlyField label={t('organizationConsole.settings.identity.organizationId')} value={`#${overview.id}`} mono />
          <ReadOnlyField label={t('organizationConsole.settings.identity.role')} value={t(`organizationConsole.roles.${overview.role}`)} />
        </CardContent>
      </Card>

      <Card className="overflow-hidden">
        <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <CardTitle className="text-base">{t('organizationConsole.settings.entitlement.title')}</CardTitle>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.entitlement.subtitle')}</p>
            </div>
            <Badge>{t(`organizationConsole.entitlement.${overview.entitlement.status}`)}</Badge>
          </div>
        </CardHeader>
        <CardContent className="grid gap-0 p-0 sm:grid-cols-2 lg:grid-cols-4">
          <ReadOnlyField label={t('organizationConsole.settings.entitlement.seats')} value={`${overview.entitlement.active_seats} / ${overview.entitlement.seat_limit}`} />
          <ReadOnlyField label={t('organizationConsole.settings.entitlement.teams')} value={`${overview.entitlement.active_teams} / ${overview.entitlement.team_limit}`} />
          <ReadOnlyField label={t('organizationConsole.settings.entitlement.keys')} value={String(overview.entitlement.organization_key_limit)} />
          <ReadOnlyField label={t('organizationConsole.settings.entitlement.audit')} value={t('organizationConsole.settings.entitlement.days', { count: overview.entitlement.audit_retention_days })} />
        </CardContent>
      </Card>

      <Card className="overflow-hidden">
        <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
          <CardTitle className="text-base">{t('organizationConsole.settings.permissions.title')}</CardTitle>
          <p className="text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.permissions.subtitle')}</p>
        </CardHeader>
        <CardContent className="grid gap-2 p-4 sm:grid-cols-2 lg:grid-cols-3">
          {permissionCodes.map((permission) => {
            const granted = overview.permissions.includes(permission)
            return (
              <div key={permission} className="flex min-w-0 items-center justify-between gap-3 rounded-lg border border-[var(--hairline)] px-3 py-2">
                <span className="truncate text-xs" title={permission}>{t(`organizationConsole.permissions.${permission}`)}</span>
                <Badge className={granted ? 'text-success' : 'text-muted-foreground'}>{t(granted ? 'organizationConsole.common.allowed' : 'organizationConsole.common.denied')}</Badge>
              </div>
            )
          })}
        </CardContent>
      </Card>

      <Card className="overflow-hidden">
        <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
          <div className="flex items-center gap-3">
            <span className="grid size-9 place-items-center rounded-lg bg-warning/10 text-warning"><Crown className="size-4" aria-hidden="true" /></span>
            <div>
              <CardTitle className="text-base">{t('organizationConsole.settings.ownership.title')}</CardTitle>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.ownership.subtitle')}</p>
            </div>
          </div>
        </CardHeader>
        <CardContent className="p-4">
          {canTransfer ? (
            <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
              <div>
                <label htmlFor="organization-owner-target" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.settings.ownership.target')}</label>
                <Select id="organization-owner-target" value={targetId ?? ''} disabled={membersQuery.isPending || mutation.isPending} onChange={(event) => setTargetId(Number(event.target.value) || undefined)}>
                  <option value="">{t('organizationConsole.settings.ownership.select')}</option>
                  {activeMembers.map((member) => <option key={member.id} value={member.id}>{t('organizationConsole.members.memberLabel', { id: member.user_id })} · {t(`organizationConsole.roles.${member.role}`)}</option>)}
                </Select>
              </div>
              <Button type="button" variant="destructive" disabled={!target || !currentOwner || mutation.isPending} onClick={() => setConfirmOpen(true)}>
                <Crown aria-hidden="true" />{t('organizationConsole.settings.ownership.action')}
              </Button>
            </div>
          ) : (
            <div className="flex items-start gap-3 rounded-lg bg-surface-2/55 p-3">
              <UsersRound className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
              <p className="text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.ownership.ownerOnly')}</p>
            </div>
          )}
          {mutation.isError ? <p role="alert" className="mt-3 text-xs text-destructive">{t(`organizationConsole.errors.${organizationMemberErrorCode(mutation.error) === 'organization_conflict' ? 'conflict' : 'mutation'}`)}</p> : null}
        </CardContent>
      </Card>

      <div className="rounded-xl border border-dashed border-[var(--hairline)] p-4">
        <div className="flex items-start gap-3">
          <Building2 className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <div>
            <h3 className="text-sm font-semibold">{t('organizationConsole.settings.unavailable.title')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.unavailable.body')}</p>
          </div>
        </div>
      </div>

      <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('organizationConsole.settings.ownership.confirmTitle')}</AlertDialogTitle>
            <AlertDialogDescription>{t('organizationConsole.settings.ownership.confirmBody', { organization: overview.name, userId: target?.user_id })}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={mutation.isPending}>{t('organizationConsole.actions.cancel')}</AlertDialogCancel>
            <AlertDialogAction disabled={mutation.isPending} onClick={(event) => { event.preventDefault(); void transfer() }}>{t('organizationConsole.settings.ownership.confirm')}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function ReadOnlyField({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="min-w-0 border-b border-[var(--hairline)] px-4 py-3 last:border-b-0 sm:border-r sm:last:border-r-0">
      <div className="text-[0.6875rem] text-muted-foreground">{label}</div>
      <div className={`mt-1 truncate text-sm font-medium ${mono ? 'font-mono' : ''}`} title={value}>{value}</div>
    </div>
  )
}
