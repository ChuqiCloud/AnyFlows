import { z } from 'zod'

import type {
  AdminAuthenticationSettings,
  AdminAuthenticationSettingsRequest,
} from '@/lib/api/generated/types.gen'

export type AuthenticationSettingsValues = {
  passwordLoginEnabled: boolean
  registrationEnabled: boolean
  defaultGroupId: string
  initialQuota: number
  invitationRebateQuota: number
  emailRequired: boolean
  rateLimitAttempts: number
  rateLimitWindowSeconds: string
}

type ValidationMessages = {
  incompatibleCapabilities: string
  invalidGroup: string
  invalidQuota: string
  invalidRebateQuota: string
  invalidAttempts: string
  invalidWindow: string
}

export const REGISTRATION_WINDOW_OPTIONS = [60, 300, 900, 3_600, 21_600, 86_400] as const

/** 从服务端认证快照构造完整表单值。 */
export function authenticationSettingsValues(
  settings: AdminAuthenticationSettings,
): AuthenticationSettingsValues {
  return {
    passwordLoginEnabled: settings.password_login_enabled,
    registrationEnabled: settings.registration_enabled,
    defaultGroupId: String(settings.default_group_id),
    initialQuota: settings.initial_quota,
    invitationRebateQuota: settings.invitation_rebate_quota,
    emailRequired: settings.email_required,
    rateLimitAttempts: settings.rate_limit_attempts,
    rateLimitWindowSeconds: String(settings.rate_limit_window_seconds),
  }
}

/** 校验认证能力组合与注册策略的闭合整数边界。 */
export function buildAuthenticationSettingsSchema(messages: ValidationMessages) {
  return z.object({
    passwordLoginEnabled: z.boolean(),
    registrationEnabled: z.boolean(),
    defaultGroupId: z.string().refine(
      (value) => parsePositiveInteger(value) !== undefined,
      messages.invalidGroup,
    ),
    initialQuota: z.number()
      .int(messages.invalidQuota)
      .safe(messages.invalidQuota)
      .min(0, messages.invalidQuota),
    invitationRebateQuota: z.number()
      .int(messages.invalidRebateQuota)
      .safe(messages.invalidRebateQuota)
      .min(0, messages.invalidRebateQuota),
    emailRequired: z.boolean(),
    rateLimitAttempts: z.number()
      .int(messages.invalidAttempts)
      .min(1, messages.invalidAttempts)
      .max(100, messages.invalidAttempts),
    rateLimitWindowSeconds: z.string().refine(
      (value) => {
        const parsed = Number(value)
        return Number.isSafeInteger(parsed) && parsed >= 60 && parsed <= 86_400
      },
      messages.invalidWindow,
    ),
  }).superRefine((values, context) => {
    if (values.registrationEnabled && !values.passwordLoginEnabled) {
      context.addIssue({
        code: 'custom',
        message: messages.incompatibleCapabilities,
        path: ['registrationEnabled'],
      })
    }
  })
}

/** 把表单值精确转换为完整认证设置请求。 */
export function toAuthenticationSettingsRequest(
  values: AuthenticationSettingsValues,
): AdminAuthenticationSettingsRequest {
  const defaultGroupId = parsePositiveInteger(values.defaultGroupId)
  const rateLimitWindowSeconds = Number(values.rateLimitWindowSeconds)
  if (defaultGroupId === undefined || !Number.isSafeInteger(rateLimitWindowSeconds)) {
    throw new Error('认证设置整数未通过校验')
  }
  return {
    password_login_enabled: values.passwordLoginEnabled,
    registration_enabled: values.registrationEnabled,
    default_group_id: defaultGroupId,
    initial_quota: values.initialQuota,
    invitation_rebate_quota: values.invitationRebateQuota,
    email_required: values.emailRequired,
    rate_limit_attempts: values.rateLimitAttempts,
    rate_limit_window_seconds: rateLimitWindowSeconds,
  }
}

function parsePositiveInteger(value: string) {
  if (!value) return undefined
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed >= 1 ? parsed : undefined
}
