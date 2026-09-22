import { z } from 'zod'

import type { AdminToken, AdminTokenWriteRequest } from '@/lib/api/generated/types.gen'

export type TokenEditorMode = 'create' | 'update'

export type TokenFormValues = {
  userId: number
  name: string
  status: 'enabled' | 'disabled'
  groupId: string
  remainQuota: number
  unlimitedQuota: boolean
  expiredAt: string
  modelLimits: string
  allowIps: string
  crossGroupRetry: boolean
  rateLimit5h: string
  rateLimit1d: string
  rateLimit7d: string
  maxRequests: string
}

type ValidationMessages = {
  invalidField: string
  invalidModels: string
  invalidIps: string
  invalidDate: string
}

const ipOrCidrSchema = z.union([z.ipv4(), z.ipv6(), z.cidrv4(), z.cidrv6()])

/** 前端复现后端的容量与格式边界，提交时仍以后端校验为最终准则。 */
export function buildTokenFormSchema(messages: ValidationMessages) {
  return z.object({
    userId: z.number().int(messages.invalidField).safe(messages.invalidField).min(1, messages.invalidField),
    name: z.string().refine(isValidName, messages.invalidField),
    status: z.enum(['enabled', 'disabled']),
    groupId: z.string().refine((value) => parseOptionalInteger(value, 1) !== undefined, messages.invalidField),
    remainQuota: z.number().int(messages.invalidField).safe(messages.invalidField).min(0, messages.invalidField),
    unlimitedQuota: z.boolean(),
    expiredAt: z.string().refine(isValidExpiration, messages.invalidDate),
    modelLimits: z.string().refine(isValidModelLimits, messages.invalidModels),
    allowIps: z.string().refine(isValidIpAllowlist, messages.invalidIps),
    crossGroupRetry: z.boolean(),
    rateLimit5h: optionalNonNegativeInteger(messages.invalidField),
    rateLimit1d: optionalNonNegativeInteger(messages.invalidField),
    rateLimit7d: optionalNonNegativeInteger(messages.invalidField),
    maxRequests: optionalNonNegativeInteger(messages.invalidField),
  })
}

export function defaultTokenValues(token?: AdminToken): TokenFormValues {
  return {
    userId: token?.user_id ?? 1,
    name: token?.name ?? '',
    status: token?.status ?? 'enabled',
    groupId: formatOptionalInteger(token?.group_id),
    remainQuota: token?.remain_quota ?? 0,
    unlimitedQuota: token?.unlimited_quota ?? false,
    expiredAt: formatLocalDateTime(token?.expired_at),
    modelLimits: token?.model_limits?.join('\n') ?? '',
    allowIps: token?.allow_ips?.join('\n') ?? '',
    crossGroupRetry: token?.cross_group_retry ?? false,
    rateLimit5h: formatOptionalInteger(token?.rate_limit_5h),
    rateLimit1d: formatOptionalInteger(token?.rate_limit_1d),
    rateLimit7d: formatOptionalInteger(token?.rate_limit_7d),
    maxRequests: formatOptionalInteger(token?.max_requests),
  }
}

export function toTokenRequest(values: TokenFormValues): AdminTokenWriteRequest {
  return {
    user_id: values.userId,
    name: values.name.trim(),
    status: values.status,
    group_id: requireOptionalInteger(values.groupId, 1),
    remain_quota: values.remainQuota,
    unlimited_quota: values.unlimitedQuota,
    expired_at: values.expiredAt ? Math.floor(new Date(values.expiredAt).getTime() / 1000) : null,
    model_limits: optionalList(values.modelLimits),
    allow_ips: optionalList(values.allowIps),
    cross_group_retry: values.crossGroupRetry,
    rate_limit_5h: requireOptionalInteger(values.rateLimit5h, 0),
    rate_limit_1d: requireOptionalInteger(values.rateLimit1d, 0),
    rate_limit_7d: requireOptionalInteger(values.rateLimit7d, 0),
    max_requests: requireOptionalInteger(values.maxRequests, 0),
  }
}

function optionalNonNegativeInteger(message: string) {
  return z.string().refine((value) => parseOptionalInteger(value, 0) !== undefined, message)
}

function parseOptionalInteger(value: string, minimum: number): number | null | undefined {
  if (!value.trim()) return null
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed >= minimum ? parsed : undefined
}

function requireOptionalInteger(value: string, minimum: number) {
  const parsed = parseOptionalInteger(value, minimum)
  return parsed === undefined ? null : parsed
}

function optionalList(value: string) {
  const items = splitList(value)
  return items.length > 0 ? items : null
}

function splitList(value: string) {
  return value.split(/[\n,]/).map((item) => item.trim()).filter(Boolean)
}

function isValidName(value: string) {
  const normalized = value.trim()
  return normalized.length > 0
    && new TextEncoder().encode(normalized).length <= 128
    && ![...normalized].some((character) => /\p{Cc}/u.test(character))
}

function isValidModelLimits(value: string) {
  const models = splitList(value)
  if (models.length === 0) return true
  const encoder = new TextEncoder()
  const sizes = models.map((model) => encoder.encode(model).length)
  return models.length <= 512
    && sizes.every((size) => size > 0 && size <= 256)
    && sizes.reduce((total, size) => total + size, 0) <= 32 * 1024
}

function isValidIpAllowlist(value: string) {
  const entries = splitList(value)
  if (entries.length === 0) return true
  const encoder = new TextEncoder()
  const sizes = entries.map((entry) => encoder.encode(entry).length)
  return entries.length <= 64
    && sizes.every((size) => size > 0 && size <= 64)
    && sizes.reduce((total, size) => total + size, 0) <= 4096
    && entries.every((entry) => ipOrCidrSchema.safeParse(entry).success)
}

function isValidExpiration(value: string) {
  if (!value) return true
  const timestamp = new Date(value).getTime()
  return Number.isFinite(timestamp) && timestamp >= 0
}

function formatOptionalInteger(value?: number | null) {
  return value === null || value === undefined ? '' : String(value)
}

function formatLocalDateTime(value?: number | null) {
  if (value === null || value === undefined) return ''
  const date = new Date(value * 1000)
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000)
  return local.toISOString().slice(0, 16)
}
