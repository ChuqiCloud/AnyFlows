import assert from 'node:assert/strict'
import test from 'node:test'

import {
  DEFAULT_QUOTA_DISPLAY_POLICY,
  balanceDisplayScaleChanged,
  formatQuota,
} from '../src/lib/quota-display.ts'

const custom = {
  mode: 'custom_unit',
  unit_name: '算力积分',
  unit_symbol: '积分',
  quota_units_per_display_unit: '10000',
  symbol_position: 'suffix',
  fraction_digits: 2,
}

test('原始额度模式保持历史整数展示', () => {
  assert.equal(formatQuota(20_000_000, DEFAULT_QUOTA_DISPLAY_POLICY, 'en-US'), '20,000,000')
})

test('自定义面额使用整数除法和固定小数位', () => {
  assert.equal(formatQuota(20_000_000, custom, 'en-US'), '2,000.00\u00a0积分')
  assert.equal(formatQuota(-15_005, custom, 'en-US'), '-1.50\u00a0积分')
  assert.equal(formatQuota(15_050, custom, 'en-US'), '1.51\u00a0积分')
})

test('前缀符号与面额变化识别保持稳定', () => {
  assert.equal(formatQuota(10_000, { ...custom, unit_symbol: 'P', symbol_position: 'prefix', fraction_digits: 0 }, 'en-US'), 'P1')
  assert.equal(formatQuota(-10_000, { ...custom, unit_symbol: 'P', symbol_position: 'prefix', fraction_digits: 0 }, 'en-US'), '-P1')
  assert.equal(balanceDisplayScaleChanged(DEFAULT_QUOTA_DISPLAY_POLICY, custom), true)
  assert.equal(balanceDisplayScaleChanged(custom, { ...custom, unit_symbol: '点数' }), false)
})

test('非法面额和非安全额度不会静默丢失精度', () => {
  assert.throws(() => formatQuota(Number.MAX_SAFE_INTEGER + 1, custom, 'en-US'), RangeError)
  assert.throws(() => formatQuota(1, { ...custom, quota_units_per_display_unit: '0' }, 'en-US'), RangeError)
})
