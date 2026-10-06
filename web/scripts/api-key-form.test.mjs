import assert from 'node:assert/strict'
import test from 'node:test'

import {
  apiKeyRequestWithStatus,
  defaultApiKeyValues,
  isValidIpEntry,
  isValidIpList,
  toApiKeyRequest,
} from '../src/features/api-keys/api-key-form-model.ts'

test('新 API Key 默认启用且不设置独立额度上限', () => {
  const values = defaultApiKeyValues()
  assert.equal(values.status, 'enabled')
  assert.equal(values.unlimitedQuota, true)
  assert.equal(values.remainQuota, 0)
})

test('空模型和 IP 集合提交为不限制', () => {
  const request = toApiKeyRequest({
    ...defaultApiKeyValues(),
    name: 'playground',
  })
  assert.equal(request.model_limits, null)
  assert.equal(request.allow_ips, null)
  assert.equal(request.expired_at, null)
})

test('IP 编辑器接受 IPv4 IPv6 与 CIDR 并拒绝重复项', () => {
  for (const value of ['203.0.113.8', '203.0.113.0/24', '2001:db8::1', '2001:db8::/32']) {
    assert.equal(isValidIpEntry(value), true, value)
  }
  assert.equal(isValidIpEntry('203.0.113.999'), false)
  assert.equal(isValidIpList(['203.0.113.8', '203.0.113.8']), false)
})

test('列表启停保留额度和访问范围', () => {
  const request = apiKeyRequestWithStatus({
    id: 9,
    key_prefix: 'sk-af-public000001',
    name: 'primary',
    status: 'enabled',
    remain_quota: 100,
    unlimited_quota: false,
    used_quota: 20,
    expired_at: 1_900_000_000,
    model_limits: ['gpt-5.5'],
    allow_ips: ['203.0.113.0/24'],
    created_at: 1_800_000_000,
    updated_at: 1_800_000_000,
  }, 'disabled')
  assert.equal(request.status, 'disabled')
  assert.deepEqual(request.model_limits, ['gpt-5.5'])
  assert.deepEqual(request.allow_ips, ['203.0.113.0/24'])
  assert.equal(request.remain_quota, 100)
})
