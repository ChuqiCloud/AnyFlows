import { CheckCircle2, CircleOff, LoaderCircle, Pause, Play } from 'lucide-react'
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
    <AlertDialog open={operation !== undefined} onOpenChange={(open) => !open && close()}>
      <AlertDialogContent>
        <AlertDialogHeader>
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
          <AlertDialogTitle>
            {t(mutation.isSuccess
              ? 'subscriptions.lifecycle.successTitle'
              : `subscriptions.lifecycle.${action ?? 'suspend'}Title`)}
          </AlertDialogTitle>
          <AlertDialogDescription>
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
          </AlertDialogDescription>
        </AlertDialogHeader>

        {mutation.isError ? (
          <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
            {t(`subscriptions.errors.${errorKey}`)}
          </p>
        ) : null}

        <AlertDialogFooter>
          {mutation.isSuccess ? (
            <AlertDialogCancel>{t('subscriptions.actions.done')}</AlertDialogCancel>
          ) : (
            <>
              <AlertDialogCancel disabled={mutation.isPending}>{t('subscriptions.actions.cancelDialog')}</AlertDialogCancel>
              <AlertDialogAction
                className={action === 'cancel' ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90' : undefined}
                disabled={mutation.isPending}
                onClick={(event) => {
                  event.preventDefault()
                  submit()
                }}
              >
                {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Icon aria-hidden="true" />}
                {t(mutation.isPending
                  ? 'subscriptions.actions.transitioning'
                  : `subscriptions.actions.confirm${capitalize(action ?? 'suspend')}`)}
              </AlertDialogAction>
            </>
          )}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
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
