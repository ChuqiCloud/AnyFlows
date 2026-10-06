import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { LoaderCircle, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminRoute } from '@/lib/api/generated/types.gen'
import { routeWriteErrorCode, useDeleteAdminRoute } from './route-api'

type RouteDeleteDialogProps = {
  route?: AdminRoute
  onClose: () => void
}

export function RouteDeleteDialog({ route, onClose }: RouteDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteAdminRoute()

  const confirm = async () => {
    if (!route) return
    try {
      await mutation.mutateAsync(route.id)
      onClose()
    } catch {
      // 删除冲突时保留确认框，让管理员看到稳定错误并决定下一步。
    }
  }

  const errorCode = mutation.error ? routeWriteErrorCode(mutation.error) : undefined
  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={route !== undefined}
      onOpenChange={(open) => !open && !mutation.isPending && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
              <h2 className="text-base font-semibold">{t('routes.delete.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('routes.delete.description', { name: route?.name ?? '' })}</p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              {errorCode ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs font-normal text-destructive">{t(`routes.errors.${errorCode}`, { defaultValue: t('routes.errors.unknown') })}</p> : null}
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={onClose}>{t('routes.actions.cancel')}</Button>
              <Button color="danger" variant="flat" isDisabled={mutation.isPending} onPress={() => void confirm()}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Trash2 className="size-4" aria-hidden="true" />}
                {t('routes.actions.delete')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
