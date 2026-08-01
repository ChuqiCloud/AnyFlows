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

const messages = {
  invalidName: 'name',
  invalidDisplayName: 'displayName',
  invalidRatio: 'ratio',
  invalidNumber: 'number',
  invalidGroup: 'group',
  invalidPeakWindow: 'peakWindow',
}

test('倍率通过字符串算法精确转换为百万分整数', () => {
  assert.equal(ratioToMicros('1.234567'), 1_234_567)
  assert.equal(ratioToMicros('0.000001'), 1)
  assert.equal(ratioFromMicros(1_230_000), '1.23')
  for (const value of ['1.2345678', '1e2', '-1', '.5', '01']) {
    assert.equal(ratioToMicros(value), undefined, value)
  }
})

test('高峰倍率启用后要求倍率和 UTC 窗口三字段闭合', () => {
  const schema = buildGroupFormSchema(undefined, messages)
  const values = {
    ...defaultGroupFormValues(),
    name: 'standard',
    displayName: 'Standard',
    peakEnabled: true,
    peakRatio: '1.5',
    peakStart: '08:00',
    peakEnd: '20:00',
  }
  assert.equal(schema.safeParse(values).success, true)
  assert.equal(schema.safeParse({ ...values, peakEnd: '' }).success, false)
  assert.equal(schema.safeParse({ ...values, peakEnd: '08:00' }).success, false)
  assert.equal(schema.safeParse({ ...values, peakStart: '20:00:00' }).success, false)
  assert.equal(toGroupRequest(values).peak_start, '08:00:00')
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
