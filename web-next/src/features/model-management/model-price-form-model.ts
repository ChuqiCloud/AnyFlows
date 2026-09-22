import { z } from 'zod'

import type {
  AdminModelPrice,
  AdminModelPriceBillingMode,
  AdminModelPriceSourceCandidate,
  AdminModelPriceWriteItem,
} from '@/lib/api/generated/types.gen'

export type ModelPriceDraft = {
  billingMode: AdminModelPriceBillingMode
  input: string
  output: string
  cacheRead: string
  cacheCreation5m: string
  cacheCreation1h: string
  expression: string
}

export type StagedModelPriceDraft = {
  draft: ModelPriceDraft
  expectedVersion: number | null
  contextWindow: number | null
}

export type ModelPriceValidationMessages = {
  decimal: string
  free: string
  expression: string
  expressionTooLong: string
  expressionValues: string
}

export type ModelPriceEvidenceField =
  | 'input'
  | 'output'
  | 'cacheRead'
  | 'cacheCreation5m'
  | 'cacheCreation1h'

const DECIMAL_PATTERN = /^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$/
const ZERO_DECIMAL_PATTERN = /^0+(\.0+)?$/
const MAX_EXPRESSION_BYTES = 8 * 1024

/** 创建与后端十进制字符串边界一致的价格草稿校验器。 */
export function buildModelPriceDraftSchema(messages: ModelPriceValidationMessages) {
  const decimal = z.string().max(39, messages.decimal).regex(DECIMAL_PATTERN, messages.decimal)
  return z.object({
    billingMode: z.enum(['per_token', 'free', 'expression']),
    input: decimal,
    output: decimal,
    cacheRead: decimal,
    cacheCreation5m: decimal,
    cacheCreation1h: decimal,
    expression: z.string().max(8192, messages.expression),
  }).superRefine((value, context) => {
    if (value.billingMode === 'expression') {
      if (value.expression.trim().length === 0) {
        context.addIssue({ code: 'custom', message: messages.expression, path: ['expression'] })
      }
      if (new TextEncoder().encode(value.expression).byteLength > MAX_EXPRESSION_BYTES) {
        context.addIssue({ code: 'custom', message: messages.expressionTooLong, path: ['expression'] })
      }
      const fields = ['input', 'output', 'cacheRead', 'cacheCreation5m', 'cacheCreation1h'] as const
      for (const field of fields) {
        if (!ZERO_DECIMAL_PATTERN.test(value[field])) {
          context.addIssue({ code: 'custom', message: messages.expressionValues, path: [field] })
        }
      }
      return
    }
    if (value.billingMode === 'free') {
      const fields = ['input', 'output', 'cacheRead', 'cacheCreation5m', 'cacheCreation1h'] as const
      for (const field of fields) {
        if (!ZERO_DECIMAL_PATTERN.test(value[field])) {
          context.addIssue({ code: 'custom', message: messages.free, path: [field] })
        }
      }
    }
  })
}

export function defaultModelPriceDraft(price?: AdminModelPrice): ModelPriceDraft {
  return {
    billingMode: price?.billing_mode ?? 'per_token',
    input: price?.prices.input ?? '',
    output: price?.prices.output ?? '',
    cacheRead: price?.prices.cache_read ?? '',
    cacheCreation5m: price?.prices.cache_creation_5m ?? '',
    cacheCreation1h: price?.prices.cache_creation_1h ?? '',
    expression: price?.billing_expression ?? '',
  }
}

export function freeModelPriceDraft(): ModelPriceDraft {
  return {
    billingMode: 'free',
    input: '0',
    output: '0',
    cacheRead: '0',
    cacheCreation5m: '0',
    cacheCreation1h: '0',
    expression: '',
  }
}

/** 表达式模式固定使用零价列，避免旧的五价语义参与动态结算。 */
export function expressionModelPriceDraft(expression = ''): ModelPriceDraft {
  return {
    billingMode: 'expression',
    input: '0',
    output: '0',
    cacheRead: '0',
    cacheCreation5m: '0',
    cacheCreation1h: '0',
    expression,
  }
}

/** 仅用于控制选择状态，显示文案由表单使用本地化校验器提供。 */
export function modelPriceDraftIsComplete(draft?: ModelPriceDraft) {
  if (!draft) return false
  return buildModelPriceDraftSchema({ decimal: 'decimal', free: 'free', expression: 'expression', expressionTooLong: 'expression too long', expressionValues: 'expression values' }).safeParse(draft).success
}

export function toModelPriceWriteItem(
  model: string,
  staged: StagedModelPriceDraft,
): AdminModelPriceWriteItem {
  const { draft } = staged
  return {
    model,
    expected_version: staged.expectedVersion,
    context_window: staged.contextWindow,
    billing_mode: draft.billingMode,
    prices: {
      input: draft.input,
      output: draft.output,
      cache_read: draft.cacheRead,
      cache_creation_5m: draft.cacheCreation5m,
      cache_creation_1h: draft.cacheCreation1h,
    },
    billing_expression: draft.billingMode === 'expression' ? draft.expression.trim() : null,
  }
}

/** 只允许语义等价的五类公开价格证据进入对应正式字段。 */
export function applyModelPriceEvidence(
  draft: ModelPriceDraft,
  candidate: AdminModelPriceSourceCandidate,
  field: ModelPriceEvidenceField,
): ModelPriceDraft {
  const sourceKey = {
    input: 'input',
    output: 'output',
    cacheRead: 'cache_read',
    cacheCreation5m: 'cache_creation_5m',
    cacheCreation1h: 'cache_creation_1h',
  } as const
  const value = candidate.costs[sourceKey[field]]
  return value === null ? draft : { ...draft, [field]: value }
}

/** 一次采用来源中可等价映射的价格字段，未提供的维度保持当前草稿。 */
export function applyAllModelPriceEvidence(
  draft: ModelPriceDraft,
  candidate: AdminModelPriceSourceCandidate,
): ModelPriceDraft {
  return {
    ...draft,
    input: candidate.costs.input ?? draft.input,
    output: candidate.costs.output ?? draft.output,
    cacheRead: candidate.costs.cache_read ?? draft.cacheRead,
    cacheCreation5m: candidate.costs.cache_creation_5m ?? draft.cacheCreation5m,
    cacheCreation1h: candidate.costs.cache_creation_1h ?? draft.cacheCreation1h,
  }
}

export function modelPriceCandidateNeedsReview(candidate?: AdminModelPriceSourceCandidate) {
  return candidate !== undefined && (
    candidate.has_tiered_pricing
    || candidate.source_deprecated
    || candidate.source_deprecation_date !== null
    || candidate.costs.input === null
    || candidate.costs.output === null
    || candidate.costs.cache_read === null
    || candidate.costs.cache_creation_5m === null
    || candidate.costs.cache_creation_1h === null
    || candidate.costs.cache_write !== null
  )
}
