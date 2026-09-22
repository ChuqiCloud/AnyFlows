import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Select, SelectItem } from '@heroui/react'
import { Check, Copy, LoaderCircle, MailPlus } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useInvitePlatformOrganizationMember } from './platform-organization-api'

const invitationRoles = ['admin', 'billing', 'member', 'viewer'] as const

// HeroUI Select 不接受数值 key，统一用字符串映射。
const NO_ORGANIZATION_KEY = '__none__'

export type PlatformOrganizationChoice = {
  id: number
  name: string
}

/** 创建仍需目标用户接受的一次性企业邀请，绝不直接写入成员关系。 */
export function PlatformOrganizationInvitationDialog({
  open,
  reason,
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
  reason: string
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
  const mutation = useInvitePlatformOrganizationMember({ organizationId, reason })
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
    <Modal backdrop="blur" hideCloseButton isDismissable={false} isOpen={open} onOpenChange={(next) => { if (!next) close() }}>
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <h2 className="text-base font-semibold">{t('platformOrganizations.invitation.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('platformOrganizations.invitation.description')}</p>
            </ModalHeader>
            <ModalBody className="gap-4">
              {mutation.data ? (
                <div className="grid gap-3">
                  <p className="rounded-md bg-success/10 px-3 py-2 text-xs leading-5 text-success">
                    {t('platformOrganizations.invitation.issued')}
                  </p>
                  <div className="flex gap-2">
                    <Input className="min-w-0 font-mono" classNames={{ input: 'text-xs' }} isReadOnly size="sm" value={mutation.data.token} aria-label={t('platformOrganizations.invitation.token')} />
                    <Button isIconOnly type="button" size="md" variant="bordered" aria-label={t('platformOrganizations.invitation.copy')} onClick={() => void copy()}>
                      {copied ? <Check className="size-4 text-success" aria-hidden="true" /> : <Copy className="size-4" aria-hidden="true" />}
                    </Button>
                  </div>
                  <p className="text-xs leading-5 text-muted-foreground">{t('platformOrganizations.invitation.tokenHint')}</p>
                </div>
              ) : (
                <div className="grid gap-4">
                  <div>
                    <label className="mb-1.5 block text-xs font-medium" htmlFor="platform-invitation-organization">{t('platformOrganizations.invitation.organization')}</label>
                    <Select
                      aria-label={t('platformOrganizations.invitation.organization')}
                      id="platform-invitation-organization"
                      isDisabled={organizations.length <= 1 || mutation.isPending}
                      items={organizations.length === 0
                        ? [{ key: NO_ORGANIZATION_KEY, label: t('platformOrganizations.invitation.loadingOrganizations') }]
                        : organizations.map((organization) => ({ key: String(organization.id), label: organization.name }))}
                      selectedKeys={organizationId ? [String(organizationId)] : []}
                      size="sm"
                      onSelectionChange={(keys) => setOrganizationId(Number(Array.from(keys)[0]) || 0)}
                    >
                      {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                    </Select>
                    {hasMoreOrganizations ? <Button type="button" size="sm" variant="light" className="mt-1" isDisabled={loadingMoreOrganizations} onClick={onLoadMoreOrganizations}>{loadingMoreOrganizations ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : null}{t('platformOrganizations.invitation.loadMoreOrganizations')}</Button> : null}
                  </div>
                  <div>
                    <label className="mb-1.5 block text-xs font-medium" htmlFor="platform-invitation-email">{t('platformOrganizations.invitation.email')}</label>
                    <Input autoComplete="email" id="platform-invitation-email" isDisabled={mutation.isPending} isReadOnly={lockEmail} size="sm" type="email" value={email} onChange={(event) => setEmail(event.target.value)} />
                  </div>
                  <div>
                    <label className="mb-1.5 block text-xs font-medium" htmlFor="platform-invitation-role">{t('platformOrganizations.invitation.role')}</label>
                    <Select
                      aria-label={t('platformOrganizations.invitation.role')}
                      id="platform-invitation-role"
                      isDisabled={mutation.isPending}
                      items={invitationRoles.map((value) => ({ key: value, label: t(`platformOrganizations.roles.${value}`) }))}
                      selectedKeys={[role]}
                      size="sm"
                      onSelectionChange={(keys) => setRole(String(Array.from(keys)[0] ?? 'member'))}
                    >
                      {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                    </Select>
                  </div>
                  <p className="text-xs leading-5 text-muted-foreground">{t('platformOrganizations.invitation.acceptHint')}</p>
                  {mutation.isError ? <p role="alert" className="rounded-md bg-destructive/8 px-3 py-2 text-xs text-destructive">{t('platformOrganizations.invitation.error')}</p> : null}
                </div>
              )}
            </ModalBody>
            <ModalFooter>
              {mutation.data ? (
                <Button color="primary" onPress={close}>{t('platformOrganizations.invitation.done')}</Button>
              ) : (
                <>
                  <Button variant="light" isDisabled={mutation.isPending} onPress={close}>{t('platformOrganizations.invitation.cancel')}</Button>
                  <Button color="primary" isDisabled={!organizationId || !emailValid || mutation.isPending} onPress={() => void submit()}>
                    {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <MailPlus className="size-4" aria-hidden="true" />}
                    {t('platformOrganizations.invitation.submit')}
                  </Button>
                </>
              )}
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
