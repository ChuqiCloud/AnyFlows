import assert from 'node:assert/strict'
import test from 'node:test'

import {
  channelCapabilities,
  channelEditorTypes,
  channelProtocols,
  protocolsForChannel,
  selectChannelProtocol,
  supportsModelDiscovery,
  supportsProbe,
} from '../src/features/channels/channel-capabilities.ts'
import { buildChannelFormSchema, defaultChannelValues } from '../src/features/channels/channel-form-model.ts'
import { credentialKindsForChannel } from '../src/features/credentials/credential-model.ts'

const messages = Object.fromEntries([
  'duplicateKey', 'invalidEntries', 'invalidField', 'invalidHeader', 'invalidParameter',
  'invalidAutoBanRules', 'invalidClientSimulationRisk', 'invalidClientSimulationBodyRisk',
  'invalidRouting', 'invalidTimeout', 'invalidUrl',
].map((key) => [key, key]))

test('channel form accepts exactly the native protocols exposed by each adapter', () => {
  const schema = buildChannelFormSchema('create', messages)
  for (const channelType of channelEditorTypes) {
    for (const protocol of channelProtocols) {
      const values = { ...defaultChannelValues(), name: 'Channel', channelType, protocol }
      assert.equal(
        schema.safeParse(values).success,
        protocolsForChannel(channelType).includes(protocol),
        `${channelType}/${protocol}`,
      )
    }
  }
})

test('switching adapters preserves compatible protocols and resets incompatible selections', () => {
  assert.equal(selectChannelProtocol('openai', 'openai_responses'), 'openai_responses')
  assert.equal(selectChannelProtocol('opencode', 'anthropic'), 'anthropic')
  assert.equal(selectChannelProtocol('nvidia', 'anthropic'), 'openai_chat')
  assert.equal(selectChannelProtocol('codex_oauth', 'openai_chat'), 'openai_responses')
  for (const channelType of channelEditorTypes) {
    assert.ok(protocolsForChannel(channelType).includes(selectChannelProtocol(channelType)))
  }
})

test('unsupported channel operations and unassembled adapters stay unavailable', () => {
  assert.equal(supportsProbe('xai', 'xai_video'), false)
  assert.equal(supportsProbe('nvidia', 'openai_chat'), true)
  assert.equal(supportsProbe('nvidia', 'openai_responses'), false)
  assert.equal(supportsModelDiscovery('openai', 'openai_responses'), true)
  for (const [type, protocol] of [['xai', 'xai_video'], ['jina', 'jina_rerank'], ['cohere', 'cohere_rerank']]) {
    assert.equal(supportsModelDiscovery(type, protocol), false)
  }
  for (const type of ['bedrock', 'vertex', 'custom']) {
    assert.equal(channelEditorTypes.includes(type), false)
    assert.equal(supportsProbe(type, 'openai_chat'), false)
    assert.equal(supportsModelDiscovery(type, 'openai_chat'), false)
  }
})

test('new credential choices preserve authentication boundaries and legacy compatibility', () => {
  assert.deepEqual(credentialKindsForChannel('openai', 'codex'), ['oauth'])
  assert.deepEqual(credentialKindsForChannel('opencode'), ['api_key', 'public'])
  assert.deepEqual(credentialKindsForChannel('nvidia'), ['api_key'])
  assert.deepEqual(credentialKindsForChannel('vertex'), ['oauth', 'service_account'])
  assert.ok(channelCapabilities.anthropic.credentialKinds.includes('setup_token'))
  assert.equal(credentialKindsForChannel('anthropic').includes('setup_token'), false)
})
