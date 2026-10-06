import assert from 'node:assert/strict'
import test from 'node:test'

import {
  formatContextWindow,
  formatExactContextWindow,
} from '../src/features/models/model-context-window.ts'

test('上下文窗口按 K 和 M 自动换算', () => {
  assert.equal(formatContextWindow(999), '999')
  assert.equal(formatContextWindow(1_000), '1K')
  assert.equal(formatContextWindow(256_000), '256K')
  assert.equal(formatContextWindow(262_144), '262K')
  assert.equal(formatContextWindow(300_000), '300K')
  assert.equal(formatContextWindow(1_000_000), '1M')
  assert.equal(formatContextWindow(1_500_000), '1.5M')
  assert.equal(formatContextWindow(1_048_576), '1M')
})

test('接近百万的上下文不会显示为 1000K', () => {
  assert.equal(formatContextWindow(999_999), '1M')
})

test('完整上下文值保留本地化千位分隔', () => {
  assert.equal(formatExactContextWindow(1_048_576, 'en-US'), '1,048,576')
})
