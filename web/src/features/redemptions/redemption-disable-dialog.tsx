import { Ban, LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
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
    <AlertDialog open={batch !== undefined} onOpenChange={(open) => !open && !mutation.isPending && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive">
            <Ban className="size-4" aria-hidden="true" />
          </div>
          <AlertDialogTitle>{t('redemptions.disable.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('redemptions.disable.description', { name: batch?.name })}</AlertDialogDescription>
        </AlertDialogHeader>
        {mutation.isError ? (
          <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
            {t(`redemptions.errors.${errorKey}`)}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('redemptions.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            disabled={mutation.isPending}
            onClick={(event) => {
              event.preventDefault()
              void disable()
            }}
          >
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Ban aria-hidden="true" />}
            {t(mutation.isPending ? 'redemptions.actions.disabling' : 'redemptions.actions.disable')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function disableErrorKey(code: string | undefined) {
  if (code === 'redemption_batch_not_found') return 'notFound'
  if (code === 'redemption_batch_conflict') return 'conflict'
  if (code === 'redemption_outcome_unknown') return 'outcomeUnknown'
  return 'disable'
}
