import assert from 'node:assert/strict'
import test from 'node:test'

import {
  EMPTY_MODEL_CATALOG_FILTERS,
  countModelCatalogFilters,
  toggleModelCatalogFilter,
} from '../src/features/models/model-catalog-filter-model.ts'

test('供应商与高级条件共同计入活动筛选数量', () => {
  assert.equal(countModelCatalogFilters(EMPTY_MODEL_CATALOG_FILTERS), 0)
  assert.equal(countModelCatalogFilters({
    ...EMPTY_MODEL_CATALOG_FILTERS,
    billingMode: 'free',
    providers: ['OpenAI', 'community'],
    inputModalities: ['text'],
    capabilities: ['tool_calls'],
  }), 5)
})

test('多选筛选切换保持稳定排序且支持移除', () => {
  assert.deepEqual(toggleModelCatalogFilter(['OpenAI'], 'Anthropic', true), ['Anthropic', 'OpenAI'])
  assert.deepEqual(toggleModelCatalogFilter(['Anthropic', 'OpenAI'], 'OpenAI', false), ['Anthropic'])
})
