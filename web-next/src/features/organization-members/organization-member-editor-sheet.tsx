import { Button, Checkbox, Chip, Drawer, DrawerBody, DrawerContent, DrawerFooter, DrawerHeader, Select, SelectItem } from '@heroui/react'
import { Building2, LoaderCircle, Plus, Save, Star, Trash2 } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { OrganizationCustomRoleResponse, OrganizationDepartmentResponse, OrganizationDepartmentMembershipResponse, OrganizationMembershipResponse, OrganizationTeamResponse } from '@/lib/api/generated/types.gen'
import { organizationMemberErrorCode, useAddOrganizationMemberDepartmentMembership, useChangeOrganizationMember, useOrganizationMemberDepartmentMemberships, useRemoveOrganizationMemberDepartmentMembership, useSetOrganizationMemberCustomRole, useSetPrimaryOrganizationMemberDepartmentMembership } from './organization-member-api'

const editableRoles = ['admin', 'billing', 'member', 'viewer'] as const
const editableStatuses = ['active', 'suspended', 'removed'] as const

/** HeroUI Select 不接受空字符串 key，用哨兵 key 表达"未选择"。 */
const NONE_KEY = '__none__'

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
  const activeTeams = teams.filter((team) => team.status === 'active')
  const teamItems = [
    { key: NONE_KEY, label: t('organizationConsole.members.noTeam') },
    ...activeTeams.map((team) => ({ key: String(team.id), label: team.name })),
  ]
  const activeCustomRoles = customRoles.filter((item) => item.status === 'active')
  const customRoleItems = [
    { key: NONE_KEY, label: t('organizationConsole.members.customRoles.none') },
    // 已分配但已停用的角色仍要展示，否则下拉会丢当前选中项。
    ...(assignedRole && assignedRole.status !== 'active'
      ? [{ key: assignedRole.role_key, label: `${assignedRole.name} (${t('organizationConsole.members.customRoles.disabled')})` }]
      : []),
    ...activeCustomRoles.map((item) => ({ key: item.role_key, label: item.name })),
  ]

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
    <Drawer
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-lg' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t('organizationConsole.members.editor.title')}</h2>
              <p className="text-sm text-muted-foreground">{t('organizationConsole.members.editor.description', { organization: organizationName, userId: member?.user_id })}</p>
            </DrawerHeader>
            <DrawerBody className="gap-4 p-4">
              {member ? (
                <div className="grid gap-4">
                <ReadOnlyValue label={t('organizationConsole.members.fields.member')} value={t('organizationConsole.members.memberLabel', { id: member.user_id })} />
                <div>
                  <label htmlFor="organization-member-role" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.role')}</label>
                  <Select
                    aria-label={t('organizationConsole.members.fields.role')}
                    id="organization-member-role"
                    isDisabled={pending || member.role === 'owner'}
                    selectedKeys={[role]}
                    size="sm"
                    onSelectionChange={(keys) => { setRole(String(Array.from(keys)[0] ?? 'member')); memberMutation.reset(); customRoleMutation.reset() }}
                  >
                    {editableRoles.map((value) => <SelectItem key={value}>{t(`organizationConsole.roles.${value}`)}</SelectItem>)}
                  </Select>
                </div>
                <div>
                  <label htmlFor="organization-member-status" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.status')}</label>
                  <Select
                    aria-label={t('organizationConsole.members.fields.status')}
                    id="organization-member-status"
                    isDisabled={pending || member.role === 'owner'}
                    selectedKeys={[status]}
                    size="sm"
                    onSelectionChange={(keys) => { setStatus(String(Array.from(keys)[0] ?? 'active')); memberMutation.reset(); customRoleMutation.reset() }}
                  >
                    {editableStatuses.map((value) => <SelectItem key={value}>{t(`organizationConsole.memberStatus.${value}`)}</SelectItem>)}
                  </Select>
                </div>
                <div>
                  <label htmlFor="organization-member-team" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.team')}</label>
                  <Select
                    aria-label={t('organizationConsole.members.fields.team')}
                    id="organization-member-team"
                    isDisabled={pending || member.role === 'owner'}
                    items={teamItems}
                    selectedKeys={[teamId === undefined ? NONE_KEY : String(teamId)]}
                    size="sm"
                    onSelectionChange={(keys) => {
                      const next = String(Array.from(keys)[0] ?? NONE_KEY)
                      setTeamId(next === NONE_KEY ? undefined : Number(next))
                      memberMutation.reset()
                      customRoleMutation.reset()
                    }}
                  >
                    {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                  </Select>
                </div>
                <div>
                  <label htmlFor="organization-member-custom-role" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.customRole')}</label>
                  <Select
                    aria-label={t('organizationConsole.members.fields.customRole')}
                    id="organization-member-custom-role"
                    isDisabled={pending || !canAssignCurrentRole || !customRolesReady}
                    items={customRoleItems}
                    selectedKeys={[customRoleKey === '' ? NONE_KEY : customRoleKey]}
                    size="sm"
                    onSelectionChange={(keys) => {
                      const next = String(Array.from(keys)[0] ?? NONE_KEY)
                      setCustomRoleKey(next === NONE_KEY ? '' : next)
                      customRoleMutation.reset()
                    }}
                  >
                    {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
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
            </DrawerBody>
            <DrawerFooter className="mt-auto border-t border-[var(--hairline)]">
              <Button type="button" variant="bordered" isDisabled={pending} onClick={() => onOpenChange(false)}>{t('organizationConsole.actions.cancel')}</Button>
              <Button type="button" color="primary" isDisabled={!member || member.role === 'owner' || pending} onClick={() => void save()}>
                {pending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Save className="size-4" aria-hidden="true" />}
                {t('organizationConsole.actions.save')}
              </Button>
            </DrawerFooter>
          </>
        )}
      </DrawerContent>
    </Drawer>
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
                <div className="flex min-w-0 items-center gap-1.5"><span className="truncate text-xs font-medium">{departmentName(record.department_id)}</span>{record.is_primary ? <Chip className="text-brand" size="sm" variant="flat"><Star className="size-3" aria-hidden="true" />{t('organizationConsole.members.departments.primary')}</Chip> : null}<Chip className={record.status === 'active' ? 'text-success' : 'text-muted-foreground'} size="sm" variant="flat">{t(`organizationConsole.members.departments.status.${record.status}`)}</Chip></div>
                <p className="mt-0.5 text-[0.6875rem] text-muted-foreground">{t('organizationConsole.members.departments.version', { version: record.version })}</p>
              </div>
              {canManage && record.status === 'active' ? <div className="flex shrink-0 items-center gap-1"><Button type="button" isIconOnly size="sm" variant="light" aria-label={t('organizationConsole.members.departments.setPrimary')} title={t('organizationConsole.members.departments.setPrimary')} isDisabled={pending || record.is_primary || member.status !== 'active'} onClick={() => void setPrimary(record)}><Star className="size-3.5" aria-hidden="true" /></Button><Button type="button" isIconOnly size="sm" variant="light" className="text-muted-foreground hover:text-destructive" aria-label={t('organizationConsole.members.departments.remove')} title={record.is_primary ? t('organizationConsole.members.departments.primaryProtected') : t('organizationConsole.members.departments.remove')} isDisabled={pending || record.is_primary || member.status !== 'active'} onClick={() => void remove(record)}><Trash2 className="size-3.5" aria-hidden="true" /></Button></div> : null}
            </div>
          ))}
        </div>
      )}
      {canManage ? <div className="grid gap-2 border-t border-[var(--hairline)] pt-3"><div className="flex flex-col gap-2 sm:flex-row"><Select aria-label={t('organizationConsole.members.departments.select')} className="min-w-0" isDisabled={pending || availableDepartments.length === 0 || member.status !== 'active'} items={availableDepartments.map((department) => ({ key: String(department.id), label: department.name }))} placeholder={t('organizationConsole.members.departments.select')} selectedKeys={departmentId === '' ? [] : [departmentId]} size="sm" onSelectionChange={(keys) => setDepartmentId(String(Array.from(keys)[0] ?? ''))}>{(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}</Select><Button type="button" size="sm" color="primary" isDisabled={pending || !departmentId || member.status !== 'active'} onClick={() => void add()}><Plus className="size-3.5" aria-hidden="true" />{t('organizationConsole.members.departments.add')}</Button></div><Checkbox isDisabled={pending || !departmentId || member.status !== 'active'} isSelected={addAsPrimary} size="sm" onValueChange={setAddAsPrimary}>{t('organizationConsole.members.departments.addAsPrimary')}</Checkbox></div> : null}
      {error ? <p role="alert" className="rounded-md bg-destructive/8 px-2.5 py-2 text-xs text-destructive">{error}</p> : null}
    </section>
  )
}

function ReadOnlyValue({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-[var(--hairline)] bg-surface-2/45 px-3 py-2"><div className="text-[0.6875rem] text-muted-foreground">{label}</div><div className="mt-1 text-sm font-medium">{value}</div></div>
}
