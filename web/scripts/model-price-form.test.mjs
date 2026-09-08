import assert from 'node:assert/strict'
import test from 'node:test'

import {
  applyModelPriceEvidence,
  buildModelPriceDraftSchema,
  defaultModelPriceDraft,
  expressionModelPriceDraft,
  freeModelPriceDraft,
  modelPriceCandidateNeedsReview,
  modelPriceDraftIsComplete,
  toModelPriceWriteItem,
} from '../src/features/model-management/model-price-form-model.ts'
import {
  defaultExpressionVisualDraft,
  expressionFromVisualDraft,
  expressionVariables,
  parseExpressionVisualDraft,
} from '../src/features/model-management/model-price-expression-model.ts'
import {
  DEFAULT_EXPRESSION_PREVIEW_RATIOS,
  DEFAULT_EXPRESSION_PREVIEW_USAGE,
  buildExpressionPreviewRequest,
  expressionPreviewResultFields,
  formatPreviewRatioMicros,
  parsePreviewInteger,
  parsePreviewRatioMicros,
} from '../src/features/model-management/model-price-expression-preview-model.ts'

const messages = { decimal: 'decimal', free: 'free', expression: 'expression', expressionTooLong: 'expression too long', expressionValues: 'expression values' }

test('价格草稿只接受普通非负十进制字符串', () => {
  const schema = buildModelPriceDraftSchema(messages)
  const valid = {
    ...defaultModelPriceDraft(),
    input: '1.234567890123456789',
    output: '9.5',
    cacheRead: '0',
    cacheCreation5m: '2',
    cacheCreation1h: '3',
  }

  assert.equal(schema.safeParse(valid).success, true)
  for (const input of ['-1', '+1', '.5', '1.', '1e-3', '', '01', '10000000000', '0.12345678901234567890123456789']) {
    assert.equal(schema.safeParse({ ...valid, input }).success, false, input)
  }
})

test('显式免费要求五类价格全部为零', () => {
  const free = freeModelPriceDraft()
  assert.equal(modelPriceDraftIsComplete(free), true)
  assert.equal(modelPriceDraftIsComplete({ ...free, output: '0.1' }), false)
})

test('公开价表证据只按五类等价字段由管理员明确采用', () => {
  const draft = freeModelPriceDraft()
  const candidate = {
    model: 'gpt-test',
    provider: 'openai',
    provider_name: 'OpenAI',
    source_model: 'gpt-test',
    source_name: 'GPT Test',
    last_updated: null,
    source_deprecation_date: null,
    costs: {
      input: '1.25',
      output: '5',
      cache_read: '0.5',
      cache_write: null,
      cache_creation_5m: '1.5',
      cache_creation_1h: '2.5',
    },
    has_tiered_pricing: false,
    source_deprecated: false,
  }

  const fields = ['input', 'output', 'cacheRead', 'cacheCreation5m', 'cacheCreation1h']
  const applied = fields.reduce(
    (current, field) => applyModelPriceEvidence(current, candidate, field),
    draft,
  )
  assert.deepEqual(applied, {
    ...draft,
    input: '1.25',
    output: '5',
    cacheRead: '0.5',
    cacheCreation5m: '1.5',
    cacheCreation1h: '2.5',
  })
  assert.equal(modelPriceCandidateNeedsReview(candidate), false)
  assert.equal(modelPriceCandidateNeedsReview({
    ...candidate,
    source_deprecation_date: '2026-12-31',
  }), true)
  assert.equal(modelPriceCandidateNeedsReview({
    ...candidate,
    costs: { ...candidate.costs, cache_creation_1h: null },
  }), true)
})

test('写入请求保留正版本或显式创建前置条件', () => {
  const draft = freeModelPriceDraft()
  assert.equal(toModelPriceWriteItem('new-model', { draft, expectedVersion: null }).expected_version, null)
  assert.equal(toModelPriceWriteItem('priced-model', { draft, expectedVersion: 7 }).expected_version, 7)
})

test('表达式草稿固定零价列并单独提交正文', () => {
  const draft = expressionModelPriceDraft('v1:tier("base", p * 1.25 + c * 5)')
  assert.equal(modelPriceDraftIsComplete(draft), true)
  assert.equal(modelPriceDraftIsComplete({ ...draft, output: '0.1' }), false)
  assert.equal(modelPriceDraftIsComplete({ ...draft, expression: `v1:tier("${'计'.repeat(3000)}", p)` }), false)
  const request = toModelPriceWriteItem('expression-model', { draft, expectedVersion: 3 })
  assert.equal(request.billing_mode, 'expression')
  assert.equal(request.billing_expression, draft.expression)
  assert.deepEqual(request.prices, { input: '0', output: '0', cache_read: '0', cache_creation_5m: '0', cache_creation_1h: '0' })
})

