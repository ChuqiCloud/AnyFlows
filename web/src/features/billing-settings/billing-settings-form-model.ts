import { z } from 'zod'

import type {
  AdminBalanceAlertSettings,
  AdminBalanceAlertSettingsRequest,
} from '@/lib/api/generated/types.gen'

export const BALANCE_ALERT_WINDOW_OPTIONS = [3_600, 21_600, 43_200, 86_400, 259_200, 604_800] as const

export type BillingSettingsValues = {
  enabled: boolean
  defaultThreshold: number
  reminderIntervalSeconds: string
  subscriptionAlertEnabled: boolean
  subscriptionRemainingPercent: number
}

type ValidationMessages = {
  invalidThreshold: string
  invalidWindow: string
  invalidSubscriptionPercent: string
}

/** 从服务端快照构造完整计费设置表单值。 */
export function billingSettingsValues(settings: AdminBalanceAlertSettings): BillingSettingsValues {
  return {
    enabled: settings.enabled,
    defaultThreshold: settings.default_threshold,
    reminderIntervalSeconds: String(settings.reminder_interval_seconds),
    subscriptionAlertEnabled: settings.subscription_alert_enabled,
    subscriptionRemainingPercent: settings.subscription_remaining_percent,
  }
}

/** 校验余额阈值和提醒窗口的安全整数边界。 */
export function buildBillingSettingsSchema(messages: ValidationMessages) {
  return z.object({
    enabled: z.boolean(),
    defaultThreshold: z.number()
      .int(messages.invalidThreshold)
      .safe(messages.invalidThreshold)
      .min(1, messages.invalidThreshold),
    reminderIntervalSeconds: z.string().refine((value) => {
      const parsed = Number(value)
      return Number.isSafeInteger(parsed) && parsed >= 3_600 && parsed <= 604_800
    }, messages.invalidWindow),
    subscriptionAlertEnabled: z.boolean(),
    subscriptionRemainingPercent: z.number()
      .int(messages.invalidSubscriptionPercent)
      .safe(messages.invalidSubscriptionPercent)
      .min(1, messages.invalidSubscriptionPercent)
      .max(99, messages.invalidSubscriptionPercent),
  })
}

/** 把表单值转换为后端完整覆盖请求。 */
export function toBillingSettingsRequest(
  values: BillingSettingsValues,
): AdminBalanceAlertSettingsRequest {
  const reminderIntervalSeconds = Number(values.reminderIntervalSeconds)
  if (!Number.isSafeInteger(reminderIntervalSeconds)) {
    throw new Error('余额预警窗口未通过校验')
  }
  return {
    enabled: values.enabled,
    default_threshold: values.defaultThreshold,
    reminder_interval_seconds: reminderIntervalSeconds,
    subscription_alert_enabled: values.subscriptionAlertEnabled,
    subscription_remaining_percent: values.subscriptionRemainingPercent,
  }
}
