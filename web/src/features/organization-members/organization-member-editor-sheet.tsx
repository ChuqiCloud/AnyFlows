import { Building2, LoaderCircle, Plus, Save, Star, Trash2 } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { OrganizationCustomRoleResponse, OrganizationDepartmentResponse, OrganizationDepartmentMembershipResponse, OrganizationMembershipResponse, OrganizationTeamResponse } from '@/lib/api/generated/types.gen'
import { organizationMemberErrorCode, useAddOrganizationMemberDepartmentMembership, useChangeOrganizationMember, useOrganizationMemberDepartmentMemberships, useRemoveOrganizationMemberDepartmentMembership, useSetOrganizationMemberCustomRole, useSetPrimaryOrganizationMemberDepartmentMembership } from './organization-member-api'

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
  departments,
  departmentsReady,
  canReadDepartments,
  canManageDepartments,
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
  departments: OrganizationDepartmentResponse[]
  departmentsReady: boolean
  canReadDepartments: boolean
  canManageDepartments: boolean
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
            <DepartmentMembershipEditor member={member} organizationId={organizationId} departments={departments} departmentsReady={departmentsReady} canRead={canReadDepartments} canManage={canManageDepartments} />
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

function DepartmentMembershipEditor({ member, organizationId, departments, departmentsReady, canRead, canManage }: { member: OrganizationMembershipResponse; organizationId: number; departments: OrganizationDepartmentResponse[]; departmentsReady: boolean; canRead: boolean; canManage: boolean }) {
  const { t } = useTranslation()
  const query = useOrganizationMemberDepartmentMemberships(organizationId, member.id, canRead)
  const addMutation = useAddOrganizationMemberDepartmentMembership(organizationId)
  const primaryMutation = useSetPrimaryOrganizationMemberDepartmentMembership(organizationId)
  const removeMutation = useRemoveOrganizationMemberDepartmentMembership(organizationId)
  const [departmentId, setDepartmentId] = useState('')
  const [addAsPrimary, setAddAsPrimary] = useState(false)
  const records = query.data?.department_memberships ?? []
  const activeRecords = records.filter((record) => record.status === 'active')
  const assignedIds = useMemo(() => new Set(activeRecords.map((record) => record.department_id)), [activeRecords])
  const availableDepartments = useMemo(() => departments.filter((department) => department.status === 'active' && !assignedIds.has(department.id)), [assignedIds, departments])
  const pending = addMutation.isPending || primaryMutation.isPending || removeMutation.isPending
  const error = addMutation.isError || primaryMutation.isError || removeMutation.isError
    ? t('organizationConsole.members.departments.errors.mutation')
    : undefined

  useEffect(() => {
    if (!availableDepartments.some((department) => department.id.toString() === departmentId)) {
      setDepartmentId(availableDepartments[0]?.id.toString() ?? '')
    }
  }, [availableDepartments, departmentId])

  if (!canRead) return null
  const departmentName = (id: number) => departments.find((department) => department.id === id)?.name ?? t('organizationConsole.members.departments.unknown', { id })
  const add = async () => {
    if (!departmentId || pending) return
    try {
      await addMutation.mutateAsync({
        membershipId: member.id,
        body: { department_id: Number(departmentId), is_primary: addAsPrimary, expected_membership_version: addAsPrimary ? member.version : null },
      })
      setAddAsPrimary(false)
      addMutation.reset()
    } catch {
      // 保留当前选择，等待管理员刷新成员版本或处理服务端冲突。
    }
  }
  const setPrimary = async (record: OrganizationDepartmentMembershipResponse) => {
    if (pending || record.is_primary) return
    try {
      await primaryMutation.mutateAsync({ membershipId: member.id, departmentId: record.department_id, body: { expected_membership_version: member.version } })
      primaryMutation.reset()
    } catch {
      // 成员版本冲突时保留服务端事实，不做乐观覆盖。
    }
  }
  const remove = async (record: OrganizationDepartmentMembershipResponse) => {
    if (pending || record.is_primary) return
    try {
      await removeMutation.mutateAsync({ membershipId: member.id, departmentId: record.department_id, body: { expected_version: record.version } })
      removeMutation.reset()
    } catch {
      // 解除冲突或主部门保护由服务端返回，列表保持可重试。
    }
  }

  return (
    <section className="grid gap-3 rounded-lg border border-[var(--hairline)] bg-surface-2/35 p-3" aria-labelledby="organization-member-departments-title">
      <div className="flex items-start gap-2">
        <Building2 className="mt-0.5 size-4 shrink-0 text-brand" aria-hidden="true" />
        <div className="min-w-0">
          <h3 id="organization-member-departments-title" className="text-sm font-medium">{t('organizationConsole.members.departments.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.members.departments.description')}</p>
        </div>
      </div>
      {query.isPending || !departmentsReady ? <p className="text-xs text-muted-foreground">{t('organizationConsole.members.departments.loading')}</p> : query.isError ? <p role="alert" className="text-xs text-destructive">{t('organizationConsole.members.departments.errors.load')}</p> : records.length === 0 ? <p className="text-xs text-muted-foreground">{t('organizationConsole.members.departments.empty')}</p> : (
        <div className="grid gap-2">
          {records.map((record) => (
            <div key={record.id} className="flex items-center justify-between gap-2 rounded-md border border-[var(--hairline)] bg-background px-2.5 py-2">
              <div className="min-w-0">
                <div className="flex min-w-0 items-center gap-1.5"><span className="truncate text-xs font-medium">{departmentName(record.department_id)}</span>{record.is_primary ? <Badge className="text-brand"><Star aria-hidden="true" />{t('organizationConsole.members.departments.primary')}</Badge> : null}<Badge className={record.status === 'active' ? 'text-success' : 'text-muted-foreground'}>{t(`organizationConsole.members.departments.status.${record.status}`)}</Badge></div>
                <p className="mt-0.5 text-[0.6875rem] text-muted-foreground">{t('organizationConsole.members.departments.version', { version: record.version })}</p>
              </div>
              {canManage && record.status === 'active' ? <div className="flex shrink-0 items-center gap-1"><Button type="button" size="icon-sm" variant="ghost" aria-label={t('organizationConsole.members.departments.setPrimary')} title={t('organizationConsole.members.departments.setPrimary')} disabled={pending || record.is_primary || member.status !== 'active'} onClick={() => void setPrimary(record)}><Star aria-hidden="true" /></Button><Button type="button" size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-destructive" aria-label={t('organizationConsole.members.departments.remove')} title={record.is_primary ? t('organizationConsole.members.departments.primaryProtected') : t('organizationConsole.members.departments.remove')} disabled={pending || record.is_primary || member.status !== 'active'} onClick={() => void remove(record)}><Trash2 aria-hidden="true" /></Button></div> : null}
            </div>
          ))}
        </div>
      )}
      {canManage ? <div className="grid gap-2 border-t border-[var(--hairline)] pt-3"><div className="flex flex-col gap-2 sm:flex-row"><Select value={departmentId} disabled={pending || availableDepartments.length === 0 || member.status !== 'active'} onChange={(event) => setDepartmentId(event.target.value)}><option value="">{t('organizationConsole.members.departments.select')}</option>{availableDepartments.map((department) => <option key={department.id} value={department.id}>{department.name}</option>)}</Select><Button type="button" size="sm" disabled={pending || !departmentId || member.status !== 'active'} onClick={() => void add()}><Plus aria-hidden="true" />{t('organizationConsole.members.departments.add')}</Button></div><label className="flex items-center gap-2 text-xs text-muted-foreground"><input type="checkbox" checked={addAsPrimary} disabled={pending || !departmentId || member.status !== 'active'} onChange={(event) => setAddAsPrimary(event.target.checked)} />{t('organizationConsole.members.departments.addAsPrimary')}</label></div> : null}
      {error ? <p role="alert" className="rounded-md bg-destructive/8 px-2.5 py-2 text-xs text-destructive">{error}</p> : null}
    </section>
  )
}

function ReadOnlyValue({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-[var(--hairline)] bg-surface-2/45 px-3 py-2"><div className="text-[0.6875rem] text-muted-foreground">{label}</div><div className="mt-1 text-sm font-medium">{value}</div></div>
}