test('可视化表达式只解析自身生成的线性正文并保留变量边界', () => {
  const visual = defaultExpressionVisualDraft()
  visual.components.input.rate = '1.25'
  visual.components.output.rate = '5'
  visual.components.cacheRead.enabled = true
  visual.components.cacheRead.rate = '0.5'
  const source = expressionFromVisualDraft(visual)
  assert.equal(source, 'v1:tier("base", p * 1.25 + c * 5 + cr * 0.5)')
  assert.deepEqual(expressionVariables(source), ['p', 'c', 'cr'])
  assert.deepEqual(parseExpressionVisualDraft(source), visual)
  assert.equal(parseExpressionVisualDraft('v1:tier("base", len > 100 ? tier("long", p) : tier("base", p))'), undefined)
})

test('试算倍率通过 BigInt 精确转换为百万分整数', () => {
  assert.equal(parsePreviewRatioMicros('1'), 1_000_000)
  assert.equal(parsePreviewRatioMicros('0.000001'), 1)
  assert.equal(parsePreviewRatioMicros('123.456789'), 123_456_789)
  assert.equal(formatPreviewRatioMicros(1_500_000), '1.5')
  assert.equal(formatPreviewRatioMicros(1), '0.000001')
  for (const value of ['-1', '+1', '01', '.5', '1.', '1.0000001', '1e3', '9007199254.740992']) {
    assert.equal(parsePreviewRatioMicros(value), undefined, value)
  }
})

test('试算请求只接受安全整数并保持结构化用量口径', () => {
  assert.equal(parsePreviewInteger('9007199254740991'), Number.MAX_SAFE_INTEGER)
  assert.equal(parsePreviewInteger('9007199254740992'), undefined)
  const request = buildExpressionPreviewRequest(
    '  v1:tier("base", p + c)  ',
    { ...DEFAULT_EXPRESSION_PREVIEW_USAGE, cacheReadTokens: '200', semantics: 'inclusive' },
    { ...DEFAULT_EXPRESSION_PREVIEW_RATIOS, group: '1.5', groupModel: '0.8', peak: '2' },
  )

  assert.deepEqual(request, {
    billing_expression: 'v1:tier("base", p + c)',
    usage: {
      input_tokens: 1000,
      output_tokens: 500,
      cache_read_tokens: 200,
      cache_creation_5m_tokens: 0,
      cache_creation_1h_tokens: 0,
      semantics: 'inclusive',
    },
    ratios: { group_micros: 1_500_000, group_model_micros: 800_000, peak_micros: 2_000_000 },
  })
  assert.equal(buildExpressionPreviewRequest(
    'tier("base", p)',
    { ...DEFAULT_EXPRESSION_PREVIEW_USAGE, inputTokens: '10', cacheReadTokens: '11' },
    DEFAULT_EXPRESSION_PREVIEW_RATIOS,
  ), undefined)
  assert.equal(buildExpressionPreviewRequest(
    'tier("base", len)',
    { ...DEFAULT_EXPRESSION_PREVIEW_USAGE, inputTokens: String(Number.MAX_SAFE_INTEGER), cacheReadTokens: '1', semantics: 'cache_separated' },
    DEFAULT_EXPRESSION_PREVIEW_RATIOS,
  ), undefined)
})

test('试算结果变量严格采用服务端返回值和稳定顺序', () => {
  const fields = expressionPreviewResultFields({
    matched_tier: 'base',
    base_usd: '0.0067',
    total_usd: '0.01608',
    quota: '8040',
    variables: {
      input_tokens: 800,
      output_tokens: 500,
      cache_read_tokens: 200,
      cache_creation_5m_tokens: 100,
      cache_creation_1h_tokens: 50,
      context_length_tokens: 1000,
    },
  })

  assert.deepEqual(fields, [
    { key: 'p', value: '800' },
    { key: 'c', value: '500' },
    { key: 'cr', value: '200' },
    { key: 'cc', value: '100' },
    { key: 'cc1h', value: '50' },
    { key: 'len', value: '1000' },
  ])
})
