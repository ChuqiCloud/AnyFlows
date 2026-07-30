import assert from 'node:assert/strict'
import test from 'node:test'

import {
  buildChannelFormSchema,
  defaultChannelValues,
  toCreateRequest,
} from '../src/features/channels/channel-form-model.ts'

const messages = {
  duplicateKey: 'duplicate',
  invalidEntries: 'entries',
  invalidField: 'field',
  invalidHeader: 'header',
  invalidJson: 'json',
  invalidParameter: 'parameter',
  invalidRouting: 'routing',
  invalidUrl: 'url',
}

test('Anthropic 渠道默认值保留类型和 Messages 协议', () => {
  const values = defaultChannelValues({
    id: 1,
    type: 'anthropic',
    name: 'Anthropic',
    protocol: 'anthropic',
    base_url: 'https://api.anthropic.com',
    status: 'enabled',
    weight: 10,
    priority: 0,
    auto_ban: true,
    models: ['claude-test'],
    group_ids: [1],
    model_mapping: {},
    param_override: { temperature: 1, stop_sequences: ['private'] },
    balance: null,
    used_quota: 0,
    tag: null,
    created_at: 1,
    updated_at: 1,
  })

  assert.equal(values.channelType, 'anthropic')
  assert.equal(values.protocol, 'anthropic')
})

test('渠道类型和协议错配必须失败关闭', () => {
  const values = {
    ...defaultChannelValues(),
    name: 'Mismatch',
    channelType: 'anthropic',
    protocol: 'openai_chat',
  }

  assert.equal(buildChannelFormSchema('create', messages).safeParse(values).success, false)
})

test('Anthropic 支持停止序列但温度上限为一', () => {
  const values = {
    ...defaultChannelValues(),
    name: 'Anthropic',
    channelType: 'anthropic',
    protocol: 'anthropic',
    paramOverride: [
      { id: 'temperature', key: 'temperature', value: '1', values: [] },
      { id: 'stop', key: 'stop_sequences', value: '', values: ['private'] },
    ],
  }
  const schema = buildChannelFormSchema('create', messages)

  assert.equal(schema.safeParse(values).success, true)
  assert.equal(schema.safeParse({
    ...values,
    paramOverride: [{ id: 'temperature', key: 'temperature', value: '1.01', values: [] }],
  }).success, false)

  const request = toCreateRequest(values)
  assert.equal(request.type, 'anthropic')
  assert.equal(request.protocol, 'anthropic')
  assert.deepEqual(request.param_override.stop_sequences, ['private'])
})
