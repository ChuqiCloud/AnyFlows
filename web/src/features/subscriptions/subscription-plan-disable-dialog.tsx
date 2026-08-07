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
    <AlertDialog open={plan !== undefined} onOpenChange={(open) => !open && !mutation.isPending && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <span className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive">
            <Ban className="size-4" aria-hidden="true" />
          </span>
          <AlertDialogTitle>{t('subscriptions.disable.title')}</AlertDialogTitle>
          <AlertDialogDescription>
            {t('subscriptions.disable.description', { name: plan?.name })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {mutation.isError ? (
          <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
            {t(`subscriptions.errors.${errorKey}`)}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('subscriptions.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            disabled={mutation.isPending}
            onClick={(event) => {
              event.preventDefault()
              void disable()
            }}
          >
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Ban aria-hidden="true" />}
            {t(mutation.isPending ? 'subscriptions.actions.disablingPlan' : 'subscriptions.actions.disablePlan')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function disableErrorKey(code: string | undefined) {
  if (code === 'subscription_plan_not_found') return 'planNotFound'
  if (code === 'subscription_conflict') return 'disableConflict'
  if (code === 'subscription_outcome_unknown') return 'outcomeUnknown'
  return 'disablePlan'
}
