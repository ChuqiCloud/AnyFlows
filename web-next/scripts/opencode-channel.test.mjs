import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import {
  buildChannelFormSchema,
  defaultChannelValues,
  toCreateRequest,
  toUpdateRequest,
} from '../src/features/channels/channel-form-model.ts'
import { credentialKindsForChannel } from '../src/features/credentials/credential-model.ts'
import { findProvider } from '../src/components/brand/provider-catalog.ts'

const messages = Object.fromEntries([
  'duplicateKey', 'invalidEntries', 'invalidField', 'invalidHeader', 'invalidParameter',
  'invalidAutoBanRules', 'invalidClientSimulationRisk', 'invalidClientSimulationBodyRisk',
  'invalidRouting', 'invalidTimeout', 'invalidUrl',
].map((key) => [key, key]))

const opencode = () => ({
  ...defaultChannelValues(),
  name: 'OpenCode channel',
  provider: 'opencode',
  channelType: 'opencode',
})

test('OpenCode preserves all three native protocols on create and update', () => {
  for (const protocol of ['openai_chat', 'openai_responses', 'anthropic']) {
    const values = { ...opencode(), protocol }
    assert.equal(buildChannelFormSchema('create', messages).safeParse(values).success, true)
    const request = toCreateRequest(values)
    assert.equal(request.type, 'opencode')
    assert.equal(request.protocol, protocol)
    assert.equal(request.base_url, null)

    const editing = defaultChannelValues({ ...request, id: 7 })
    assert.equal(editing.channelType, 'opencode')
    assert.equal(editing.protocol, protocol)
    assert.equal(buildChannelFormSchema('update', messages).safeParse(editing).success, true)
    const update = toUpdateRequest(editing)
    assert.equal(update.type, 'opencode')
    assert.equal(update.protocol, protocol)
    assert.equal(update.base_url, null)
    assert.equal(update.settings, undefined)
    assert.equal(update.header_override, undefined)
  }
})

test('OpenCode rejects external URLs, unsupported protocols and other service behaviors', () => {
  const schema = buildChannelFormSchema('create', messages)
  const values = opencode()
  assert.equal(schema.safeParse({ ...values, baseUrl: 'https://example.com' }).success, false)
  for (const protocol of ['gemini', 'openai_embeddings', 'openai_images', 'openai_audio', 'openai_speech', 'jina_rerank', 'cohere_rerank', 'xai_video']) {
    assert.equal(schema.safeParse({ ...values, protocol }).success, false, protocol)
  }
  assert.equal(schema.safeParse({ ...values, protocol: 'openai_responses', responsesWebsocketEnabled: true }).success, false)
  assert.equal(schema.safeParse({ ...values, protocol: 'openai_responses', responsesCompactMode: 'force_on' }).success, false)
  assert.equal(schema.safeParse({ ...values, protocol: 'anthropic', clientSimulationProfile: 'anthropic_cli_headers_v1', clientSimulationRiskAccepted: true }).success, false)
  assert.deepEqual(credentialKindsForChannel('opencode'), ['api_key'])
  assert.equal(findProvider('opencode').logo, 'OpenCode')
})

test('OpenAPI channel write contracts include OpenCode', () => {
  const schema = JSON.parse(readFileSync(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
  for (const name of ['AdminChannelCreateRequest', 'AdminChannelUpdateRequest']) {
    const type = schema.components.schemas[name].properties.type
    const variants = type.enum ?? type.oneOf?.[0]?.enum ?? []
    assert.ok(variants.includes('opencode'), name)
  }
})
