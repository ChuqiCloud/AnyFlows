import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Select, SelectItem } from '@heroui/react'
import { LoaderCircle, Save } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

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
    <Modal backdrop="blur" hideCloseButton isDismissable={false} isOpen={member !== undefined} onOpenChange={onOpenChange}>
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <h2 className="text-base font-semibold">{t('platformOrganizations.members.editor.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('platformOrganizations.members.editor.description', { organization: organizationName, username: member?.username })}</p>
            </ModalHeader>
            <ModalBody className="gap-4">
              <div className="grid gap-4">
                <div>
                  <label className="mb-1.5 block text-xs font-medium" htmlFor="platform-member-role">{t('platformOrganizations.members.editor.role')}</label>
                  <Select
                    aria-label={t('platformOrganizations.members.editor.role')}
                    id="platform-member-role"
                    isDisabled={mutation.isPending}
                    items={editableRoles.map((value) => ({ key: value, label: t(`platformOrganizations.roles.${value}`) }))}
                    selectedKeys={[role]}
                    size="sm"
                    onSelectionChange={(keys) => { setRole(String(Array.from(keys)[0] ?? 'member')); mutation.reset() }}
                  >
                    {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                  </Select>
                </div>
                <div>
                  <label className="mb-1.5 block text-xs font-medium" htmlFor="platform-member-status">{t('platformOrganizations.members.editor.status')}</label>
                  <Select
                    aria-label={t('platformOrganizations.members.editor.status')}
                    id="platform-member-status"
                    isDisabled={mutation.isPending}
                    items={editableStatuses.map((value) => ({ key: value, label: t(`platformOrganizations.members.status.${value}`) }))}
                    selectedKeys={[status]}
                    size="sm"
                    onSelectionChange={(keys) => { setStatus(String(Array.from(keys)[0] ?? 'active')); mutation.reset() }}
                  >
                    {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                  </Select>
                </div>
                {status === 'removed' ? <p className="rounded-md bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t('platformOrganizations.members.editor.removedWarning')}</p> : null}
                <p className="text-xs leading-5 text-muted-foreground">{t('platformOrganizations.members.editor.boundary')}</p>
                {mutation.isError ? <p role="alert" className="rounded-md bg-destructive/8 px-3 py-2 text-xs text-destructive">{t('platformOrganizations.members.editor.error')}</p> : null}
              </div>
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={() => onOpenChange(false)}>{t('platformOrganizations.members.editor.cancel')}</Button>
              <Button color="primary" isDisabled={unchanged || mutation.isPending} onPress={() => void submit()}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Save className="size-4" aria-hidden="true" />}
                {t('platformOrganizations.members.editor.save')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
