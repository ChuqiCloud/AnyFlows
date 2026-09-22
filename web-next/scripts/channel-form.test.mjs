import assert from 'node:assert/strict'
import test from 'node:test'

import {
  buildChannelFormSchema,
  defaultChannelValues,
  toCreateRequest,
  toUpdateRequest,
} from '../src/features/channels/channel-form-model.ts'

const messages = {
  duplicateKey: 'duplicate',
  invalidEntries: 'entries',
  invalidField: 'field',
  invalidHeader: 'header',
  invalidParameter: 'parameter',
  invalidAutoBanRules: 'auto-ban-rules',
  invalidClientSimulationRisk: 'client-simulation-risk',
  invalidClientSimulationBodyRisk: 'client-simulation-body-risk',
  invalidRouting: 'routing',
  invalidTimeout: 'timeout',
  invalidUrl: 'url',
}

test('custom provider persists independently from the transport and leaves settings untouched', () => {
  const values = { ...defaultChannelValues(), name: 'Private channel', provider: 'Acme Private AI' }
  assert.equal(buildChannelFormSchema('create', messages).safeParse(values).success, true)
  assert.equal(toCreateRequest(values).provider, 'Acme Private AI')
  assert.equal(toCreateRequest(values).type, 'openai')
  assert.equal(toUpdateRequest(values).settings, undefined)
  assert.equal(defaultChannelValues({ type: 'openai', provider: 'deepseek' }).provider, 'deepseek')
  assert.equal(defaultChannelValues({ type: 'anthropic' }).provider, 'anthropic')
  for (const provider of ['', 'x'.repeat(65), 'bad\nvalue', '厂'.repeat(22)]) {
    assert.equal(buildChannelFormSchema('create', messages).safeParse({ ...values, provider }).success, false)
  }
})

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

test('客户端仿真仅允许 Anthropic 并要求首次启用确认风险', () => {
  const schema = buildChannelFormSchema('create', messages)
  const enabled = {
    ...defaultChannelValues(),
    name: 'Anthropic simulation',
    channelType: 'anthropic',
    protocol: 'anthropic',
    clientSimulationProfile: 'anthropic_cli_headers_v1',
  }

  assert.equal(schema.safeParse(enabled).success, false)
  assert.equal(schema.safeParse({
    ...enabled,
    clientSimulationRiskAccepted: true,
  }).success, true)
  assert.equal(schema.safeParse({
    ...enabled,
    channelType: 'openai',
    protocol: 'openai_chat',
    clientSimulationRiskAccepted: true,
  }).success, false)

  const request = toCreateRequest({
    ...enabled,
    clientSimulationRiskAccepted: true,
  })
  assert.equal(request.client_simulation_profile, 'anthropic_cli_headers_v1')
  assert.equal(request.client_simulation_risk_accepted, true)
})

test('正文仿真档案要求匹配 Header 档案与独立风险确认', () => {
  const schema = buildChannelFormSchema('create', messages)
  const enabled = {
    ...defaultChannelValues(),
    name: 'Anthropic body simulation',
    channelType: 'anthropic',
    protocol: 'anthropic',
    clientSimulationProfile: 'anthropic_cli_headers_v1',
    clientSimulationRiskAccepted: true,
    clientSimulationBodyProfile: 'anthropic_cli_system_date_v1',
  }

  assert.equal(schema.safeParse(enabled).success, false)
  assert.equal(schema.safeParse({
    ...enabled,
    clientSimulationBodyRiskAccepted: true,
  }).success, true)
  assert.equal(schema.safeParse({
    ...enabled,
    clientSimulationProfile: '',
    clientSimulationBodyRiskAccepted: true,
  }).success, false)

  const request = toCreateRequest({
    ...enabled,
    clientSimulationBodyRiskAccepted: true,
  })
  assert.equal(request.client_simulation_body_profile, 'anthropic_cli_system_date_v1')
  assert.equal(request.client_simulation_body_risk_accepted, true)
})

