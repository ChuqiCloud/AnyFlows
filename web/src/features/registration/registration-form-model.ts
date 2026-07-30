import { z } from 'zod'

import type {
  RegistrationPolicyRequest,
  RegistrationPolicyResponse,
  RegistrationRequestWritable,
} from '@/lib/api/generated/types.gen'

export type RegistrationFormValues = {
  username: string
  email: string
  password: string
  passwordConfirmation: string
}

export type RegistrationSettingsValues = {
  enabled: boolean
  defaultGroupId: string
  initialQuota: number
  emailRequired: boolean
  rateLimitAttempts: number
  rateLimitWindowSeconds: string
}

type RegistrationValidationMessages = {
  invalidUsername: string
  invalidEmail: string
  emailRequired: string
  invalidPassword: string
  passwordMismatch: string
}

type SettingsValidationMessages = {
  invalidGroup: string
  invalidQuota: string
  invalidAttempts: string
  invalidWindow: string
}

export const REGISTRATION_WINDOW_OPTIONS = [60, 300, 900, 3_600, 21_600, 86_400] as const

export const defaultRegistrationFormValues: RegistrationFormValues = {
  username: '',
  email: '',
  password: '',
  passwordConfirmation: '',
}

/** 复现公开注册字段边界，最终结果仍以后端校验为准。 */
export function buildRegistrationFormSchema(
  emailRequired: boolean,
  messages: RegistrationValidationMessages,
) {
  return z.object({
    username: z.string().refine(isValidUsername, messages.invalidUsername),
    email: z.string().refine(isValidOptionalEmail, messages.invalidEmail),
    password: z.string().refine(isValidPassword, messages.invalidPassword),
    passwordConfirmation: z.string(),
  }).superRefine((values, context) => {
    if (emailRequired && values.email.length === 0) {
      context.addIssue({
        code: 'custom',
        message: messages.emailRequired,
        path: ['email'],
      })
    }
  }).refine(
    (values) => values.password === values.passwordConfirmation,
    { message: messages.passwordMismatch, path: ['passwordConfirmation'] },
  )
}

export function toRegistrationRequest(values: RegistrationFormValues): RegistrationRequestWritable {
  return {
    username: values.username,
    email: values.email || null,
    password: values.password,
  }
}

/** 从服务端策略构造可编辑值，不在前端推导默认策略。 */
export function settingsValuesFromPolicy(
  policy: RegistrationPolicyResponse,
): RegistrationSettingsValues {
  return {
    enabled: policy.enabled,
    defaultGroupId: String(policy.default_group_id),
    initialQuota: policy.initial_quota,
    emailRequired: policy.email_required,
    rateLimitAttempts: policy.rate_limit_attempts,
    rateLimitWindowSeconds: String(policy.rate_limit_window_seconds),
  }
}

/** 校验策略编辑值只落在后端允许的闭合边界。 */
export function buildRegistrationSettingsSchema(messages: SettingsValidationMessages) {
  return z.object({
    enabled: z.boolean(),
    defaultGroupId: z.string().refine(
      (value) => parsePositiveInteger(value) !== undefined,
      messages.invalidGroup,
    ),
    initialQuota: z.number()
      .int(messages.invalidQuota)
      .safe(messages.invalidQuota)
      .min(0, messages.invalidQuota),
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
  })
}

export function toRegistrationPolicyRequest(
  values: RegistrationSettingsValues,
): RegistrationPolicyRequest {
  const defaultGroupId = parsePositiveInteger(values.defaultGroupId)
  const rateLimitWindowSeconds = Number(values.rateLimitWindowSeconds)
  if (defaultGroupId === undefined || !Number.isSafeInteger(rateLimitWindowSeconds)) {
    throw new Error('注册策略整数未通过校验')
  }
  return {
    enabled: values.enabled,
    default_group_id: defaultGroupId,
    initial_quota: values.initialQuota,
    email_required: values.emailRequired,
    rate_limit_attempts: values.rateLimitAttempts,
    rate_limit_window_seconds: rateLimitWindowSeconds,
  }
}

function isValidUsername(value: string) {
  const bytes = new TextEncoder().encode(value).length
  return bytes > 0
    && bytes <= 64
    && value.trim() === value
    && !hasControlCharacter(value)
}

function isValidOptionalEmail(value: string) {
  if (value.length === 0) return true
  const bytes = new TextEncoder().encode(value).length
  if (
    bytes > 320
    || value.trim() !== value
    || hasControlCharacter(value)
    || [...value].some((character) => /\s/u.test(character))
  ) {
    return false
  }
  const parts = value.split('@')
  if (parts.length !== 2) return false
  const [local, domain] = parts
  return Boolean(
    local
    && domain
    && !local.startsWith('.')
    && !local.endsWith('.')
    && !domain.startsWith('.')
    && !domain.endsWith('.'),
  )
}

function isValidPassword(value: string) {
  const bytes = new TextEncoder().encode(value).length
  return bytes >= 12 && bytes <= 128 && !hasControlCharacter(value)
}

function hasControlCharacter(value: string) {
  return [...value].some((character) => /\p{Cc}/u.test(character))
}

function parsePositiveInteger(value: string) {
  if (!value) return undefined
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed >= 1 ? parsed : undefined
}
