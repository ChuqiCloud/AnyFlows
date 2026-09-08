import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  createTopupAttempt,
  parseStoredTopupAttempt,
  topupAmountMinor,
} from '../src/features/wallet/topup-form-model.ts'

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

test('当前用户充值建单不接受用户或额度等越权字段', () => {
  assert.equal(
    openapi.paths['/api/account/wallet/topups'].post.operationId,
    'createUserTopupOrder',
  )
  const request = openapi.components.schemas.UserTopupOrderCreateRequest.properties
  assert.equal(request.amount_minor.type, 'integer')
  assert.equal(request.idempotency_key.type, 'string')
  assert.equal(request.user_id, undefined)
  assert.equal(request.quota_amount, undefined)
  assert.equal(request.currency, undefined)

  const response = openapi.components.schemas.UserTopupOrder.properties
  assert.equal(response.user_id, undefined)
  assert.equal(response.idempotency_key, undefined)
  assert.equal(response.provider_order_id, undefined)
  assert.equal(response.trade_no, undefined)
})

test('当前用户充值配置不公开任何服务端支付密钥', () => {
  assert.equal(
    openapi.paths['/api/account/wallet/topups/config'].get.operationId,
    'getUserTopupConfiguration',
  )
  const schema = openapi.components.schemas.UserTopupConfiguration
  const serialized = JSON.stringify(schema)
  assert.doesNotMatch(serialized, /secret_key|webhook_secret|merchant_key|pkey/i)
})

test('充值金额转换与幂等恢复不经过浮点金额', () => {
  const bounds = { min: 50, max: 99_999_999 }
  assert.equal(topupAmountMinor('0.50', bounds), 50)
  assert.equal(topupAmountMinor('10.09', bounds), 1_009)
  assert.equal(topupAmountMinor('10.001', bounds), undefined)
  assert.equal(topupAmountMinor('0.49', bounds), undefined)

  const attempt = createTopupAttempt(1_009, 'stripe', 'card', (buffer) => buffer.fill(0xab))
  assert.deepEqual(attempt, {
    amountMinor: 1_009,
    idempotencyKey: 'abababababababababababababababab',
    paymentMethod: 'card',
    provider: 'stripe',
  })
  assert.deepEqual(parseStoredTopupAttempt(JSON.stringify(attempt), bounds), attempt)
  assert.equal(parseStoredTopupAttempt('{"amountMinor":1009,"idempotencyKey":"invalid"}', bounds), undefined)
})

test('钱包充值使用 Stripe Payment Element 而不是自建银行卡字段', () => {
  const panel = readFileSync(new URL('../src/features/wallet/wallet-topup-panel.tsx', import.meta.url), 'utf8')
  const payment = readFileSync(new URL('../src/features/wallet/wallet-stripe-payment-form.tsx', import.meta.url), 'utf8')
  assert.match(panel, /<Elements/)
  assert.match(payment, /<PaymentElement/)
  assert.doesNotMatch(`${panel}\n${payment}`, /card_number|card_cvc|card_expiry/)
  assert.doesNotMatch(panel, /setItem\([^\n]+client_secret/)
})
