import assert from 'node:assert/strict'
import test from 'node:test'

import {
  buildPaginationItems,
  getPageSlice,
} from '../src/components/data-table/data-table-pagination-model.ts'

test('共享分页完整展示少量可访问页', () => {
  assert.deepEqual(buildPaginationItems(2, 4), [1, 2, 3, 4])
})

test('共享分页在中间页保留首尾和相邻页', () => {
  assert.deepEqual(buildPaginationItems(6, 12), [1, 'ellipsis', 5, 6, 7, 'ellipsis', 12])
})

test('共享分页在尾部合并相邻页面', () => {
  assert.deepEqual(buildPaginationItems(12, 12), [1, 'ellipsis', 8, 9, 10, 11, 12])
})

test('本地表格在数据缩短后把当前页收敛到末页', () => {
  assert.deepEqual(getPageSlice(34, 20, 4), {
    endIndex: 34,
    pageCount: 2,
    pageIndex: 1,
    startIndex: 20,
  })
})
