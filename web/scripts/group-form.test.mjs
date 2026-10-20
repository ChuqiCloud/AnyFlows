import assert from 'node:assert/strict'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildGroupFormSchema,
  defaultGroupFormValues,
  ratioFromMicros,
  ratioToMicros,
  toGroupRequest,
} from '../src/features/groups/group-form-model.ts'
import { newPeakRule, peakRulesOverlap } from '../src/features/groups/group-peak-model.ts'

const messages = {
  invalidName: 'name',
  invalidDisplayName: 'displayName',
  invalidRatio: 'ratio',
  invalidNumber: 'number',
  invalidGroup: 'group',
  invalidPeakWindow: 'peakWindow',
  invalidWeekdays: 'weekdays',
  invalidModels: 'models',
  overlappingRules: 'overlap',
  invalidRules: 'rules',
  invalidTimezone: 'timezone',
}

test('倍率通过字符串算法精确转换为百万分整数', () => {
  assert.equal(ratioToMicros('1.234567'), 1_234_567)
  assert.equal(ratioToMicros('0.000001'), 1)
  assert.equal(ratioFromMicros(1_230_000), '1.23')
  for (const value of ['1.2345678', '1e2', '-1', '.5', '01']) {
    assert.equal(ratioToMicros(value), undefined, value)
  }
})

function pricingValues(rules = [newPeakRule()]) {
  return {
    ...defaultGroupFormValues(), name: 'standard', displayName: 'Standard',
    peakEnabled: true, peakRules: rules,
  }
}

test('周计划支持指定模型、折扣、时区和跨午夜', () => {
  const schema = buildGroupFormSchema(undefined, messages)
  const rule = { ...newPeakRule(), weekdays: [1, 3, 5], start: '23:00', end: '02:00',
    ratio: '0.5', modelScope: 'selected', models: 'deepseek-chat，deepseek-reasoner' }
  const values = pricingValues([rule])
  assert.equal(schema.safeParse(values).success, true)
  assert.deepEqual(toGroupRequest(values).peak_windows, [{
    weekdays: [1, 3, 5], start_second: 82800, end_second: 7200,
    ratio_micros: 500000, models: ['deepseek-chat', 'deepseek-reasoner'], utc_offset_minutes: 480,
  }])
  assert.equal(toGroupRequest(values).peak_start, null)
  assert.equal(toGroupRequest(values).peak_ratio_micros, null)
})

test('全天零倍率有效，关闭分时计费时显式提交空计划', () => {
  const values = pricingValues([{ ...newPeakRule(), start: '00:00', end: '24:00', ratio: '0' }])
  assert.equal(buildGroupFormSchema(undefined, messages).safeParse(values).success, true)
  assert.equal(toGroupRequest(values).peak_windows[0].end_second, 86400)
  assert.equal(toGroupRequest(values).peak_windows[0].ratio_micros, 0)
  assert.deepEqual(toGroupRequest({ ...values, peakEnabled: false }).peak_windows, [])
})

test('旧每日 UTC 配置转换为全星期和全部模型，空计划抑制旧配置', () => {
  const existing = { ratio_micros: 1000000, peak_ratio_micros: 1500000,
    peak_start: '08:00:00', peak_end: '20:00:00', flags: {} }
  const values = defaultGroupFormValues(existing)
  assert.equal(values.peakEnabled, true)
  assert.deepEqual(values.peakRules, [{ weekdays: [1, 2, 3, 4, 5, 6, 7], start: '08:00:00', end: '20:00:00',
    ratio: '1.5', modelScope: 'all', models: '', utcOffset: '0' }])
  assert.equal(defaultGroupFormValues({ ...existing, peak_windows: [] }).peakEnabled, false)
})

test('同一模型的冲突检测覆盖时区与周界，允许不同模型和相邻区间', () => {
  const sunday = { ...newPeakRule(), weekdays: [7], start: '23:00', end: '01:00', utcOffset: '0',
    modelScope: 'selected', models: 'deepseek-chat' }
  const monday = { ...sunday, weekdays: [1], start: '08:00', end: '10:00', utcOffset: '480' }
  assert.equal(peakRulesOverlap(sunday, monday), true)
  const schema = buildGroupFormSchema(undefined, messages)
  assert.equal(schema.safeParse(pricingValues([sunday, monday])).success, false)
  assert.equal(schema.safeParse(pricingValues([sunday, { ...monday, models: 'other-model' }])).success, true)
  assert.equal(schema.safeParse(pricingValues([sunday, { ...monday, modelScope: 'all' }])).success, false)
  assert.equal(schema.safeParse(pricingValues([sunday, { ...monday, start: '09:00' }])).success, true)
})

test('拒绝无星期、重复模型、空指定模型、无效时区和超限规则', () => {
  const schema = buildGroupFormSchema(undefined, messages)
  for (const override of [
    { weekdays: [] }, { weekdays: [1, 1] }, { weekdays: [8] },
    { start: '24:00' }, { end: '' }, { end: '08:00' }, { end: '25:00' },
    { ratio: '-1' }, { utcOffset: '841' }, { utcOffset: '1.5' },
    { modelScope: 'selected', models: '' },
    { modelScope: 'selected', models: 'ds\nds' },
    { modelScope: 'selected', models: 'x'.repeat(257) },
    { modelScope: 'selected', models: Array.from({ length: 65 }, (_, index) => `model-${index}`).join('\n') },
  ]) {
    assert.equal(schema.safeParse(pricingValues([{ ...newPeakRule(), ...override }])).success, false, JSON.stringify(override))
  }
  assert.equal(schema.safeParse(pricingValues([])).success, false)
  assert.equal(schema.safeParse(pricingValues(Array.from({ length: 33 }, newPeakRule))).success, false)
})

test('系统设置路由可直接访问分组管理页', () => {
  assert.deepEqual(routeFromHash('#/console/system-settings/groups'), { view: 'group-settings' })
})

test('编辑分组拒绝把回退目标指向自身', () => {
  const schema = buildGroupFormSchema(7, messages)
  const values = {
    ...defaultGroupFormValues(),
    name: 'standard',
    displayName: 'Standard',
    fallbackGroupId: '7',
  }
  assert.equal(schema.safeParse(values).success, false)
  assert.equal(schema.safeParse({ ...values, fallbackGroupId: '8' }).success, true)
})

test('空限额转换为 null 且未知 flags 在更新时保留', () => {
  const existing = {
    id: 7,
    name: 'standard',
    display_name: 'Standard',
    ratio_micros: 1_000_000,
    peak_ratio_micros: null,
    peak_start: null,
    peak_end: null,
    is_exclusive: false,
    daily_limit: null,
    weekly_limit: null,
    monthly_limit: null,
    rpm_limit: null,
    fallback_group_id: null,
    flags: { future_policy: { mode: 'strict' }, claude_code_only: false },
  }
  const request = toGroupRequest({
    ...defaultGroupFormValues(existing),
    dailyLimit: '',
    weeklyLimit: '',
    monthlyLimit: '',
    rpmLimit: '',
    claudeCodeOnly: true,
  }, existing)

  assert.equal(request.daily_limit, null)
  assert.equal(request.rpm_limit, null)
  assert.deepEqual(request.flags.future_policy, { mode: 'strict' })
  assert.equal(request.flags.claude_code_only, true)
})
