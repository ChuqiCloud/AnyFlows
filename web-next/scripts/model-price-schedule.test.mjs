import assert from 'node:assert/strict'
import test from 'node:test'

import {
  buildModelPriceDraftSchema,
  defaultModelPriceDraft,
  expressionModelPriceDraft,
  freeModelPriceDraft,
  toModelPriceWriteItem,
} from '../src/features/model-management/model-price-form-model.ts'
import { newModelPeakRule } from '../src/features/model-management/model-price-peak-model.ts'

const messages = {
  decimal: 'decimal', free: 'free', expression: 'expression', expressionTooLong: 'expression length', expressionValues: 'expression values',
  ratio: 'ratio', peakWindow: 'window', weekdays: 'weekdays', timezone: 'timezone', rules: 'rules', overlap: 'overlap',
}

function write(draft) {
  return toModelPriceWriteItem('scheduled-model', { draft, expectedVersion: 1, contextWindow: null })
}

test('模型规则精确往返时区、跨午夜、零倍率，模型范围固定为所属模型', () => {
  const peakRules = [{ ...newModelPeakRule(), weekdays: [1, 3], start: '23:00', end: '02:00', ratio: '0', utcOffset: '480' }]
  const draft = freeModelPriceDraft(peakRules)
  assert.equal(buildModelPriceDraftSchema(messages).safeParse(draft).success, true)
  const windows = write(draft).peak_windows
  assert.deepEqual(windows, [{
    weekdays: [1, 3], start_second: 82800, end_second: 7200, ratio_micros: 0, models: [], utc_offset_minutes: 480,
  }])
  const restored = defaultModelPriceDraft({ billing_mode: 'free', prices: write(draft).prices, peak_windows: windows })
  assert.deepEqual(restored.peakRules, [{ ...peakRules[0], start: '23:00:00', end: '02:00:00' }])
})

test('模型不配置和清空规则均显式提交空计划以继承分组', () => {
  assert.deepEqual(defaultModelPriceDraft().peakRules, [])
  assert.deepEqual(defaultModelPriceDraft({ prices: write(freeModelPriceDraft()).prices, peak_windows: null }).peakRules, [])
  assert.deepEqual(write(freeModelPriceDraft()).peak_windows, [])
  const rules = [{ ...newModelPeakRule(), ratio: '1', start: '00:00', end: '24:00' }]
  assert.equal(write(freeModelPriceDraft(rules)).peak_windows[0].ratio_micros, 1_000_000)
  assert.deepEqual(expressionModelPriceDraft('v1:tier("base", p)', rules).peakRules, rules)
})

test('模型规则拒绝无效倍率、时间、星期、时区以及周界重叠', () => {
  const schema = buildModelPriceDraftSchema(messages)
  for (const override of [
    { ratio: '-1' }, { ratio: '1.0000001' }, { weekdays: [] }, { weekdays: [1, 1] }, { weekdays: [8] },
    { start: '24:00' }, { end: '08:00' }, { utcOffset: '841' }, { utcOffset: '1.5' },
  ]) {
    assert.equal(schema.safeParse(freeModelPriceDraft([{ ...newModelPeakRule(), ...override }])).success, false, JSON.stringify(override))
  }
  const sunday = { ...newModelPeakRule(), weekdays: [7], start: '23:00', end: '01:00', utcOffset: '0' }
  const monday = { ...newModelPeakRule(), weekdays: [1], start: '08:00', end: '10:00', utcOffset: '480' }
  assert.equal(schema.safeParse(freeModelPriceDraft([sunday, monday])).success, false)
  assert.equal(schema.safeParse(freeModelPriceDraft([sunday, { ...monday, start: '09:00' }])).success, true)
  assert.equal(schema.safeParse(freeModelPriceDraft(Array.from({ length: 33 }, newModelPeakRule))).success, false)
})
