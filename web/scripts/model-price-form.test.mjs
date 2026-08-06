import assert from 'node:assert/strict'
import test from 'node:test'

import {
  applyModelPriceEvidence,
  buildModelPriceDraftSchema,
  defaultModelPriceDraft,
  freeModelPriceDraft,
  modelPriceCanUseFixedEditor,
  modelPriceDraftIsComplete,
  toModelPriceWriteItem,
} from '../src/features/model-management/model-price-form-model.ts'

const messages = { decimal: 'decimal', free: 'free' }

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

test('五价表单拒绝覆盖表达式价格', () => {
  assert.equal(modelPriceCanUseFixedEditor(), true)
  assert.equal(modelPriceCanUseFixedEditor({ billing_mode: 'per_token' }), true)
  assert.equal(modelPriceCanUseFixedEditor({ billing_mode: 'expression' }), false)
})
