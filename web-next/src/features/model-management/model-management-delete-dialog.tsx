import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { LoaderCircle, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminModel } from '@/lib/api/generated/types.gen'
import { useDeleteAdminModel } from './model-management-api'

type ModelManagementDeleteDialogProps = {
  model?: AdminModel
  onClose: () => void
}

export function ModelManagementDeleteDialog({ model, onClose }: ModelManagementDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteAdminModel()

  const confirm = async () => {
    if (!model) return
    try {
      await mutation.mutateAsync(model.id)
      onClose()
    } catch {
      // 删除失败时保持确认框打开，避免管理员误判元数据状态。
    }
  }

  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={model !== undefined}
      onOpenChange={(open) => !open && !mutation.isPending && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
              <h2 className="text-base font-semibold">{t('modelManagement.delete.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('modelManagement.delete.description', { model: model?.model })}</p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs font-normal text-destructive">{t('modelManagement.delete.failed')}</p> : null}
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={onClose}>{t('modelManagement.actions.cancel')}</Button>
              <Button color="danger" variant="flat" isDisabled={mutation.isPending} onPress={() => void confirm()}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Trash2 className="size-4" aria-hidden="true" />}
                {t('modelManagement.actions.delete')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
