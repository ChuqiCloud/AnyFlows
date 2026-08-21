import { z } from 'zod'

import type {
  AdminSubscriptionPlanCreateRequest,
  SubscriptionCycle,
} from '@/lib/api/generated/types.gen'

export type SubscriptionPlanValues = {
  name: string
  quotaAmount: string
  cycle: SubscriptionCycle
  priceProvider: 'stripe' | 'epay'
  priceCurrency: string
  priceAmountMinor: string
}

type SubscriptionPlanValidationMessages = {
  name: string
  quotaAmount: string
  priceCurrency: string
  priceAmountMinor: string
}

/** 返回新建计划表单的稳定初值。 */
export function defaultSubscriptionPlanValues(): SubscriptionPlanValues {
  return {
    name: '',
    quotaAmount: '',
    cycle: 'monthly',
    priceProvider: 'stripe',
    priceCurrency: 'USD',
    priceAmountMinor: '100',
  }
}

/** 构建与服务端名称、整数额度边界一致的表单校验。 */
export function buildSubscriptionPlanSchema(messages: SubscriptionPlanValidationMessages) {
  return z.object({
    name: z.string().refine(validPlanName, messages.name),
    quotaAmount: z.string().refine(
      (value) => parsePositiveSafeInteger(value) !== undefined,
      messages.quotaAmount,
    ),
    cycle: z.enum(['daily', 'weekly', 'monthly', 'yearly']),
    priceProvider: z.enum(['stripe', 'epay']),
    priceCurrency: z.string().refine(validCurrency, messages.priceCurrency),
    priceAmountMinor: z.string().refine(
      (value) => parsePositiveSafeInteger(value) !== undefined,
      messages.priceAmountMinor,
    ),
  })
}

/** 将已校验的字符串输入转换为精确的 API 请求。 */
export function toSubscriptionPlanRequest(
  values: SubscriptionPlanValues,
): AdminSubscriptionPlanCreateRequest {
  const quotaAmount = parsePositiveSafeInteger(values.quotaAmount)
  const priceAmountMinor = parsePositiveSafeInteger(values.priceAmountMinor)
  if (
    !validPlanName(values.name)
    || quotaAmount === undefined
    || !['stripe', 'epay'].includes(values.priceProvider)
    || !validCurrency(values.priceCurrency)
    || priceAmountMinor === undefined
  ) {
    throw new Error('订阅计划表单尚未通过校验')
  }
  return {
    name: values.name,
    quota_amount: quotaAmount,
    cycle: values.cycle,
    price_provider: values.priceProvider,
    price_currency: values.priceCurrency,
    price_amount_minor: priceAmountMinor,
  }
}

/** 生成可跨网络重试复用的 128 位小写十六进制幂等键。 */
export function createSubscriptionIdempotencyKey() {
  const bytes = new Uint8Array(new ArrayBuffer(16))
  globalThis.crypto.getRandomValues(bytes)
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
}

function parsePositiveSafeInteger(value: string) {
  if (!/^[1-9]\d*$/.test(value)) return undefined
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : undefined
}

function validPlanName(value: string) {
  return value !== ''
    && value.trim() === value
    && new TextEncoder().encode(value).length <= 80
    && !Array.from(value).some((character) => {
      const codePoint = character.codePointAt(0)
      return codePoint !== undefined && (codePoint <= 0x1f || codePoint === 0x7f)
    })
}

function validCurrency(value: string) {
  return /^[A-Z]{3}$/.test(value)
}