test('已启用的同一仿真档案无需在每次更新时重复确认', () => {
  const values = {
    ...defaultChannelValues(),
    name: 'Existing simulation',
    channelType: 'anthropic',
    protocol: 'anthropic',
    clientSimulationProfile: 'anthropic_cli_headers_v1',
  }
  const schema = buildChannelFormSchema(
    'update',
    messages,
    'anthropic_cli_headers_v1',
  )

  assert.equal(schema.safeParse(values).success, true)
  assert.equal(schema.safeParse({ ...values, clientSimulationProfile: '' }).success, true)
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

test('Responses Compact 三态能力与专属映射只允许原生 Responses 渠道', () => {
  const schema = buildChannelFormSchema('create', messages)
  const valid = {
    ...defaultChannelValues(),
    name: 'Responses Compact',
    channelType: 'openai',
    protocol: 'openai_responses',
    responsesCompactMode: 'force_on',
    responsesCompactModelMapping: [{
      id: 'compact-mapping-1',
      key: 'gpt-public',
      value: 'gpt-compact',
    }],
  }

  assert.equal(schema.safeParse(valid).success, true)
  const request = toCreateRequest(valid)
  assert.equal(request.responses_compact_mode, 'force_on')
  assert.deepEqual(request.responses_compact_model_mapping, {
    'gpt-public': 'gpt-compact',
  })
  assert.equal(schema.safeParse({ ...valid, protocol: 'openai_chat' }).success, false)
  assert.equal(schema.safeParse({
    ...valid,
    responsesCompactModelMapping: [
      ...valid.responsesCompactModelMapping,
      {
        id: 'compact-mapping-2',
        key: 'gpt-public',
        value: 'gpt-other',
      },
    ],
  }).success, false)
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

test('OpenAI Audio 渠道禁止携带通用 Chat 参数覆盖', () => {
  const schema = buildChannelFormSchema('create', messages)
  const values = {
    ...defaultChannelValues(),
    name: 'Audio transcription',
    channelType: 'openai',
    protocol: 'openai_audio',
  }

  assert.equal(schema.safeParse(values).success, true)
  const request = toCreateRequest(values)
  assert.equal(request.type, 'openai')
  assert.equal(request.protocol, 'openai_audio')

  const speechValues = {
    ...defaultChannelValues(),
    name: 'Speech',
    channelType: 'openai',
    protocol: 'openai_speech',
  }
  assert.equal(schema.safeParse(speechValues).success, true)
  const speech = toCreateRequest(speechValues)
  assert.equal(speech.protocol, 'openai_speech')
  assert.deepEqual(request.param_override, {})
  assert.equal(schema.safeParse({
    ...values,
    paramOverride: [{ id: 'temperature', key: 'temperature', value: '0.5', values: [] }],
  }).success, false)
  assert.equal(schema.safeParse({
    ...speechValues,
    paramOverride: [{ id: 'temperature', key: 'temperature', value: '0.5', values: [] }],
  }).success, false)
})

test('Jina 渠道只允许 Rerank 协议且禁止 Chat 参数覆盖', () => {
  const schema = buildChannelFormSchema('create', messages)
  const values = {
    ...defaultChannelValues(),
    name: 'Jina Rerank',
    channelType: 'jina',
    protocol: 'jina_rerank',
  }

  assert.equal(schema.safeParse(values).success, true)
  const request = toCreateRequest(values)
  assert.equal(request.type, 'jina')
  assert.equal(request.protocol, 'jina_rerank')
  assert.deepEqual(request.param_override, {})
  assert.equal(schema.safeParse({ ...values, protocol: 'openai_chat' }).success, false)
  assert.equal(schema.safeParse({
    ...values,
    paramOverride: [{ id: 'temperature', key: 'temperature', value: '0.5', values: [] }],
  }).success, false)
})

test('Cohere 渠道只允许 v2 Rerank 协议且禁止 Chat 参数覆盖', () => {
  const schema = buildChannelFormSchema('create', messages)
  const values = {
    ...defaultChannelValues(),
    name: 'Cohere Rerank',
    channelType: 'cohere',
    protocol: 'cohere_rerank',
  }

  assert.equal(schema.safeParse(values).success, true)
  const request = toCreateRequest(values)
  assert.equal(request.type, 'cohere')
  assert.equal(request.protocol, 'cohere_rerank')
  assert.deepEqual(request.param_override, {})
  assert.equal(schema.safeParse({ ...values, protocol: 'jina_rerank' }).success, false)
  assert.equal(schema.safeParse({
    ...values,
    paramOverride: [{ id: 'temperature', key: 'temperature', value: '0.5', values: [] }],
  }).success, false)
})

test('xAI 渠道只允许视频任务协议且禁止 Chat 参数覆盖', () => {
  const schema = buildChannelFormSchema('create', messages)
  const values = {
    ...defaultChannelValues(),
    name: 'xAI Video',
    channelType: 'xai',
    protocol: 'xai_video',
  }

  assert.equal(schema.safeParse(values).success, true)
  const request = toCreateRequest(values)
  assert.equal(request.type, 'xai')
  assert.equal(request.protocol, 'xai_video')
  assert.deepEqual(request.param_override, {})
  assert.equal(schema.safeParse({ ...values, protocol: 'openai_images' }).success, false)
  assert.equal(schema.safeParse({
    ...values,
    paramOverride: [{ id: 'temperature', key: 'temperature', value: '0.5', values: [] }],
  }).success, false)
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
