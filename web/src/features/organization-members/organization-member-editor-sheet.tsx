import { LoaderCircle, Save } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { OrganizationCustomRoleResponse, OrganizationMembershipResponse, OrganizationTeamResponse } from '@/lib/api/generated/types.gen'
import { organizationMemberErrorCode, useChangeOrganizationMember, useSetOrganizationMemberCustomRole } from './organization-member-api'

const editableRoles = ['admin', 'billing', 'member', 'viewer'] as const
const editableStatuses = ['active', 'suspended', 'removed'] as const

export function OrganizationMemberEditorSheet({
  organizationId,
  organizationName,
  member,
  teams,
  customRoles,
  customRolesReady,
  customRolesEnabled,
  canAssignCustomRoles,
  open,
  onOpenChange,
}: {
  organizationId: number
  organizationName: string
  member?: OrganizationMembershipResponse
  teams: OrganizationTeamResponse[]
  customRoles: OrganizationCustomRoleResponse[]
  customRolesReady: boolean
  customRolesEnabled: boolean
  canAssignCustomRoles: boolean
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const memberMutation = useChangeOrganizationMember(organizationId)
  const customRoleMutation = useSetOrganizationMemberCustomRole(organizationId)
  const resetMemberMutation = memberMutation.reset
  const resetCustomRoleMutation = customRoleMutation.reset
  const [role, setRole] = useState('member')
  const [status, setStatus] = useState('active')
  const [teamId, setTeamId] = useState<number>()
  const [customRoleKey, setCustomRoleKey] = useState('')
  const pending = memberMutation.isPending || customRoleMutation.isPending
  const canAssignCurrentRole = customRolesEnabled && canAssignCustomRoles && role === 'member' && status === 'active'
  const assignedRole = customRoles.find((item) => item.role_key === customRoleKey)

  useEffect(() => {
    if (!open || !member) return
    setRole(member.role)
    setStatus(member.status)
    setTeamId(member.team_id ?? undefined)
    setCustomRoleKey(member.custom_role_key ?? '')
    resetMemberMutation()
    resetCustomRoleMutation()
  }, [member, open, resetCustomRoleMutation, resetMemberMutation])

  const save = async () => {
    if (!member || member.role === 'owner') return
    try {
      const updated = await memberMutation.mutateAsync({
        membershipId: member.id,
        body: {
          expected_version: member.version,
          role,
          status,
          team_id: teamId ?? null,
        },
      })
      if (canAssignCurrentRole && customRolesReady && customRoleKey !== (member.custom_role_key ?? '')) {
        await customRoleMutation.mutateAsync({
          membershipId: updated.id,
          body: {
            expected_version: updated.version,
            role_key: customRoleKey || null,
          },
        })
      }
      onOpenChange(false)
    } catch {
      // CAS 或结果未知时保留当前选择，刷新成员事实后再处理。
    }
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-lg">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('organizationConsole.members.editor.title')}</SheetTitle>
          <SheetDescription>{t('organizationConsole.members.editor.description', { organization: organizationName, userId: member?.user_id })}</SheetDescription>
        </SheetHeader>
        {member ? (
          <div className="grid gap-4 overflow-y-auto p-4">
            <ReadOnlyValue label={t('organizationConsole.members.fields.member')} value={t('organizationConsole.members.memberLabel', { id: member.user_id })} />
            <div>
              <label htmlFor="organization-member-role" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.role')}</label>
              <Select id="organization-member-role" value={role} disabled={pending || member.role === 'owner'} onChange={(event) => { setRole(event.target.value); memberMutation.reset(); customRoleMutation.reset() }}>
                {editableRoles.map((value) => <option key={value} value={value}>{t(`organizationConsole.roles.${value}`)}</option>)}
              </Select>
            </div>
            <div>
              <label htmlFor="organization-member-status" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.status')}</label>
              <Select id="organization-member-status" value={status} disabled={pending || member.role === 'owner'} onChange={(event) => { setStatus(event.target.value); memberMutation.reset(); customRoleMutation.reset() }}>
                {editableStatuses.map((value) => <option key={value} value={value}>{t(`organizationConsole.memberStatus.${value}`)}</option>)}
              </Select>
            </div>
            <div>
              <label htmlFor="organization-member-team" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.team')}</label>
              <Select id="organization-member-team" value={teamId ?? ''} disabled={pending || member.role === 'owner'} onChange={(event) => { setTeamId(Number(event.target.value) || undefined); memberMutation.reset(); customRoleMutation.reset() }}>
                <option value="">{t('organizationConsole.members.noTeam')}</option>
                {teams.filter((team) => team.status === 'active').map((team) => <option key={team.id} value={team.id}>{team.name}</option>)}
              </Select>
            </div>
            <div>
              <label htmlFor="organization-member-custom-role" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.customRole')}</label>
              <Select id="organization-member-custom-role" value={customRoleKey} disabled={pending || !canAssignCurrentRole || !customRolesReady} onChange={(event) => { setCustomRoleKey(event.target.value); customRoleMutation.reset() }}>
                <option value="">{t('organizationConsole.members.customRoles.none')}</option>
                {assignedRole && assignedRole.status !== 'active' ? <option value={assignedRole.role_key}>{assignedRole.name} ({t('organizationConsole.members.customRoles.disabled')})</option> : null}
                {customRoles.filter((item) => item.status === 'active').map((item) => <option key={item.role_key} value={item.role_key}>{item.name}</option>)}
              </Select>
              {!customRolesEnabled ? <p className="mt-1.5 text-xs leading-5 text-muted-foreground">{t('organizationConsole.members.customRoles.disabledByEntitlement')}</p> : null}
              {customRolesEnabled && !canAssignCustomRoles ? <p className="mt-1.5 text-xs leading-5 text-muted-foreground">{t('organizationConsole.members.customRoles.assignmentRestricted')}</p> : null}
              {canAssignCurrentRole && !customRolesReady ? <p className="mt-1.5 text-xs leading-5 text-muted-foreground">{t('organizationConsole.members.customRoles.loading')}</p> : null}
            </div>
            {status === 'removed' ? <p className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t('organizationConsole.members.editor.removedWarning', { organization: organizationName })}</p> : null}
            {member.role === 'owner' ? <p className="rounded-lg bg-warning/8 px-3 py-2 text-xs leading-5 text-warning">{t('organizationConsole.members.editor.ownerHint')}</p> : null}
            {(memberMutation.isError || customRoleMutation.isError) ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(`organizationConsole.errors.${organizationMemberErrorCode(memberMutation.error ?? customRoleMutation.error) === 'organization_conflict' ? 'conflict' : 'mutation'}`)}</p> : null}
          </div>
        ) : null}
        <SheetFooter className="mt-auto border-t border-[var(--hairline)]">
          <Button type="button" variant="outline" disabled={pending} onClick={() => onOpenChange(false)}>{t('organizationConsole.actions.cancel')}</Button>
          <Button type="button" disabled={!member || member.role === 'owner' || pending} onClick={() => void save()}>
            {pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Save aria-hidden="true" />}
            {t('organizationConsole.actions.save')}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  )
}

function ReadOnlyValue({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-[var(--hairline)] bg-surface-2/45 px-3 py-2"><div className="text-[0.6875rem] text-muted-foreground">{label}</div><div className="mt-1 text-sm font-medium">{value}</div></div>
}
