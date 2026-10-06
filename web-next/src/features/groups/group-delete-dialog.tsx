import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { LoaderCircle, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminGroup } from '@/lib/api/generated/types.gen'
import { groupWriteErrorCode, useDeleteAdminGroup } from './group-api'

type GroupDeleteDialogProps = {
  group?: AdminGroup
  onClose: () => void
}

export function GroupDeleteDialog({ group, onClose }: GroupDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteAdminGroup()

  const confirm = async () => {
    if (!group) return
    try {
      await mutation.mutateAsync(group.id)
      onClose()
    } catch {
      // 引用冲突或运行时刷新失败时保持确认框打开，避免误报删除成功。
    }
  }

  const errorCode = mutation.error ? groupWriteErrorCode(mutation.error) ?? 'unknown' : undefined
  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={group !== undefined}
      onOpenChange={(open) => !open && !mutation.isPending && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
              <h2 className="text-base font-semibold">{t('groups.delete.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('groups.delete.description', { name: group?.display_name })}</p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              {errorCode ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs font-normal text-destructive">{t(`groups.errors.${errorCode}`, { defaultValue: t('groups.errors.unknown') })}</p> : null}
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={onClose}>{t('groups.actions.cancel')}</Button>
              <Button color="danger" variant="flat" isDisabled={mutation.isPending} onPress={() => void confirm()}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Trash2 className="size-4" aria-hidden="true" />}
                {t('groups.actions.delete')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
