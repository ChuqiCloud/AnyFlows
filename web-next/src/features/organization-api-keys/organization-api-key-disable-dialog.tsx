import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { Power } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { OrganizationTokenResponse } from '@/lib/api/generated/types.gen'
import { useDisableOrganizationApiKey } from './organization-api-key-api'

type OrganizationApiKeyDisableDialogProps = {
  organizationId: number
  organizationName: string
  directory: boolean
  token?: OrganizationTokenResponse
  onClose: () => void
}

export function OrganizationApiKeyDisableDialog(props: OrganizationApiKeyDisableDialogProps) {
  const { t } = useTranslation()
  const mutation = useDisableOrganizationApiKey(props.organizationId, props.directory)
  const disable = async () => {
    if (!props.token) return
    try {
      await mutation.mutateAsync(props.token.id)
      props.onClose()
    } catch {
      // 对话框保持打开，让用户明确看到停用失败。
    }
  }

  return (
    <Modal backdrop="blur" hideCloseButton isDismissable={false} isOpen={props.token !== undefined} onOpenChange={(open) => !open && props.onClose()}>
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-warning/10 text-warning"><Power className="size-4" aria-hidden="true" /></div>
              <h2 className="text-base font-semibold">{t('organizationApiKeys.disable.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">
                {t(props.directory ? 'organizationApiKeys.disable.directoryDescription' : 'organizationApiKeys.disable.selfDescription', {
                  name: props.token?.name,
                  organization: props.organizationName,
                  owner: props.token?.owner_user_id,
                })}
              </p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs font-normal text-destructive">{t('organizationApiKeys.disable.failed')}</p> : null}
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={props.onClose}>{t('apiKeys.actions.cancel')}</Button>
              <Button color="danger" variant="flat" isDisabled={mutation.isPending} onPress={() => void disable()}>{t('organizationApiKeys.actions.disable')}</Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
