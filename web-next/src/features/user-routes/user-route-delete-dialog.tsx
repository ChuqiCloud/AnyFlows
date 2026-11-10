import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { LoaderCircle, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useDeleteUserRoute, userRouteErrorCode } from './user-route-api'
import type { UserRoute } from './user-route-model'

export function UserRouteDeleteDialog({ route, onClose }: { route?: UserRoute; onClose: () => void }) {
  const { t } = useTranslation()
  const mutation = useDeleteUserRoute()
  const confirm = async () => {
    if (!route) return
    try {
      await mutation.mutateAsync(route.id)
      onClose()
    } catch {
      // Keep the dialog open so the user can retry.
    }
  }
  const code = mutation.error ? userRouteErrorCode(mutation.error) ?? 'unknown' : undefined
  return (
    <Modal backdrop="blur" hideCloseButton isDismissable={!mutation.isPending} isOpen={route !== undefined} onOpenChange={(open) => !open && !mutation.isPending && onClose()}>
      <ModalContent>{() => (
        <>
          <ModalHeader className="grid gap-1.5">
            <div className="grid size-10 place-items-center rounded-lg bg-danger/10 text-danger"><Trash2 className="size-4" aria-hidden="true" /></div>
            <h2 className="text-base font-semibold">{t('userRoutes.delete.title')}</h2>
            <p className="text-sm font-normal leading-5 text-muted-foreground">{t('userRoutes.delete.body', { name: route?.name })}</p>
          </ModalHeader>
          <ModalBody>
            {code ? <p role="alert" className="rounded-lg bg-danger/10 px-3 py-2 text-xs text-danger">{t(`userRoutes.errors.${code}`, { defaultValue: t('userRoutes.errors.unknown') })}</p> : null}
          </ModalBody>
          <ModalFooter>
            <Button variant="light" isDisabled={mutation.isPending} onPress={onClose}>{t('userRoutes.actions.cancel')}</Button>
            <Button color="danger" variant="flat" isDisabled={mutation.isPending} onPress={() => void confirm()}>
              {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Trash2 className="size-4" aria-hidden="true" />}
              {t('userRoutes.actions.delete')}
            </Button>
          </ModalFooter>
        </>
      )}</ModalContent>
    </Modal>
  )
}
