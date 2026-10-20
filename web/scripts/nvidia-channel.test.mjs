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

const nvidia = () => ({
  ...defaultChannelValues(),
  name: 'NVIDIA channel',
  provider: 'nvidia',
  channelType: 'nvidia',
  protocol: 'openai_chat',
})

test('NVIDIA create and edit preserve adapter, namespaced models and custom NIM URLs', () => {
  for (const baseUrl of ['', 'https://nim.example/proxy/v1']) {
    const values = { ...nvidia(), baseUrl, models: ['meta/llama-3.1-8b-instruct'], groupIds: [7] }
    assert.equal(buildChannelFormSchema('create', messages).safeParse(values).success, true)
    const request = toCreateRequest(values)
    assert.equal(request.type, 'nvidia')
    assert.equal(request.provider, 'nvidia')
    assert.equal(request.protocol, 'openai_chat')
    assert.equal(request.base_url, baseUrl || null)
    const editing = defaultChannelValues({ ...request, id: 7 })
    assert.equal(editing.channelType, 'nvidia')
    assert.equal(editing.protocol, 'openai_chat')
    assert.equal(editing.baseUrl, baseUrl)
    assert.equal(buildChannelFormSchema('update', messages).safeParse(editing).success, true)
    assert.equal(toUpdateRequest(editing).type, 'nvidia')
    assert.deepEqual(toUpdateRequest(editing).models, values.models)
  }
})

test('NVIDIA rejects unsupported endpoints and exposes only API Key credentials', () => {
  const schema = buildChannelFormSchema('create', messages)
  for (const protocol of ['openai_responses', 'openai_embeddings', 'openai_images', 'openai_audio', 'openai_speech', 'anthropic', 'gemini', 'jina_rerank', 'cohere_rerank', 'xai_video']) {
    assert.equal(schema.safeParse({ ...nvidia(), protocol }).success, false, protocol)
  }
  assert.equal(schema.safeParse({ ...nvidia(), responsesWebsocketEnabled: true }).success, false)
  assert.deepEqual(credentialKindsForChannel('nvidia'), ['api_key'])
  assert.equal(findProvider('nvidia').logo, 'NVIDIA')
})

test('NVIDIA is included in exported channel contracts', () => {
  const schema = JSON.parse(readFileSync(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
  for (const name of ['AdminChannelCreateRequest', 'AdminChannelUpdateRequest']) {
    const type = schema.components.schemas[name].properties.type
    assert.ok((type.enum ?? type.oneOf?.[0]?.enum ?? []).includes('nvidia'), name)
  }
})
