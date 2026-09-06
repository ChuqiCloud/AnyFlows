import { LoaderCircle, Save } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Select } from '@/components/ui/select'
import type { PlatformOrganizationMemberResponse } from '@/lib/api/generated/types.gen'

import { useChangePlatformOrganizationMember } from './platform-organization-api'

const editableRoles = ['admin', 'billing', 'member', 'viewer'] as const
const editableStatuses = ['active', 'suspended', 'removed'] as const

/** 只调整固定角色和生命周期，Owner 转移与团队归属不在平台代操作中混改。 */
export function PlatformOrganizationMemberDialog({
  organizationName,
  member,
  reason,
  onOpenChange,
}: {
  organizationName: string
  member?: PlatformOrganizationMemberResponse
  reason: string
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const [role, setRole] = useState('member')
  const [status, setStatus] = useState('active')
  const mutation = useChangePlatformOrganizationMember({ organizationId: member?.organization_id ?? 0, reason })
  const resetMutation = mutation.reset

  useEffect(() => {
    if (!member) return
    setRole(member.role)
    setStatus(member.status)
    resetMutation()
  }, [member, resetMutation])

  const unchanged = role === member?.role && status === member?.status
  const submit = async () => {
    if (!member || member.role === 'owner' || member.status === 'removed' || unchanged) return
    try {
      await mutation.mutateAsync({
        membershipId: member.membership_id,
        body: { role, status, expected_version: member.version },
      })
      onOpenChange(false)
    } catch {
      // CAS 或业务不变量失败时保留目标快照，供管理员核对并刷新。
    }
  }

  return (
    <AlertDialog open={member !== undefined} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('platformOrganizations.members.editor.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('platformOrganizations.members.editor.description', { organization: organizationName, username: member?.username })}</AlertDialogDescription>
        </AlertDialogHeader>
        <div className="grid gap-4">
          <div>
            <label htmlFor="platform-member-role" className="mb-1.5 block text-xs font-medium">{t('platformOrganizations.members.editor.role')}</label>
            <Select id="platform-member-role" value={role} disabled={mutation.isPending} onChange={(event) => { setRole(event.target.value); mutation.reset() }}>
              {editableRoles.map((value) => <option key={value} value={value}>{t(`platformOrganizations.roles.${value}`)}</option>)}
            </Select>
          </div>
          <div>
            <label htmlFor="platform-member-status" className="mb-1.5 block text-xs font-medium">{t('platformOrganizations.members.editor.status')}</label>
            <Select id="platform-member-status" value={status} disabled={mutation.isPending} onChange={(event) => { setStatus(event.target.value); mutation.reset() }}>
              {editableStatuses.map((value) => <option key={value} value={value}>{t(`platformOrganizations.members.status.${value}`)}</option>)}
            </Select>
          </div>
          {status === 'removed' ? <p className="rounded-md bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t('platformOrganizations.members.editor.removedWarning')}</p> : null}
          <p className="text-xs leading-5 text-muted-foreground">{t('platformOrganizations.members.editor.boundary')}</p>
          {mutation.isError ? <p role="alert" className="rounded-md bg-destructive/8 px-3 py-2 text-xs text-destructive">{t('platformOrganizations.members.editor.error')}</p> : null}
        </div>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('platformOrganizations.members.editor.cancel')}</AlertDialogCancel>
          <AlertDialogAction disabled={unchanged || mutation.isPending} onClick={(event) => { event.preventDefault(); void submit() }}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Save aria-hidden="true" />}
            {t('platformOrganizations.members.editor.save')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
