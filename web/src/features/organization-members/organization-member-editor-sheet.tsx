import { LoaderCircle, Save } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { OrganizationMembershipResponse, OrganizationTeamResponse } from '@/lib/api/generated/types.gen'
import { organizationMemberErrorCode, useChangeOrganizationMember } from './organization-member-api'

const editableRoles = ['admin', 'billing', 'member', 'viewer'] as const
const editableStatuses = ['active', 'suspended', 'removed'] as const

export function OrganizationMemberEditorSheet({
  organizationId,
  organizationName,
  member,
  teams,
  open,
  onOpenChange,
}: {
  organizationId: number
  organizationName: string
  member?: OrganizationMembershipResponse
  teams: OrganizationTeamResponse[]
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const mutation = useChangeOrganizationMember(organizationId)
  const resetMutation = mutation.reset
  const [role, setRole] = useState('member')
  const [status, setStatus] = useState('active')
  const [teamId, setTeamId] = useState<number>()

  useEffect(() => {
    if (!open || !member) return
    setRole(member.role)
    setStatus(member.status)
    setTeamId(member.team_id ?? undefined)
    resetMutation()
  }, [member, open, resetMutation])

  const save = async () => {
    if (!member || member.role === 'owner') return
    try {
      await mutation.mutateAsync({
        membershipId: member.id,
        body: {
          expected_version: member.version,
          role,
          status,
          team_id: teamId ?? null,
        },
      })
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
              <Select id="organization-member-role" value={role} disabled={mutation.isPending || member.role === 'owner'} onChange={(event) => { setRole(event.target.value); mutation.reset() }}>
                {editableRoles.map((value) => <option key={value} value={value}>{t(`organizationConsole.roles.${value}`)}</option>)}
              </Select>
            </div>
            <div>
              <label htmlFor="organization-member-status" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.status')}</label>
              <Select id="organization-member-status" value={status} disabled={mutation.isPending || member.role === 'owner'} onChange={(event) => { setStatus(event.target.value); mutation.reset() }}>
                {editableStatuses.map((value) => <option key={value} value={value}>{t(`organizationConsole.memberStatus.${value}`)}</option>)}
              </Select>
            </div>
            <div>
              <label htmlFor="organization-member-team" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.team')}</label>
              <Select id="organization-member-team" value={teamId ?? ''} disabled={mutation.isPending || member.role === 'owner'} onChange={(event) => { setTeamId(Number(event.target.value) || undefined); mutation.reset() }}>
                <option value="">{t('organizationConsole.members.noTeam')}</option>
                {teams.filter((team) => team.status === 'active').map((team) => <option key={team.id} value={team.id}>{team.name}</option>)}
              </Select>
            </div>
            {status === 'removed' ? <p className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t('organizationConsole.members.editor.removedWarning', { organization: organizationName })}</p> : null}
            {member.role === 'owner' ? <p className="rounded-lg bg-warning/8 px-3 py-2 text-xs leading-5 text-warning">{t('organizationConsole.members.editor.ownerHint')}</p> : null}
            {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(`organizationConsole.errors.${organizationMemberErrorCode(mutation.error) === 'organization_conflict' ? 'conflict' : 'mutation'}`)}</p> : null}
          </div>
        ) : null}
        <SheetFooter className="mt-auto border-t border-[var(--hairline)]">
          <Button type="button" variant="outline" disabled={mutation.isPending} onClick={() => onOpenChange(false)}>{t('organizationConsole.actions.cancel')}</Button>
          <Button type="button" disabled={!member || member.role === 'owner' || mutation.isPending} onClick={() => void save()}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Save aria-hidden="true" />}
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
