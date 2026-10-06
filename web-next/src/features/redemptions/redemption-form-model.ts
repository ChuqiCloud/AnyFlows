import { z } from 'zod'

import type { AdminRedemptionBatchCreateRequest } from '@/lib/api/generated/types.gen'

export type RedemptionBatchValues = {
  name: string
  quotaAmount: string
  codeCount: string
  expires: boolean
  expiresAt: string
}

type RedemptionValidationMessages = {
  name: string
  quotaAmount: string
  codeCount: string
  expiresAt: string
}

export function defaultRedemptionBatchValues(now = new Date()): RedemptionBatchValues {
  return {
    name: '',
    quotaAmount: '',
    codeCount: '10',
    expires: false,
    expiresAt: defaultExpirationText(now),
  }
}

export function buildRedemptionBatchSchema(messages: RedemptionValidationMessages) {
  return z.object({
    name: z.string().refine(validName, messages.name),
    quotaAmount: z.string().refine((value) => parsePositiveSafeInteger(value) !== undefined, messages.quotaAmount),
    codeCount: z.string().refine((value) => {
      const count = parsePositiveSafeInteger(value)
      return count !== undefined && count <= 1_000
    }, messages.codeCount),
    expires: z.boolean(),
    expiresAt: z.string(),
  }).superRefine((values, context) => {
    if (values.expires && expirationUnixSeconds(values.expiresAt) === undefined) {
      context.addIssue({
        code: 'custom',
        message: messages.expiresAt,
        path: ['expiresAt'],
      })
    }
  })
}

export function toRedemptionBatchRequest(
  values: RedemptionBatchValues,
  nowMilliseconds = Date.now(),
): AdminRedemptionBatchCreateRequest {
  const quotaAmount = parsePositiveSafeInteger(values.quotaAmount)
  const codeCount = parsePositiveSafeInteger(values.codeCount)
  const expiresAt = values.expires
    ? expirationUnixSeconds(values.expiresAt, nowMilliseconds)
    : null
  if (
    !validName(values.name)
    || quotaAmount === undefined
    || codeCount === undefined
    || codeCount > 1_000
    || (values.expires && expiresAt === undefined)
  ) {
    throw new Error('兑换码批次表单尚未通过校验')
  }
  return {
    name: values.name,
    quota_amount: quotaAmount,
    code_count: codeCount,
    expires_at: expiresAt ?? null,
  }
}

export function expirationUnixSeconds(value: string, nowMilliseconds = Date.now()) {
  if (value === '') return undefined
  const milliseconds = new Date(value).getTime()
  if (!Number.isFinite(milliseconds)) return undefined
  const seconds = Math.floor(milliseconds / 1_000)
  return seconds > Math.floor(nowMilliseconds / 1_000) && Number.isSafeInteger(seconds)
    ? seconds
    : undefined
}

function parsePositiveSafeInteger(value: string) {
  if (!/^[1-9]\d*$/.test(value)) return undefined
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : undefined
}

function validName(value: string) {
  return value !== ''
    && value.trim() === value
    && new TextEncoder().encode(value).length <= 80
    && !/[\u0000-\u001f\u007f]/.test(value)
}

function defaultExpirationText(now: Date) {
  const next = new Date(now.getTime() + 30 * 24 * 60 * 60 * 1_000)
  const local = new Date(next.getTime() - next.getTimezoneOffset() * 60_000)
  return local.toISOString().slice(0, 16)
}
