import { z } from 'zod'

import type { RegistrationRequestWritable } from '@/lib/api/generated/types.gen'

export type RegistrationFormValues = {
  username: string
  email: string
  verificationCode: string
  inviteCode: string
  password: string
  passwordConfirmation: string
}

type RegistrationValidationMessages = {
  invalidUsername: string
  invalidEmail: string
  emailRequired: string
  invalidVerificationCode: string
  verificationCodeRequired: string
  invalidInviteCode: string
  invalidPassword: string
  passwordMismatch: string
}

export const defaultRegistrationFormValues: RegistrationFormValues = {
  username: '',
  email: '',
  verificationCode: '',
  inviteCode: '',
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
    verificationCode: z.string().refine(
      (value) => value.length === 0 || isValidVerificationCode(value),
      messages.invalidVerificationCode,
    ),
    inviteCode: z.string().refine(
      (value) => value.length === 0 || /^af-[A-Za-z0-9_-]{22}$/u.test(value),
      messages.invalidInviteCode,
    ),
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
    if (emailRequired && values.email.length > 0 && values.verificationCode.length === 0) {
      context.addIssue({
        code: 'custom',
        message: messages.verificationCodeRequired,
        path: ['verificationCode'],
      })
    }
  }).refine(
    (values) => values.password === values.passwordConfirmation,
    { message: messages.passwordMismatch, path: ['passwordConfirmation'] },
  )
}

export function toRegistrationRequest(values: RegistrationFormValues, emailRequired: boolean, turnstileToken?: string): RegistrationRequestWritable {
  return {
    username: values.username,
    email: emailRequired ? (values.email || null) : null,
    verification_code: emailRequired ? (values.verificationCode || null) : null,
    invite_code: values.inviteCode || null,
    password: values.password,
    ...(turnstileToken ? { turnstile_token: turnstileToken } : {}),
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

function isValidVerificationCode(value: string) {
  return /^\d{6}$/u.test(value)
}

function hasControlCharacter(value: string) {
  return [...value].some((character) => /\p{Cc}/u.test(character))
}
