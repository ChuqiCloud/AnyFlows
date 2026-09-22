import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { Ban, LoaderCircle } from 'lucide-react'
import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'

import type { OrganizationCustomRoleResponse } from '@/lib/api/generated/types.gen'
import { organizationCustomRoleErrorCode, useDisableOrganizationCustomRole } from './organization-custom-role-api'

export function OrganizationCustomRoleDisableDialog({
  organizationId,
  role,
  onClose,
}: {
  organizationId: number
  role?: OrganizationCustomRoleResponse
  onClose: () => void
}) {
  const { t } = useTranslation()
  const mutation = useDisableOrganizationCustomRole(organizationId)
  const resetMutation = mutation.reset

  useEffect(() => {
    if (role) resetMutation()
  }, [resetMutation, role])

  const disable = async () => {
    if (!role) return
    try {
      await mutation.mutateAsync({ roleKey: role.role_key, expectedVersion: role.version })
      onClose()
    } catch {
      // 保留确认目标与错误提示，避免把 CAS 冲突误显示为角色已经停用。
    }
  }

  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={role !== undefined}
      onOpenChange={(open) => !open && !mutation.isPending && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive">
                <Ban className="size-4" aria-hidden="true" />
              </div>
              <h2 className="text-base font-semibold">{t('organizationConsole.customRoles.disable.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('organizationConsole.customRoles.disable.description', { name: role?.name })}</p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 font-normal text-destructive">{t(`organizationConsole.customRoles.errors.${disableErrorKey(organizationCustomRoleErrorCode(mutation.error))}`)}</p> : null}
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={onClose}>{t('organizationConsole.actions.cancel')}</Button>
              <Button
                color="danger"
                variant="flat"
                isDisabled={mutation.isPending}
                onPress={() => void disable()}
              >
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Ban className="size-4" aria-hidden="true" />}
                {t(mutation.isPending ? 'organizationConsole.customRoles.actions.disabling' : 'organizationConsole.customRoles.actions.disable')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}

function disableErrorKey(code: string | undefined) {
  if (code === 'organization_lifecycle_conflict') return 'conflict'
  if (code === 'organization_lifecycle_transition_invalid') return 'inactive'
  if (code === 'organization_entitlement_restricted') return 'entitlement'
  if (code === 'organization_not_found') return 'notFound'
  return 'mutation'
}
