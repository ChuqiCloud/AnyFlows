import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

type ModelSyncApplyDialogProps = {
  count: number
  onApply: () => void
  onOpenChange: (open: boolean) => void
  open: boolean
  pending: boolean
}

export function ModelSyncApplyDialog(props: ModelSyncApplyDialogProps) {
  const { t } = useTranslation()
  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={props.open}
      onOpenChange={props.onOpenChange}
    >
      <ModalContent>
        {(onClose) => (
          <>
            <ModalHeader className="grid gap-1.5">
              <h2 className="text-base font-semibold">{t('modelManagement.sync.apply.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">
                {t('modelManagement.sync.apply.description', { count: props.count })}
              </p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              <div className="rounded-lg bg-surface-2/55 px-3 py-2 text-xs leading-5 text-muted-foreground">
                {t('modelManagement.sync.apply.boundary')}
              </div>
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={props.pending} onPress={onClose}>{t('modelManagement.actions.cancel')}</Button>
              <Button color="primary" isDisabled={props.pending} onPress={props.onApply}>
                {props.pending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
                {t('modelManagement.sync.actions.apply')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
