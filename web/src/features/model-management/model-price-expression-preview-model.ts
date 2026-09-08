import type {
  AdminModelPriceExpressionPreviewRequest,
  AdminModelPriceExpressionPreviewResponse,
} from '@/lib/api/generated/types.gen'

export type ExpressionPreviewSemantics = 'inclusive' | 'cache_separated'

export type ExpressionPreviewUsageDraft = {
  inputTokens: string
  outputTokens: string
  cacheReadTokens: string
  cacheCreation5mTokens: string
  cacheCreation1hTokens: string
  semantics: ExpressionPreviewSemantics
}

export type ExpressionPreviewRatiosDraft = {
  group: string
  groupModel: string
  peak: string
}

export const DEFAULT_EXPRESSION_PREVIEW_USAGE: ExpressionPreviewUsageDraft = {
  inputTokens: '1000',
  outputTokens: '500',
  cacheReadTokens: '0',
  cacheCreation5mTokens: '0',
  cacheCreation1hTokens: '0',
  semantics: 'inclusive',
}

export const DEFAULT_EXPRESSION_PREVIEW_RATIOS: ExpressionPreviewRatiosDraft = {
  group: '1',
  groupModel: '1',
  peak: '1',
}

const MAX_SAFE_INTEGER = BigInt(Number.MAX_SAFE_INTEGER)
const UNSIGNED_INTEGER_PATTERN = /^(0|[1-9][0-9]*)$/
const RATIO_PATTERN = /^(0|[1-9][0-9]*)(\.[0-9]{1,6})?$/

/** 将表单中的非负整数精确转换为 OpenAPI 可安全承载的 JavaScript 数字。 */
export function parsePreviewInteger(value: string): number | undefined {
  const normalized = value.trim()
  if (!UNSIGNED_INTEGER_PATTERN.test(normalized)) return undefined
  try {
    const parsed = BigInt(normalized)
    return parsed <= MAX_SAFE_INTEGER ? Number(parsed) : undefined
  } catch {
    return undefined
  }
}

/** 将十进制倍率精确转换为百万分整数，不经过 JavaScript 浮点计算。 */
export function parsePreviewRatioMicros(value: string): number | undefined {
  const normalized = value.trim()
  if (!RATIO_PATTERN.test(normalized)) return undefined
  const [integer, fraction = ''] = normalized.split('.')
  const microsText = `${integer}${fraction.padEnd(6, '0')}`
  try {
    const parsed = BigInt(microsText)
    return parsed <= MAX_SAFE_INTEGER ? Number(parsed) : undefined
  } catch {
    return undefined
  }
}

/** 把服务端百万分整数格式化为可继续编辑的精确十进制文本。 */
export function formatPreviewRatioMicros(micros: number): string {
  if (!Number.isSafeInteger(micros) || micros < 0) return ''
  const text = BigInt(micros).toString().padStart(7, '0')
  const integer = text.slice(0, -6)
  const fraction = text.slice(-6).replace(/0+$/, '')
  return fraction ? `${integer}.${fraction}` : integer
}

/** 构造服务端试算请求；任何未通过精确边界的字段都会阻止发送。 */
export function buildExpressionPreviewRequest(
  source: string,
  usage: ExpressionPreviewUsageDraft,
  ratios: ExpressionPreviewRatiosDraft,
): AdminModelPriceExpressionPreviewRequest | undefined {
  const tokenValues = [
    usage.inputTokens,
    usage.outputTokens,
    usage.cacheReadTokens,
    usage.cacheCreation5mTokens,
    usage.cacheCreation1hTokens,
  ].map(parsePreviewInteger)
  const ratioValues = [ratios.group, ratios.groupModel, ratios.peak].map(parsePreviewRatioMicros)
  if (!source.trim() || tokenValues.some((value) => value === undefined) || ratioValues.some((value) => value === undefined)) {
    return undefined
  }
  const [input_tokens, output_tokens, cache_read_tokens, cache_creation_5m_tokens, cache_creation_1h_tokens] = tokenValues as number[]
  const [group_micros, group_model_micros, peak_micros] = ratioValues as number[]
  const cacheTokens = BigInt(cache_read_tokens) + BigInt(cache_creation_5m_tokens) + BigInt(cache_creation_1h_tokens)
  if (
    (usage.semantics === 'inclusive' && cacheTokens > BigInt(input_tokens))
    || (usage.semantics === 'cache_separated' && BigInt(input_tokens) + cacheTokens > MAX_SAFE_INTEGER)
  ) {
    return undefined
  }
  return {
    billing_expression: source.trim(),
    usage: {
      input_tokens,
      output_tokens,
      cache_read_tokens,
      cache_creation_5m_tokens,
      cache_creation_1h_tokens,
      semantics: usage.semantics,
    },
    ratios: { group_micros, group_model_micros, peak_micros },
  }
}

export function expressionPreviewRequestIsComplete(
  source: string,
  usage: ExpressionPreviewUsageDraft,
  ratios: ExpressionPreviewRatiosDraft,
) {
  return buildExpressionPreviewRequest(source, usage, ratios) !== undefined
}

export type ExpressionPreviewResultField = {
  key: string
  value: string
}

/** 将服务端规范化变量整理成稳定的展示顺序，避免前端自行推导计费值。 */
export function expressionPreviewResultFields(
  result: AdminModelPriceExpressionPreviewResponse,
): ExpressionPreviewResultField[] {
  const fields = [
    ['p', result.variables.input_tokens],
    ['c', result.variables.output_tokens],
    ['cr', result.variables.cache_read_tokens],
    ['cc', result.variables.cache_creation_5m_tokens],
    ['cc1h', result.variables.cache_creation_1h_tokens],
    ['len', result.variables.context_length_tokens],
  ] as const
  return fields.map(([key, value]) => ({ key, value: String(value) }))
}
