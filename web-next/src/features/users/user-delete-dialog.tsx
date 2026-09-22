import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { LoaderCircle, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminUser } from '@/lib/api/generated/types.gen'
import { useDeleteAdminUser } from './user-api'

type UserDeleteDialogProps = {
  user?: AdminUser
  onClose: () => void
}

export function UserDeleteDialog({ user, onClose }: UserDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteAdminUser()

  const confirm = async () => {
    if (!user) return
    try {
      await mutation.mutateAsync(user.id)
      onClose()
    } catch {
      // 失败时保持确认框打开，避免管理员误以为账户已经删除。
    }
  }

  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={user !== undefined}
      onOpenChange={(open) => !open && !mutation.isPending && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
              <h2 className="text-base font-semibold">{t('users.delete.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('users.delete.description', { username: user?.username })}</p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs font-normal text-destructive">{t('users.delete.failed')}</p> : null}
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={onClose}>{t('users.actions.cancel')}</Button>
              <Button color="danger" variant="flat" isDisabled={mutation.isPending} onPress={() => void confirm()}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Trash2 className="size-4" aria-hidden="true" />}
                {t('users.actions.delete')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
