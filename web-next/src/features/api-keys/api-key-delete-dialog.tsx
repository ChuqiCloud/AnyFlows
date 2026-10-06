import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { LoaderCircle, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { UserToken } from '@/lib/api/generated/types.gen'
import { useDeleteApiKey } from './api-key-api'

type ApiKeyDeleteDialogProps = {
  token?: UserToken
  onClose: () => void
  onDeleted: () => void
}

export function ApiKeyDeleteDialog({ token, onClose, onDeleted }: ApiKeyDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteApiKey()

  const confirm = async () => {
    if (!token) return
    try {
      await mutation.mutateAsync(token.id)
      onDeleted()
    } catch {
      // 失败时保持确认框打开，避免用户误以为 Key 已经失效。
    }
  }

  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={token !== undefined}
      onOpenChange={(open) => !open && !mutation.isPending && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
              <h2 className="text-base font-semibold">{t('apiKeys.delete.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('apiKeys.delete.description', { name: token?.name })}</p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs font-normal text-destructive">{t('apiKeys.delete.failed')}</p> : null}
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={onClose}>{t('apiKeys.actions.cancel')}</Button>
              <Button color="danger" variant="flat" isDisabled={mutation.isPending} onPress={() => void confirm()}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Trash2 className="size-4" aria-hidden="true" />}
                {t('apiKeys.actions.delete')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
