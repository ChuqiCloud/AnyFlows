import assert from 'node:assert/strict'
import test from 'node:test'

import {
  formatUserRoutePriceRatio,
  parseUserRoutePriceRatio,
  toUserRouteRequest,
  validateUserRouteDraft,
  userRouteStrategies,
} from '../src/features/user-routes/user-route-model.ts'
import { defaultApiKeyValues, toCreateApiKeyRequest } from '../src/features/api-keys/api-key-form-model.ts'

const groups = [
  { id: 1, display_name: '基础组', ratio_micros: 1_000_000, is_default: true },
  { id: 7, display_name: '低价组', ratio_micros: 500_000, is_default: false },
]

function draft(overrides = {}) {
  return {
    name: '可靠路由', strategy: 'price_first', enabled: true,
    maxGroupAttempts: '2', maxPriceRatio: '',
    members: [
      { rowId: 'a', groupId: '7', priority: '0', weight: '2' },
      { rowId: 'b', groupId: '1', priority: '1', weight: '1' },
    ],
    ...overrides,
  }
}

test('候选分组必须来自完整授权目录且不能重复', () => {
  assert.equal(validateUserRouteDraft(draft(), groups), undefined)
  assert.equal(validateUserRouteDraft(draft({ members: [{ rowId: 'a', groupId: '99', priority: '0', weight: '1' }] }), groups), 'groups')
  assert.equal(validateUserRouteDraft(draft({ members: [draft().members[0], { ...draft().members[1], groupId: '7' }] }), groups), 'duplicate')
  assert.equal(validateUserRouteDraft(draft(), undefined), 'groups')
  assert.equal(validateUserRouteDraft(draft({ members: [{ rowId: 'a', groupId: '1e0', priority: '0', weight: '1' }] }), groups), 'groups')
  assert.equal(validateUserRouteDraft(draft({ members: [] }), groups), 'members')
  assert.equal(validateUserRouteDraft(draft({ members: Array.from({ length: 17 }, (_, index) => ({ rowId: String(index), groupId: String(index + 1), priority: String(index), weight: '1' })) }), groups), 'members')
})

test('路由限制接受零倍率上限并拒绝精度超限和不安全值', () => {
  assert.equal(validateUserRouteDraft(draft({ maxPriceRatio: '0' }), groups), undefined)
  assert.equal(toUserRouteRequest(draft({ maxPriceRatio: '0' }), groups).max_price_ratio_micros, 0)
  assert.equal(validateUserRouteDraft(draft({ maxPriceRatio: '0.0000001' }), groups), 'price')
  assert.equal(validateUserRouteDraft(draft({ maxPriceRatio: '9007199254.740992' }), groups), 'price')
  assert.equal(validateUserRouteDraft(draft({ maxGroupAttempts: '17' }), groups), 'attempts')
  assert.equal(parseUserRoutePriceRatio('9007199254.740991'), 9_007_199_254_740_991)
  assert.equal(parseUserRoutePriceRatio('9007199254.740992'), undefined)
  assert.equal(formatUserRoutePriceRatio(1_250_000), '1.25')
  assert.equal(formatUserRoutePriceRatio(0), '0')
})

test('五种策略均序列化，前端草稿行标识不进入 API 请求', () => {
  for (const strategy of userRouteStrategies) {
    const request = toUserRouteRequest(draft({ strategy }), groups)
    assert.equal(request.strategy, strategy)
    assert.deepEqual(request.members, [
      { group_id: 7, priority: 0, weight: 2 },
      { group_id: 1, priority: 1, weight: 1 },
    ])
    assert.equal('rowId' in request.members[0], false)
  }
})

test('签发 Key 时路由和单分组互斥，且只接受启用路由', () => {
  const values = { ...defaultApiKeyValues(), name: 'routed', routingMode: 'route', userRouteId: '12' }
  const route = { id: 12, name: '路由', strategy: 'ordered', enabled: true, max_group_attempts: 2, max_price_ratio_micros: null, members: [] }
  const request = toCreateApiKeyRequest(values, groups, [route])
  assert.equal(request.group_id, null)
  assert.equal(request.user_route_id, 12)
  assert.throws(() => toCreateApiKeyRequest(values, groups, [{ ...route, enabled: false }]))
  assert.throws(() => toCreateApiKeyRequest({ ...values, userRouteId: '99' }, groups, [route]))
  assert.throws(() => toCreateApiKeyRequest({ ...values, userRouteId: '1.2e1' }, groups, [route]))

  const singleGroup = toCreateApiKeyRequest({ ...values, routingMode: 'group', groupId: '7' }, groups, [route])
  assert.equal(singleGroup.group_id, 7)
  assert.equal('user_route_id' in singleGroup, false)
})
