import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { CheckCircle2, CircleOff, LoaderCircle, Pause, Play } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type {
  AdminUserSubscriptionLifecycleAction,
  UserSubscription,
} from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import {
  subscriptionErrorCode,
  useTransitionAdminUserSubscriptionLifecycle,
} from './subscription-api'

export type SubscriptionLifecycleOperation = {
  subscription: UserSubscription
  action: AdminUserSubscriptionLifecycleAction
}

type SubscriptionLifecycleDialogProps = {
  userId: number
  operation?: SubscriptionLifecycleOperation
  onClose: () => void
}

/** 确认生命周期影响，并以服务端版本提交一次闭合动作。 */
export function SubscriptionLifecycleDialog({
  userId,
  operation,
  onClose,
}: SubscriptionLifecycleDialogProps) {
  const { t } = useTranslation()
  const mutation = useTransitionAdminUserSubscriptionLifecycle()
  const action = operation?.action
  const Icon = action === 'suspend' ? Pause : action === 'resume' ? Play : CircleOff
  const errorKey = lifecycleErrorKey(subscriptionErrorCode(mutation.error))

  const close = () => {
    if (mutation.isPending) return
    mutation.reset()
    onClose()
  }
  const submit = () => {
    if (!operation) return
    mutation.mutate({
      userId,
      subscriptionId: operation.subscription.subscription_id,
      body: {
        action: operation.action,
        expected_version: operation.subscription.version,
      },
    })
  }

  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={operation !== undefined}
      onOpenChange={(open) => !open && close()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <span className={cn(
                'grid size-10 place-items-center rounded-lg',
                action === 'cancel'
                  ? 'bg-destructive/10 text-destructive'
                  : 'bg-primary/10 text-primary',
              )}>
                {mutation.isSuccess
                  ? <CheckCircle2 className="size-4 text-success" aria-hidden="true" />
                  : <Icon className="size-4" aria-hidden="true" />}
              </span>
              <h2 className="text-base font-semibold">
                {t(mutation.isSuccess
                  ? 'subscriptions.lifecycle.successTitle'
                  : `subscriptions.lifecycle.${action ?? 'suspend'}Title`)}
              </h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">
                {mutation.isSuccess
                  ? t(
                      mutation.data.periods_elapsed > 0
                        ? 'subscriptions.lifecycle.successAdvanced'
                        : 'subscriptions.lifecycle.success',
                      { periods: mutation.data.periods_elapsed },
                    )
                  : t(`subscriptions.lifecycle.${action ?? 'suspend'}Description`, {
                      name: operation?.subscription.plan_name,
                    })}
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
              {mutation.isSuccess ? (
                <Button variant="light" onPress={close}>{t('subscriptions.actions.done')}</Button>
              ) : (
                <>
                  <Button variant="light" isDisabled={mutation.isPending} onPress={close}>{t('subscriptions.actions.cancelDialog')}</Button>
                  <Button
                    color={action === 'cancel' ? 'danger' : 'primary'}
                    isDisabled={mutation.isPending}
                    variant={action === 'cancel' ? 'flat' : 'solid'}
                    onPress={submit}
                  >
                    {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Icon className="size-4" aria-hidden="true" />}
                    {t(mutation.isPending
                      ? 'subscriptions.actions.transitioning'
                      : `subscriptions.actions.confirm${capitalize(action ?? 'suspend')}`)}
                  </Button>
                </>
              )}
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}

function lifecycleErrorKey(code: string | undefined) {
  if (code === 'subscription_not_found') return 'lifecycleNotFound'
  if (code === 'subscription_transition_invalid') return 'lifecycleInvalid'
  if (code === 'subscription_in_use') return 'lifecycleInUse'
  if (code === 'subscription_conflict') return 'lifecycleConflict'
  if (code === 'subscription_outcome_unknown') return 'outcomeUnknown'
  return 'lifecycle'
}

function capitalize(value: string) {
  return `${value.slice(0, 1).toUpperCase()}${value.slice(1)}`
}
