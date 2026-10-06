import assert from 'node:assert/strict'
import test from 'node:test'

import { filterProviders, findProvider, findProviderOption, providerCatalog, providerDisplayName } from '../src/components/brand/provider-catalog.ts'

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

test('provider identity respects compound aliases and custom names', () => {
  assert.equal(findProvider('google-vertex').id, 'vertex')
  assert.equal(findProvider('amazon-bedrock').id, 'bedrock')
  assert.equal(findProvider('deepseek-chat').id, 'deepseek')
  assert.equal(findProvider('DeepSeekPrivate'), undefined)
  assert.equal(findProvider(''), undefined)
  const custom = { id: 'deepseek-custom', name: 'Private Service', logo: 'Jina', aliases: ['private-models'] }
  assert.equal(findProviderOption('deepseek-custom', [...providerCatalog, custom]), custom)
  assert.equal(findProviderOption('private-models', [custom]), custom)
})
