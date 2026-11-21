import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import { lobeHubModelCount, lobeHubProviders } from '../src/components/brand/lobehub-providers.ts'
import { findProvider } from '../src/components/brand/provider-catalog.ts'

const snapshot = JSON.parse(readFileSync(new URL('../src/features/model-management/lobehub-catalog.json', import.meta.url), 'utf8'))

test('all source provider and model pairs survive canonical grouping', () => {
  assert.match(snapshot.revision, /^[a-f0-9]{40}$/)
  const providers = new Set(lobeHubProviders.map(({ id }) => id))
  assert.equal(providers.size, lobeHubProviders.length)
  for (const provider of lobeHubProviders) {
    assert.match(provider.id, /^[a-z0-9][a-z0-9_-]{0,63}$/)
    assert.ok(provider.name.trim() === provider.name && Buffer.byteLength(provider.name) <= 128)
    assert.equal(findProvider(provider.id)?.id, provider.id)
  }
  assert.equal(snapshot.models.length, lobeHubModelCount)
  assert.equal(new Set(snapshot.models.map(({ id }) => id)).size, lobeHubModelCount)
  assert.equal(snapshot.models.reduce((count, model) => count + model.providers.length, 0), snapshot.sourceCount)
  for (const model of snapshot.models) {
    assert.ok(model.providers.includes(model.provider))
    assert.ok(model.providers.every((provider) => providers.has(provider)))
    assert.ok(model.id.trim() === model.id && Buffer.byteLength(model.id) <= 256)
    assert.ok(model.name.trim() === model.name && Buffer.byteLength(model.name) <= 128)
    for (const direction of [model.input, model.output]) {
      assert.ok(direction.length > 0 && direction.every((value) => ['text', 'image', 'audio', 'video'].includes(value)))
    }
  }
  const model = (id) => snapshot.models.find((entry) => entry.id === id)
  assert.equal(model('gpt-6.1-sol').provider, 'openai')
  assert.equal(model('glm-5').provider, 'zhipu')
  assert.deepEqual(model('sora-2').output, ['video'])
  assert.deepEqual(snapshot.models.find(({ type }) => type === 'text2music').output, ['audio'])
})
