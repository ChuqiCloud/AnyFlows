import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import {
  formatCompactRequestId,
  formatUsageLatency,
  usageLatencyTone,
} from '../src/features/usage-logs/usage-log-format.ts'
import {
  filterUsageLogs,
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

test('日志筛选使用 OpenAPI 实际协议枚举并区分响应模式', () => {
  assert.equal(usageLogProtocols.includes('open_ai_responses'), true)
  assert.equal(usageLogProtocols.includes('openai_responses'), false)
  assert.deepEqual(filterUsageLogs(logs, 'GPT', 'open_ai_responses', 'stream').map((log) => log.id), [1])
  assert.deepEqual(filterUsageLogs(logs, '', 'all', 'sync').map((log) => log.id), [2])
})

test('当前页概览只聚合可见请求事实', () => {
  assert.deepEqual(summarizeUsageLogs(logs), {
    requestCount: 2,
    tokenCount: 4_200,
    quota: 2_100,
    averageFirstTokenMs: 1_500,
  })
})

test('耗时格式兼顾毫秒与秒级扫描', () => {
  assert.equal(formatUsageLatency(null, 'zh-CN'), '-')
  assert.equal(formatUsageLatency(850, 'zh-CN'), '850 ms')
  assert.equal(formatUsageLatency(2_500, 'zh-CN'), '2.5 s')
})

test('首字与总耗时按独立边界返回语义色', () => {
  assert.equal(usageLatencyTone(null, 'first-token'), 'neutral')
  assert.equal(usageLatencyTone(1_000, 'first-token'), 'success')
  assert.equal(usageLatencyTone(1_001, 'first-token'), 'warning')
  assert.equal(usageLatencyTone(5_001, 'first-token'), 'destructive')
  assert.equal(usageLatencyTone(5_000, 'duration'), 'success')
  assert.equal(usageLatencyTone(15_000, 'duration'), 'warning')
  assert.equal(usageLatencyTone(15_001, 'duration'), 'destructive')
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
  }
})
