import { Check, Copy, LoaderCircle, MailPlus } from 'lucide-react'
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
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'

import { useInvitePlatformOrganizationMember } from './platform-organization-api'

const invitationRoles = ['admin', 'billing', 'member', 'viewer'] as const

export type PlatformOrganizationChoice = {
  id: number
  name: string
}

/** 创建仍需目标用户接受的一次性企业邀请，绝不直接写入成员关系。 */
export function PlatformOrganizationInvitationDialog({
  open,
  organizations,
  initialOrganizationId,
  initialEmail,
  lockEmail = false,
  hasMoreOrganizations = false,
  loadingMoreOrganizations = false,
  onLoadMoreOrganizations,
  onOpenChange,
}: {
  open: boolean
  organizations: PlatformOrganizationChoice[]
  initialOrganizationId?: number
  initialEmail?: string
  lockEmail?: boolean
  hasMoreOrganizations?: boolean
  loadingMoreOrganizations?: boolean
  onLoadMoreOrganizations?: () => void
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const [organizationId, setOrganizationId] = useState(initialOrganizationId ?? organizations[0]?.id ?? 0)
  const [email, setEmail] = useState(initialEmail ?? '')
  const [role, setRole] = useState('member')
  const [copied, setCopied] = useState(false)
  const mutation = useInvitePlatformOrganizationMember(organizationId)
  const resetMutation = mutation.reset
  const defaultOrganizationId = initialOrganizationId ?? organizations[0]?.id ?? 0

  useEffect(() => {
    if (!open) return
    setOrganizationId(defaultOrganizationId)
    setEmail(initialEmail ?? '')
    setRole('member')
    setCopied(false)
    resetMutation()
  }, [defaultOrganizationId, initialEmail, open, resetMutation])

  const normalizedEmail = email.trim()
  const emailValid = normalizedEmail === email && /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)
  const submit = async () => {
    if (!organizationId || !emailValid) return
    try {
      await mutation.mutateAsync({ email, role, expires_at: null })
    } catch {
      // 失败时保留目标企业和邮箱，便于按原输入重试。
    }
  }
  const copy = async () => {
    if (!mutation.data?.token) return
    try {
      await navigator.clipboard.writeText(mutation.data.token)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }
  const close = () => {
    mutation.reset()
    onOpenChange(false)
  }

  return (
    <AlertDialog open={open} onOpenChange={(next) => { if (!next) close() }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('platformOrganizations.invitation.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('platformOrganizations.invitation.description')}</AlertDialogDescription>
        </AlertDialogHeader>
        {mutation.data ? (
          <div className="grid gap-3">
            <p className="rounded-md bg-success/10 px-3 py-2 text-xs leading-5 text-success">
              {t('platformOrganizations.invitation.issued')}
            </p>
            <div className="flex gap-2">
              <Input value={mutation.data.token} readOnly className="min-w-0 font-mono text-xs" aria-label={t('platformOrganizations.invitation.token')} />
              <Button type="button" size="icon" variant="outline" aria-label={t('platformOrganizations.invitation.copy')} onClick={() => void copy()}>
                {copied ? <Check className="text-success" aria-hidden="true" /> : <Copy aria-hidden="true" />}
              </Button>
            </div>
            <p className="text-xs leading-5 text-muted-foreground">{t('platformOrganizations.invitation.tokenHint')}</p>
          </div>
        ) : (
          <div className="grid gap-4">
            <div>
              <label htmlFor="platform-invitation-organization" className="mb-1.5 block text-xs font-medium">{t('platformOrganizations.invitation.organization')}</label>
              <Select id="platform-invitation-organization" value={organizationId || ''} disabled={organizations.length <= 1 || mutation.isPending} onChange={(event) => setOrganizationId(Number(event.target.value))}>
                {organizations.length === 0 ? <option value="">{t('platformOrganizations.invitation.loadingOrganizations')}</option> : null}
                {organizations.map((organization) => <option key={organization.id} value={organization.id}>{organization.name}</option>)}
              </Select>
              {hasMoreOrganizations ? <Button type="button" size="sm" variant="ghost" className="mt-1" disabled={loadingMoreOrganizations} onClick={onLoadMoreOrganizations}>{loadingMoreOrganizations ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t('platformOrganizations.invitation.loadMoreOrganizations')}</Button> : null}
            </div>
            <div>
              <label htmlFor="platform-invitation-email" className="mb-1.5 block text-xs font-medium">{t('platformOrganizations.invitation.email')}</label>
              <Input id="platform-invitation-email" type="email" autoComplete="email" value={email} readOnly={lockEmail} disabled={mutation.isPending} onChange={(event) => setEmail(event.target.value)} />
            </div>
            <div>
              <label htmlFor="platform-invitation-role" className="mb-1.5 block text-xs font-medium">{t('platformOrganizations.invitation.role')}</label>
              <Select id="platform-invitation-role" value={role} disabled={mutation.isPending} onChange={(event) => setRole(event.target.value)}>
                {invitationRoles.map((value) => <option key={value} value={value}>{t(`platformOrganizations.roles.${value}`)}</option>)}
              </Select>
            </div>
            <p className="text-xs leading-5 text-muted-foreground">{t('platformOrganizations.invitation.acceptHint')}</p>
            {mutation.isError ? <p role="alert" className="rounded-md bg-destructive/8 px-3 py-2 text-xs text-destructive">{t('platformOrganizations.invitation.error')}</p> : null}
          </div>
        )}
        <AlertDialogFooter>
          {mutation.data ? (
            <AlertDialogAction onClick={close}>{t('platformOrganizations.invitation.done')}</AlertDialogAction>
          ) : (
            <>
              <AlertDialogCancel disabled={mutation.isPending}>{t('platformOrganizations.invitation.cancel')}</AlertDialogCancel>
              <AlertDialogAction disabled={!organizationId || !emailValid || mutation.isPending} onClick={(event) => { event.preventDefault(); void submit() }}>
                {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <MailPlus aria-hidden="true" />}
                {t('platformOrganizations.invitation.submit')}
              </AlertDialogAction>
            </>
          )}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
