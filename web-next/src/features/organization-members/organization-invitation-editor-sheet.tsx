import { Button, Drawer, DrawerBody, DrawerContent, DrawerFooter, DrawerHeader, Input, Select, SelectItem } from '@heroui/react'
import { LoaderCircle, Send } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { OrganizationInvitationCreateRequest, OrganizationTeamResponse } from '@/lib/api/generated/types.gen'
import { organizationMemberErrorCode, useCreateOrganizationInvitation } from './organization-member-api'

const invitationRoles = ['admin', 'billing', 'member', 'viewer'] as const

/** HeroUI Select 不接受空字符串 key，用哨兵 key 表达"不加入团队"。 */
const NO_TEAM_KEY = '__none__'

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
  const teamItems = [
    { key: NO_TEAM_KEY, label: t('organizationConsole.members.noTeam') },
    ...teams.filter((team) => team.status === 'active').map((team) => ({ key: String(team.id), label: team.name })),
  ]

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
              <h2 className="text-base font-medium text-foreground">{t('organizationConsole.invitations.editor.title')}</h2>
              <p className="text-sm text-muted-foreground">{t('organizationConsole.invitations.editor.description', { organization: organizationName })}</p>
            </DrawerHeader>
            <DrawerBody className="gap-4 p-4">
              <div>
                <label htmlFor="organization-invitation-email" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.invitations.fields.email')}</label>
                <Input id="organization-invitation-email" type="email" autoComplete="email" value={email} isDisabled={mutation.isPending} size="sm" onChange={(event) => { setEmail(event.target.value); mutation.reset() }} />
              </div>
              <div>
                <label htmlFor="organization-invitation-role" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.role')}</label>
                <Select
                  aria-label={t('organizationConsole.members.fields.role')}
                  id="organization-invitation-role"
                  isDisabled={mutation.isPending}
                  selectedKeys={[role]}
                  size="sm"
                  onSelectionChange={(keys) => { setRole(String(Array.from(keys)[0] ?? 'member')); mutation.reset() }}
                >
                  {invitationRoles.map((value) => <SelectItem key={value}>{t(`organizationConsole.roles.${value}`)}</SelectItem>)}
                </Select>
              </div>
              <div>
                <label htmlFor="organization-invitation-team" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.members.fields.team')}</label>
                <Select
                  aria-label={t('organizationConsole.members.fields.team')}
                  id="organization-invitation-team"
                  isDisabled={mutation.isPending}
                  items={teamItems}
                  selectedKeys={[teamId === undefined ? NO_TEAM_KEY : String(teamId)]}
                  size="sm"
                  onSelectionChange={(keys) => {
                    const next = String(Array.from(keys)[0] ?? NO_TEAM_KEY)
                    setTeamId(next === NO_TEAM_KEY ? undefined : Number(next))
                    mutation.reset()
                  }}
                >
                  {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                </Select>
              </div>
              <div>
                <label htmlFor="organization-invitation-expiry" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.invitations.fields.expiresAt')}</label>
                <Input id="organization-invitation-expiry" type="datetime-local" value={expiresAt} isDisabled={mutation.isPending} size="sm" onChange={(event) => { setExpiresAt(event.target.value); mutation.reset() }} />
                <p className="mt-1 text-[0.6875rem] leading-5 text-muted-foreground">{t('organizationConsole.invitations.fields.expiresHint')}</p>
              </div>
              {!body && (email || expiresAt) ? <p className="text-xs text-destructive">{t('organizationConsole.invitations.validation')}</p> : null}
              {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(`organizationConsole.errors.${organizationMemberErrorCode(mutation.error) === 'organization_conflict' ? 'conflict' : 'mutation'}`)}</p> : null}
            </DrawerBody>
            <DrawerFooter className="mt-auto border-t border-[var(--hairline)]">
              <Button type="button" variant="bordered" isDisabled={mutation.isPending} onClick={() => onOpenChange(false)}>{t('organizationConsole.actions.cancel')}</Button>
              <Button type="button" color="primary" isDisabled={!body || mutation.isPending} onClick={() => void submit()}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Send className="size-4" aria-hidden="true" />}
                {t('organizationConsole.invitations.actions.send')}
              </Button>
            </DrawerFooter>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}

export function invitationBody(email: string, role: string, teamId?: number, expiresAt = ''): OrganizationInvitationCreateRequest | undefined {
  const normalized = email.trim()
  if (normalized !== email || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(normalized)) return undefined
  const expires = expiresAt ? Math.floor(new Date(expiresAt).getTime() / 1_000) : undefined
  if (expiresAt && (!Number.isSafeInteger(expires) || (expires ?? 0) <= Math.floor(Date.now() / 1_000))) return undefined
  return { email: normalized, role, team_id: teamId ?? null, expires_at: expires ?? null }
}
