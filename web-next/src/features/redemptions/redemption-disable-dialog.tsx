import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { Ban, LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminRedemptionBatch } from '@/lib/api/generated/types.gen'
import { redemptionErrorCode, useDisableAdminRedemptionBatch } from './redemption-api'

type RedemptionDisableDialogProps = {
  batch?: AdminRedemptionBatch
  onClose: () => void
}

export function RedemptionDisableDialog({ batch, onClose }: RedemptionDisableDialogProps) {
  const { t } = useTranslation()
  const mutation = useDisableAdminRedemptionBatch()
  const errorKey = disableErrorKey(redemptionErrorCode(mutation.error))

  const disable = async () => {
    if (!batch) return
    await mutation.mutateAsync({
      batchId: batch.batch_id,
      body: { expected_version: batch.version },
    })
    onClose()
  }

  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={batch !== undefined}
      onOpenChange={(open) => !open && !mutation.isPending && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive">
                <Ban className="size-4" aria-hidden="true" />
              </div>
              <h2 className="text-base font-semibold">{t('redemptions.disable.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('redemptions.disable.description', { name: batch?.name })}</p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              {mutation.isError ? (
                <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs font-normal text-destructive">
                  {t(`redemptions.errors.${errorKey}`)}
                </p>
              ) : null}
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={onClose}>{t('redemptions.actions.cancel')}</Button>
              <Button color="danger" variant="flat" isDisabled={mutation.isPending} onPress={() => void disable()}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Ban className="size-4" aria-hidden="true" />}
                {t(mutation.isPending ? 'redemptions.actions.disabling' : 'redemptions.actions.disable')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}

function disableErrorKey(code: string | undefined) {
  if (code === 'redemption_batch_not_found') return 'notFound'
  if (code === 'redemption_batch_conflict') return 'conflict'
  if (code === 'redemption_outcome_unknown') return 'outcomeUnknown'
  return 'disable'
}
