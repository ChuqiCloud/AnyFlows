import assert from 'node:assert/strict'
import test from 'node:test'

import { filterProviders, findProvider, providerDisplayName } from '../src/components/brand/provider-catalog.ts'

test('provider aliases share brand names while custom names keep their spelling', () => {
  assert.equal(providerDisplayName('deepseek'), 'DeepSeek')
  assert.equal(providerDisplayName('MINIMAX'), 'MiniMax')
  assert.equal(providerDisplayName('Google Gemini'), 'Google Gemini')
  assert.equal(findProvider('gemini').id, 'google')
  assert.equal(findProvider('silicon-cloud').id, 'siliconflow')
  assert.equal(providerDisplayName('Acme Private AI'), 'Acme Private AI')
  assert.equal(findProvider('unknown'), undefined)
})

test('provider search covers names, identifiers and localized aliases', () => {
  assert.ok(filterProviders('  DeepSeek ').some((item) => item.id === 'deepseek'))
  assert.ok(filterProviders('硅基').some((item) => item.id === 'siliconflow'))
  assert.ok(filterProviders('Jina').some((item) => item.id === 'jina'))
  assert.equal(filterProviders('Acme Private AI').length, 0)
})
