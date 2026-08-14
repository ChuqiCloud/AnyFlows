import assert from 'node:assert/strict'
import test from 'node:test'

import { buildCursorPageItems } from '../src/features/debug-traces/debug-trace-pagination-model.ts'

test('调试追踪分页完整展示少量已访问页', () => {
  assert.deepEqual(buildCursorPageItems(2, 4), [1, 2, 3, 4])
})

test('调试追踪分页在中间页保留首尾和相邻页', () => {
  assert.deepEqual(buildCursorPageItems(6, 12), [1, 'ellipsis', 5, 6, 7, 'ellipsis', 12])
})

test('调试追踪分页在尾部合并相邻页面', () => {
  assert.deepEqual(buildCursorPageItems(12, 12), [1, 'ellipsis', 8, 9, 10, 11, 12])
})
