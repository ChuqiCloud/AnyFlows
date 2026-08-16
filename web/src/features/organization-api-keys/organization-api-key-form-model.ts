import { z } from 'zod'

import {
  buildApiKeyFormSchema,
  defaultApiKeyValues,
  toApiKeyRequest,
  type ApiKeyFormValues,
} from '../api-keys/api-key-form-model.ts'
import type {
  OrganizationTokenResponse,
  OrganizationTokenWriteRequest,
} from '@/lib/api/generated/types.gen'

export type OrganizationApiKeyFormValues = ApiKeyFormValues & {
  rateLimit5h: string
  rateLimit1d: string
  rateLimit7d: string
  maxRequests: string
}

/** 在个人 Key 公共边界上增加企业 Key 的多窗口与请求次数限制。 */
export function buildOrganizationApiKeyFormSchema(messages: {
  invalidField: string
  invalidModels: string
  invalidIps: string
  invalidDate: string
}) {
  return buildApiKeyFormSchema(messages).extend({
    rateLimit5h: optionalNonNegativeInteger(messages.invalidField),
    rateLimit1d: optionalNonNegativeInteger(messages.invalidField),
    rateLimit7d: optionalNonNegativeInteger(messages.invalidField),
    maxRequests: optionalNonNegativeInteger(messages.invalidField),
  })
}

export function defaultOrganizationApiKeyValues(
  token?: OrganizationTokenResponse,
): OrganizationApiKeyFormValues {
  return {
    ...defaultApiKeyValues(token),
    rateLimit5h: formatOptionalInteger(token?.rate_limit_5h),
    rateLimit1d: formatOptionalInteger(token?.rate_limit_1d),
    rateLimit7d: formatOptionalInteger(token?.rate_limit_7d),
    maxRequests: formatOptionalInteger(token?.max_requests),
  }
}

export function toOrganizationApiKeyRequest(
  values: OrganizationApiKeyFormValues,
): OrganizationTokenWriteRequest {
  return {
    ...toApiKeyRequest(values),
    rate_limit_5h: requireOptionalInteger(values.rateLimit5h),
    rate_limit_1d: requireOptionalInteger(values.rateLimit1d),
    rate_limit_7d: requireOptionalInteger(values.rateLimit7d),
    max_requests: requireOptionalInteger(values.maxRequests),
  }
}

function optionalNonNegativeInteger(message: string) {
  return z.string().refine((value) => parseOptionalInteger(value) !== undefined, message)
}

function parseOptionalInteger(value: string): number | null | undefined {
  if (!value.trim()) return null
  if (!/^\d+$/.test(value)) return undefined
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) ? parsed : undefined
}

function requireOptionalInteger(value: string) {
  const parsed = parseOptionalInteger(value)
  return parsed === undefined ? null : parsed
}

function formatOptionalInteger(value?: number | null) {
  return value === null || value === undefined ? '' : String(value)
}
