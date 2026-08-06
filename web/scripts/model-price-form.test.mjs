import assert from 'node:assert/strict'
import test from 'node:test'

import {
  applyModelPriceEvidence,
  buildModelPriceDraftSchema,
  defaultModelPriceDraft,
  expressionModelPriceDraft,
  freeModelPriceDraft,
  modelPriceDraftIsComplete,
  toModelPriceWriteItem,
} from '../src/features/model-management/model-price-form-model.ts'
import {
  defaultExpressionVisualDraft,
  expressionFromVisualDraft,
  expressionVariables,
  parseExpressionVisualDraft,
} from '../src/features/model-management/model-price-expression-model.ts'

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

test('models.dev 证据只按等价字段由管理员明确采用', () => {
  const draft = freeModelPriceDraft()
  const candidate = {
    model: 'gpt-test',
    provider: 'openai',
    provider_name: 'OpenAI',
    source_model: 'gpt-test',
    source_name: 'GPT Test',
    last_updated: null,
    costs: { input: '1.25', output: '5', cache_read: '0.5', cache_write: '2' },
    has_tiered_pricing: false,
    source_deprecated: false,
  }

  const applied = applyModelPriceEvidence(draft, candidate, 'input')
  assert.equal(applied.input, '1.25')
  assert.equal(applied.cacheCreation5m, '0')
  assert.equal(applied.cacheCreation1h, '0')
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
