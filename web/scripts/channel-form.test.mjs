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
  invalidTimeout: 'timeout',
  invalidUrl: 'url',
}

test('Anthropic 渠道默认值保留类型和 Messages 协议', () => {
  const values = defaultChannelValues({
    id: 1,
    type: 'anthropic',
    name: 'Anthropic',
    protocol: 'anthropic',
    base_url: 'https://api.anthropic.com',
    timeout_secs: 60,
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
  assert.equal(values.timeoutSeconds, '60')
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

test('渠道超时留空使用服务默认并只接受一到九百秒整数', () => {
  const schema = buildChannelFormSchema('create', messages)
  const inherited = {
    ...defaultChannelValues(),
    name: 'Inherited timeout',
  }
  assert.equal(schema.safeParse(inherited).success, true)
  assert.equal(toCreateRequest(inherited).timeout_secs, null)

  for (const timeoutSeconds of ['1', '60', '900']) {
    const values = { ...inherited, timeoutSeconds }
    assert.equal(schema.safeParse(values).success, true)
    assert.equal(toCreateRequest(values).timeout_secs, Number(timeoutSeconds))
  }
  for (const timeoutSeconds of ['0', '1.5', '901', 'not-a-number']) {
    assert.equal(schema.safeParse({ ...inherited, timeoutSeconds }).success, false)
  }
})
