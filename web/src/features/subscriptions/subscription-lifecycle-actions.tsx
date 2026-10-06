import { CircleOff, Pause, Play } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import type {
  AdminUserSubscriptionLifecycleAction,
  UserSubscription,
} from '@/lib/api/generated/types.gen'

type SubscriptionLifecycleActionsProps = {
  subscription: UserSubscription
  onSelect: (action: AdminUserSubscriptionLifecycleAction) => void
}

/** 只呈现当前订阅状态允许的管理员动作。 */
export function SubscriptionLifecycleActions({
  subscription,
  onSelect,
}: SubscriptionLifecycleActionsProps) {
  const { t } = useTranslation()
  const actions = allowedActions(subscription.status)

  if (actions.length === 0) {
    return <span className="text-[0.6875rem] text-muted-foreground">{t('subscriptions.lifecycle.readOnly')}</span>
  }

  return (
    <div className="flex min-h-6 flex-wrap items-center gap-1">
      {actions.map((action) => {
        const Icon = action === 'suspend' ? Pause : action === 'resume' ? Play : CircleOff
        return (
          <Button
            key={action}
            type="button"
            size="xs"
            variant={action === 'cancel' ? 'destructive' : 'secondary'}
            title={t(actionLabelKey(action))}
            onClick={() => onSelect(action)}
          >
            <Icon aria-hidden="true" />
            {t(actionLabelKey(action))}
          </Button>
        )
      })}
    </div>
  )
}

function actionLabelKey(action: AdminUserSubscriptionLifecycleAction) {
  return action === 'cancel'
    ? 'subscriptions.actions.cancelSubscription'
    : `subscriptions.actions.${action}`
}

function allowedActions(
  status: UserSubscription['status'],
): AdminUserSubscriptionLifecycleAction[] {
  if (status === 'active') return ['suspend', 'cancel']
  if (status === 'suspended') return ['resume', 'cancel']
  return []
}
