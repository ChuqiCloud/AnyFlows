import { z } from 'zod'

import type {
  AdminSubscriptionPlanCreateRequest,
  SubscriptionCycle,
} from '@/lib/api/generated/types.gen'

export type SubscriptionPlanValues = {
  name: string
  quotaAmount: string
  cycle: SubscriptionCycle
}

type SubscriptionPlanValidationMessages = {
  name: string
  quotaAmount: string
}

/** 返回新建计划表单的稳定初值。 */
export function defaultSubscriptionPlanValues(): SubscriptionPlanValues {
  return {
    name: '',
    quotaAmount: '',
    cycle: 'monthly',
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
  })
}

/** 将已校验的字符串输入转换为精确的 API 请求。 */
export function toSubscriptionPlanRequest(
  values: SubscriptionPlanValues,
): AdminSubscriptionPlanCreateRequest {
  const quotaAmount = parsePositiveSafeInteger(values.quotaAmount)
  if (!validPlanName(values.name) || quotaAmount === undefined) {
    throw new Error('订阅计划表单尚未通过校验')
  }
  return {
    name: values.name,
    quota_amount: quotaAmount,
    cycle: values.cycle,
  }
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
