export const expressionPriceVariables = [
  { key: 'input', identifier: 'p', field: 'input' },
  { key: 'output', identifier: 'c', field: 'output' },
  { key: 'cacheRead', identifier: 'cr', field: 'cacheRead' },
  { key: 'cacheCreation5m', identifier: 'cc', field: 'cacheCreation5m' },
  { key: 'cacheCreation1h', identifier: 'cc1h', field: 'cacheCreation1h' },
] as const

export type ExpressionPriceVariable = (typeof expressionPriceVariables)[number]['key']

export type ExpressionVisualDraft = {
  tierName: string
  components: Record<ExpressionPriceVariable, { enabled: boolean; rate: string }>
}

const DECIMAL_PATTERN = /^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$/
const TIER_NAME_PATTERN = /^[A-Za-z0-9_-]{1,64}$/

function component(rate = '0', enabled = false) {
  return { rate, enabled }
}

/** 创建不携带外部证据的可视化初始值。 */
export function defaultExpressionVisualDraft(): ExpressionVisualDraft {
  return {
    tierName: 'base',
    components: {
      input: component('0', true),
      output: component('0', true),
      cacheRead: component(),
      cacheCreation5m: component(),
      cacheCreation1h: component(),
    },
  }
}

/** 只解析由可视化编辑器生成的线性正文，避免原始表达式被静默改写。 */
export function parseExpressionVisualDraft(source: string): ExpressionVisualDraft | undefined {
  const expression = source.trim().replace(/^v1:/, '')
  const match = /^tier\("([A-Za-z0-9_-]{1,64})",\s*(.+)\)$/.exec(expression)
  if (!match) return undefined
  const components = defaultExpressionVisualDraft().components
  for (const { key } of expressionPriceVariables) {
    components[key] = { ...components[key], enabled: false }
  }
  for (const term of match[2].split(/\s+\+\s+/)) {
    const termMatch = /^(p|c|cr|cc|cc1h)\s*\*\s*(0(?:\.[0-9]{1,28})?|[1-9][0-9]{0,9}(?:\.[0-9]{1,28})?)$/.exec(term.trim())
    if (!termMatch) return undefined
    const variable = expressionPriceVariables.find((item) => item.identifier === termMatch[1])
    if (!variable || components[variable.key].enabled) return undefined
    components[variable.key] = { enabled: true, rate: termMatch[2] }
  }
  return { tierName: match[1], components }
}

/** 把可视化草稿转换为带版本前缀的后端表达式正文。 */
export function expressionFromVisualDraft(draft: ExpressionVisualDraft) {
  if (!TIER_NAME_PATTERN.test(draft.tierName.trim())) return ''
  const terms = expressionPriceVariables
    .filter(({ key }) => draft.components[key].enabled)
    .map(({ key, identifier }) => `${identifier} * ${draft.components[key].rate.trim()}`)
  if (terms.length === 0 || terms.some((term) => !DECIMAL_PATTERN.test(term.split('*')[1].trim()))) return ''
  return `v1:tier(${JSON.stringify(draft.tierName.trim())}, ${terms.join(' + ')})`
}

export function expressionVisualDraftIsValid(draft: ExpressionVisualDraft) {
  return expressionFromVisualDraft(draft).length > 0
}

/** 仅用于展示当前正文引用的受控变量，不参与计费计算。 */
export function expressionVariables(source: string) {
  const identifiers = new Set(source.match(/\b(?:p|c|cr|cc|cc1h|len)\b/g) ?? [])
  return ['p', 'c', 'cr', 'cc', 'cc1h', 'len'].filter((identifier) => identifiers.has(identifier))
}

export function expressionSourceBytes(source: string) {
  return new TextEncoder().encode(source).byteLength
}
