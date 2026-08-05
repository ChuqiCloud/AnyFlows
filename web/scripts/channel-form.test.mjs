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
  invalidParameter: 'parameter',
  invalidAutoBanRules: 'auto-ban-rules',
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
    auto_ban_rules: { status_codes: [503], keywords: ['workspace disabled'] },
    pool_mode: true,
    responses_websocket_enabled: false,
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
  assert.deepEqual(values.autoBanStatusCodes, [503])
  assert.deepEqual(values.autoBanKeywords, ['workspace disabled'])
  assert.equal(values.poolMode, true)
})

test('外部账号池模式按结构化字段提交且默认关闭', () => {
  const defaults = defaultChannelValues()
  assert.equal(defaults.poolMode, false)
  assert.equal(toCreateRequest({ ...defaults, name: 'Pool channel', poolMode: true }).pool_mode, true)
})

test('自动停用规则只接受有界精确 5xx 状态码和关键词', () => {
  const schema = buildChannelFormSchema('create', messages)
  const valid = {
    ...defaultChannelValues(),
    name: 'Auto ban rules',
    autoBanStatusCodes: [500, 503],
    autoBanKeywords: ['workspace disabled'],
  }

  assert.equal(schema.safeParse(valid).success, true)
  assert.deepEqual(toCreateRequest(valid).auto_ban_rules, {
    status_codes: [500, 503],
    keywords: ['workspace disabled'],
  })
  assert.equal(schema.safeParse({ ...valid, autoBanStatusCodes: [499] }).success, false)
  assert.equal(schema.safeParse({ ...valid, autoBanKeywords: ['line\nbreak'] }).success, false)
  assert.equal(schema.safeParse({
    ...valid,
    autoBanStatusCodes: Array.from({ length: 65 }, (_, index) => 500 + (index % 100)),
  }).success, false)
})

test('Responses WebSocket 仅允许原生 Responses 渠道开启', () => {
  const schema = buildChannelFormSchema('create', messages)
  const valid = {
    ...defaultChannelValues(),
    name: 'Responses WS',
    channelType: 'openai',
    protocol: 'openai_responses',
    responsesWebsocketEnabled: true,
  }
  assert.equal(schema.safeParse(valid).success, true)
  assert.equal(toCreateRequest(valid).responses_websocket_enabled, true)
  assert.equal(schema.safeParse({ ...valid, protocol: 'openai_chat' }).success, false)
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
