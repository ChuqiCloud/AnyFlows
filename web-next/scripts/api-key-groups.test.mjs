import assert from 'node:assert/strict'
import test from 'node:test'

import { formatGroupBaseRatio, isSelectableTokenGroup } from '../src/features/api-keys/api-key-group-model.ts'
import {
  apiKeyRequestWithStatus,
  defaultApiKeyValues,
  toApiKeyRequest,
  toCreateApiKeyRequest,
} from '../src/features/api-keys/api-key-form-model.ts'

const groups = [
  { id: 1, display_name: '默认', ratio_micros: 1_000_000, is_default: true },
  { id: 7, display_name: '免费', ratio_micros: 0, is_default: false },
]

test('授权目录未加载时拒绝签发，未知组不能成为固定绑定', () => {
  assert.equal(isSelectableTokenGroup(''), false)
  assert.equal(isSelectableTokenGroup('7'), false)
  assert.equal(isSelectableTokenGroup('8', groups), false)
  assert.equal(isSelectableTokenGroup('NaN', groups), false)
  assert.throws(() => toCreateApiKeyRequest({ ...defaultApiKeyValues(), name: 'key', groupId: '8' }, groups))
})

test('创建请求区分跟随默认与固定分组，并允许零倍率组', () => {
  const values = { ...defaultApiKeyValues(), name: 'key' }
  assert.equal(toCreateApiKeyRequest(values, groups).group_id, null)
  assert.equal(toCreateApiKeyRequest({ ...values, groupId: '1' }, groups).group_id, 1)
  assert.equal(toCreateApiKeyRequest({ ...values, groupId: '7' }, groups).group_id, 7)
  assert.equal(formatGroupBaseRatio(groups[1].ratio_micros), '0')
  assert.equal(formatGroupBaseRatio(1_250_000), '1.25')
})

test('旧Key固定组即使已不在目录中，编辑及启停都不提交新绑定', () => {
  const token = {
    id: 4, name: 'retained', key_prefix: 'sk-af-prefix', status: 'enabled',
    group_id: 99, effective_group_id: 99, group_display_name: '历史分组',
    remain_quota: 100, unlimited_quota: false, used_quota: 20,
    expired_at: null, model_limits: ['model-a'], allow_ips: null,
    created_at: 1_800_000_000, updated_at: 1_800_000_000,
  }
  const values = defaultApiKeyValues(token)
  assert.equal(values.groupId, '99')
  assert.equal(isSelectableTokenGroup(values.groupId, groups), false)
  const edited = toApiKeyRequest({ ...values, name: 'renamed' })
  assert.equal('group_id' in edited, false)
  assert.equal(edited.name, 'renamed')
  const toggled = apiKeyRequestWithStatus(token, 'disabled')
  assert.equal('group_id' in toggled, false)
  assert.equal(toggled.status, 'disabled')
  assert.deepEqual(toggled.model_limits, ['model-a'])
})

test('智能路由 Key 只在签发时提交路由绑定，编辑和启停不提交分组或路由', () => {
  const route = { id: 12, name: '稳健路由', strategy: 'stable_first', enabled: true, max_group_attempts: 2, max_price_ratio_micros: null, members: [{ group_id: 1, priority: 0, weight: 1 }] }
  const values = { ...defaultApiKeyValues(), name: 'routed', routingMode: 'route', userRouteId: '12' }
  const create = toCreateApiKeyRequest(values, groups, [route])
  assert.equal(create.group_id, null)
  assert.equal(create.user_route_id, 12)

  const token = {
    id: 5, name: 'routed', key_prefix: 'sk-af-route', status: 'enabled',
    group_id: null, effective_group_id: 1, group_display_name: '默认', user_route_id: 12,
    remain_quota: 100, unlimited_quota: false, used_quota: 20,
    expired_at: null, model_limits: null, allow_ips: null,
    created_at: 1_800_000_000, updated_at: 1_800_000_000,
  }
  const edited = toApiKeyRequest({ ...defaultApiKeyValues(token), name: 'renamed' })
  const toggled = apiKeyRequestWithStatus(token, 'disabled')
  for (const request of [edited, toggled]) {
    assert.equal('group_id' in request, false)
    assert.equal('user_route_id' in request, false)
  }
})
