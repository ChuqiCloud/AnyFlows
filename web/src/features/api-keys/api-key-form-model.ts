import { z } from 'zod'

import type { UserToken, UserTokenWriteRequest } from '@/lib/api/generated/types.gen'

const MAX_MODEL_COUNT = 512
const MAX_MODEL_BYTES = 256
const MAX_MODEL_TOTAL_BYTES = 32 * 1024
const MAX_IP_COUNT = 64
const MAX_IP_BYTES = 64
const MAX_IP_TOTAL_BYTES = 4096

const ipOrCidrSchema = z.union([z.ipv4(), z.ipv6(), z.cidrv4(), z.cidrv6()])

export type ApiKeyFormValues = {
  name: string
  status: 'enabled' | 'disabled'
  remainQuota: number
  unlimitedQuota: boolean
  expiredAt: string
  modelLimits: string[]
  allowIps: string[]
}

type ValidationMessages = {
  invalidField: string
  invalidModels: string
  invalidIps: string
  invalidDate: string
}

/** 前端复现公开容量边界，服务端仍是最终校验来源。 */
export function buildApiKeyFormSchema(messages: ValidationMessages) {
  return z.object({
    name: z.string().refine(isValidName, messages.invalidField),
    status: z.enum(['enabled', 'disabled']),
    remainQuota: z.number().int(messages.invalidField).safe(messages.invalidField).min(0, messages.invalidField),
    unlimitedQuota: z.boolean(),
    expiredAt: z.string().refine(isValidExpiration, messages.invalidDate),
    modelLimits: z.array(z.string()).refine(isValidModelList, messages.invalidModels),
    allowIps: z.array(z.string()).refine(isValidIpList, messages.invalidIps),
  })
}

export function defaultApiKeyValues(token?: UserToken): ApiKeyFormValues {
  return {
    name: token?.name ?? '',
    status: token?.status ?? 'enabled',
    remainQuota: token?.remain_quota ?? 0,
    unlimitedQuota: token?.unlimited_quota ?? true,
    expiredAt: formatLocalDateTime(token?.expired_at),
    modelLimits: token?.model_limits ? [...token.model_limits] : [],
    allowIps: token?.allow_ips ? [...token.allow_ips] : [],
  }
}

export function toApiKeyRequest(values: ApiKeyFormValues): UserTokenWriteRequest {
  return {
    name: values.name.trim(),
    status: values.status,
    remain_quota: values.remainQuota,
    unlimited_quota: values.unlimitedQuota,
    expired_at: values.expiredAt ? Math.floor(new Date(values.expiredAt).getTime() / 1000) : null,
    model_limits: values.modelLimits.length > 0 ? [...values.modelLimits] : null,
    allow_ips: values.allowIps.length > 0 ? [...values.allowIps] : null,
  }
}

export function apiKeyRequestWithStatus(
  token: UserToken,
  status: UserToken['status'],
): UserTokenWriteRequest {
  return {
    name: token.name,
    status,
    remain_quota: token.remain_quota,
    unlimited_quota: token.unlimited_quota,
    expired_at: token.expired_at,
    model_limits: token.model_limits ? [...token.model_limits] : null,
    allow_ips: token.allow_ips ? [...token.allow_ips] : null,
  }
}

export function isValidIpEntry(value: string) {
  return ipOrCidrSchema.safeParse(value).success
}

export function isValidIpList(values: string[]) {
  const encoder = new TextEncoder()
  const sizes = values.map((value) => encoder.encode(value).length)
  return values.length <= MAX_IP_COUNT
    && new Set(values).size === values.length
    && sizes.every((size) => size > 0 && size <= MAX_IP_BYTES)
    && sizes.reduce((total, size) => total + size, 0) <= MAX_IP_TOTAL_BYTES
    && values.every(isValidIpEntry)
}

function isValidModelList(values: string[]) {
  const encoder = new TextEncoder()
  const sizes = values.map((value) => encoder.encode(value).length)
  return values.length <= MAX_MODEL_COUNT
    && new Set(values).size === values.length
    && values.every((value) => value.length > 0 && value.trim() === value)
    && sizes.every((size) => size <= MAX_MODEL_BYTES)
    && sizes.reduce((total, size) => total + size, 0) <= MAX_MODEL_TOTAL_BYTES
}

function isValidName(value: string) {
  const normalized = value.trim()
  return normalized.length > 0
    && new TextEncoder().encode(normalized).length <= 128
    && ![...normalized].some((character) => /\p{Cc}/u.test(character))
}

function isValidExpiration(value: string) {
  if (!value) return true
  const timestamp = new Date(value).getTime()
  return Number.isFinite(timestamp) && timestamp >= 0
}

function formatLocalDateTime(value?: number | null) {
  if (value === null || value === undefined) return ''
  const date = new Date(value * 1000)
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000)
  return local.toISOString().slice(0, 16)
}
