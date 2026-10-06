import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { Ban, LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminSubscriptionPlan } from '@/lib/api/generated/types.gen'
import { subscriptionErrorCode, useDisableAdminSubscriptionPlan } from './subscription-api'

type SubscriptionPlanDisableDialogProps = {
  plan?: AdminSubscriptionPlan
  onClose: () => void
}

/** 以当前计划版本确认停用，冲突时保留服务端事实。 */
export function SubscriptionPlanDisableDialog({ plan, onClose }: SubscriptionPlanDisableDialogProps) {
  const { t } = useTranslation()
  const mutation = useDisableAdminSubscriptionPlan()
  const errorKey = disableErrorKey(subscriptionErrorCode(mutation.error))

  const disable = async () => {
    if (!plan) return
    await mutation.mutateAsync({
      planId: plan.plan_id,
      body: { expected_version: plan.version },
    })
    onClose()
  }

  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={plan !== undefined}
      onOpenChange={(open) => !open && !mutation.isPending && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <span className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive">
                <Ban className="size-4" aria-hidden="true" />
              </span>
              <h2 className="text-base font-semibold">{t('subscriptions.disable.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">
                {t('subscriptions.disable.description', { name: plan?.name })}
              </p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              {mutation.isError ? (
                <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs font-normal text-destructive">
                  {t(`subscriptions.errors.${errorKey}`)}
                </p>
              ) : null}
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={onClose}>{t('subscriptions.actions.cancel')}</Button>
              <Button
                color="danger"
                isDisabled={mutation.isPending}
                variant="flat"
                onPress={() => void disable()}
              >
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Ban className="size-4" aria-hidden="true" />}
                {t(mutation.isPending ? 'subscriptions.actions.disablingPlan' : 'subscriptions.actions.disablePlan')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}

function disableErrorKey(code: string | undefined) {
  if (code === 'subscription_plan_not_found') return 'planNotFound'
  if (code === 'subscription_conflict') return 'disableConflict'
  if (code === 'subscription_outcome_unknown') return 'outcomeUnknown'
  return 'disablePlan'
}
