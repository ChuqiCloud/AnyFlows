import { LoaderCircle, Send } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { OrganizationInvitationCreateRequest, OrganizationTeamResponse } from '@/lib/api/generated/types.gen'
import { organizationMemberErrorCode, useCreateOrganizationInvitation } from './organization-member-api'

const invitationRoles = ['admin', 'billing', 'member', 'viewer'] as const

export function OrganizationInvitationEditorSheet({
  organizationId,
  organizationName,
  teams,
  open,
  onOpenChange,
  onIssued,
}: {
  organizationId: number
  organizationName: string
  teams: OrganizationTeamResponse[]
  open: boolean
  onOpenChange: (open: boolean) => void
  onIssued: (token: string) => void
}) {
  const { t } = useTranslation()
  const mutation = useCreateOrganizationInvitation(organizationId)
  const resetMutation = mutation.reset
  const [email, setEmail] = useState('')
  const [role, setRole] = useState('member')
  const [teamId, setTeamId] = useState<number>()
  const [expiresAt, setExpiresAt] = useState('')

  useEffect(() => {
    if (!open) return
    setEmail('')
    setRole('member')
    setTeamId(undefined)
    setExpiresAt('')
    resetMutation()
  }, [open, resetMutation])

  const body = invitationBody(email, role, teamId, expiresAt)
  const submit = async () => {
    if (!body) return
    try {
      const result = await mutation.mutateAsync(body)
      onOpenChange(false)
      onIssued(result.token)
    } catch {
      // 失败时保留邀请事实，便于按原输入重试。
    }
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-lg">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('organizationConsole.invitations.editor.title')}</SheetTitle>
          <SheetDescription>{t('organizationConsole.invitations.editor.description', { organization: organizationName })}</SheetDescription>
        </SheetHeader>
        <div className="grid gap-4 overflow-y-auto p-4">
          <div>
            <label htmlFor="organization-invitation-email" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.invitations.fields.email')}</label>
            <Input id="organization-invitation-email" type="email" autoComplete="email" value={email} disabled={mutation.isPending} onChange={(event) => { setEmail(event.target.value); mutation.reset() }} />
          </div>
          <div>
            <label htmlFor="organization-invitation-role" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.role')}</label>
            <Select id="organization-invitation-role" value={role} disabled={mutation.isPending} onChange={(event) => { setRole(event.target.value); mutation.reset() }}>
              {invitationRoles.map((value) => <option key={value} value={value}>{t(`organizationConsole.roles.${value}`)}</option>)}
            </Select>
          </div>
          <div>
            <label htmlFor="organization-invitation-team" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.team')}</label>
            <Select id="organization-invitation-team" value={teamId ?? ''} disabled={mutation.isPending} onChange={(event) => { setTeamId(Number(event.target.value) || undefined); mutation.reset() }}>
              <option value="">{t('organizationConsole.members.noTeam')}</option>
              {teams.filter((team) => team.status === 'active').map((team) => <option key={team.id} value={team.id}>{team.name}</option>)}
            </Select>
          </div>
          <div>
            <label htmlFor="organization-invitation-expiry" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.invitations.fields.expiresAt')}</label>
            <Input id="organization-invitation-expiry" type="datetime-local" value={expiresAt} disabled={mutation.isPending} onChange={(event) => { setExpiresAt(event.target.value); mutation.reset() }} />
            <p className="mt-1 text-[0.6875rem] leading-5 text-muted-foreground">{t('organizationConsole.invitations.fields.expiresHint')}</p>
          </div>
          {!body && (email || expiresAt) ? <p className="text-xs text-destructive">{t('organizationConsole.invitations.validation')}</p> : null}
          {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(`organizationConsole.errors.${organizationMemberErrorCode(mutation.error) === 'organization_conflict' ? 'conflict' : 'mutation'}`)}</p> : null}
        </div>
        <SheetFooter className="mt-auto border-t border-[var(--hairline)]">
          <Button type="button" variant="outline" disabled={mutation.isPending} onClick={() => onOpenChange(false)}>{t('organizationConsole.actions.cancel')}</Button>
          <Button type="button" disabled={!body || mutation.isPending} onClick={() => void submit()}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Send aria-hidden="true" />}
            {t('organizationConsole.invitations.actions.send')}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  )
}

export function invitationBody(email: string, role: string, teamId?: number, expiresAt = ''): OrganizationInvitationCreateRequest | undefined {
  const normalized = email.trim()
  if (normalized !== email || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(normalized)) return undefined
  const expires = expiresAt ? Math.floor(new Date(expiresAt).getTime() / 1_000) : undefined
  if (expiresAt && (!Number.isSafeInteger(expires) || (expires ?? 0) <= Math.floor(Date.now() / 1_000))) return undefined
  return { email: normalized, role, team_id: teamId ?? null, expires_at: expires ?? null }
}
