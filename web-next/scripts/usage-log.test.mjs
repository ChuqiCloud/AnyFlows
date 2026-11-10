import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import {
  formatCompactRequestId,
  formatUsageLatency,
  usageLatencyTone,
} from '../src/features/usage-logs/usage-log-format.ts'
import {
  filterFailedCallLogs,
  filterUsageLogs,
  isAdminFailedCallLog,
  isFailedCallLog,
  summarizeUsageLogs,
  usageLogProtocols,
} from '../src/features/usage-logs/usage-log-model.ts'

const logs = [
  {
    id: 1,
    token_id: 11,
    request_id: 'req-one',
    model: 'gpt-5.5',
    protocol: 'open_ai_responses',
    operation: 'responses',
    is_stream: true,
    reasoning_effort: 'high',
    reasoning_budget_tokens: 1_024,
    first_token_ms: 2_500,
    duration_ms: 3_000,
    billing_mode: 'per_token',
    input_tokens: 4_000,
    output_tokens: 100,
    cache_read: 3_000,
    cache_creation_5m: 0,
    cache_creation_1h: 0,
    reasoning_tokens: 40,
    quota: 2_100,
    created_at: 1,
  },
  {
    id: 2,
    token_id: 12,
    request_id: 'req-two',
    model: 'deepseek-v4-flash',
    protocol: 'anthropic',
    operation: 'chat',
    is_stream: false,
    reasoning_effort: null,
    reasoning_budget_tokens: null,
    first_token_ms: 500,
    duration_ms: 1_000,
    billing_mode: 'free',
    input_tokens: 80,
    output_tokens: 20,
    cache_read: 0,
    cache_creation_5m: 0,
    cache_creation_1h: 0,
    reasoning_tokens: 0,
    quota: 0,
    created_at: 2,
  },
]

const filters = (overrides = {}) => ({
  model: '',
  protocol: 'all',
  mode: 'all',
  username: '',
  token: '',
  channel: '',
  group: '',
  requestId: '',
  upstreamRequestId: '',
  ...overrides,
})

test('日志筛选使用 OpenAPI 实际协议枚举并区分响应模式', () => {
  assert.equal(usageLogProtocols.includes('open_ai_responses'), true)
  assert.equal(usageLogProtocols.includes('openai_responses'), false)
  assert.deepEqual(filterUsageLogs(logs, filters({ model: 'GPT', protocol: 'open_ai_responses', mode: 'stream' })).map((log) => log.id), [1])
  assert.deepEqual(filterUsageLogs(logs, filters({ mode: 'sync' })).map((log) => log.id), [2])
})

test('当前页概览只聚合可见请求事实', () => {
  assert.deepEqual(summarizeUsageLogs(logs), {
    requestCount: 2,
    tokenCount: 4_200,
    quota: 2_100,
    averageFirstTokenMs: 1_500,
    rpm: 0,
    tpm: 70,
  })
})

test('失败日志筛选保留错误全文、请求 ID 与可空请求主体', () => {
  const message = `上游响应失败\n${'invalid_request:'.repeat(80)}\n错误末尾`
  const failed = {
    id: 9,
    request_id: 'request-failed-with-details',
    user_id: null,
    username: null,
    token_id: null,
    group_id: null,
    organization_id: null,
    organization_team_id: null,
    channel_id: null,
    model: 'gpt-5.5',
    protocol: 'open_ai_responses',
    operation: 'responses',
    duration_ms: 1_000,
    error_kind: 'upstream_response',
    error_code: 'upstream_bad_request',
    error_message: message,
    created_at: 3,
  }
  const original = structuredClone(failed)
  assert.deepEqual(filterFailedCallLogs([failed], ' GPT ', 'open_ai_responses'), [failed])
  assert.deepEqual(filterFailedCallLogs([failed], '', 'anthropic'), [])
  assert.equal(filterFailedCallLogs([failed], '', 'all')[0].error_message, message)
  assert.equal(isFailedCallLog(failed), true)
  assert.equal(isAdminFailedCallLog(failed), true)
  assert.equal(isFailedCallLog(logs[0]), false)
  assert.deepEqual(failed, original, '筛选不得截断错误或修改缺失主体')
})

test('用户安全错误摘要可识别为失败记录但不识别为管理明细', () => {
  const failed = {
    id: 10,
    request_id: 'request-user-failed',
    model: 'deepseek-v4-flash',
    protocol: 'anthropic',
    operation: 'chat',
    duration_ms: 0,
    error_code: 'upstream_unavailable',
    error_message: '模型服务暂时不可用',
    created_at: 4,
  }
  assert.equal(isFailedCallLog(failed), true)
  assert.equal(isAdminFailedCallLog(failed), false)
  assert.deepEqual(filterFailedCallLogs([failed], 'deepseek', 'anthropic'), [failed])
})

test('耗时格式兼顾毫秒与秒级扫描', () => {
  assert.equal(formatUsageLatency(null, 'zh-CN'), '-')
  assert.equal(formatUsageLatency(850, 'zh-CN'), '850 ms')
  assert.equal(formatUsageLatency(2_500, 'zh-CN'), '2.5 s')
})

test('首字与总耗时按独立边界返回语义色', () => {
  assert.equal(usageLatencyTone(null, 'first-token'), 'neutral')
  assert.equal(usageLatencyTone(1_000, 'first-token'), 'success')
  assert.equal(usageLatencyTone(1_001, 'first-token'), 'success')
  assert.equal(usageLatencyTone(5_001, 'first-token'), 'warning')
  assert.equal(usageLatencyTone(10_000, 'first-token'), 'destructive')
  assert.equal(usageLatencyTone(5_000, 'duration'), 'success')
  assert.equal(usageLatencyTone(15_000, 'duration'), 'warning')
  assert.equal(usageLatencyTone(15_001, 'duration'), 'warning')
  assert.equal(usageLatencyTone(30_000, 'duration'), 'destructive')
})

test('请求 ID 紧凑展示不改变可复制原值', () => {
  assert.equal(formatCompactRequestId('request-short'), 'request-short')
  assert.equal(formatCompactRequestId('019c1234-5678-7abc-9def-0123456789ab'), '019c1234-...6789ab')
})

test('中英文协议徽标覆盖生成客户端的全部闭合枚举', () => {
  for (const locale of ['zh', 'en']) {
    const messages = JSON.parse(readFileSync(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'))
    for (const protocol of usageLogProtocols) {
      assert.equal(typeof messages.usageLogs.protocol[protocol], 'string')
    }
    assert.equal(typeof messages.usageLogs.actions.openDetails, 'string')
    assert.equal(typeof messages.usageLogs.detailSections.billing, 'string')
    for (const key of ['details', 'errorCode', 'errorMessage', 'errorKind', 'principal', 'unknownUser', 'organization', 'team']) {
      assert.equal(typeof messages.usageLogs.failed[key], 'string', `usageLogs.failed.${key}`)
    }
    for (const key of ['copyValue', 'copiedValue', 'copyValueFailed']) {
      assert.equal(typeof messages.usageLogs.actions[key], 'string', `usageLogs.actions.${key}`)
    }
  }
})
