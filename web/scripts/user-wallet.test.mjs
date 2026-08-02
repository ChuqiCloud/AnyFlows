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
