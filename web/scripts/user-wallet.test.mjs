import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'

const openapi = JSON.parse(readFileSync(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))

test('当前用户钱包路由属于登录用户账户空间', () => {
  assert.deepEqual(routeFromHash('#/console/wallet'), { view: 'wallet' })
})

test('当前用户钱包契约不接受或返回其他主体标识', () => {
  assert.equal(openapi.paths['/api/account/wallet'].get.operationId, 'getUserWallet')
  assert.equal(
    openapi.paths['/api/account/wallet/entries'].get.operationId,
    'listUserWalletEntries',
  )
  const properties = openapi.components.schemas.UserWalletEntry.properties
  assert.deepEqual(Object.keys(properties).sort(), [
    'balance_after',
    'balance_before',
    'created_at',
    'entry_type',
    'id',
    'quota_delta',
    'reason',
  ])
  assert.equal(properties.user_id, undefined)
  assert.equal(properties.actor_user_id, undefined)
  assert.equal(properties.event_id, undefined)
})

test('当前用户充值建单只接受幂等键和整数美分金额', () => {
  assert.equal(
    openapi.paths['/api/account/wallet/topups'].post.operationId,
    'createUserTopupOrder',
  )
  const request = openapi.components.schemas.UserTopupOrderCreateRequest.properties
  assert.deepEqual(Object.keys(request).sort(), ['amount_minor', 'idempotency_key'])
  assert.equal(request.user_id, undefined)
  assert.equal(request.quota_amount, undefined)
  assert.equal(request.currency, undefined)
  assert.equal(request.provider, undefined)

  const response = openapi.components.schemas.UserTopupOrder.properties
  assert.equal(response.user_id, undefined)
  assert.equal(response.idempotency_key, undefined)
  assert.equal(response.provider_order_id, undefined)
  assert.equal(response.trade_no, undefined)
})
